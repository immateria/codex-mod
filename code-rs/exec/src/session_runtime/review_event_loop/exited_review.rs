use super::super::state::ReviewRuntimeState;
use super::LoopControl;
use crate::review_output::review_summary_line;
use code_auto_drive_core::AutoResolvePhase;
use code_core::config::Config;
use code_core::git_info::current_branch_name;
use code_core::protocol::ExitedReviewModeEvent;
use code_core::protocol::ReviewRequest;
use code_core::review_coord::current_snapshot_epoch_for;

pub(super) async fn handle_exited_review_mode_event(
    config: &Config,
    review_request: &Option<ReviewRequest>,
    state: &mut ReviewRuntimeState,
    event: &ExitedReviewModeEvent,
) -> anyhow::Result<LoopControl> {
    // Any review that just finished should release follow-up locks.
    state.auto_resolve_followup_guard = None;
    // Release the global review lock as soon as the review finishes so follow-up
    // auto-resolve steps can acquire it.
    state.review_guard = None;
    state.review_runs = state.review_runs.saturating_add(1);
    if let Some(output) = event.review_output.as_ref() {
        state.review_outputs.push(output.clone());
    }
    if let Some(snapshot) = event.snapshot.as_ref() {
        state.final_review_snapshot = Some(snapshot.clone());
        // detect stale snapshot epoch
        if let Some(start_epoch) = state.last_review_epoch {
            let current_epoch = current_snapshot_epoch_for(&config.cwd);
            if current_epoch != start_epoch {
                eprintln!(
                    "Snapshot epoch changed during review; aborting auto-resolve and requiring restart."
                );
                state.auto_resolve_state = None;
                state.auto_resolve_base_snapshot = None;
                return Ok(LoopControl::Continue);
            }
        }
    }

    // Surface review result to the parent CLI via stderr; avoid injecting
    // synthetic user turns into the /review sub-agent conversation.
    if review_request.is_some() {
        let findings_count = event
            .review_output
            .as_ref()
            .map(|output| output.findings.len())
            .unwrap_or(0);
        let branch = current_branch_name(&config.cwd)
            .await
            .unwrap_or_else(|| "unknown".to_string());
        let worktree = config.cwd.clone();
        let summary = event.review_output.as_ref().and_then(review_summary_line);

        if findings_count == 0 {
            eprintln!(
                "[developer] Auto-review completed on branch '{branch}' (worktree: {}). No issues reported.",
                worktree.display()
            );
        } else {
            match summary {
                Some(ref text) if !text.is_empty() => eprintln!(
                    "[developer] Auto-review found {findings_count} issue(s) on branch '{branch}'. Summary: {text}. Worktree: {}. Merge this worktree/branch to apply fixes.",
                    worktree.display()
                ),
                _ => eprintln!(
                    "[developer] Auto-review found {findings_count} issue(s) on branch '{branch}'. Worktree: {}. Merge this worktree/branch to apply fixes.",
                    worktree.display()
                ),
            }
        }
    }

    if let Some(resolve_state) = state.auto_resolve_state.as_mut() {
        resolve_state.attempt = resolve_state.attempt.saturating_add(1);
        resolve_state.last_review = event.review_output.clone();
        resolve_state.last_fix_message = None;

        match event.review_output.as_ref() {
            Some(output) if output.findings.is_empty() => {
                eprintln!("Auto-resolve: review reported no actionable findings. Exiting.");
                state.auto_resolve_state = None;
                state.auto_resolve_base_snapshot = None;
            }
            Some(_)
                if resolve_state.max_attempts > 0
                    && resolve_state.attempt > resolve_state.max_attempts =>
            {
                let limit = resolve_state.max_attempts;
                let msg = if limit == 1 {
                    "Auto-resolve: reached the review attempt limit (1 allowed re-review). Handing control back.".to_string()
                } else {
                    format!(
                        "Auto-resolve: reached the review attempt limit ({limit} allowed re-reviews). Handing control back."
                    )
                };
                eprintln!("{msg}");
                state.auto_resolve_state = None;
                state.auto_resolve_base_snapshot = None;
            }
            Some(output) => {
                resolve_state.phase = AutoResolvePhase::PendingFix {
                    review: output.clone(),
                };
            }
            None => {
                eprintln!("Auto-resolve: review ended without findings. Please inspect manually.");
                state.auto_resolve_state = None;
                state.auto_resolve_base_snapshot = None;
            }
        }
    }

    Ok(LoopControl::ProcessEvent)
}
