mod event_bridge;
mod review_event_loop;
mod review_submission;
mod review_runtime;
mod state;

use code_auto_drive_core::AutoResolveState;
use code_core::CodexConversation;
use code_core::config::Config;
use code_core::protocol::ReviewOutputEvent;
use code_core::protocol::ReviewRequest;
use code_core::protocol::ReviewSnapshotInfo;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::Instant;

pub(crate) struct SessionRuntimeParams<'a> {
    pub(crate) conversation: Arc<CodexConversation>,
    pub(crate) config: &'a Config,
    pub(crate) event_processor: &'a mut dyn crate::event_processor::EventProcessor,
    pub(crate) review_request: Option<ReviewRequest>,
    pub(crate) prompt_to_send: String,
    pub(crate) images: Vec<PathBuf>,
    pub(crate) run_deadline: Option<Instant>,
    pub(crate) max_seconds: Option<u64>,
    pub(crate) auto_resolve_state: Option<AutoResolveState>,
    pub(crate) max_auto_resolve_attempts: u32,
    pub(crate) is_auto_review: bool,
}

pub(crate) struct SessionRuntimeOutcome {
    pub(crate) review_outputs: Vec<ReviewOutputEvent>,
    pub(crate) final_review_snapshot: Option<ReviewSnapshotInfo>,
    pub(crate) review_runs: u32,
    pub(crate) error_seen: bool,
}

pub(crate) use review_runtime::run_session_runtime;
