use crate::auto_review_status::AutoReviewTracker;
use crate::auto_review_status::emit_auto_review_completion;
use crate::auto_runtime::AUTO_REVIEW_SHUTDOWN_GRACE_MS;
use crate::auto_runtime::TurnResult;
use crate::auto_runtime::build_auto_prompt;
use crate::auto_runtime::send_shutdown_if_ready;
use crate::auto_runtime::submit_and_wait;
use crate::event_processor::CodexStatus;
use crate::event_processor::EventProcessor;
use crate::event_processor::handle_last_message;
use crate::review_output::make_assistant_message;
use crate::review_output::make_user_message;
use code_auto_drive_core::AutoCoordinatorCommand;
use code_auto_drive_core::AutoCoordinatorEvent;
use code_auto_drive_core::AutoCoordinatorEventSender;
use code_auto_drive_core::AutoCoordinatorStatus;
use code_auto_drive_core::AutoDriveHistory;
use code_auto_drive_core::MODEL_SLUG;
use code_auto_drive_core::start_auto_coordinator;
use code_core::AutoDriveMode;
use code_core::AutoDrivePidFile;
use code_core::CodexConversation;
use code_core::config::Config;
use code_core::protocol::EventMsg;
use code_core::protocol::InputItem;
use code_core::protocol::Op;
use code_core::protocol::TaskCompleteEvent;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::Duration;
use tokio::time::Instant;

pub(crate) fn build_auto_drive_exec_config(config: &Config) -> Config {
    let mut auto_config = config.clone();
    auto_config.model = config.auto_drive.model.trim().to_string();
    if auto_config.model.is_empty() {
        auto_config.model = MODEL_SLUG.to_string();
    }
    auto_config.model_reasoning_effort = config.auto_drive.model_reasoning_effort;
    auto_config
}

pub(crate) async fn run_auto_drive_session(
    goal: String,
    images: Vec<PathBuf>,
    config: Config,
    conversation: Arc<CodexConversation>,
    mut event_processor: Box<dyn EventProcessor>,
    last_message_path: Option<PathBuf>,
    run_deadline: Option<Instant>,
) -> anyhow::Result<()> {
    let mut final_last_message: Option<String> = None;
    let mut error_seen = false;
    let mut auto_review_tracker = AutoReviewTracker::new(&config.cwd);
    let mut shutdown_sent = false;

    if !images.is_empty() {
        let items: Vec<InputItem> = images
            .into_iter()
            .map(|path| InputItem::LocalImage { path })
            .collect();
        let initial_images_event_id = conversation
            .submit(Op::UserInput {
                items,
                final_output_json_schema: None,
            })
            .await?;
        loop {
            let event = if let Some(deadline) = run_deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match tokio::time::timeout(remaining, conversation.next_event()).await {
                    Ok(event) => event?,
                    Err(_) => {
                        eprintln!(
                            "Time budget exceeded (--max-seconds={})",
                            config.max_run_seconds.unwrap_or_default()
                        );
                        let _ = conversation.submit(Op::Interrupt).await;
                        let _ = conversation.submit(Op::Shutdown).await;
                        return Err(anyhow::anyhow!("Time budget exceeded"));
                    }
                }
            } else {
                conversation.next_event().await?
            };

            let is_complete = event.id == initial_images_event_id
                && matches!(
                    event.msg,
                    EventMsg::TaskComplete(TaskCompleteEvent {
                        last_agent_message: _,
                    })
                );
            let status = event_processor.process_event(event);
            if is_complete || matches!(status, CodexStatus::Shutdown) {
                break;
            }
        }
    }

    let mut history = AutoDriveHistory::new();

    let mut auto_drive_pid_guard =
        AutoDrivePidFile::write(&config.code_home, Some(goal.as_str()), AutoDriveMode::Exec);

    let auto_config = build_auto_drive_exec_config(&config);

    let (auto_tx, mut auto_rx) = tokio::sync::mpsc::unbounded_channel();
    let sender = AutoCoordinatorEventSender::new(move |event| {
        let _ = auto_tx.send(event);
    });

    let handle = start_auto_coordinator(
        sender,
        goal.clone(),
        history.raw_snapshot(),
        auto_config,
        config.debug,
        false,
    )?;

    loop {
        let maybe_event = if let Some(deadline) = run_deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, auto_rx.recv()).await {
                Ok(event) => event,
                Err(_) => {
                    let _ = handle.send(AutoCoordinatorCommand::Stop);
                    handle.cancel();
                    let _ = conversation.submit(Op::Interrupt).await;
                    let _ = conversation.submit(Op::Shutdown).await;
                    return Err(anyhow::anyhow!("Time budget exceeded"));
                }
            }
        } else {
            auto_rx.recv().await
        };

        let Some(event) = maybe_event else {
            break;
        };

        match event {
            AutoCoordinatorEvent::Thinking { delta, .. } => {
                eprintln!("[auto] {delta}");
            }
            AutoCoordinatorEvent::Action { message } => {
                eprintln!("[auto] {message}");
            }
            AutoCoordinatorEvent::TokenMetrics {
                total_usage,
                last_turn_usage,
                turn_count,
                ..
            } => {
                eprintln!(
                    "[auto] turn {} tokens (turn/total): {}/{}",
                    turn_count,
                    last_turn_usage.blended_total(),
                    total_usage.blended_total()
                );
            }
            AutoCoordinatorEvent::CompactedHistory { conversation, .. } => {
                history.replace_all(conversation.to_vec());
            }
            AutoCoordinatorEvent::UserReply {
                user_response,
                cli_command,
            } => {
                if let Some(text) = user_response.filter(|s| !s.trim().is_empty()) {
                    history.append_raw(&[make_assistant_message(text.clone())]);
                    final_last_message = Some(text);
                }

                if let Some(cmd) = cli_command {
                    let prompt_text = cmd.trim();
                    if !prompt_text.is_empty() {
                        history.append_raw(&[make_user_message(prompt_text.to_string())]);
                        let TurnResult {
                            last_agent_message,
                            error_seen: turn_error,
                        } = match submit_and_wait(
                            &conversation,
                            event_processor.as_mut(),
                            &mut auto_review_tracker,
                            prompt_text.to_string(),
                            run_deadline,
                        )
                        .await
                        {
                            Ok(result) => result,
                            Err(err) => {
                                let _ = handle.send(AutoCoordinatorCommand::Stop);
                                handle.cancel();
                                return Err(err);
                            }
                        };
                        error_seen |= turn_error;
                        if let Some(text) = last_agent_message {
                            history.append_raw(&[make_assistant_message(text.clone())]);
                            final_last_message = Some(text);
                        }
                        let _ = handle
                            .send(AutoCoordinatorCommand::UpdateConversation(
                                history.raw_snapshot().into(),
                            ));
                    }
                }
            }
            AutoCoordinatorEvent::Decision {
                seq,
                status,
                status_title,
                status_sent_to_user,
                goal: maybe_goal,
                cli,
                agents_timing,
                agents,
                transcript,
            } => {
                history.append_raw(&transcript);
                let _ = handle.send(AutoCoordinatorCommand::AckDecision { seq });

                if let Some(title) = status_title.filter(|s| !s.trim().is_empty()) {
                    eprintln!("[auto] status: {title}");
                }
                if let Some(sent) = status_sent_to_user.filter(|s| !s.trim().is_empty()) {
                    eprintln!("[auto] update: {sent}");
                }
                if let Some(goal_text) = maybe_goal.filter(|s| !s.trim().is_empty()) {
                    eprintln!("[auto] goal: {goal_text}");
                }

                let Some(cli_action) = cli else {
                    if matches!(status, AutoCoordinatorStatus::Success | AutoCoordinatorStatus::Failed)
                    {
                        let _ = handle.send(AutoCoordinatorCommand::Stop);
                    }
                    continue;
                };

                let prompt_text = build_auto_prompt(&cli_action, &agents, agents_timing);
                history.append_raw(&[make_user_message(prompt_text.clone())]);

                let TurnResult {
                    last_agent_message,
                    error_seen: turn_error,
                } = match submit_and_wait(
                    &conversation,
                    event_processor.as_mut(),
                    &mut auto_review_tracker,
                    prompt_text,
                    run_deadline,
                )
                .await
                {
                    Ok(result) => result,
                    Err(err) => {
                        let _ = handle.send(AutoCoordinatorCommand::Stop);
                        handle.cancel();
                        return Err(err);
                    }
                };
                error_seen |= turn_error;
                if let Some(text) = last_agent_message {
                    history.append_raw(&[make_assistant_message(text.clone())]);
                    final_last_message = Some(text);
                }

                if handle
                    .send(AutoCoordinatorCommand::UpdateConversation(
                        history.raw_snapshot().into(),
                    ))
                    .is_err()
                {
                    break;
                }
            }
            AutoCoordinatorEvent::StopAck => {
                break;
            }
        }
    }

    handle.cancel();

    if !auto_review_tracker.is_running() {
        let grace_deadline = Instant::now() + Duration::from_millis(AUTO_REVIEW_SHUTDOWN_GRACE_MS);
        while Instant::now() < grace_deadline {
            let remaining = grace_deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, conversation.next_event()).await {
                Ok(Ok(event)) => {
                    if let EventMsg::AgentStatusUpdate(status) = &event.msg {
                        let completions = auto_review_tracker.update(status);
                        for completion in completions {
                            emit_auto_review_completion(&completion);
                        }
                    }

                    let processor_status = event_processor.process_event(event);
                    if matches!(processor_status, CodexStatus::Shutdown)
                        || auto_review_tracker.is_running()
                    {
                        break;
                    }
                }
                Ok(Err(err)) => return Err(err.into()),
                Err(_) => break,
            }
        }
    }

    if auto_review_tracker.is_running() {
        loop {
            let event = if let Some(deadline) = run_deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match tokio::time::timeout(remaining, conversation.next_event()).await {
                    Ok(event) => event?,
                    Err(_) => {
                        eprintln!(
                            "Time budget exceeded (--max-seconds={})",
                            config.max_run_seconds.unwrap_or_default()
                        );
                        let _ = conversation.submit(Op::Interrupt).await;
                        let _ = conversation.submit(Op::Shutdown).await;
                        return Err(anyhow::anyhow!("Time budget exceeded"));
                    }
                }
            } else {
                conversation.next_event().await?
            };

            if let EventMsg::AgentStatusUpdate(status) = &event.msg {
                let completions = auto_review_tracker.update(status);
                for completion in completions {
                    emit_auto_review_completion(&completion);
                }
            }

            let status = event_processor.process_event(event);

            if !auto_review_tracker.is_running() {
                break;
            }

            if matches!(status, CodexStatus::Shutdown) {
                break;
            }
        }
    }

    let _ = send_shutdown_if_ready(&conversation, &auto_review_tracker, &mut shutdown_sent).await?;

    loop {
        let event = if let Some(deadline) = run_deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, conversation.next_event()).await {
                Ok(event) => event?,
                Err(_) => {
                    eprintln!(
                        "Time budget exceeded (--max-seconds={})",
                        config.max_run_seconds.unwrap_or_default()
                    );
                    let _ = conversation.submit(Op::Interrupt).await;
                    let _ = conversation.submit(Op::Shutdown).await;
                    return Err(anyhow::anyhow!("Time budget exceeded"));
                }
            }
        } else {
            conversation.next_event().await?
        };

        if let EventMsg::AgentStatusUpdate(status) = &event.msg {
            let completions = auto_review_tracker.update(status);
            for completion in completions {
                emit_auto_review_completion(&completion);
            }
        }

        if matches!(event.msg, EventMsg::ShutdownComplete) {
            break;
        }
        let status = event_processor.process_event(event);
        if matches!(status, CodexStatus::Shutdown) {
            break;
        }
    }

    if let Some(path) = last_message_path.as_deref() {
        handle_last_message(final_last_message.as_deref(), path);
    }

    event_processor.print_final_output();

    if error_seen {
        if let Some(guard) = auto_drive_pid_guard.take() {
            guard.cleanup();
        }
        std::process::exit(1);
    }

    Ok(())
}
