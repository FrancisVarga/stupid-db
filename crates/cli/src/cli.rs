use clap::Parser;

/// Interactive AI agent for stupid-db.
///
/// Provides a terminal-based REPL that connects to LLM providers
/// and executes tools through the agentic loop.
#[derive(Parser, Debug)]
#[command(name = "stupid-cli", about = "Interactive AI agent for stupid-db")]
pub struct CliArgs {
    /// LLM provider to use: claude, openai, or ollama
    #[arg(long, default_value = "claude")]
    pub provider: String,

    /// Model name override (uses provider default if not set)
    #[arg(long)]
    pub model: Option<String>,

    /// API key (overrides env var and config file)
    #[arg(long)]
    pub api_key: Option<String>,

    /// Path to config file (default: ~/.config/stupid-cli/config.toml)
    #[arg(long)]
    pub config: Option<String>,

    /// Resume a previous session by name or ID
    #[arg(long)]
    pub session: Option<String>,

    /// List all saved sessions
    #[arg(long)]
    pub list_sessions: bool,

    /// System prompt override
    #[arg(long)]
    pub system_prompt: Option<String>,

    /// Maximum agentic loop iterations per turn
    #[arg(long, default_value = "10")]
    pub max_iterations: usize,

    /// Server URL for remote mode (connects to stupid-db server instead of local LLM).
    /// When set, CLI becomes a thin client — messages are visible in the dashboard.
    #[arg(long)]
    pub server: Option<String>,
}
