use std::sync::mpsc::Receiver;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use super::super::state::{App, HIGH_EVENT_BURST_MAX};
use crate::app_event::AppEvent;

impl App<'_> {
    /// Pull the next event with priority for interactive input.
    /// Never returns None due to idleness; only returns None if both channels disconnect.
    pub(super) fn next_event_priority(&mut self) -> Option<AppEvent> {
        next_event_priority_impl(
            &self.app_event_rx_high,
            &self.app_event_rx_bulk,
            &mut self.consecutive_high_events,
        )
    }
}

pub(super) fn is_image_clipboard_paste_shortcut(key_event: &KeyEvent) -> bool {
    if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return false;
    }

    match key_event {
        KeyEvent {
            code: KeyCode::Char('v' | 'V'),
            modifiers,
            ..
        } => {
            modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                && modifiers.contains(crossterm::event::KeyModifiers::ALT)
        }
        _ => false,
    }
}

fn next_event_priority_impl(
    high_rx: &Receiver<AppEvent>,
    bulk_rx: &Receiver<AppEvent>,
    consecutive_high_events: &mut u32,
) -> Option<AppEvent> {
    use std::sync::mpsc::RecvTimeoutError::{Disconnected, Timeout};

    loop {
        if *consecutive_high_events >= HIGH_EVENT_BURST_MAX
            && let Ok(ev) = bulk_rx.try_recv()
        {
            *consecutive_high_events = 0;
            return Some(ev);
        }

        if let Ok(ev) = high_rx.try_recv() {
            *consecutive_high_events = consecutive_high_events.saturating_add(1);
            return Some(ev);
        }

        *consecutive_high_events = 0;
        if let Ok(ev) = bulk_rx.try_recv() {
            return Some(ev);
        }

        match high_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(ev) => {
                *consecutive_high_events = 1;
                return Some(ev);
            }
            Err(Timeout) => continue,
            Err(Disconnected) => break,
        }
    }

    bulk_rx.recv().ok()
}

#[cfg(test)]
mod next_event_priority_tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn next_event_priority_serves_bulk_amid_high_burst() {
        let (high_tx, high_rx) = channel();
        let (bulk_tx, bulk_rx) = channel();

        for _ in 0..(HIGH_EVENT_BURST_MAX + 4) {
            high_tx
                .send(AppEvent::RequestRedraw)
                .expect("send high event");
        }

        bulk_tx
            .send(AppEvent::FlushPendingExecEnds)
            .expect("send bulk event");

        // Keep high non-empty beyond the burst window.
        for _ in 0..4 {
            high_tx
                .send(AppEvent::RequestRedraw)
                .expect("send high event");
        }

        let mut consecutive = 0;
        let mut saw_bulk = false;
        for _ in 0..(HIGH_EVENT_BURST_MAX + 2) {
            let ev = next_event_priority_impl(&high_rx, &bulk_rx, &mut consecutive)
                .expect("expected an event");
            if matches!(ev, AppEvent::FlushPendingExecEnds) {
                saw_bulk = true;
                break;
            }
        }

        assert!(
            saw_bulk,
            "bulk event should not be starved behind continuous high-priority events"
        );
    }

    #[test]
    fn image_clipboard_fallback_shortcut_is_ctrl_alt_v_only() {
        assert!(is_image_clipboard_paste_shortcut(&KeyEvent::new(
            KeyCode::Char('v'),
            crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::ALT,
        )));

        assert!(!is_image_clipboard_paste_shortcut(&KeyEvent::new(
            KeyCode::Char('v'),
            crossterm::event::KeyModifiers::CONTROL,
        )));

        assert!(!is_image_clipboard_paste_shortcut(&KeyEvent::new(
            KeyCode::Char('v'),
            crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::SHIFT,
        )));

        assert!(!is_image_clipboard_paste_shortcut(&KeyEvent::new(
            KeyCode::Insert,
            crossterm::event::KeyModifiers::SHIFT,
        )));
    }
}
