use super::state::ReviewRuntimeState;
use crate::auto_runtime::capture_auto_resolve_snapshot;
use crate::review_scope::apply_commit_scope_to_review_request;
use crate::review_scope::capture_snapshot_against_base;
use code_core::CodexConversation;
use code_core::config::Config;
use code_core::protocol::InputItem;
use code_core::protocol::Op;
use code_core::protocol::ReviewRequest;
use code_core::review_coord::bump_snapshot_epoch_for;
use code_core::review_coord::clear_stale_lock_if_dead;
use code_core::review_coord::current_snapshot_epoch_for;
use code_core::review_coord::try_acquire_lock;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

pub(super) async fn submit_initial_turn(
    conversation: &Arc<CodexConversation>,
    config: &Config,
    review_request: &Option<ReviewRequest>,
    prompt_to_send: String,
    images: Vec<PathBuf>,
    is_auto_review: bool,
    state: &mut ReviewRuntimeState,
) -> anyhow::Result<bool> {
    // Clear stale review lock in case a prior process crashed.
    let _ = clear_stale_lock_if_dead(Some(&config.cwd));

    let skip_review_lock = std::env::var("CODE_REVIEW_LOCK_LEASE")
        .map(|value| value == "1")
        .unwrap_or(false);

    if let Some(mut request) = review_request.clone() {
        // Cross-process review coordination.
        if !skip_review_lock {
            match try_acquire_lock("review", &config.cwd) {
                Ok(Some(guard)) => state.review_guard = Some(guard),
                Ok(None) => {
                    eprintln!("Another review is already running; skipping this /review.");
                    return Ok(false);
                }
                Err(err) => {
                    eprintln!("Warning: could not acquire review lock: {err}");
                }
            }
        }

        if state.auto_resolve_state.is_some() {
            if state.auto_resolve_base_snapshot.is_none() {
                state.auto_resolve_base_snapshot = capture_auto_resolve_snapshot(
                    &config.cwd,
                    None,
                    "auto-resolve base snapshot",
                );
                if let Some(resolve_state) = state.auto_resolve_state.as_mut() {
                    resolve_state.snapshot_epoch = Some(current_snapshot_epoch_for(&config.cwd));
                }
            }

            if let Some(base) = state.auto_resolve_base_snapshot.as_ref()
                && let Some((snap, diff_paths)) = capture_snapshot_against_base(
                    &config.cwd,
                    base,
                    "auto-resolve working snapshot",
                    capture_auto_resolve_snapshot,
                    bump_snapshot_epoch_for,
                )
            {
                request = apply_commit_scope_to_review_request(
                    request,
                    snap.id(),
                    base.id(),
                    Some(diff_paths.as_slice()),
                );
                if let Some(resolve_state) = state.auto_resolve_state.as_mut() {
                    resolve_state.last_reviewed_commit = Some(snap.id().to_string());
                }
            }
        }

        // Capture baseline epoch after any snapshot creation so we don't trip on our own bumps.
        state.last_review_epoch = Some(current_snapshot_epoch_for(&config.cwd));

        let event_id = conversation.submit(Op::Review { review_request: request }).await?;
        if is_auto_review {
            eprintln!("[auto-review] phase: reviewing (started)");
        }
        info!("Sent /review with event ID: {event_id}");
        return Ok(true);
    }

    let mut items: Vec<InputItem> = Vec::new();
    items.push(InputItem::Text {
        text: prompt_to_send,
    });
    items.extend(images.into_iter().map(|path| InputItem::LocalImage { path }));

    // Fallback for older core protocol: send only user input items.
    let event_id = conversation
        .submit(Op::UserInput {
            items,
            final_output_json_schema: None,
        })
        .await?;
    info!("Sent prompt with event ID: {event_id}");
    Ok(true)
}
