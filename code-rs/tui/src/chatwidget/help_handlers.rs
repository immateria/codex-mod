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

    // Overlay active: process navigation + close + tab switching
    let Some(ref mut overlay) = chat.help.overlay else { return false };

    use super::internals::state::{HelpFocus, HelpTab};
    let focus = chat.help.focus.get();

    match key_event.code {
        // Tab/BackTab cycle focus: Content → PrevArrow → NextArrow → Close → Content
        KeyCode::Tab => {
            chat.help.focus.set(focus.next());
            chat.request_redraw();
            true
        }
        KeyCode::BackTab => {
            chat.help.focus.set(focus.prev());
            chat.request_redraw();
            true
        }
        // Enter activates the focused element
        KeyCode::Enter => {
            match focus {
                HelpFocus::Content => false,
                HelpFocus::PrevArrow => {
                    overlay.active_tab = overlay.active_tab.prev();
                    chat.request_redraw();
                    true
                }
                HelpFocus::NextArrow => {
                    overlay.active_tab = overlay.active_tab.next();
                    chat.request_redraw();
                    true
                }
                HelpFocus::CloseButton => {
                    let _ = overlay;
                    chat.help.overlay = None;
                    chat.help.focus.set(HelpFocus::Content);
                    chat.request_redraw();
                    true
                }
            }
        }
        // Direct tab switching via arrow keys (when focus is on Content)
        KeyCode::Left if focus == HelpFocus::Content => {
            overlay.active_tab = overlay.active_tab.prev();
            chat.request_redraw();
            true
        }
        KeyCode::Right if focus == HelpFocus::Content => {
            overlay.active_tab = overlay.active_tab.next();
            chat.request_redraw();
            true
        }
        // Number keys always switch tabs regardless of focus
        KeyCode::Char('1') => {
            overlay.active_tab = HelpTab::Shortcuts;
            chat.help.focus.set(HelpFocus::Content);
            chat.request_redraw();
            true
        }
        KeyCode::Char('2') => {
            overlay.active_tab = HelpTab::Commands;
            chat.help.focus.set(HelpFocus::Content);
            chat.request_redraw();
            true
        }
        KeyCode::Char('3') => {
            overlay.active_tab = HelpTab::Tips;
            chat.help.focus.set(HelpFocus::Content);
            chat.request_redraw();
            true
        }
        // Scrolling (when focus is on Content, or always for Up/Down/Page)
        KeyCode::Up => {
            let s = overlay.scroll_mut();
            *s = s.saturating_sub(1);
            chat.request_redraw();
            true
        }
        KeyCode::Down => {
            let visible_rows = chat.help.body_visible_rows.get() as usize;
            let max_off = overlay.lines().len().saturating_sub(visible_rows.max(1));
            let cur = overlay.scroll() as usize;
            let next = cur.saturating_add(1).min(max_off);
            *overlay.scroll_mut() = next as u16;
            chat.request_redraw();
            true
        }
        KeyCode::PageUp => {
            let h = chat.help.body_visible_rows.get() as usize;
            let cur = overlay.scroll() as usize;
            *overlay.scroll_mut() = cur.saturating_sub(h) as u16;
            chat.request_redraw();
            true
        }
        KeyCode::PageDown | KeyCode::Char(' ') => {
            let h = chat.help.body_visible_rows.get() as usize;
            let cur = overlay.scroll() as usize;
            let visible_rows = chat.help.body_visible_rows.get() as usize;
            let max_off = overlay.lines().len().saturating_sub(visible_rows.max(1));
            *overlay.scroll_mut() = cur.saturating_add(h).min(max_off) as u16;
            chat.request_redraw();
            true
        }
        KeyCode::Home => {
            *overlay.scroll_mut() = 0;
            chat.request_redraw();
            true
        }
        KeyCode::End => {
            *overlay.scroll_mut() = u16::MAX;
            chat.request_redraw();
            true
        }
        KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('q') => {
            chat.help.overlay = None;
            chat.help.focus.set(HelpFocus::Content);
            chat.request_redraw();
            true
        }
        KeyCode::Char('/') if key_event.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
            chat.help.overlay = None;
            chat.help.focus.set(HelpFocus::Content);
            chat.request_redraw();
            true
        }
        _ => false,
    }
}
