use super::*;

mod developer_message;
mod review;
mod run;

pub(super) async fn spawn_review_thread(
    sess: Arc<Session>,
    config: Arc<Config>,
    sub_id: String,
    review_request: ReviewRequest,
) {
    review::spawn_review_thread(sess, config, sub_id, review_request).await;
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AgentTaskKind {
    Regular,
    Review,
    Compact,
}

/// A series of Turns in response to user input.
pub(in crate::codex) struct AgentTask {
    sess: Arc<Session>,
    pub(in crate::codex) sub_id: String,
    handle: AbortHandle,
    kind: AgentTaskKind,
}

impl AgentTask {
    pub(in crate::codex) fn spawn(
        sess: Arc<Session>,
        turn_context: Arc<TurnContext>,
        sub_id: String,
        input: Vec<InputItem>,
    ) -> Self {
        let handle = {
            let sess_clone = Arc::clone(&sess);
            let tc_clone = Arc::clone(&turn_context);
            let sub_clone = sub_id.clone();
            tokio::spawn(async move {
                run::run_agent(sess_clone, tc_clone, sub_clone, input).await;
            })
            .abort_handle()
        };
        Self {
            sess,
            sub_id,
            handle,
            kind: AgentTaskKind::Regular,
        }
    }

    pub(in crate::codex) fn compact(
        sess: Arc<Session>,
        turn_context: Arc<TurnContext>,
        sub_id: String,
        input: Vec<InputItem>,
    ) -> Self {
        let handle = {
            let sess_clone = Arc::clone(&sess);
            let tc_clone = Arc::clone(&turn_context);
            let sub_clone = sub_id.clone();
            tokio::spawn(async move {
                compact::run_compact_task(
                    sess_clone,
                    tc_clone,
                    sub_clone,
                    input,
                )
                .await;
            })
            .abort_handle()
        };
        Self {
            sess,
            sub_id,
            handle,
            kind: AgentTaskKind::Compact,
        }
    }

    pub(in crate::codex) fn review(
        sess: Arc<Session>,
        turn_context: Arc<TurnContext>,
        sub_id: String,
        input: Vec<InputItem>,
    ) -> Self {
        let handle = {
            let sess_clone = Arc::clone(&sess);
            let tc_clone = Arc::clone(&turn_context);
            let sub_clone = sub_id.clone();
            tokio::spawn(async move {
                run::run_agent(sess_clone, tc_clone, sub_clone, input).await;
            })
            .abort_handle()
        };
        Self {
            sess,
            sub_id,
            handle,
            kind: AgentTaskKind::Review,
        }
    }

    pub(in crate::codex) fn abort(self, reason: TurnAbortReason) {
        if !self.handle.is_finished() {
            self.handle.abort();
            let event = self
                .sess
                .make_event(&self.sub_id, EventMsg::TurnAborted(TurnAbortedEvent { reason }));
            let sess = self.sess.clone();
            let sub_id = self.sub_id.clone();
            let kind = self.kind;
            tokio::spawn(async move {
                if kind == AgentTaskKind::Review {
                    review::exit_review_mode(sess.clone(), sub_id, None).await;
                }
                sess.send_event(event).await;
            });
        }
    }
}
