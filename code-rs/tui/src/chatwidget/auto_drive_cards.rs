use super::{tool_cards, ChatWidget, OrderKey};
use super::tool_cards::ToolCardSlot;
use crate::history_cell::{AutoDriveActionKind, AutoDriveCardCell, AutoDriveStatus};

pub(super) struct AutoDriveTracker {
    pub slot: ToolCardSlot,
    pub cell: AutoDriveCardCell,
    pub session_id: u64,
    pub request_ordinal: u64,
    pub active: bool,
}

impl AutoDriveTracker {
    fn new(order_key: OrderKey, session_id: u64, request_ordinal: u64, goal: Option<String>) -> Self {
        Self {
            slot: ToolCardSlot::new(order_key),
            cell: AutoDriveCardCell::new(goal),
            session_id,
            request_ordinal,
            active: true,
        }
    }

    fn card_key(&self) -> String {
        format!("auto_drive:{}", self.session_id)
    }

    fn assign_key(&mut self) {
        let key = self.card_key();
        self.cell.set_signature(Some(key.clone()));
        tool_cards::assign_tool_card_key(&mut self.slot, &mut self.cell, Some(key.clone()));
        self.slot.set_signature(Some(key));
    }

    fn ensure_insert(&mut self, chat: &mut ChatWidget<'_>) {
        self.assign_key();
        tool_cards::ensure_tool_card::<AutoDriveCardCell>(chat, &mut self.slot, &self.cell);
    }

    fn sync_to_history(&mut self, chat: &mut ChatWidget<'_>) {
        tool_cards::replace_tool_card::<AutoDriveCardCell>(chat, &mut self.slot, &self.cell);
    }
}

/// Borrow the active tracker mutably, execute `f`, then sync the card back
/// to the history. Returns `Some(R)` if a tracker was present, `None`
/// otherwise. The tracker is never temporarily removed from its slot, so
/// an early return or panic inside `f` cannot lose it.
fn with_tracker<R>(
    chat: &mut ChatWidget<'_>,
    f: impl FnOnce(&mut AutoDriveTracker, &mut ChatWidget<'_>) -> R,
) -> Option<R> {
    let mut tracker = chat.tools_state.auto_drive_tracker.take()?;
    let result = f(&mut tracker, chat);
    tracker.sync_to_history(chat);
    chat.tools_state.auto_drive_tracker = Some(tracker);
    Some(result)
}

/// Like [`with_tracker`] but also updates `request_ordinal` and `order_key`
/// from the supplied key before calling `f`.
fn with_tracker_keyed<R>(
    chat: &mut ChatWidget<'_>,
    order_key: OrderKey,
    f: impl FnOnce(&mut AutoDriveTracker) -> R,
) -> Option<R> {
    with_tracker(chat, |tracker, _chat| {
        tracker.request_ordinal = order_key.req;
        tracker.slot.set_order_key(order_key);
        f(tracker)
    })
}

pub(super) fn start_session(
    chat: &mut ChatWidget<'_>,
    order_key: OrderKey,
    goal: Option<String>,
) {
    let request_ordinal = order_key.req;

    // If a tracker for the same request already exists, just refresh its key.
    if chat.tools_state.auto_drive_tracker.as_ref().is_some_and(|t| t.request_ordinal == request_ordinal) {
        with_tracker(chat, |tracker, _| {
            tracker.slot.set_order_key(order_key);
        });
        return;
    }

    let session_id = chat.auto_drive_card_sequence;
    chat.auto_drive_card_sequence = chat.auto_drive_card_sequence.wrapping_add(1);

    let mut tracker = AutoDriveTracker::new(order_key, session_id, request_ordinal, goal);
    tracker.ensure_insert(chat);
    chat.tools_state.auto_drive_tracker = Some(tracker);
}

pub(super) fn record_action(
    chat: &mut ChatWidget<'_>,
    order_key: OrderKey,
    text: impl Into<String>,
    kind: AutoDriveActionKind,
) {
    let text = text.into();
    with_tracker_keyed(chat, order_key, |tracker| {
        tracker.cell.push_action(text, kind);
    });
}

pub(super) fn update_goal(
    chat: &mut ChatWidget<'_>,
    order_key: OrderKey,
    goal: Option<String>,
) {
    with_tracker_keyed(chat, order_key, |tracker| {
        tracker.cell.set_goal(goal);
    });
}

pub(super) fn set_status(chat: &mut ChatWidget<'_>, order_key: OrderKey, status: AutoDriveStatus) {
    with_tracker_keyed(chat, order_key, |tracker| {
        tracker.cell.set_status(status);
    });
}

pub(super) fn finalize(
    chat: &mut ChatWidget<'_>,
    order_key: OrderKey,
    message: Option<String>,
    status: AutoDriveStatus,
    action_kind: AutoDriveActionKind,
    completion_message: Option<String>,
) {
    with_tracker_keyed(chat, order_key, |tracker| {
        if let Some(msg) = message {
            tracker.cell.push_action(msg, action_kind);
        }
        tracker.cell.set_completion_message(completion_message);
        tracker.cell.set_status(status);
        tracker.active = false;
    });
}

pub(super) fn start_celebration(
    chat: &mut ChatWidget<'_>,
    message: Option<String>,
) -> bool {
    with_tracker(chat, |tracker, _| {
        tracker.cell.start_celebration(message);
    })
    .is_some()
}

pub(super) fn stop_celebration(chat: &mut ChatWidget<'_>) -> bool {
    with_tracker(chat, |tracker, _| {
        tracker.cell.stop_celebration();
    })
    .is_some()
}

pub(super) fn update_completion_message(
    chat: &mut ChatWidget<'_>,
    message: Option<String>,
) -> bool {
    with_tracker(chat, |tracker, _| {
        tracker.cell.set_completion_message(message);
    })
    .is_some()
}

pub(super) fn clear(chat: &mut ChatWidget<'_>) {
    chat.tools_state.auto_drive_tracker = None;
}
