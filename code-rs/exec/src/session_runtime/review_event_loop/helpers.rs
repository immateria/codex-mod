use super::super::state::ReviewRuntimeState;
use super::LoopControl;
use super::ShutdownState;
use crate::auto_review_status::AutoReviewTracker;
use crate::review_scope::build_followup_review_request;
use code_auto_drive_core::AutoResolveState;
use code_core::CodexConversation;
use code_core::config::Config;
use code_core::protocol::Op;
use code_core::review_coord::current_snapshot_epoch_for;
use code_git_tooling::GhostCommit;
use std::sync::Arc;

pub(super) struct SubmitFollowupReviewParams<'a> {
    pub state_snapshot: &'a AutoResolveState,
    pub snap: &'a GhostCommit,
    pub diff_paths: &'a [String],
    pub base_id: &'a str,
    pub announce_phase: bool,
    pub update_snapshot_epoch: bool,
}

fn clear_auto_resolve_state(
    state: &mut ReviewRuntimeState,
    clear_followup_guard: bool,
    clear_fix_guard: bool,
) {
    state.auto_resolve_state = None;
    state.auto_resolve_base_snapshot = None;
    if clear_followup_guard {
        state.auto_resolve_followup_guard = None;
    }
    if clear_fix_guard {
        state.auto_resolve_fix_guard = None;
    }
}

pub(super) async fn abort_auto_resolve_and_continue(
    conversation: &Arc<CodexConversation>,
    auto_review_tracker: &AutoReviewTracker,
    shutdown_state: &mut ShutdownState,
    state: &mut ReviewRuntimeState,
    message: &str,
    clear_followup_guard: bool,
    clear_fix_guard: bool,
) -> anyhow::Result<LoopControl> {
    eprintln!("{message}");
    clear_auto_resolve_state(state, clear_followup_guard, clear_fix_guard);
    shutdown_state.request(conversation, auto_review_tracker).await?;
    Ok(LoopControl::Continue)
}

pub(super) async fn stop_auto_resolve_and_request_shutdown(
    conversation: &Arc<CodexConversation>,
    auto_review_tracker: &AutoReviewTracker,
    shutdown_state: &mut ShutdownState,
    state: &mut ReviewRuntimeState,
    message: &str,
    clear_followup_guard: bool,
    clear_fix_guard: bool,
) -> anyhow::Result<()> {
    eprintln!("{message}");
    clear_auto_resolve_state(state, clear_followup_guard, clear_fix_guard);
    shutdown_state.request(conversation, auto_review_tracker).await
}

pub(super) async fn submit_followup_review(
    conversation: &Arc<CodexConversation>,
    config: &Config,
    state: &mut ReviewRuntimeState,
    params: SubmitFollowupReviewParams<'_>,
) -> anyhow::Result<()> {
    let SubmitFollowupReviewParams {
        state_snapshot,
        snap,
        diff_paths,
        base_id,
        announce_phase,
        update_snapshot_epoch,
    } = params;
    if let Some(resolve_state) = state.auto_resolve_state.as_mut() {
        resolve_state.last_reviewed_commit = Some(snap.id().to_string());
        if update_snapshot_epoch {
            resolve_state.snapshot_epoch = Some(current_snapshot_epoch_for(&config.cwd));
        }
    }

    let followup_request = build_followup_review_request(
        state_snapshot,
        &config.cwd,
        Some(snap),
        Some(diff_paths),
        Some(base_id),
    )
    .await;
    state.last_review_epoch = Some(current_snapshot_epoch_for(&config.cwd));
    if announce_phase {
        eprintln!("[auto-review] phase: reviewing (started)");
    }
    let _ = conversation
        .submit(Op::Review {
            review_request: followup_request,
        })
        .await?;
    Ok(())
}
