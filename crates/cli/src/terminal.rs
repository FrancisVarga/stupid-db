use anyhow::Result;
use crossterm::{
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use stupid_tool_runtime::stream::StreamEvent;
use tracing::debug;

/// Color scheme for terminal output.
struct Colors;

impl Colors {
    const USER_PROMPT: Color = Color::Green;
    const ASSISTANT_TEXT: Color = Color::Cyan;
    const TOOL_CALL: Color = Color::Yellow;
    const TOOL_RESULT: Color = Color::DarkGreen;
    const ERROR: Color = Color::Red;
    const DIM: Color = Color::DarkGrey;
    const HEADER: Color = Color::Magenta;
}

/// Manages terminal I/O for the interactive REPL.
pub struct Terminal {
    /// Flag set when Ctrl+C is pressed to cancel current operation.
    cancelled: Arc<AtomicBool>,
}

impl Terminal {
    /// Create a new terminal handler.
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get a clone of the cancellation flag for async tasks.
    pub fn cancellation_token(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }

    /// Reset the cancellation flag.
    pub fn reset_cancel(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    /// Check if cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Print the startup banner.
    pub fn print_banner(&self, provider: &str, model: &str) -> Result<()> {
        let mut stdout = io::stdout();
        execute!(
            stdout,
            SetForegroundColor(Colors::HEADER),
            Print("stupid-cli"),
            ResetColor,
            Print(" - Interactive AI Agent\n"),
            SetForegroundColor(Colors::DIM),
            Print(format!("Provider: {} | Model: {}\n", provider, model)),
            Print("Type 'exit' or 'quit' to end. Ctrl+C cancels current operation.\n"),
            Print("---\n"),
            ResetColor,
        )?;
        stdout.flush()?;
        Ok(())
    }

    /// Read a line of user input with prompt.
    /// Returns None if the user wants to exit.
    pub fn read_input(&self) -> Result<Option<String>> {
        let mut stdout = io::stdout();
        execute!(
            stdout,
            Print("\n"),
            SetForegroundColor(Colors::USER_PROMPT),
            Print("you> "),
            ResetColor,
        )?;
        stdout.flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_string();

        if trimmed.is_empty() {
            return Ok(Some(String::new()));
        }

        if trimmed == "exit" || trimmed == "quit" || trimmed == "/exit" || trimmed == "/quit" {
            return Ok(None);
        }

        Ok(Some(trimmed))
    }

    /// Display a stream event in the terminal with appropriate formatting.
    pub fn display_event(&self, event: &StreamEvent) -> Result<()> {
        let mut stdout = io::stdout();
        match event {
            StreamEvent::TextDelta { text } => {
                execute!(
                    stdout,
                    SetForegroundColor(Colors::ASSISTANT_TEXT),
                    Print(text),
                    ResetColor,
                )?;
                stdout.flush()?;
            }
            StreamEvent::ToolCallStart { id: _, name } => {
                execute!(
                    stdout,
                    Print("\n"),
                    SetForegroundColor(Colors::TOOL_CALL),
                    Print(format!("[tool: {}] ", name)),
                    ResetColor,
                )?;
                stdout.flush()?;
            }
            StreamEvent::ToolCallDelta { arguments_delta, .. } => {
                // Accumulate silently; the full args are shown at ToolCallEnd
                debug!(delta = %arguments_delta, "Tool call argument delta");
            }
            StreamEvent::ToolCallEnd { id } => {
                execute!(
                    stdout,
                    SetForegroundColor(Colors::DIM),
                    Print(format!("[call {}...done]\n", &id[..id.len().min(8)])),
                    ResetColor,
                )?;
                stdout.flush()?;
            }
            StreamEvent::MessageEnd { stop_reason } => {
                debug!(?stop_reason, "Message ended");
                execute!(stdout, Print("\n"))?;
                stdout.flush()?;
            }
            StreamEvent::Error { message } => {
                execute!(
                    stdout,
                    Print("\n"),
                    SetForegroundColor(Colors::ERROR),
                    Print(format!("[error: {}]\n", message)),
                    ResetColor,
                )?;
                stdout.flush()?;
            }
        }
        Ok(())
    }

    /// Display a tool execution result.
    pub fn display_tool_result(&self, tool_name: &str, content: &str, is_error: bool) -> Result<()> {
        let mut stdout = io::stdout();
        let color = if is_error {
            Colors::ERROR
        } else {
            Colors::TOOL_RESULT
        };
        let label = if is_error { "error" } else { "result" };

        // Truncate long results for display
        let display_content = if content.len() > 500 {
            format!("{}... ({} chars total)", &content[..500], content.len())
        } else {
            content.to_string()
        };

        execute!(
            stdout,
            SetForegroundColor(color),
            Print(format!("  [{} {}]: {}\n", tool_name, label, display_content)),
            ResetColor,
        )?;
        stdout.flush()?;
        Ok(())
    }

    /// Show a spinner/waiting indicator. Returns a handle to stop it.
    pub fn start_spinner(&self, message: &str) -> Result<SpinnerHandle> {
        let mut stdout = io::stdout();
        execute!(
            stdout,
            SetForegroundColor(Colors::DIM),
            Print(format!("{} ", message)),
            ResetColor,
        )?;
        stdout.flush()?;

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let handle = std::thread::spawn(move || {
            let frames = ['|', '/', '-', '\\'];
            let mut i = 0;
            while running_clone.load(Ordering::SeqCst) {
                let mut stdout = io::stdout();
                execute!(
                    stdout,
                    SetForegroundColor(Colors::DIM),
                    Print(format!("\r{} {}", frames[i % frames.len()], " ")),
                    ResetColor,
                )
                .ok();
                stdout.flush().ok();
                i += 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            // Clear spinner
            let mut stdout = io::stdout();
            execute!(stdout, Print("\r  \r")).ok();
            stdout.flush().ok();
        });

        Ok(SpinnerHandle {
            running,
            _thread: handle,
        })
    }

    /// Prompt the user for a yes/no permission decision.
    pub fn prompt_permission(&self, tool_name: &str, input_summary: &str) -> Result<bool> {
        let mut stdout = io::stdout();
        execute!(
            stdout,
            Print("\n"),
            SetForegroundColor(Colors::TOOL_CALL),
            Print(format!(
                "Tool '{}' requires confirmation.\n  Input: {}\n",
                tool_name, input_summary
            )),
            ResetColor,
            SetForegroundColor(Colors::USER_PROMPT),
            Print("Allow? [Y/n] "),
            ResetColor,
        )?;
        stdout.flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_lowercase();

        Ok(trimmed.is_empty() || trimmed == "y" || trimmed == "yes")
    }

    /// Print a session listing.
    pub fn print_sessions(&self, sessions: &[crate::session::SessionSummary]) -> Result<()> {
        let mut stdout = io::stdout();
        if sessions.is_empty() {
            execute!(
                stdout,
                SetForegroundColor(Colors::DIM),
                Print("No saved sessions found.\n"),
                ResetColor,
            )?;
            return Ok(());
        }

        execute!(
            stdout,
            SetForegroundColor(Colors::HEADER),
            Print("Saved Sessions:\n"),
            SetForegroundColor(Colors::DIM),
            Print(format!(
                "{:<20} {:<40} {:<10} {:<6}\n",
                "ID", "NAME", "PROVIDER", "MSGS"
            )),
            Print(format!("{}\n", "-".repeat(80))),
            ResetColor,
        )?;

        for s in sessions {
            execute!(
                stdout,
                Print(format!(
                    "{:<20} {:<40} {:<10} {:<6}\n",
                    s.id,
                    if s.name.len() > 38 {
                        format!("{}...", &s.name[..35])
                    } else {
                        s.name.clone()
                    },
                    s.provider,
                    s.message_count,
                )),
            )?;
        }

        stdout.flush()?;
        Ok(())
    }

    /// Print an error message.
    pub fn print_error(&self, msg: &str) -> Result<()> {
        let mut stdout = io::stdout();
        execute!(
            stdout,
            SetForegroundColor(Colors::ERROR),
            Print(format!("Error: {}\n", msg)),
            ResetColor,
        )?;
        stdout.flush()?;
        Ok(())
    }

    /// Print an info message.
    pub fn print_info(&self, msg: &str) -> Result<()> {
        let mut stdout = io::stdout();
        execute!(
            stdout,
            SetForegroundColor(Colors::DIM),
            Print(format!("{}\n", msg)),
            ResetColor,
        )?;
        stdout.flush()?;
        Ok(())
    }
}

/// Handle to a running spinner. Drop or call stop() to terminate it.
pub struct SpinnerHandle {
    running: Arc<AtomicBool>,
    _thread: std::thread::JoinHandle<()>,
}

impl SpinnerHandle {
    /// Stop the spinner animation.
    pub fn stop(self) {
        self.running.store(false, Ordering::SeqCst);
        // Thread will exit on next loop iteration
    }
}

impl Drop for SpinnerHandle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_creation() {
        let term = Terminal::new();
        assert!(!term.is_cancelled());
    }

    #[test]
    fn test_cancellation_token() {
        let term = Terminal::new();
        let token = term.cancellation_token();
        assert!(!token.load(Ordering::SeqCst));
        token.store(true, Ordering::SeqCst);
        assert!(term.is_cancelled());
        term.reset_cancel();
        // Note: token still holds true, but the Terminal's own flag is reset
        assert!(!term.cancelled.load(Ordering::SeqCst));
    }
}
