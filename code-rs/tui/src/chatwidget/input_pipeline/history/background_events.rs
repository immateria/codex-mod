use super::super::prelude::*;

impl ChatWidget<'_> {
    /// Insert a background event near the top of the current request so it appears
    /// before imminent provider output (e.g. Exec begin).
    pub(crate) fn insert_background_event_early(&mut self, message: String) {
        let ticket = self.make_background_before_next_output_ticket();
        self.insert_background_event_with_placement(
            message,
            BackgroundPlacement::BeforeNextOutput,
            Some(ticket.next_order()),
        );
    }
    /// Insert a background event using the specified placement semantics.
    pub(crate) fn insert_background_event_with_placement(
        &mut self,
        message: String,
        placement: BackgroundPlacement,
        order: Option<code_core::protocol::OrderMeta>,
    ) {
        if order.is_none() {
            if matches!(placement, BackgroundPlacement::Tail) {
                tracing::error!(
                    target: "code_order",
                    "missing order metadata for tail background event; dropping message"
                );
                return;
            } else {
                tracing::warn!(
                    target: "code_order",
                    "background event without order metadata placement={:?}",
                    placement
                );
            }
        }
        let system_placement = match placement {
            BackgroundPlacement::Tail => SystemPlacement::Tail,
            BackgroundPlacement::BeforeNextOutput => {
                if self.pending_user_prompts_for_next_turn > 0 {
                    SystemPlacement::Early
                } else {
                    SystemPlacement::PrePrompt
                }
            }
        };
        let cell = history_cell::new_background_event(message);
        let record = HistoryDomainRecord::BackgroundEvent(cell.state().clone());
        self.push_system_cell(
            Box::new(cell),
            system_placement,
            None,
            order.as_ref(),
            "background",
            Some(record),
        );
    }

    pub(crate) fn push_background_tail(&mut self, message: impl Into<String>) {
        let ticket = self.make_background_tail_ticket();
        self.insert_background_event_with_placement(
            message.into(),
            BackgroundPlacement::Tail,
            Some(ticket.next_order()),
        );
    }

    pub(crate) fn push_background_before_next_output(&mut self, message: impl Into<String>) {
        let ticket = self.make_background_before_next_output_ticket();
        self.insert_background_event_with_placement(
            message.into(),
            BackgroundPlacement::BeforeNextOutput,
            Some(ticket.next_order()),
        );
    }
}
