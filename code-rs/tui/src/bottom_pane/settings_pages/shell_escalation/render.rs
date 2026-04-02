use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::rows::{KeyValueRow, StyledText};
use crate::bottom_pane::settings_ui::toggle;

impl ShellEscalationSettingsView {
    pub(super) fn row_specs(&self, rows: &[RowKind]) -> Vec<KeyValueRow<'static>> {
        let apply_suffix = if self.dirty { " *" } else { "" };

        let mut enabled_status = toggle::enabled_word_warning_off(self.enabled);
        enabled_status.style = enabled_status.style.bold();

        let zsh_path = self
            .zsh_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| "(unset)".to_string());

        let wrapper_override = self
            .wrapper_override
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| "auto".to_string());

        rows.iter()
            .copied()
            .map(|row| match row {
                RowKind::Enabled => KeyValueRow::new("Enabled").with_value(enabled_status.clone()),
                RowKind::ZshPath => KeyValueRow::new("Zsh path").with_value(StyledText::new(
                    zsh_path.clone(),
                    Style::default().fg(crate::colors::text_dim()),
                )),
                RowKind::WrapperOverride => KeyValueRow::new("Wrapper override").with_value(
                    StyledText::new(wrapper_override.clone(), Style::default().fg(crate::colors::text_dim())),
                ),
                RowKind::Apply => KeyValueRow::new("Apply changes").with_value(StyledText::new(
                    apply_suffix,
                    Style::default().fg(crate::colors::warning()),
                )),
                RowKind::Close => KeyValueRow::new("Close"),
            })
            .collect()
    }

    fn render_main_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        let rows = self.build_rows();
        let total = rows.len();
        let state = self.state.clamped(total);
        let selected_idx = state.selected_idx.unwrap_or(0);
        let scroll_top = state.scroll_top;

        let row_specs = self.row_specs(&rows);
        let page = self.main_page();
        let Some(layout) =
            page.render_in_chrome(chrome, area, buf, scroll_top, Some(selected_idx), &row_specs)
        else {
            return;
        };
        self.viewport_rows.set(layout.visible_rows());
    }

    fn render_edit_in_chrome(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        target: EditTarget,
        field: &FormTextField,
    ) {
        let _layout = self.edit_page(target).render_in_chrome(chrome, area, buf, field);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main_in_chrome(ChromeMode::Framed, area, buf),
            ViewMode::EditText { target, field } => self.render_edit_in_chrome(
                ChromeMode::Framed,
                area,
                buf,
                *target,
                field,
            ),
            ViewMode::Transition => self.render_main_in_chrome(ChromeMode::Framed, area, buf),
        }
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main_in_chrome(ChromeMode::ContentOnly, area, buf),
            ViewMode::EditText { target, field } => self.render_edit_in_chrome(
                ChromeMode::ContentOnly,
                area,
                buf,
                *target,
                field,
            ),
            ViewMode::Transition => self.render_main_in_chrome(ChromeMode::ContentOnly, area, buf),
        }
    }
}
