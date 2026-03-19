use super::*;

impl ChatWidget<'_> {
    /// Compute an OrderKey for system (non‑LLM) notices in a way that avoids
    /// creating multiple synthetic request buckets before the first provider turn.
    pub(super) fn system_order_key(
        &mut self,
        placement: SystemPlacement,
        order: Option<&code_core::protocol::OrderMeta>,
    ) -> OrderKey {
        // If the provider supplied OrderMeta, honor it strictly.
        if let Some(om) = order {
            return self.provider_order_key_from_order_meta(om);
        }

        // Derive a stable request bucket for system notices when OrderMeta is absent.
        // Default to the current provider request if known; else use a sticky
        // pre-turn synthetic req=1 to group UI confirmations before the first turn.
        // If a user prompt for the next turn is already queued, attach new
        // system notices to the upcoming request to avoid retroactive inserts.
        let mut req = if self.last_seen_request_index > 0 {
            self.last_seen_request_index
        } else {
            if self.synthetic_system_req.is_none() {
                self.synthetic_system_req = Some(1);
            }
            self.synthetic_system_req.unwrap_or(1)
        };
        if order.is_none() && self.pending_user_prompts_for_next_turn > 0 {
            req = req.saturating_add(1);
        }

        self.internal_seq = self.internal_seq.saturating_add(1);
        let mut out = match placement {
            SystemPlacement::Early => i32::MIN + 2,
            SystemPlacement::Tail => i32::MAX,
            SystemPlacement::PrePrompt => i32::MIN,
        };

        if order.is_none()
            && self.pending_user_prompts_for_next_turn > 0
            && matches!(placement, SystemPlacement::Early)
        {
            out = i32::MIN;
        }

        let mut key = OrderKey {
            req,
            out,
            seq: self.internal_seq,
        };

        if matches!(placement, SystemPlacement::Tail) {
            let reference = self
                .last_assigned_order
                .or_else(|| self.cell_order_seq.iter().copied().max());
            if let Some(max_key) = reference
                && key <= max_key {
                    key = Self::order_key_successor(max_key);
                }
        }

        self.internal_seq = self.internal_seq.max(key.seq);
        self.last_assigned_order = Some(match self.last_assigned_order {
            Some(prev) => prev.max(key),
            None => key,
        });

        key
    }

    pub(super) fn background_tail_request_ordinal(&mut self) -> u64 {
        let mut req = if self.last_seen_request_index > 0 {
            self.last_seen_request_index
        } else {
            *self.synthetic_system_req.get_or_insert(1)
        };
        if self.pending_user_prompts_for_next_turn > 0 {
            req = req.saturating_add(1);
        }
        if let Some(last) = self.last_assigned_order {
            req = req.max(last.req);
        }
        if let Some(max_req) = self.ui_background_seq_counters.keys().copied().max() {
            req = req.max(max_req);
        }
        req
    }

    pub(super) fn background_order_ticket_for_req(&mut self, req: u64) -> BackgroundOrderTicket {
        let seed = self
            .last_assigned_order
            .filter(|key| key.req == req)
            .map(|key| key.seq.saturating_add(1))
            .unwrap_or(0);

        let counter = self
            .ui_background_seq_counters
            .entry(req)
            .or_insert_with(|| Arc::new(AtomicU64::new(seed)))
            .clone();

        if seed > 0 {
            let current = counter.load(Ordering::SeqCst);
            if current < seed {
                counter.store(seed, Ordering::SeqCst);
            }
        }
        BackgroundOrderTicket {
            request_ordinal: req,
            seq_counter: counter,
        }
    }

    pub(super) fn background_tail_order_meta(&mut self) -> code_core::protocol::OrderMeta {
        self.background_tail_order_ticket_internal().next_order()
    }

    pub(super) fn send_background_tail_ordered(&mut self, message: impl Into<String>) {
        let order = self.background_tail_order_meta();
        self.app_event_tx
            .send_background_event_with_order(message.into(), order);
    }

    pub(super) fn rebuild_ui_background_seq_counters(&mut self) {
        self.ui_background_seq_counters.clear();
        let mut next_per_req: HashMap<u64, u64> = HashMap::new();
        for key in &self.cell_order_seq {
            if key.out == i32::MAX {
                let next = key.seq.saturating_add(1);
                let entry = next_per_req.entry(key.req).or_insert(0);
                *entry = (*entry).max(next);
            }
        }
        for (req, next) in next_per_req {
            self.ui_background_seq_counters
                .insert(req, Arc::new(AtomicU64::new(next)));
        }
    }

    /// Insert or replace a system notice cell with consistent ordering.
    /// If `id_for_replace` is provided and we have a prior index for it, replace in place.
    pub(super) fn push_system_cell(
        &mut self,
        cell: Box<dyn HistoryCell>,
        placement: SystemPlacement,
        id_for_replace: Option<String>,
        order: Option<&code_core::protocol::OrderMeta>,
        tag: &'static str,
        record: Option<HistoryDomainRecord>,
    ) {
        if let Some(id) = id_for_replace.as_ref()
            && let Some(&idx) = self.system_cell_by_id.get(id) {
                if let Some(record) = record {
                    self.history_replace_with_record(idx, cell, record);
                } else {
                    self.history_replace_at(idx, cell);
                }
                return;
            }
        let key = self.system_order_key(placement, order);
        let pos = self.history_insert_with_key_global_tagged(cell, key, tag, record);
        if let Some(id) = id_for_replace {
            self.system_cell_by_id.insert(id, pos);
        }
    }

    /// Decide where to place a UI confirmation right now.
    /// If we're truly pre-turn (no provider traffic yet, and no queued prompt),
    /// place before the first user prompt. Otherwise, append to end of current.
    pub(super) fn ui_placement_for_now(&self) -> SystemPlacement {
        if self.last_seen_request_index == 0 && self.pending_user_prompts_for_next_turn == 0 {
            SystemPlacement::PrePrompt
        } else {
            SystemPlacement::Tail
        }
    }

    // Synthetic key for internal content that should appear at the TOP of the NEXT request
    // (e.g., the user’s prompt preceding the model’s output for that turn).
    pub(super) fn next_req_key_top(&mut self) -> OrderKey {
        let req = self.last_seen_request_index.saturating_add(1);
        self.internal_seq = self.internal_seq.saturating_add(1);
        OrderKey {
            req,
            out: i32::MIN,
            seq: self.internal_seq,
        }
    }

    // Synthetic key for a user prompt that should appear just after banners but
    // still before any model output within the next request.
    pub(super) fn next_req_key_prompt(&mut self) -> OrderKey {
        let req = self.last_seen_request_index.saturating_add(1);
        self.internal_seq = self.internal_seq.saturating_add(1);
        OrderKey {
            req,
            out: i32::MIN + 1,
            seq: self.internal_seq,
        }
    }

    // Synthetic key for internal notices tied to the upcoming turn that
    // should appear immediately after the user prompt but still before any
    // model output for that turn.
    pub(super) fn next_req_key_after_prompt(&mut self) -> OrderKey {
        let req = self.last_seen_request_index.saturating_add(1);
        self.internal_seq = self.internal_seq.saturating_add(1);
        OrderKey {
            req,
            out: i32::MIN + 2,
            seq: self.internal_seq,
        }
    }
}
