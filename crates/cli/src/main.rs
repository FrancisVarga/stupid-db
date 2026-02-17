mod cli;
mod config;
mod provider_bridge;
mod session;
#[allow(dead_code)]
mod terminal;

use anyhow::{Context, Result};
use clap::Parser;
use std::sync::Arc;
use tracing::{error, info};

use stupid_tool_runtime::permission::{PermissionLevel, PermissionPolicy, PolicyChecker};
use stupid_tool_runtime::tool::{EchoTool, ToolContext};
use stupid_tool_runtime::{AgenticLoop, Conversation, PermissionChecker, ToolRegistry};

use crate::cli::CliArgs;
use crate::config::CliConfig;
use crate::provider_bridge::create_tool_aware_provider;
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

    // Resolve provider settings
    let provider_name = &args.provider;
    let model = config.resolve_model(provider_name, args.model.as_deref());
    let api_key = config.resolve_api_key(provider_name, args.api_key.as_deref());

    // Create LLM provider
    let provider = create_tool_aware_provider(
        &config,
        provider_name,
        &model,
        api_key.as_deref(),
    )
    .context("failed to create LLM provider")?;

    // Create tool registry with default tools
    let mut registry = ToolRegistry::new();
    registry.register(EchoTool).expect("failed to register echo tool");

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
    let agentic_loop = AgenticLoop::new(
        provider,
        Arc::new(registry),
        permission_checker,
    )
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
    if let Some(ref prompt) = session.system_prompt {
        conversation = conversation.with_system_prompt(prompt.clone());
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

    // Tool context — prefer agents/stupid-db-claude-code as the working
    // directory so Claude Code always operates within its dedicated workspace.
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

        // Run through agentic loop
        match agentic_loop
            .run(&mut conversation, input.clone(), &tool_context)
            .await
        {
            Ok(events) => {
                // Display all events
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
