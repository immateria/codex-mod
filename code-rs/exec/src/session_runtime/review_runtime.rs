use super::event_bridge::start_event_stream;
use super::review_event_loop::ReviewEventLoopParams;
use super::review_event_loop::run_review_event_loop;
use super::review_submission::submit_initial_turn;
use super::state::ReviewRuntimeState;
use super::SessionRuntimeOutcome;
use super::SessionRuntimeParams;

pub(crate) async fn run_session_runtime(
    params: SessionRuntimeParams<'_>,
) -> anyhow::Result<SessionRuntimeOutcome> {
    let SessionRuntimeParams {
        conversation,
        config,
        event_processor,
        review_request,
        prompt_to_send,
        images,
        run_deadline,
        max_seconds,
        auto_resolve_state,
        max_auto_resolve_attempts: _max_auto_resolve_attempts,
        is_auto_review,
    } = params;

    let mut state = ReviewRuntimeState::new(auto_resolve_state);
    let mut rx = start_event_stream(conversation.clone());

    let submitted = submit_initial_turn(
        &conversation,
        config,
        &review_request,
        prompt_to_send,
        images,
        is_auto_review,
        &mut state,
    )
    .await?;
    if !submitted {
        return Ok(SessionRuntimeOutcome {
            review_outputs: state.review_outputs,
            final_review_snapshot: state.final_review_snapshot,
            review_runs: state.review_runs,
            error_seen: false,
        });
    }

    let error_seen = run_review_event_loop(ReviewEventLoopParams {
        conversation: &conversation,
        config,
        event_processor,
        review_request: &review_request,
        run_deadline,
        max_seconds,
        rx: &mut rx,
        state: &mut state,
    })
    .await?;

    Ok(SessionRuntimeOutcome {
        review_outputs: state.review_outputs,
        final_review_snapshot: state.final_review_snapshot,
        review_runs: state.review_runs,
        error_seen,
    })
}
