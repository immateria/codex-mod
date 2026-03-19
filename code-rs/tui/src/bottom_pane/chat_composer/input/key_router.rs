use super::*;

pub(super) fn handle_key_event_inner(
    view: &mut ChatComposer,
    key_event: KeyEvent,
) -> (InputResult, bool) {
    let now = Instant::now();
    let burst_active = view.paste_burst.enter_should_insert_newline(now);
    let recent_plain_char = view.paste_burst.recent_plain_char(now);

    // Track rapid plain-character bursts (common when bracketed paste is
    // unavailable) so we can suppress Enter-based submissions and insert
    // literal newlines instead.
    if matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        match key_event.code {
            KeyCode::Char(_) => {
                let unmodified =
                    key_event.modifiers.is_empty() || key_event.modifiers == KeyModifiers::SHIFT;
                if unmodified {
                    view.paste_burst.record_plain_char_for_enter_window(now);
                } else {
                    view.paste_burst.clear_enter_window();
                }
            }
            KeyCode::Tab => {
                // Tabs often appear in per-key pastes of code; treat as pastey input.
                view.paste_burst.record_plain_char_for_enter_window(now);
            }
            KeyCode::Enter => {
                // handled below
            }
            _ => view.paste_burst.clear_enter_window(),
        }
    } else if key_event.kind != KeyEventKind::Release {
        view.paste_burst.clear_enter_window();
    }

    let enter_should_newline = matches!(
        key_event,
        KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        }
    ) && (burst_active || recent_plain_char);

    if enter_should_newline {
        // Enter is a non-Down key, so clear the sticky scroll flag.
        view.next_down_scrolls_history = false;

        // Treat Enter as literal newline when a paste-like burst is active.
        view.insert_str("\n");
        view.history.reset_navigation();
        view.paste_burst.extend_enter_window(now);

        // Keep popups in sync just like the main path.
        view.resync_popups();

        return (InputResult::None, true);
    }

    // Treat Tab as literal input while we're inside a paste-like burst to
    // avoid launching file search or other Tab handlers mid-paste. This
    // keeps per-key pastes containing tabs (common in code blocks) intact.
    if matches!(
        key_event,
        KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        }
    ) && burst_active
    {
        view.insert_str("\t");
        view.history.reset_navigation();
        view.paste_burst.extend_enter_window(now);
        return (InputResult::None, true);
    }

    // Any non-Down key clears the sticky flag; handled before popup routing
    if !matches!(key_event.code, KeyCode::Down) {
        view.next_down_scrolls_history = false;
    }
    let result = match &mut view.active_popup {
        ActivePopup::Command(_) => view.handle_key_event_with_slash_popup(key_event),
        ActivePopup::File(_) => view.handle_key_event_with_file_popup(key_event),
        ActivePopup::None => view.handle_key_event_without_popup(key_event),
    };

    // Update (or hide/show) popup after processing the key.
    view.resync_popups();

    result
}

