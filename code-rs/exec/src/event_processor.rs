use std::path::Path;

use code_core::config::Config;
use code_core::protocol::Event;

pub(crate) enum CodexStatus {
    Running,
    InitiateShutdown,
    Shutdown,
}

pub(crate) trait EventProcessor {
    /// Print summary of effective configuration and user prompt.
    fn print_config_summary(&mut self, config: &Config, prompt: &str);

    /// Handle a single event emitted by the agent.
    fn process_event(&mut self, event: Event) -> CodexStatus;

    /// Emit final user-facing output once the run has completed.
    /// Default no-op for processors that do not have a final-output concept.
    fn print_final_output(&mut self) {}

    // No exit_code method; CLI controls process exit based on core events.
}

pub(crate) fn handle_last_message(last_agent_message: Option<&str>, output_file: &Path) {
    let message = last_agent_message.unwrap_or_default();
    write_last_message_file(message, Some(output_file));
    if last_agent_message.is_none() {
        eprintln!(
            "Warning: no last agent message; wrote empty content to {}",
            output_file.display()
        );
    }
}

fn write_last_message_file(contents: &str, last_message_path: Option<&Path>) {
    if let Some(path) = last_message_path
        && let Err(e) = std::fs::write(path, contents) {
            eprintln!("Failed to write last message file {path:?}: {e}");
        }
}
