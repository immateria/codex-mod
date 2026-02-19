use code_auto_drive_core::AutoResolveState;
use code_core::protocol::ReviewOutputEvent;
use code_core::protocol::ReviewSnapshotInfo;
use code_core::review_coord::ReviewGuard;
use code_git_tooling::GhostCommit;

pub(super) struct ReviewRuntimeState {
    pub(super) auto_resolve_state: Option<AutoResolveState>,
    pub(super) review_outputs: Vec<ReviewOutputEvent>,
    pub(super) final_review_snapshot: Option<ReviewSnapshotInfo>,
    pub(super) review_runs: u32,
    pub(super) last_review_epoch: Option<u64>,
    pub(super) auto_resolve_fix_guard: Option<ReviewGuard>,
    pub(super) auto_resolve_followup_guard: Option<ReviewGuard>,
    pub(super) auto_resolve_base_snapshot: Option<GhostCommit>,
    pub(super) review_guard: Option<ReviewGuard>,
}

impl ReviewRuntimeState {
    pub(super) fn new(auto_resolve_state: Option<AutoResolveState>) -> Self {
        Self {
            auto_resolve_state,
            review_outputs: Vec::new(),
            final_review_snapshot: None,
            review_runs: 0,
            last_review_epoch: None,
            auto_resolve_fix_guard: None,
            auto_resolve_followup_guard: None,
            auto_resolve_base_snapshot: None,
            review_guard: None,
        }
    }
}
