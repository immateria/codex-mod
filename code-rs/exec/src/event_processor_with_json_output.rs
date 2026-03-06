use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use code_core::config::Config;
use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::protocol::TaskCompleteEvent;
use serde_json::json;

use crate::event_processor::CodexStatus;
use crate::event_processor::EventProcessor;
use crate::event_processor::handle_last_message;
use code_common::create_config_summary_entries;

pub(crate) struct EventProcessorWithJsonOutput {
    last_message_path: Option<PathBuf>,
    had_error: bool,
}

impl EventProcessorWithJsonOutput {
    pub fn new(last_message_path: Option<PathBuf>) -> Self {
        Self { last_message_path, had_error: false }
    }
}

fn write_stdout_line(args: std::fmt::Arguments<'_>) {
    let mut stdout = std::io::stdout();
    if let Err(err) = writeln!(stdout, "{args}") {
        panic!("failed to write JSON output: {err}");
    }
}

impl EventProcessor for EventProcessorWithJsonOutput {
    fn print_config_summary(&mut self, config: &Config, prompt: &str) {
        let entries = create_config_summary_entries(config)
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect::<HashMap<String, String>>();
        let config_json = match serde_json::to_string(&entries) {
            Ok(config_json) => config_json,
            Err(err) => panic!("Failed to serialize config summary to JSON: {err}"),
        };
        write_stdout_line(format_args!("{config_json}"));

        let prompt_json = json!({
            "prompt": prompt,
        });
        write_stdout_line(format_args!("{prompt_json}"));
    }

    fn process_event(&mut self, event: Event) -> CodexStatus {
        match event.msg {
            EventMsg::Error(_) => { self.had_error = true; CodexStatus::Running }
            EventMsg::AgentMessageDelta(_) | EventMsg::AgentReasoningDelta(_) => {
                // Suppress streaming events in JSON mode.
                CodexStatus::Running
            }
            EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message }) => {
                if let Some(output_file) = self.last_message_path.as_deref() {
                    handle_last_message(last_agent_message.as_deref(), output_file);
                }
                CodexStatus::InitiateShutdown
            }
            EventMsg::ShutdownComplete => CodexStatus::Shutdown,
            _ => {
                if let Ok(line) = serde_json::to_string(&event) {
                    write_stdout_line(format_args!("{line}"));
                }
                CodexStatus::Running
            }
        }
    }

    // exit_code handled by CLI; suppress unused warnings by omitting method.
}
