use super::super::state::ReviewRuntimeState;
use super::helpers::abort_auto_resolve_and_continue;
use super::helpers::stop_auto_resolve_and_request_shutdown;
use super::helpers::SubmitFollowupReviewParams;
use super::helpers::submit_followup_review;
use super::LoopControl;
use super::ShutdownState;
use crate::auto_review_status::AutoReviewTracker;
use crate::auto_runtime::capture_auto_resolve_snapshot;
use crate::auto_runtime::dispatch_auto_fix;
use crate::review_scope::capture_snapshot_against_base;
use crate::review_scope::head_is_ancestor_of_base;
use crate::review_scope::should_skip_followup;
use code_auto_drive_core::AutoResolvePhase;
use code_core::CodexConversation;
use code_core::config::Config;
use code_core::protocol::TaskCompleteEvent;
use code_core::review_coord::bump_snapshot_epoch_for;
use code_core::review_coord::current_snapshot_epoch_for;
use code_core::review_coord::try_acquire_lock;
use std::sync::Arc;

pub(super) async fn handle_task_complete_event(
    conversation: &Arc<CodexConversation>,
    config: &Config,
    state: &mut ReviewRuntimeState,
    auto_review_tracker: &AutoReviewTracker,
    shutdown_state: &mut ShutdownState,
    task_complete: &TaskCompleteEvent,
) -> anyhow::Result<LoopControl> {
    if let Some(state_snapshot) = state.auto_resolve_state.clone() {
        let current_epoch = current_snapshot_epoch_for(&config.cwd);
        match state_snapshot.phase {
            AutoResolvePhase::PendingFix { review } => {
                if state.auto_resolve_fix_guard.is_none() {
                    state.auto_resolve_fix_guard =
                        try_acquire_lock("auto-resolve-fix", &config.cwd).ok().flatten();
                }
                if state.auto_resolve_fix_guard.is_none() {
                    return abort_auto_resolve_and_continue(
                        conversation,
                        auto_review_tracker,
                        shutdown_state,
                        state,
                        "Auto-resolve: another review is running; skipping fix.",
                        true,
                        true,
                    )
                    .await;
                }
                if let Some(resolve_state) = state.auto_resolve_state.as_mut() {
                    resolve_state.phase = AutoResolvePhase::AwaitingFix {
                        review: review.clone(),
                    };
                    resolve_state.snapshot_epoch = Some(current_epoch);
                }
                eprintln!("[auto-review] phase: resolving (started)");
                dispatch_auto_fix(conversation, &review).await?;
            }
            AutoResolvePhase::AwaitingFix { .. } => {
                // Fix phase complete; release fix guard so follow-up can take the review lock
                state.auto_resolve_fix_guard = None;
                if let Some(resolve_state) = state.auto_resolve_state.as_mut() {
                    resolve_state.last_fix_message = task_complete.last_agent_message.clone();
                    resolve_state.phase = AutoResolvePhase::WaitingForReview;
                }
                if state.auto_resolve_followup_guard.is_none() {
                    state.auto_resolve_followup_guard =
                        try_acquire_lock("auto-resolve-followup", &config.cwd)
                            .ok()
                            .flatten();
                }
                if state.auto_resolve_followup_guard.is_none() {
                    return abort_auto_resolve_and_continue(
                        conversation,
                        auto_review_tracker,
                        shutdown_state,
                        state,
                        "Auto-resolve: another review is running; stopping follow-up review.",
                        true,
                        true,
                    )
                    .await;
                }
                if let Some(base) = state.auto_resolve_base_snapshot.as_ref() {
                    let base_id = base.id().to_string();
                    if !head_is_ancestor_of_base(&config.cwd, base.id()) {
                        return abort_auto_resolve_and_continue(
                            conversation,
                            auto_review_tracker,
                            shutdown_state,
                            state,
                            "Auto-resolve: base snapshot no longer matches current HEAD; stopping to avoid stale review.",
                            true,
                            true,
                        )
                        .await;
                    }
                    // stale epoch check
                    if let Some(resolve_state) = state.auto_resolve_state.as_ref()
                        && let Some(baseline) = resolve_state.snapshot_epoch
                        && current_epoch > baseline
                    {
                        return abort_auto_resolve_and_continue(
                            conversation,
                            auto_review_tracker,
                            shutdown_state,
                            state,
                            "Auto-resolve: snapshot epoch advanced; aborting follow-up review.",
                            true,
                            true,
                        )
                        .await;
                    }
                    match capture_snapshot_against_base(
                        &config.cwd,
                        base,
                        "auto-resolve follow-up snapshot",
                        capture_auto_resolve_snapshot,
                        bump_snapshot_epoch_for,
                    ) {
                        Some((snap, diff_paths)) => {
                            if should_skip_followup(
                                state_snapshot.last_reviewed_commit.as_deref(),
                                &snap,
                            ) {
                                return abort_auto_resolve_and_continue(
                                    conversation,
                                    auto_review_tracker,
                                    shutdown_state,
                                    state,
                                    "Auto-resolve: follow-up snapshot is identical to last reviewed commit; ending loop to avoid duplicate review.",
                                    true,
                                    true,
                                )
                                .await;
                            }
                            submit_followup_review(
                                conversation,
                                config,
                                state,
                                SubmitFollowupReviewParams {
                                    state_snapshot: &state_snapshot,
                                    snap: &snap,
                                    diff_paths: diff_paths.as_slice(),
                                    base_id: &base_id,
                                    announce_phase: true,
                                    update_snapshot_epoch: true,
                                },
                            )
                            .await?;
                        }
                        None => {
                            stop_auto_resolve_and_request_shutdown(
                                conversation,
                                auto_review_tracker,
                                shutdown_state,
                                state,
                                "Auto-resolve: failed to capture follow-up snapshot or no diff detected; stopping auto-resolve.",
                                true,
                                true,
                            )
                            .await?;
                        }
                    }
                }
            }
            AutoResolvePhase::AwaitingJudge { .. } => {
                // Legacy branch: fall back to requesting a follow-up review.
                if let Some(resolve_state) = state.auto_resolve_state.as_mut() {
                    resolve_state.last_fix_message = task_complete.last_agent_message.clone();
                    resolve_state.phase = AutoResolvePhase::WaitingForReview;
                }
                if let Some(base) = state.auto_resolve_base_snapshot.as_ref() {
                    let base_id = base.id().to_string();
                    if !head_is_ancestor_of_base(&config.cwd, base.id()) {
                        return abort_auto_resolve_and_continue(
                            conversation,
                            auto_review_tracker,
                            shutdown_state,
                            state,
                            "Auto-resolve: base snapshot no longer matches current HEAD; stopping to avoid stale review.",
                            false,
                            false,
                        )
                        .await;
                    }
                    match capture_snapshot_against_base(
                        &config.cwd,
                        base,
                        "auto-resolve follow-up snapshot",
                        capture_auto_resolve_snapshot,
                        bump_snapshot_epoch_for,
                    ) {
                        Some((snap, diff_paths)) => {
                            if should_skip_followup(
                                state_snapshot.last_reviewed_commit.as_deref(),
                                &snap,
                            ) {
                                return abort_auto_resolve_and_continue(
                                    conversation,
                                    auto_review_tracker,
                                    shutdown_state,
                                    state,
                                    "Auto-resolve: follow-up snapshot is identical to last reviewed commit; ending loop to avoid duplicate review.",
                                    false,
                                    false,
                                )
                                .await;
                            }
                            submit_followup_review(
                                conversation,
                                config,
                                state,
                                SubmitFollowupReviewParams {
                                    state_snapshot: &state_snapshot,
                                    snap: &snap,
                                    diff_paths: diff_paths.as_slice(),
                                    base_id: &base_id,
                                    announce_phase: false,
                                    update_snapshot_epoch: false,
                                },
                            )
                            .await?;
                        }
                        None => {
                            eprintln!("Auto-resolve: failed to capture follow-up snapshot or no diff detected; stopping auto-resolve.");
                            state.auto_resolve_state = None;
                            state.auto_resolve_base_snapshot = None;
                            state.auto_resolve_followup_guard = None;
                            state.auto_resolve_fix_guard = None;
                        }
                    }
                }
            }
            AutoResolvePhase::WaitingForReview => {
                // Task complete from a review; handled in ExitedReviewMode.
            }
        }
    }

    if state.auto_resolve_state.is_none() && !shutdown_state.is_sent() {
        state.auto_resolve_base_snapshot = None;
        shutdown_state.request(conversation, auto_review_tracker).await?;
    }

    Ok(LoopControl::ProcessEvent)
}
