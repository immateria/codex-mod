use crate::text_formatting::string_display_width as string_width;
use crate::text_formatting::wrap_text;

fn detail_display_text(detail: &AgentDetail) -> String {
    match detail {
        AgentDetail::Progress(text)
        | AgentDetail::Result(text)
        | AgentDetail::Error(text)
        | AgentDetail::Info(text) => text.clone(),
    }
}

