mod exited_review;
mod helpers;
mod task_complete;

use super::state::ReviewRuntimeState;
use crate::auto_review_status::AutoReviewTracker;
use crate::auto_review_status::emit_auto_review_completion;
use crate::auto_runtime::request_shutdown;
use crate::event_processor::CodexStatus;
use crate::event_processor::EventProcessor;
use code_core::CodexConversation;
use code_core::config::Config;
use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::protocol::Op;
use code_core::protocol::ReviewRequest;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::Instant;

use self::exited_review::handle_exited_review_mode_event;
use self::task_complete::handle_task_complete_event;

pub(super) struct ReviewEventLoopParams<'a> {
    pub(super) conversation: &'a Arc<CodexConversation>,
    pub(super) config: &'a Config,
    pub(super) event_processor: &'a mut dyn EventProcessor,
    pub(super) review_request: &'a Option<ReviewRequest>,
    pub(super) run_deadline: Option<Instant>,
    pub(super) max_seconds: Option<u64>,
    pub(super) rx: &'a mut UnboundedReceiver<Event>,
    pub(super) state: &'a mut ReviewRuntimeState,
}

pub(super) enum LoopControl {
    Continue,
    ProcessEvent,
}

pub(super) struct ShutdownState {
    pending: bool,
    sent: bool,
    deadline: Option<Instant>,
    grace_enabled: bool,
}

impl ShutdownState {
    fn new(grace_enabled: bool) -> Self {
        Self {
            pending: false,
            sent: false,
            deadline: None,
            grace_enabled,
        }
    }

    async fn request(
        &mut self,
        conversation: &Arc<CodexConversation>,
        auto_review_tracker: &AutoReviewTracker,
    ) -> anyhow::Result<()> {
        request_shutdown(
            conversation,
            auto_review_tracker,
            &mut self.pending,
            &mut self.sent,
            &mut self.deadline,
            self.grace_enabled,
        )
        .await
    }

    fn is_sent(&self) -> bool {
        self.sent
    }

    fn is_pending(&self) -> bool {
        self.pending
    }

    fn should_poll_deadline(&self) -> bool {
        self.pending && self.deadline.is_some() && self.grace_enabled
    }

    fn deadline_or_now(&self) -> Instant {
        self.deadline.unwrap_or_else(Instant::now)
    }
}

pub(super) async fn run_review_event_loop(
    params: ReviewEventLoopParams<'_>,
) -> anyhow::Result<bool> {
    let ReviewEventLoopParams {
        conversation,
        config,
        event_processor,
        review_request,
        run_deadline,
        max_seconds,
        rx,
        state,
    } = params;

    // Track whether a fatal error was reported by the server so we can
    // exit with a non-zero status for automation-friendly signaling.
    let mut error_seen = false;
    let mut shutdown_state = ShutdownState::new(config.tui.auto_review_enabled);
    let mut auto_review_tracker = AutoReviewTracker::new(&config.cwd);

    loop {
        tokio::select! {
            _ = async {
                if let Some(deadline) = run_deadline {
                    tokio::time::sleep_until(deadline).await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                eprintln!(
                    "Time budget exceeded (--max-seconds={})",
                    max_seconds.unwrap_or_default()
                );
                error_seen = true;
                let _ = conversation.submit(Op::Interrupt).await;
                let _ = conversation.submit(Op::Shutdown).await;
                break;
            }
            maybe_event = rx.recv() => {
                let Some(event) = maybe_event else {
                    break;
                };
                if let EventMsg::AgentStatusUpdate(status) = &event.msg {
                    let completions = auto_review_tracker.update(status);
                    for completion in completions {
                        emit_auto_review_completion(&completion);
                    }
                }
                if matches!(event.msg, EventMsg::Error(_)) {
                    error_seen = true;
                }

                let loop_control = match &event.msg {
                    EventMsg::ExitedReviewMode(review_event) => {
                        handle_exited_review_mode_event(config, review_request, state, review_event)
                            .await?
                    }
                    EventMsg::TaskComplete(task_complete) => {
                        handle_task_complete_event(
                            conversation,
                            config,
                            state,
                            &auto_review_tracker,
                            &mut shutdown_state,
                            task_complete,
                        )
                        .await?
                    }
                    _ => LoopControl::ProcessEvent,
                };

                if matches!(loop_control, LoopControl::Continue) {
                    continue;
                }

                let shutdown = event_processor.process_event(event);
                match shutdown {
                    CodexStatus::Running => {}
                    CodexStatus::InitiateShutdown => {
                        shutdown_state
                            .request(conversation, &auto_review_tracker)
                            .await?;
                    }
                    CodexStatus::Shutdown => {
                        break;
                    }
                }

                if shutdown_state.is_pending() {
                    shutdown_state
                        .request(conversation, &auto_review_tracker)
                        .await?;
                }
            }
            _ = tokio::time::sleep_until(shutdown_state.deadline_or_now()),
                if shutdown_state.should_poll_deadline() =>
            {
                shutdown_state
                    .request(conversation, &auto_review_tracker)
                    .await?;
            }
        }
    }

    Ok(error_seen)
}
