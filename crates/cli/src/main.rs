mod cli;
mod config;
mod provider_bridge;
mod server_client;
mod session;
#[allow(dead_code)]
mod terminal;

use anyhow::{Context, Result};
use clap::Parser;
use futures::StreamExt;
use std::sync::Arc;
use tracing::{error, info};

use stupid_tool_runtime::permission::{PermissionLevel, PermissionPolicy, PolicyChecker};
use stupid_tool_runtime::tool::{EchoTool, ToolContext};
use stupid_tool_runtime::{AgenticLoop, Conversation, PermissionChecker, ToolRegistry};

use crate::cli::CliArgs;
use crate::config::CliConfig;
use crate::provider_bridge::create_tool_aware_provider;
use crate::server_client::ServerClient;
use crate::session::Session;
use crate::terminal::Terminal;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_target(false)
        .init();

    let args = CliArgs::parse();
    let terminal = Terminal::new();

    // Load config
    let config = CliConfig::load(args.config.as_deref())
        .context("failed to load configuration")?;

    // Handle --list-sessions
    if args.list_sessions {
        let sessions = Session::list_all()?;
        terminal.print_sessions(&sessions)?;
        return Ok(());
    }

    // Determine if we're in remote (server) mode
    let server_url = args.server.as_deref().or(config.server_url.as_deref());

    if let Some(url) = server_url {
        run_remote(url, &args, &config, &terminal).await
    } else {
        run_local(&args, &config, &terminal).await
    }
}

/// Remote mode: CLI is a thin client to the stupid-db server.
/// Messages are stored server-side and visible in the dashboard.
async fn run_remote(
    server_url: &str,
    args: &CliArgs,
    _config: &CliConfig,
    terminal: &Terminal,
) -> Result<()> {
    let client = ServerClient::new(server_url);

    // Verify server is reachable
    client
        .health_check()
        .await
        .with_context(|| format!("cannot connect to server at {}", server_url))?;

    terminal.print_info(&format!("Connected to server: {}", server_url))?;

    // Resolve or create session
    let session_id = if let Some(ref id) = args.session {
        terminal.print_info(&format!("Using session: {}", id))?;
        id.clone()
    } else {
        let session = client.create_session(None).await?;
        terminal.print_info(&format!(
            "Created session: {} ({})",
            session.id, session.name
        ))?;
        session.id
    };

    // Load system prompt for passing to server
    let system_prompt = args.system_prompt.clone().or_else(|| {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let prompt =
            stupid_tool_runtime::load_project_context(&cwd.join("agents/stupid-db-claude-code"));
        if prompt.is_empty() {
            None
        } else {
            Some(prompt)
        }
    });

    terminal.print_banner("remote", server_url)?;

    // REPL loop — send to server, render SSE events locally
    loop {
        let input = match terminal.read_input()? {
            Some(text) => text,
            None => {
                terminal.print_info("Goodbye.")?;
                break;
            }
        };

        if input.is_empty() {
            continue;
        }

        terminal.reset_cancel();

        match client
            .stream(
                &session_id,
                &input,
                system_prompt.as_deref(),
                args.max_iterations,
            )
            .await
        {
            Ok(mut event_stream) => {
                while let Some(result) = event_stream.next().await {
                    if terminal.is_cancelled() {
                        terminal.print_info("[cancelled]")?;
                        break;
                    }
                    match result {
                        Ok(event) => {
                            terminal.display_event(&event)?;
                        }
                        Err(e) => {
                            error!(error = %e, "SSE parse error");
                            terminal.print_error(&format!("Stream error: {:#}", e))?;
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "Server stream error");
                terminal.print_error(&format!("{:#}", e))?;
            }
        }
    }

    terminal.print_info(&format!("Session: {}", session_id))?;
    Ok(())
}

/// Local mode: CLI runs its own AgenticLoop with a local LLM provider.
async fn run_local(args: &CliArgs, config: &CliConfig, terminal: &Terminal) -> Result<()> {
    // Resolve provider settings
    let provider_name = &args.provider;
    let model = config.resolve_model(provider_name, args.model.as_deref());
    let api_key = config.resolve_api_key(provider_name, args.api_key.as_deref());

    // Create LLM provider
    let provider = create_tool_aware_provider(config, provider_name, &model, api_key.as_deref())
        .context("failed to create LLM provider")?;

    // Create tool registry with default tools
    let mut registry = ToolRegistry::new();
    registry
        .register(EchoTool)
        .expect("failed to register echo tool");

    // Build permission policy from config
    let mut policy = PermissionPolicy::new();
    for (tool_name, level_str) in &config.tool_permissions {
        let level = match level_str.as_str() {
            "auto" | "auto_approve" => PermissionLevel::AutoApprove,
            "confirm" | "require_confirmation" => PermissionLevel::RequireConfirmation,
            "deny" => PermissionLevel::Deny,
            _ => {
                tracing::warn!(
                    tool = %tool_name,
                    level = %level_str,
                    "Unknown permission level, defaulting to RequireConfirmation"
                );
                PermissionLevel::RequireConfirmation
            }
        };
        policy.rules.insert(tool_name.clone(), level);
    }
    let permission_checker: Arc<dyn PermissionChecker> = Arc::new(PolicyChecker::new(policy));

    // Create agentic loop
    let agentic_loop = AgenticLoop::new(provider, Arc::new(registry), permission_checker)
        .with_max_iterations(args.max_iterations);

    // Load or create session
    let mut session = if let Some(ref session_id) = args.session {
        info!(session = %session_id, "Resuming session");
        let loaded = Session::load(session_id)
            .with_context(|| format!("failed to load session '{}'", session_id))?;
        terminal.print_info(&format!(
            "Resumed session: {} ({} messages)",
            loaded.name,
            loaded.messages.len()
        ))?;
        loaded
    } else {
        Session::new(
            provider_name.clone(),
            model.clone(),
            args.system_prompt.clone(),
        )
    };

    // Build conversation from session state
    let mut conversation = Conversation::new(config.max_context_tokens);
    let system_prompt = if let Some(ref prompt) = session.system_prompt {
        prompt.clone()
    } else {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        stupid_tool_runtime::load_project_context(&cwd.join("agents/stupid-db-claude-code"))
    };
    if !system_prompt.is_empty() {
        conversation = conversation.with_system_prompt(system_prompt);
    }

    // Replay existing messages into conversation
    for msg in &session.messages {
        match msg {
            stupid_tool_runtime::conversation::ConversationMessage::User(text) => {
                conversation.add_user_message(text.clone());
            }
            stupid_tool_runtime::conversation::ConversationMessage::Assistant(content) => {
                conversation.add_assistant_response(content.clone());
            }
            stupid_tool_runtime::conversation::ConversationMessage::ToolResult(result) => {
                conversation.add_tool_result(result.clone());
            }
        }
    }

    // Print banner
    terminal.print_banner(provider_name, &model)?;

    // Tool context
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let agents_dir = cwd.join("agents/stupid-db-claude-code");
    std::fs::create_dir_all(&agents_dir).ok();
    let tool_context = ToolContext {
        working_directory: agents_dir,
    };

    // REPL loop
    loop {
        let input = match terminal.read_input()? {
            Some(text) => text,
            None => {
                terminal.print_info("Goodbye.")?;
                break;
            }
        };

        if input.is_empty() {
            continue;
        }

        terminal.reset_cancel();

        match agentic_loop
            .run(&mut conversation, input.clone(), &tool_context)
            .await
        {
            Ok(events) => {
                for event in &events {
                    if terminal.is_cancelled() {
                        terminal.print_info("[cancelled]")?;
                        break;
                    }
                    terminal.display_event(event)?;
                }
            }
            Err(e) => {
                error!(error = %e, "Agentic loop error");
                terminal.print_error(&format!("{:#}", e))?;
            }
        }

        // Sync conversation messages to session and auto-save
        session.messages = conversation.messages().to_vec();
        session.update_name_from_first_message();
        if let Err(e) = session.save() {
            tracing::warn!(error = %e, "Failed to auto-save session");
        }
    }

    // Final save
    session.messages = conversation.messages().to_vec();
    session.update_name_from_first_message();
    if let Err(e) = session.save() {
        terminal.print_error(&format!("Failed to save session: {}", e))?;
    } else {
        terminal.print_info(&format!("Session saved: {}", session.id))?;
    }

    Ok(())
}
