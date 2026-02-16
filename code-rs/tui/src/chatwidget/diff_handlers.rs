//! Diff overlay key handling extracted from ChatWidget::handle_key_event.

use super::ChatWidget;
use crossterm::event::{KeyCode, KeyEvent};

// Returns true if the key was handled by the diff overlay.
pub(super) fn handle_diff_key(chat: &mut ChatWidget<'_>, key_event: KeyEvent) -> bool {
    if chat.diffs.overlay.is_none() {
        return false;
    }
    if handle_diff_confirm_key(chat, key_event.code) {
        return true;
    }

    match key_event.code {
        KeyCode::Left => {
            move_selected_diff_tab(chat, false);
            true
        }
        KeyCode::Right => {
            move_selected_diff_tab(chat, true);
            true
        }
        KeyCode::Up => {
            scroll_selected_diff_tab(chat, false);
            true
        }
        KeyCode::Down => {
            scroll_selected_diff_tab(chat, true);
            true
        }
        KeyCode::Char('u') => {
            prompt_undo_for_selected_diff_block(chat);
            true
        }
        KeyCode::Char('e') => {
            prompt_explain_selected_diff_block(chat);
            true
        }
        KeyCode::Esc => {
            chat.diffs.overlay = None;
            chat.diffs.confirm = None;
            chat.request_redraw();
            true
        }
        _ => false,
    }
}

fn handle_diff_confirm_key(chat: &mut ChatWidget<'_>, code: KeyCode) -> bool {
    // If a confirmation banner is active, only Enter applies to it.
    if let Some(confirm) = chat.diffs.confirm.take() {
        if matches!(code, KeyCode::Enter) {
            chat.submit_user_message(confirm.text_to_submit.into());
            chat.request_redraw();
            return true;
        }
        // Put it back for other keys (Esc is handled by the global router).
        chat.diffs.confirm = Some(confirm);
    }
    false
}

fn move_selected_diff_tab(chat: &mut ChatWidget<'_>, right: bool) {
    let Some(overlay) = chat.diffs.overlay.as_mut() else {
        return;
    };

    if right {
        if overlay.selected + 1 < overlay.tabs.len() {
            overlay.selected += 1;
        }
    } else if overlay.selected > 0 {
        overlay.selected -= 1;
    }

    if let Some(offset) = overlay.scroll_offsets.get_mut(overlay.selected) {
        *offset = 0;
    }
    chat.request_redraw();
}

fn scroll_selected_diff_tab(chat: &mut ChatWidget<'_>, down: bool) {
    let visible_rows = chat.diffs.body_visible_rows.get() as usize;
    let Some(overlay) = chat.diffs.overlay.as_mut() else {
        return;
    };
    let max_off = selected_tab_max_scroll(overlay, visible_rows);
    let Some(offset) = overlay.scroll_offsets.get_mut(overlay.selected) else {
        return;
    };

    let clamped_current = (*offset as usize).min(max_off);
    let next = if down {
        clamped_current.saturating_add(1).min(max_off)
    } else {
        clamped_current.saturating_sub(1)
    };
    *offset = next as u16;
    chat.request_redraw();
}

fn prompt_undo_for_selected_diff_block(chat: &mut ChatWidget<'_>) {
    let Some(diff_text) = selected_diff_block_text(chat) else {
        return;
    };
    let submit_text = format!("Please undo this:\n{diff_text}");
    chat.diffs.confirm = Some(super::diff_ui::DiffConfirm { text_to_submit: submit_text });
    chat.request_redraw();
}

fn prompt_explain_selected_diff_block(chat: &mut ChatWidget<'_>) {
    let Some(diff_text) = selected_diff_block_text(chat) else {
        return;
    };
    let prompt =
        format!("Can you please explain what this diff does and the reason behind it?\n\n{diff_text}");
    chat.submit_user_message(prompt.into());
    chat.request_redraw();
}

fn selected_diff_block_text(chat: &ChatWidget<'_>) -> Option<String> {
    let visible_rows = chat.diffs.body_visible_rows.get() as usize;
    let overlay = chat.diffs.overlay.as_ref()?;
    let block = selected_visible_diff_block(overlay, visible_rows)?;

    let mut diff_text = String::new();
    for line in &block.lines {
        let rendered_line: String = line.spans.iter().map(|span| span.content.clone()).collect();
        diff_text.push_str(&rendered_line);
        diff_text.push('\n');
    }
    Some(diff_text)
}

fn selected_visible_diff_block(
    overlay: &super::diff_ui::DiffOverlay,
    visible_rows: usize,
) -> Option<&super::diff_ui::DiffBlock> {
    let (_, blocks) = overlay.tabs.get(overlay.selected)?;
    let max_off = selected_tab_max_scroll(overlay, visible_rows);
    let skip = overlay
        .scroll_offsets
        .get(overlay.selected)
        .copied()
        .unwrap_or(0) as usize;
    let skip = skip.min(max_off);

    let mut start = 0usize;
    for block in blocks {
        let len = block.lines.len();
        if start <= skip && skip < start + len {
            return Some(block);
        }
        start += len;
    }
    None
}

fn selected_tab_max_scroll(overlay: &super::diff_ui::DiffOverlay, visible_rows: usize) -> usize {
    let total_lines: usize = overlay
        .tabs
        .get(overlay.selected)
        .map(|(_, blocks)| blocks.iter().map(|block| block.lines.len()).sum())
        .unwrap_or(0);
    total_lines.saturating_sub(visible_rows.max(1))
}
