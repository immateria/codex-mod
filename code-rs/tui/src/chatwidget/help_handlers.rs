//! Help overlay key handling similar to the diff overlay, but simpler.

use super::ChatWidget;
use crossterm::event::{KeyCode, KeyEvent};

// Returns true if the key was handled by the guide overlay (or toggled it closed).
pub(super) fn handle_help_key(chat: &mut ChatWidget<'_>, key_event: KeyEvent) -> bool {
    use crossterm::event::KeyModifiers;

    // If no guide overlay, intercept F1 or Ctrl+/ to open it.
    if chat.help.overlay.is_none() {
        let is_f1 = matches!(key_event, KeyEvent { code: KeyCode::F(1), .. });
        let is_ctrl_slash = matches!(
            key_event,
            KeyEvent { code: KeyCode::Char('/'), modifiers, .. }
            if modifiers.contains(KeyModifiers::CONTROL)
        );
        if is_f1 || is_ctrl_slash {
            chat.toggle_help_popup();
            return true;
        }
        return false;
    }

    // Overlay active: process navigation + close
    let Some(ref mut overlay) = chat.help.overlay else { return false };
    match key_event.code {
        KeyCode::Up => {
            overlay.scroll = overlay.scroll.saturating_sub(1);
            chat.request_redraw();
            true
        }
        KeyCode::Down => {
            let visible_rows = chat.help.body_visible_rows.get() as usize;
            let max_off = overlay.lines.len().saturating_sub(visible_rows.max(1));
            let next = (overlay.scroll as usize).saturating_add(1).min(max_off);
            overlay.scroll = next as u16;
            chat.request_redraw();
            true
        }
        KeyCode::PageUp => {
            let h = chat.help.body_visible_rows.get() as usize;
            let cur = overlay.scroll as usize;
            overlay.scroll = cur.saturating_sub(h) as u16;
            chat.request_redraw();
            true
        }
        KeyCode::PageDown | KeyCode::Char(' ') => {
            let h = chat.help.body_visible_rows.get() as usize;
            let cur = overlay.scroll as usize;
            let visible_rows = chat.help.body_visible_rows.get() as usize;
            let max_off = overlay.lines.len().saturating_sub(visible_rows.max(1));
            overlay.scroll = cur.saturating_add(h).min(max_off) as u16;
            chat.request_redraw();
            true
        }
        KeyCode::Home => {
            overlay.scroll = 0;
            chat.request_redraw();
            true
        }
        KeyCode::End => {
            overlay.scroll = u16::MAX;
            chat.request_redraw();
            true
        }
        KeyCode::Esc | KeyCode::F(1) => {
            // Close on Esc or F1
            chat.help.overlay = None;
            chat.request_redraw();
            true
        }
        _ => false,
    }
}
