use super::*;

use super::developer_message::build_timeboxed_review_message;
use crate::client_common::REVIEW_PROMPT;
use crate::protocol::{ExitedReviewModeEvent, ReviewOutputEvent, ReviewSnapshotInfo};
use crate::review_format::format_review_findings_block;

pub(super) async fn spawn_review_thread(
    sess: Arc<Session>,
    config: Arc<Config>,
    sub_id: String,
    review_request: ReviewRequest,
) {
    // Ensure any running task is stopped before starting the review flow.
    sess.notify_wait_interrupted(WaitInterruptReason::SessionAborted);
    sess.abort();

    let parent_turn_context = sess.make_turn_context();

    // Determine model + family for review mode.
    let review_model = config.review_model.clone();
    let review_family = find_family_for_model(&review_model)
        .unwrap_or_else(|| derive_default_model_family(&review_model));

    // Prepare a per-review configuration that favors deterministic feedback.
    let mut review_config = (*config).clone();
    review_config.model = review_model.clone();
    review_config.model_family = review_family.clone();
    review_config.model_reasoning_effort = config.review_model_reasoning_effort;
    review_config.model_reasoning_summary = ReasoningSummaryConfig::Detailed;
    review_config.model_text_verbosity = config.model_text_verbosity;
    review_config.user_instructions = None;
    review_config.base_instructions = Some(REVIEW_PROMPT.to_string());
    if let Some(cw) = review_family.context_window {
        review_config.model_context_window = Some(cw);
    }
    if let Some(max) = review_family.max_output_tokens {
        review_config.model_max_output_tokens = Some(max);
    }
    let review_config = Arc::new(review_config);

    let review_debug_logger = match crate::debug_logger::DebugLogger::new(review_config.debug) {
        Ok(logger) => Arc::new(Mutex::new(logger)),
        Err(err) => {
            warn!("failed to create review debug logger: {err}");
            Arc::new(Mutex::new(
                crate::debug_logger::DebugLogger::new(false).unwrap(),
            ))
        }
    };

    let review_otel = parent_turn_context
        .client
        .get_otel_event_manager()
        .map(|mgr| mgr.with_model(review_config.model.as_str(), review_config.model_family.slug.as_str()));

    let review_client = ModelClient::new(crate::client::ModelClientInit {
        config: review_config.clone(),
        auth_manager: parent_turn_context.client.get_auth_manager(),
        otel_event_manager: review_otel,
        provider: parent_turn_context.client.get_provider(),
        effort: review_config.model_reasoning_effort,
        summary: review_config.model_reasoning_summary,
        verbosity: review_config.model_text_verbosity,
        session_id: sess.session_uuid(),
        debug_logger: review_debug_logger,
    });

    let review_demo_message = if config.timeboxed_exec_mode {
        build_timeboxed_review_message(parent_turn_context.demo_developer_message.clone())
    } else {
        parent_turn_context.demo_developer_message.clone()
    };

    let review_turn_context = Arc::new(TurnContext {
        client: review_client,
        cwd: parent_turn_context.cwd.clone(),
        base_instructions: Some(REVIEW_PROMPT.to_string()),
        user_instructions: None,
        demo_developer_message: review_demo_message,
        compact_prompt_override: parent_turn_context.compact_prompt_override.clone(),
        approval_policy: parent_turn_context.approval_policy,
        sandbox_policy: parent_turn_context.sandbox_policy.clone(),
        shell_environment_policy: parent_turn_context.shell_environment_policy.clone(),
        collaboration_mode: parent_turn_context.collaboration_mode,
        is_review_mode: true,
        text_format_override: None,
        final_output_json_schema: None,
    });

    let review_prompt_text = format!(
        "{}\n\n---\n\nNow, here's your task: {}",
        REVIEW_PROMPT.trim(),
        review_request.prompt.trim()
    );
    let review_input = vec![InputItem::Text {
        text: review_prompt_text,
    }];

    let task = AgentTask::review(Arc::clone(&sess), Arc::clone(&review_turn_context), sub_id.clone(), review_input);
    sess.set_active_review(review_request.clone());
    sess.set_task(task);

    let event = sess.make_event(
        &sub_id,
        EventMsg::EnteredReviewMode(review_request.clone()),
    );
    sess.send_event(event).await;
}

pub(super) async fn exit_review_mode(
    session: Arc<Session>,
    task_sub_id: String,
    review_output: Option<ReviewOutputEvent>,
) {
    let snapshot = capture_review_snapshot(&session).await;
    let event = session.make_event(
        &task_sub_id,
        EventMsg::ExitedReviewMode(ExitedReviewModeEvent {
            review_output: review_output.clone(),
            snapshot,
        }),
    );
    session.send_event(event).await;

    let _active_request = session.take_active_review();

    let developer_text = match review_output.clone() {
        Some(output) => {
            let mut sections: Vec<String> = Vec::new();
            if !output.overall_explanation.trim().is_empty() {
                sections.push(output.overall_explanation.trim().to_string());
            }
            if !output.findings.is_empty() {
                sections.push(format_review_findings_block(&output.findings, None));
            }
            if !output.overall_correctness.trim().is_empty() {
                sections.push(format!(
                    "Overall correctness: {}",
                    output.overall_correctness.trim()
                ));
            }
            if output.overall_confidence_score > 0.0 {
                sections.push(format!(
                    "Confidence score: {:.1}",
                    output.overall_confidence_score
                ));
            }

            let results = if sections.is_empty() {
                "Reviewer did not provide any findings.".to_string()
            } else {
                sections.join("\n\n")
            };

            format!(
                "<user_action>\n  <context>User initiated a review task. Here's the full review output from reviewer model. User may select one or more comments to resolve.</context>\n  <action>review</action>\n  <results>\n  {results}\n  </results>\n</user_action>\n"
            )
        }
        None => {
            "<user_action>\n  <context>User initiated a review task, but it ended without a final response. If the user asks about this, tell them to re-initiate a review with `/review` and wait for it to complete.</context>\n  <action>review</action>\n  <results>\n  None.\n  </results>\n</user_action>\n"
                .to_string()
        }
    };

    let developer_message = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text: developer_text.clone() }],
        end_turn: None,
        phase: None,
    };

    session
        .record_conversation_items(&[developer_message])
        .await;
}

async fn capture_review_snapshot(session: &Session) -> Option<ReviewSnapshotInfo> {
    let cwd = session.cwd.clone();
    let repo_root = crate::git_info::get_git_repo_root(&cwd);
    let branch = crate::git_info::current_branch_name(&cwd).await;

    if repo_root.is_none() && branch.is_none() {
        return None;
    }

    Some(ReviewSnapshotInfo {
        snapshot_commit: None,
        branch,
        worktree_path: Some(cwd),
        repo_root,
    })
}

pub(super) fn parse_review_output_event(text: &str) -> ReviewOutputEvent {
    if let Ok(parsed) = serde_json::from_str::<ReviewOutputEvent>(text) {
        return parsed;
    }

    // Attempt to extract JSON from fenced code blocks if present.
    if let Some(idx) = text.find("```json")
        && let Some(end_idx) = text[idx + 7..].find("```") {
            let json_slice = &text[idx + 7..idx + 7 + end_idx];
            if let Ok(parsed) = serde_json::from_str::<ReviewOutputEvent>(json_slice) {
                return parsed;
            }
        }

    ReviewOutputEvent {
        findings: Vec::new(),
        overall_correctness: String::new(),
        overall_explanation: text.trim().to_string(),
        overall_confidence_score: 0.0,
    }
}
