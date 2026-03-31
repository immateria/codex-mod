use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::bottom_pane::chrome::ChromeMode;
use crate::colors;

impl SecretsSettingsView {
    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::Framed, area, buf);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::ContentOnly, area, buf);
    }

    fn render_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let snapshot = self.shared_snapshot();

        match &self.mode {
            Mode::List => self.render_list(chrome, area, buf, &snapshot),
            Mode::ConfirmDelete { entry } => {
                self.render_confirm_delete(chrome, area, buf, &snapshot, entry.clone())
            }
        }
    }

    fn render_list(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        snapshot: &SecretsSharedState,
    ) {
        let rows = self.list_rows(snapshot);
        let page = self.list_page(snapshot);
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return;
        };

        let visible_rows = layout.body.height.max(1) as usize;
        self.list_viewport_rows.set(visible_rows);

        let mut state = self.list_state.get();
        state.clamp_selection(rows.len());
        state.ensure_visible(rows.len(), visible_rows);
        self.list_state.set(state);

        let _ = page.render_menu_rows_in_chrome(
            chrome,
            area,
            buf,
            state.scroll_top,
            state.selected_idx,
            &rows,
        );
    }

    fn render_confirm_delete(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        snapshot: &SecretsSharedState,
        entry: SecretListEntry,
    ) {
        let page = self.confirm_delete_page(snapshot);
        let buttons = self.confirm_delete_button_specs();
        let Some(layout) = page.render_with_standard_actions_end_in_chrome(chrome, area, buf, &buttons) else {
            return;
        };

        let scope_line = match &entry.scope {
            code_secrets::SecretScope::Environment(env_id) => format!("Scope: env ({env_id})"),
            code_secrets::SecretScope::Global => "Scope: global".to_string(),
        };

        let lines = vec![
            Line::from(format!("Delete secret `{}`?", entry.name.as_str())),
            Line::from(scope_line),
            Line::from(""),
            Line::from("This removes the stored secret value. You can re-add it with:"),
            Line::from(format!("  code secrets set {}", entry.name.as_str())),
        ];

        let paragraph = Paragraph::new(lines)
            .style(Style::new().bg(colors::background()).fg(colors::text()))
            .wrap(Wrap { trim: false });
        paragraph.render(layout.body, buf);
    }
}

