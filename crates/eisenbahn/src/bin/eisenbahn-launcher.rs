//! eisenbahn-launcher — Development launcher that orchestrates broker + workers.
//!
//! Reads `eisenbahn.toml`, spawns the broker first (with health check), then
//! spawns all configured workers as child processes with colored log prefixes.
//!
//! # Usage
//!
//! ```bash
//! # Start everything from default config
//! eisenbahn-launcher
//!
//! # Custom config path
//! eisenbahn-launcher --config path/to/eisenbahn.toml
//!
//! # Start only specific workers
//! eisenbahn-launcher --only compute,ingest
//! ```

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Notify;

use stupid_eisenbahn::EisenbahnConfig;

/// Orchestrates the eisenbahn messaging layer for local development.
///
/// Spawns the broker, waits for its health check, then launches all configured
/// workers with colored log prefixes (like docker-compose).
#[derive(Parser, Debug)]
#[command(name = "eisenbahn-launcher", version, about)]
struct Cli {
    /// Path to the eisenbahn configuration file.
    #[arg(long, default_value = "config/eisenbahn.toml")]
    config: String,

    /// Comma-separated list of workers to start (default: all).
    #[arg(long, value_delimiter = ',')]
    only: Option<Vec<String>>,

    /// Timeout in seconds to wait for broker health check.
    #[arg(long, default_value_t = 10)]
    health_timeout: u64,
}

// ── ANSI color palette for worker prefixes ───────────────────────────

const COLORS: &[&str] = &[
    "\x1b[36m", // cyan
    "\x1b[33m", // yellow
    "\x1b[32m", // green
    "\x1b[35m", // magenta
    "\x1b[34m", // blue
    "\x1b[91m", // bright red
    "\x1b[92m", // bright green
    "\x1b[93m", // bright yellow
    "\x1b[94m", // bright blue
    "\x1b[95m", // bright magenta
];
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";

/// A managed child process with metadata for log prefixing.
struct ManagedChild {
    name: String,
    child: Child,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config = EisenbahnConfig::from_file(&cli.config)?;

    // Determine which workers to start.
    let worker_filter: Option<Vec<String>> = cli.only;
    let workers_to_start: Vec<(String, stupid_eisenbahn::WorkerConfig)> = {
        let mut v: Vec<_> = config
            .workers
            .iter()
            .filter(|(name, _)| {
                worker_filter
                    .as_ref()
                    .map_or(true, |filter| filter.contains(name))
            })
            .map(|(name, cfg)| (name.clone(), cfg.clone()))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0)); // deterministic order
        v
    };

    if workers_to_start.is_empty() && worker_filter.is_some() {
        anyhow::bail!(
            "no matching workers found for --only {:?}. Available: {:?}",
            worker_filter.unwrap(),
            config.workers.keys().collect::<Vec<_>>()
        );
    }

    // Compute the max name length for aligned log prefixes.
    let max_name_len = workers_to_start
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(0)
        .max("broker".len());

    let shutdown = Arc::new(Notify::new());
    let mut children: Vec<ManagedChild> = Vec::new();

    // ── Step 1: Spawn the broker ─────────────────────────────────────
    tracing::info!("starting eisenbahn-broker");
    let broker_color = format!("{BOLD}\x1b[96m"); // bold bright cyan
    let broker_child = spawn_process(
        "cargo",
        &[
            "run",
            "--bin",
            "eisenbahn-broker",
            "--package",
            "stupid-eisenbahn",
            "--",
        ],
        &HashMap::new(),
        "broker",
        &broker_color,
        max_name_len,
    )?;
    children.push(ManagedChild {
        name: "broker".to_string(),
        child: broker_child,
    });

    // ── Step 2: Wait for broker health check ─────────────────────────
    let health_endpoint = resolve_health_endpoint(&config);
    tracing::info!(endpoint = %health_endpoint, "waiting for broker health check");

    if !wait_for_health(&health_endpoint, Duration::from_secs(cli.health_timeout)).await {
        tracing::error!("broker health check timed out after {}s", cli.health_timeout);
        kill_all(&mut children).await;
        anyhow::bail!("broker failed to start within {}s", cli.health_timeout);
    }
    tracing::info!("broker is healthy");

    // ── Step 3: Spawn workers ────────────────────────────────────────
    for (idx, (name, worker_cfg)) in workers_to_start.iter().enumerate() {
        let color = COLORS[idx % COLORS.len()];
        let total_instances = worker_cfg.instances.max(1);

        for instance in 0..total_instances {
            let display_name = if total_instances > 1 {
                format!("{name}.{instance}")
            } else {
                name.clone()
            };

            tracing::info!(
                worker = %display_name,
                binary = %worker_cfg.binary,
                "spawning worker"
            );

            let child = spawn_process(
                "cargo",
                &[
                    "run",
                    "--bin",
                    &worker_cfg.binary,
                    "--",
                ],
                &worker_cfg.env,
                &display_name,
                color,
                max_name_len,
            )?;
            children.push(ManagedChild {
                name: display_name,
                child,
            });
        }
    }

    tracing::info!(
        total = children.len(),
        "all processes started — press Ctrl+C to stop"
    );

    // ── Step 4: Wait for shutdown or child exit ──────────────────────
    let shutdown_for_signal = shutdown.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        shutdown_for_signal.notify_waiters();
    });

    // Wait for either shutdown signal or any child to exit unexpectedly.
    let exit_code = tokio::select! {
        _ = shutdown.notified() => {
            tracing::info!("shutdown signal received — stopping all processes");
            kill_all(&mut children).await;
            0
        }
        result = wait_for_any_exit(&mut children) => {
            match result {
                Ok((name, code)) => {
                    tracing::error!(worker = %name, code = code, "process exited unexpectedly");
                    kill_all(&mut children).await;
                    code.unwrap_or(1)
                }
                Err(e) => {
                    tracing::error!(error = %e, "error waiting for child processes");
                    kill_all(&mut children).await;
                    1
                }
            }
        }
    };

    tracing::info!("eisenbahn-launcher exited");
    std::process::exit(exit_code);
}

// ── Process management ───────────────────────────────────────────────

/// Spawn a child process and pipe its stdout/stderr through colored prefix lines.
fn spawn_process(
    program: &str,
    args: &[&str],
    env: &HashMap<String, String>,
    name: &str,
    color: &str,
    max_name_len: usize,
) -> anyhow::Result<Child> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    for (k, v) in env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn()?;

    // Pipe stdout with colored prefix.
    let prefix = format!("{color}{:>width$}{RESET} │ ", name, width = max_name_len);

    if let Some(stdout) = child.stdout.take() {
        let prefix = prefix.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                println!("{prefix}{line}");
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let prefix = format!(
            "{color}{:>width$}{RESET} │ ",
            name,
            width = max_name_len
        );
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("{prefix}{line}");
            }
        });
    }

    Ok(child)
}

/// Wait for any child process to exit and return its name + exit code.
async fn wait_for_any_exit(
    children: &mut Vec<ManagedChild>,
) -> anyhow::Result<(String, Option<i32>)> {
    loop {
        for managed in children.iter_mut() {
            if let Some(status) = managed.child.try_wait()? {
                return Ok((managed.name.clone(), status.code()));
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Send SIGTERM to all children and wait for them to exit (with a timeout).
async fn kill_all(children: &mut Vec<ManagedChild>) {
    // First, send termination signal to all children.
    for managed in children.iter_mut() {
        if let Some(pid) = managed.child.id() {
            // On Unix, send SIGTERM for graceful shutdown via the kill command.
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .output();
                tracing::info!(worker = %managed.name, pid = pid, "sent SIGTERM");
            }
            #[cfg(not(unix))]
            {
                let _ = managed.child.start_kill();
                tracing::info!(worker = %managed.name, pid = pid, "sent kill signal");
            }
        }
    }

    // Wait up to 5s for graceful shutdown, then force kill.
    let deadline = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(deadline);

    loop {
        let all_exited = children
            .iter_mut()
            .all(|m| m.child.try_wait().ok().flatten().is_some());

        if all_exited {
            tracing::info!("all processes exited gracefully");
            return;
        }

        tokio::select! {
            _ = &mut deadline => {
                tracing::warn!("graceful shutdown timed out — force killing remaining processes");
                for managed in children.iter_mut() {
                    if managed.child.try_wait().ok().flatten().is_none() {
                        let _ = managed.child.kill().await;
                        tracing::warn!(worker = %managed.name, "force killed");
                    }
                }
                return;
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                // Poll again.
            }
        }
    }
}

// ── Health check ─────────────────────────────────────────────────────

/// Resolve the broker's health check endpoint from config.
///
/// The broker's IPC health socket lives at `/tmp/stupid-db/broker-health.sock`.
/// For TCP, we probe the health port directly.
fn resolve_health_endpoint(config: &EisenbahnConfig) -> String {
    // The broker binary uses "broker-health" as IPC name or health_port for TCP.
    match config.transport.kind.as_str() {
        "tcp" => {
            let port = config.broker.metrics_port.unwrap_or(5557);
            format!("{}:{}", config.transport.default_host, port)
        }
        _ => {
            // IPC: health socket path
            "/tmp/stupid-db/broker-health.sock".to_string()
        }
    }
}

/// Wait for the broker to become healthy by probing its endpoint.
async fn wait_for_health(endpoint: &str, timeout: Duration) -> bool {
    let start = tokio::time::Instant::now();
    let interval = Duration::from_millis(200);

    while start.elapsed() < timeout {
        if endpoint.contains(':') && !endpoint.starts_with('/') {
            // TCP: try connecting to the port.
            if tokio::net::TcpStream::connect(endpoint).await.is_ok() {
                return true;
            }
        } else {
            // IPC: check if the socket file exists (broker creates it on bind).
            if std::path::Path::new(endpoint).exists() {
                // Try connecting to verify the broker is actually listening.
                if tokio::net::UnixStream::connect(endpoint).await.is_ok() {
                    return true;
                }
            }
        }
        tokio::time::sleep(interval).await;
    }
    false
}

// ── Signal handling ──────────────────────────────────────────────────

/// Wait for SIGINT or SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.expect("failed to listen for ctrl_c");
    }
}
