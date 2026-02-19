use crate::auto_review_status::AutoReviewTracker;
use crate::auto_review_status::emit_auto_review_completion;
use crate::event_processor::CodexStatus;
use crate::event_processor::EventProcessor;
use crate::review_output::build_fix_prompt;
use code_auto_drive_core::AutoTurnAgentsAction;
use code_auto_drive_core::AutoTurnAgentsTiming;
use code_auto_drive_core::AutoTurnCliAction;
use code_core::CodexConversation;
use code_core::protocol::EventMsg;
use code_core::protocol::InputItem;
use code_core::protocol::Op;
use code_core::protocol::ReviewOutputEvent;
use code_core::protocol::TaskCompleteEvent;
use code_core::review_coord::bump_snapshot_epoch_for;
use code_git_tooling::CreateGhostCommitOptions;
use code_git_tooling::GhostCommit;
use code_git_tooling::create_ghost_commit;
use std::path::Path;
use std::sync::Arc;
use tokio::time::Duration;
use tokio::time::Instant;

/// How long exec waits after task completion before sending Shutdown when Auto Review
/// may be about to start. Guarded so sub-agents are not delayed.
pub(crate) const AUTO_REVIEW_SHUTDOWN_GRACE_MS: u64 = 1_500;

pub(crate) fn append_timeboxed_auto_drive_goal(goal: &str) -> String {
    let trimmed_goal = goal.trim();
    if trimmed_goal.is_empty() {
        return code_core::timeboxed_exec_guidance::AUTO_EXEC_TIMEBOXED_GOAL_SUFFIX.to_string();
    }

    format!(
        "{trimmed_goal}\n\n{}",
        code_core::timeboxed_exec_guidance::AUTO_EXEC_TIMEBOXED_GOAL_SUFFIX
    )
}

pub(crate) fn merge_developer_message(existing: Option<String>, extra: &str) -> Option<String> {
    let extra_trimmed = extra.trim();
    if extra_trimmed.is_empty() {
        return existing;
    }

    match existing {
        Some(mut message) => {
            if !message.trim().is_empty() {
                message.push_str("\n\n");
            }
            message.push_str(extra_trimmed);
            Some(message)
        }
        None => Some(extra_trimmed.to_string()),
    }
}

pub(crate) async fn send_shutdown_if_ready(
    conversation: &Arc<CodexConversation>,
    auto_review_tracker: &AutoReviewTracker,
    shutdown_sent: &mut bool,
) -> anyhow::Result<bool> {
    if *shutdown_sent || auto_review_tracker.is_running() {
        return Ok(false);
    }

    conversation.submit(Op::Shutdown).await?;
    *shutdown_sent = true;
    Ok(true)
}

pub(crate) async fn request_shutdown(
    conversation: &Arc<CodexConversation>,
    auto_review_tracker: &AutoReviewTracker,
    shutdown_pending: &mut bool,
    shutdown_sent: &mut bool,
    shutdown_deadline: &mut Option<Instant>,
    auto_review_grace_enabled: bool,
) -> anyhow::Result<()> {
    if *shutdown_sent {
        *shutdown_pending = false;
        *shutdown_deadline = None;
        return Ok(());
    }

    let now = Instant::now();
    let (attempt_send, new_pending, new_deadline) = shutdown_state_after_request(
        auto_review_tracker.is_running(),
        *shutdown_pending,
        *shutdown_deadline,
        now,
        auto_review_grace_enabled,
    );
    *shutdown_pending = new_pending;
    *shutdown_deadline = new_deadline;

    if !attempt_send {
        return Ok(());
    }

    if send_shutdown_if_ready(conversation, auto_review_tracker, shutdown_sent).await? {
        *shutdown_pending = false;
        *shutdown_deadline = None;
    } else {
        *shutdown_pending = true;
        *shutdown_deadline = None;
    }

    Ok(())
}

pub(crate) fn shutdown_state_after_request(
    auto_review_running: bool,
    shutdown_pending: bool,
    shutdown_deadline: Option<Instant>,
    now: Instant,
    grace_enabled: bool,
) -> (bool, bool, Option<Instant>) {
    if auto_review_running {
        return (false, true, None);
    }

    if !grace_enabled {
        return (true, true, None);
    }

    if !shutdown_pending && shutdown_deadline.is_none() {
        let deadline = now + Duration::from_millis(AUTO_REVIEW_SHUTDOWN_GRACE_MS);
        return (false, true, Some(deadline));
    }

    if let Some(deadline) = shutdown_deadline
        && deadline > now {
            return (false, true, Some(deadline));
        }

    (true, true, None)
}

pub(crate) fn build_auto_prompt(
    cli_action: &AutoTurnCliAction,
    agents: &[AutoTurnAgentsAction],
    agents_timing: Option<AutoTurnAgentsTiming>,
) -> String {
    let mut sections: Vec<String> = Vec::new();

    if let Some(ctx) = cli_action
        .context
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        sections.push(ctx.to_string());
    }

    let cli_prompt = cli_action.prompt.trim();
    if !cli_prompt.is_empty() {
        sections.push(cli_prompt.to_string());
    }

    if !agents.is_empty() {
        let mut lines: Vec<String> = Vec::new();
        lines.push("<agents>".to_string());
        lines.push("Please use agents to help you complete this task.".to_string());

        for action in agents {
            let prompt = action
                .prompt
                .trim()
                .replace('\n', " ")
                .replace('"', "\\\"");
            let write_text = if action.write { "write: true" } else { "write: false" };

            lines.push(String::new());
            lines.push(format!("prompt: \"{prompt}\" ({write_text})"));

            if let Some(ctx) = action
                .context
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                lines.push(format!("context: {}", ctx.replace('\n', " ")));
            }

            if let Some(models) = action.models.as_ref().filter(|list| !list.is_empty()) {
                lines.push(format!("models: {}", models.join(", ")));
            }
        }

        let timing_line = match agents_timing {
            Some(AutoTurnAgentsTiming::Parallel) =>
                "Timing: parallel — continue the CLI prompt while agents run; call agent.wait when ready to merge results.".to_string(),
            Some(AutoTurnAgentsTiming::Blocking) =>
                "Timing: blocking — launch agents first, wait with agent.wait, then continue the CLI prompt.".to_string(),
            None =>
                "Timing: blocking — wait for agent.wait before continuing the CLI prompt.".to_string(),
        };
        lines.push(String::new());
        lines.push(timing_line);
        lines.push("</agents>".to_string());

        sections.push(lines.join("\n"));
    }

    sections.join("\n\n")
}

pub(crate) async fn dispatch_auto_fix(
    conversation: &Arc<CodexConversation>,
    review: &ReviewOutputEvent,
) -> anyhow::Result<()> {
    let fix_prompt = build_fix_prompt(review);
    let items: Vec<InputItem> = vec![InputItem::Text { text: fix_prompt }];
    let _ = conversation
        .submit(Op::UserInput {
            items,
            final_output_json_schema: None,
        })
        .await?;
    Ok(())
}

pub(crate) fn capture_auto_resolve_snapshot(
    cwd: &Path,
    parent: Option<&str>,
    message: &'static str,
) -> Option<GhostCommit> {
    let cwd_buf = cwd.to_path_buf();
    let hook = move || bump_snapshot_epoch_for(&cwd_buf);
    let mut options = CreateGhostCommitOptions::new(cwd)
        .message(message)
        .post_commit_hook(&hook);
    if let Some(parent) = parent {
        options = options.parent(parent);
    }
    let snap = create_ghost_commit(&options).ok();
    if snap.is_some() {
        bump_snapshot_epoch_for(cwd);
    }
    snap
}

pub(crate) struct TurnResult {
    pub(crate) last_agent_message: Option<String>,
    pub(crate) error_seen: bool,
}

pub(crate) async fn submit_and_wait(
    conversation: &Arc<CodexConversation>,
    event_processor: &mut dyn EventProcessor,
    auto_review_tracker: &mut AutoReviewTracker,
    prompt_text: String,
    run_deadline: Option<Instant>,
) -> anyhow::Result<TurnResult> {
    let mut error_seen = false;

    let submit_id = conversation
        .submit(Op::UserInput {
            items: vec![InputItem::Text { text: prompt_text }],
            final_output_json_schema: None,
        })
        .await?;

    loop {
        let res = if let Some(deadline) = run_deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    let _ = conversation.submit(Op::Interrupt).await;
                    return Err(anyhow::anyhow!("Interrupted"));
                }
                res = tokio::time::timeout(remaining, conversation.next_event()) => {
                    match res {
                        Ok(event) => event,
                        Err(_) => {
                            let _ = conversation.submit(Op::Interrupt).await;
                            let _ = conversation.submit(Op::Shutdown).await;
                            return Err(anyhow::anyhow!("Time budget exceeded"));
                        }
                    }
                }
            }
        } else {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    let _ = conversation.submit(Op::Interrupt).await;
                    return Err(anyhow::anyhow!("Interrupted"));
                }
                res = conversation.next_event() => res,
            }
        };

        let event = res?;
        let event_id = event.id.clone();
        if matches!(event.msg, EventMsg::Error(_)) {
            error_seen = true;
        }

        if let EventMsg::AgentStatusUpdate(status) = &event.msg {
            let completions = auto_review_tracker.update(status);
            for completion in completions {
                emit_auto_review_completion(&completion);
            }
        }

        let last_agent_message = if let EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message }) = &event.msg {
            last_agent_message.clone()
        } else {
            None
        };

        let status = event_processor.process_event(event);

        if matches!(status, CodexStatus::Shutdown) {
            return Ok(TurnResult {
                last_agent_message: None,
                error_seen,
            });
        }

        if last_agent_message.is_some() && event_id == submit_id {
            return Ok(TurnResult {
                last_agent_message,
                error_seen,
            });
        }
    }
}
