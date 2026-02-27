use super::*;

pub(super) struct TurnLatencyGuard<'a> {
    sess: &'a Session,
    attempt_req: u64,
    active: bool,
}

impl<'a> TurnLatencyGuard<'a> {
    pub(super) fn new(sess: &'a Session, attempt_req: u64, prompt: &Prompt) -> Self {
        sess.turn_latency_request_scheduled(attempt_req, prompt);
        Self {
            sess,
            attempt_req,
            active: true,
        }
    }

    pub(super) fn mark_completed(
        &mut self,
        output_item_count: usize,
        token_usage: Option<&TokenUsage>,
    ) {
        if !self.active {
            return;
        }
        self.sess.turn_latency_request_completed(
            self.attempt_req,
            output_item_count,
            token_usage,
        );
        self.active = false;
    }

    pub(super) fn mark_failed(&mut self, note: Option<String>) {
        if !self.active {
            return;
        }
        self.sess.turn_latency_request_failed(self.attempt_req, note);
        self.active = false;
    }
}

impl Drop for TurnLatencyGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self.sess.turn_latency_request_failed(
                self.attempt_req,
                Some("dropped_without_outcome".to_string()),
            );
        }
    }
}

