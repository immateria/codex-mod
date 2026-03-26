use std::future::Future;

use code_hooks::PreToolUseOutcome;
use code_hooks::PreToolUseRequest;
use code_hooks::SessionStartOutcome;
use code_hooks::SessionStartRequest;
use code_hooks::UserPromptSubmitOutcome;
use code_hooks::UserPromptSubmitRequest;
use code_protocol::ThreadId;
use code_protocol::models::DeveloperInstructions;
use code_protocol::models::ResponseItem;
use code_protocol::protocol::HookCompletedEvent;
use code_protocol::protocol::HookRunSummary;

use crate::protocol::AskForApproval;
use crate::protocol::EventMsg;
use crate::protocol::WarningEvent;

use super::Session;

pub(super) struct HookRuntimeOutcome {
    pub(super) should_stop: bool,
    pub(super) additional_contexts: Vec<String>,
}

struct ContextInjectingHookOutcome {
    hook_events: Vec<HookCompletedEvent>,
    outcome: HookRuntimeOutcome,
}

impl From<SessionStartOutcome> for ContextInjectingHookOutcome {
    fn from(value: SessionStartOutcome) -> Self {
        let SessionStartOutcome {
            hook_events,
            should_stop,
            stop_reason: _,
            additional_contexts,
        } = value;
        Self {
            hook_events,
            outcome: HookRuntimeOutcome {
                should_stop,
                additional_contexts,
            },
        }
    }
}

impl From<UserPromptSubmitOutcome> for ContextInjectingHookOutcome {
    fn from(value: UserPromptSubmitOutcome) -> Self {
        let UserPromptSubmitOutcome {
            hook_events,
            should_stop,
            stop_reason: _,
            additional_contexts,
        } = value;
        Self {
            hook_events,
            outcome: HookRuntimeOutcome {
                should_stop,
                additional_contexts,
            },
        }
    }
}

pub(super) fn hook_permission_mode(approval_policy: AskForApproval) -> String {
    match approval_policy {
        AskForApproval::Never => "bypassPermissions",
        AskForApproval::UnlessTrusted
        | AskForApproval::OnFailure
        | AskForApproval::OnRequest
        | AskForApproval::Reject(_) => "default",
    }
    .to_string()
}

pub(super) fn thread_id_from_session_uuid(sess: &Session) -> ThreadId {
    let session_uuid = sess.id.to_string();
    match ThreadId::try_from(session_uuid) {
        Ok(id) => id,
        Err(err) => {
            tracing::warn!("failed to convert session uuid to ThreadId: {err}");
            ThreadId::new()
        }
    }
}

pub(super) async fn run_session_start_hooks(
    sess: &Session,
    sub_id: &str,
    request: &SessionStartRequest,
    turn_id: Option<String>,
) -> HookRuntimeOutcome {
    let preview_runs = sess.hooks_json().preview_session_start(request);
    run_context_injecting_hook(
        sess,
        sub_id,
        turn_id.clone(),
        preview_runs,
        sess.hooks_json()
            .run_session_start(request.clone(), turn_id),
    )
    .await
}

pub(super) async fn run_user_prompt_submit_hooks(
    sess: &Session,
    sub_id: &str,
    request: &UserPromptSubmitRequest,
) -> HookRuntimeOutcome {
    let preview_runs = sess.hooks_json().preview_user_prompt_submit(request);
    run_context_injecting_hook(
        sess,
        sub_id,
        Some(request.turn_id.clone()),
        preview_runs,
        sess.hooks_json().run_user_prompt_submit(request.clone()),
    )
    .await
}

pub(super) async fn run_pre_tool_use_hooks(
    sess: &Session,
    sub_id: &str,
    request: &PreToolUseRequest,
) -> Option<String> {
    let preview_runs = sess.hooks_json().preview_pre_tool_use(request);
    emit_hook_started_events(sess, sub_id, Some(request.turn_id.clone()), preview_runs).await;

    let PreToolUseOutcome {
        hook_events,
        should_block,
        block_reason,
    } = sess.hooks_json().run_pre_tool_use(request.clone()).await;
    emit_hook_completed_events(sess, sub_id, hook_events).await;

    if should_block {
        Some(block_reason.unwrap_or_default())
    } else {
        None
    }
}

async fn run_context_injecting_hook<Fut, Outcome>(
    sess: &Session,
    sub_id: &str,
    turn_id: Option<String>,
    preview_runs: Vec<HookRunSummary>,
    outcome_future: Fut,
) -> HookRuntimeOutcome
where
    Fut: Future<Output = Outcome>,
    Outcome: Into<ContextInjectingHookOutcome>,
{
    emit_hook_started_events(sess, sub_id, turn_id, preview_runs).await;
    let outcome = outcome_future.await.into();
    emit_hook_completed_events(sess, sub_id, outcome.hook_events).await;
    outcome.outcome
}

pub(super) async fn record_additional_contexts(
    sess: &Session,
    additional_contexts: Vec<String>,
) {
    let developer_messages = additional_context_messages(additional_contexts);
    if developer_messages.is_empty() {
        return;
    }

    sess.record_conversation_items(developer_messages.as_slice()).await;
}

pub(super) fn additional_context_messages(additional_contexts: Vec<String>) -> Vec<ResponseItem> {
    additional_contexts
        .into_iter()
        .map(DeveloperInstructions::new)
        .map(ResponseItem::from)
        .collect()
}

async fn emit_hook_started_events(
    sess: &Session,
    sub_id: &str,
    turn_id: Option<String>,
    preview_runs: Vec<HookRunSummary>,
) {
    for run in preview_runs {
        let event = sess.make_event(
            sub_id,
            EventMsg::HookStarted(code_protocol::protocol::HookStartedEvent {
                turn_id: turn_id.clone(),
                run,
            }),
        );
        let _ = sess.tx_event.send(event).await;
    }
}

async fn emit_hook_completed_events(
    sess: &Session,
    sub_id: &str,
    completed_events: Vec<HookCompletedEvent>,
) {
    for completed in completed_events {
        let event = sess.make_event(sub_id, EventMsg::HookCompleted(completed));
        let _ = sess.tx_event.send(event).await;
    }
}

pub(super) async fn emit_hook_blocked_warning(sess: &Session, sub_id: &str) {
    let event = sess.make_event(
        sub_id,
        EventMsg::Warning(WarningEvent {
            message: "input blocked by hooks.json lifecycle hook".to_string(),
        }),
    );
    let _ = sess.tx_event.send(event).await;
}
