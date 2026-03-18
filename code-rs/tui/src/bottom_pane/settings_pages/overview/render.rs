use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::colors;
use crate::bottom_pane::chrome::ChromeMode;

use super::SettingsOverviewView;

impl SettingsOverviewView {
    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        if self.rows.is_empty() {
            let page = self.page();
            let Some(layout) = page.render_shell_in_chrome(ChromeMode::Framed, area, buf) else {
                return;
            };
            Paragraph::new(Line::from(vec![Span::styled(
                "No settings sections available.",
                Style::new().fg(colors::text_dim()),
            )]))
            .render(layout.body, buf);
            self.viewport_rows.set(layout.body.height as usize);
            return;
        }

        let scroll_top = self.scroll.scroll_top.min(self.rows.len().saturating_sub(1));
        let page = self.page();
        let rows = self.menu_rows();
        let Some(layout) = page.render_menu_rows_in_chrome(
            ChromeMode::Framed,
            area,
            buf,
            scroll_top,
            Some(self.selected_index()),
            &rows,
        ) else {
            return;
        };
        self.viewport_rows.set(layout.body.height as usize);
    }
}
