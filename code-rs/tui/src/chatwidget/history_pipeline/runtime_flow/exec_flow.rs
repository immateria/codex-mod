use super::*;

impl ChatWidget<'_> {
    /// Handle exec command begin immediately
    pub(in super::super::super) fn handle_exec_begin_now(
        &mut self,
        ev: ExecCommandBeginEvent,
        order: &code_core::protocol::OrderMeta,
    ) {
        exec_tools::handle_exec_begin_now(self, ev, order);
    }

    /// Common exec-begin handling used for both immediate and deferred paths.
    /// Ensures we finalize any active stream, create the running cell, and
    /// immediately apply a pending end if it arrived first.
    pub(in super::super::super) fn handle_exec_begin_ordered(
        &mut self,
        ev: ExecCommandBeginEvent,
        order: code_core::protocol::OrderMeta,
        seq: u64,
    ) {
        self.finalize_active_stream();
        tracing::info!(
            "[order] ExecCommandBegin call_id={} seq={}",
            ev.call_id,
            seq
        );
        self.handle_exec_begin_now(ev.clone(), &order);
        self.ensure_spinner_for_activity("exec-begin");
        if let Some((pending_end, order2, _ts)) = self
            .exec
            .pending_exec_ends
            .remove(&ExecCallId(ev.call_id))
        {
            self.handle_exec_end_now(pending_end, &order2);
        }
        if self.interrupts.has_queued() {
            self.flush_interrupt_queue();
        }
    }

    /// Handle exec command end immediately
    pub(in super::super::super) fn handle_exec_end_now(
        &mut self,
        ev: ExecCommandEndEvent,
        order: &code_core::protocol::OrderMeta,
    ) {
        exec_tools::handle_exec_end_now(self, ev, order);
    }

    /// Handle or defer an exec end based on whether the matching begin has
    /// already been seen. When no running entry exists yet, stash the end so
    /// it can be paired once the begin arrives, falling back to a timed flush.
    pub(in super::super::super) fn enqueue_or_handle_exec_end(
        &mut self,
        ev: ExecCommandEndEvent,
        order: code_core::protocol::OrderMeta,
    ) {
        let call_id = ExecCallId(ev.call_id.clone());
        let has_running = self.exec.running_commands.contains_key(&call_id);
        if has_running {
            self.handle_exec_end_now(ev, &order);
            return;
        }

        // If the history already knows about this call_id (e.g., Begin was handled
        // but running_commands was cleared), finish it immediately to avoid leaving
        // the cell stuck in a running state.
        if self
            .history_state
            .history_id_for_exec_call(call_id.as_ref())
            .is_some()
        {
            self.handle_exec_end_now(ev, &order);
            return;
        }

        self.exec
            .pending_exec_ends
            .insert(call_id, (ev, order.clone(), std::time::Instant::now()));
        let tx = self.app_event_tx.clone();
        let fallback_tx = tx.clone();
        if thread_spawner::spawn_lightweight("exec-flush", move || {
            std::thread::sleep(std::time::Duration::from_millis(120));
            tx.send(crate::app_event::AppEvent::FlushPendingExecEnds);
        })
        .is_none()
        {
            fallback_tx.send(crate::app_event::AppEvent::FlushPendingExecEnds);
        }
    }
}
