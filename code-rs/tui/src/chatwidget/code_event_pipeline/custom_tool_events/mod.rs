use super::*;
use code_core::protocol::OrderMeta;

mod begin;
mod end;

struct CustomToolEndContext {
    call_id: String,
    tool_name: String,
    duration: Duration,
    success: bool,
    content: String,
    params_string: Option<String>,
    order_key: OrderKey,
    resolved_idx: Option<usize>,
    image_view_path: Option<std::path::PathBuf>,
}

struct WaitEndState {
    trimmed: String,
    wait_missing_job: bool,
    wait_interrupted: bool,
    wait_still_pending: bool,
    exec_running: bool,
    exec_completed: bool,
    note_lines: Vec<(String, bool)>,
    history_id: Option<HistoryId>,
    wait_total: Option<Duration>,
    wait_notes_snapshot: Vec<(String, bool)>,
}

const WAIT_CANCELLED_BY_USER: &str = "Cancelled by user.";

impl ChatWidget<'_> {
    fn custom_tool_order_key(&mut self, order: Option<&OrderMeta>, phase: &str) -> OrderKey {
        match order {
            Some(om) => self.provider_order_key_from_order_meta(om),
            None => {
                tracing::warn!("missing OrderMeta on {phase}; using synthetic key");
                self.next_internal_key()
            }
        }
    }
}
