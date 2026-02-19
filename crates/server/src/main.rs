mod anomaly_rules;
mod api;
mod app_config;
mod catalog_api;
mod cli;
mod db;
mod eisenbahn_client;
mod vector_store;
mod router;
mod rules;
mod startup;
mod athena_connections;
mod athena_query;
mod athena_query_log;
mod background;
mod connections;
mod credential_store;
mod export;
mod graph_ops;
mod import;
mod ingestion;
mod live;
mod queue;
mod queue_connections;
mod rule_runner;
mod state;

use tracing::info;

/// Initialize shared state, spawn background tasks, and start the HTTP server.
async fn serve(config: &stupid_core::Config, segment_id: Option<&str>, eisenbahn: bool) -> anyhow::Result<()> {
    config.log_summary();

    let (state, ctx) = startup::build_app_state(config, eisenbahn).await?;
    let eb_client = ctx.eb_client.clone();

    let app = router::build_router(state.clone());

    // Bind and start serving IMMEDIATELY.
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on http://localhost:{} (data loading in background)", config.server.port);

    // Spawn all background tasks (data loader, rule evaluator, file watcher, queue consumers).
    startup::spawn_background_tasks(config, state, ctx, segment_id)?;

    // Start the eisenbahn event loop and worker runner if connected.
    if let Some(ref eb) = eb_client {
        eb.start().await;
        info!("eisenbahn client active — server registered as api-gateway worker");
    }

    axum::serve(listener, app).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    let config = app_config::load_config();
    let args: Vec<String> = std::env::args().collect();

    // Dispatch non-serve subcommands; returns false for `serve`.
    if !cli::dispatch(&config, &args).await? {
        let (segment_id, eisenbahn) = cli::parse_serve_args(&args);
        serve(&config, segment_id.as_deref(), eisenbahn).await?;
    }

    Ok(())
}
