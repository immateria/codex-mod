use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::bottom_pane::chrome::ChromeMode;
use crate::colors;

impl AppsSettingsView {
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
            Mode::Overview => self.render_overview(chrome, area, buf, &snapshot),
            Mode::AccountDetail { account_id } => {
                self.render_account_detail(chrome, area, buf, &snapshot, account_id)
            }
        }
    }

    fn render_overview(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        snapshot: &AppsSharedState,
    ) {
        let rows = self.overview_rows(snapshot);
        let page = self.overview_page(snapshot);
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

    fn render_account_detail(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        snapshot: &AppsSharedState,
        account_id: &str,
    ) {
        let page = self.account_detail_page(snapshot, account_id);
        let Some(layout) = page.render_shell_in_chrome(chrome, area, buf) else {
            return;
        };

        let lines = build_account_detail_lines(snapshot, account_id);
        let paragraph = Paragraph::new(lines)
            .style(Style::new().bg(colors::background()).fg(colors::text()))
            .wrap(Wrap { trim: false });
        paragraph.render(layout.body, buf);
    }
}

fn build_account_detail_lines(snapshot: &AppsSharedState, account_id: &str) -> Vec<Line<'static>> {
    match snapshot.status_by_account_id.get(account_id) {
        Some(crate::chatwidget::AppsAccountStatusState::Loading) => vec![Line::from(Span::styled(
            "Loading...",
            Style::new().fg(colors::function()),
        ))],
        Some(crate::chatwidget::AppsAccountStatusState::Failed { error, needs_login }) => {
            let mut lines = vec![Line::from(Span::styled(
                format!("Error: {error}"),
                Style::new().fg(colors::warning()),
            ))];
            if *needs_login {
                lines.push(Line::from(Span::styled(
                    "Press `l` to log in, then `r` to refresh.",
                    Style::new().fg(colors::text_dim()),
                )));
            }
            lines
        }
        Some(crate::chatwidget::AppsAccountStatusState::Ready { connected_apps, .. }) => {
            if connected_apps.is_empty() {
                return vec![Line::from(Span::styled(
                    "No connected apps detected.",
                    Style::new().fg(colors::text_dim()),
                ))];
            }
            let mut lines = Vec::new();
            for app in connected_apps {
                let mut label = format!("- {} (tools: {})", app.name, app.tool_count);
                if let Some(description) = app.description.as_ref()
                    && !description.is_empty()
                {
                    label.push_str(&format!(" - {description}"));
                }
                lines.push(Line::from(Span::raw(label)));
            }
            lines
        }
        _ => vec![
            Line::from(Span::styled(
                "Press `r` to load connected apps for this account.",
                Style::new().fg(colors::text_dim()),
            )),
        ],
    }
}
