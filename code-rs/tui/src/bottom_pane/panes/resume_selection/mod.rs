use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Row, Table};
use ratatui::widgets::Widget;
use std::cell::Cell;

use crate::app_event::AppEvent;
use crate::app_event::SessionPickerAction;
use crate::app_event_sender::AppEventSender;

use crate::bottom_pane::{BottomPane, BottomPaneView, CancellationEvent, ConditionalUpdate};
use crate::bottom_pane::popup_consts::MAX_POPUP_ROWS;
use crate::components::popup_frame::render_popup_frame;

const RESUME_POPUP_ROWS: usize = 14;
const EFFECTIVE_MAX_ROWS: usize = if RESUME_POPUP_ROWS < MAX_POPUP_ROWS {
    RESUME_POPUP_ROWS
} else {
    MAX_POPUP_ROWS
};

pub(crate) struct ResumeRow {
    pub(crate) modified: String,
    pub(crate) created: String,
    pub(crate) user_msgs: String,
    pub(crate) branch: String,
    pub(crate) last_user_message: String,
    pub(crate) path: std::path::PathBuf,
}

pub(crate) struct ResumeSelectionView {
    title: String,
    subtitle: String,
    rows: Vec<ResumeRow>,
    selected: usize,
    // Topmost row index currently visible in the table viewport
    top: usize,
    viewport_rows: Cell<usize>,
    complete: bool,
    action: SessionPickerAction,
    app_event_tx: AppEventSender,
}

impl ResumeSelectionView {
    pub(crate) fn new(
        title: String,
        subtitle: String,
        rows: Vec<ResumeRow>,
        action: SessionPickerAction,
        app_event_tx: AppEventSender,
    ) -> Self {
        Self {
            title,
            subtitle,
            rows,
            selected: 0,
            top: 0,
            viewport_rows: Cell::new(EFFECTIVE_MAX_ROWS),
            complete: false,
            action,
            app_event_tx,
        }
    }

    fn move_up(&mut self) {
        if self.rows.is_empty() { return; }
        if self.selected == 0 { self.selected = self.rows.len().saturating_sub(1); }
        else { self.selected -= 1; }
        self.ensure_selected_visible();
    }

    fn move_down(&mut self) {
        if self.rows.is_empty() { return; }
        self.selected = (self.selected + 1) % self.rows.len();
        self.ensure_selected_visible();
    }

    fn page_up(&mut self) {
        if self.rows.is_empty() { return; }
        let page = self.visible_rows();
        if self.selected >= page { self.selected -= page; } else { self.selected = 0; }
        self.ensure_selected_visible();
    }

    fn page_down(&mut self) {
        if self.rows.is_empty() { return; }
        let page = self.visible_rows();
        self.selected = (self.selected + page).min(self.rows.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    fn go_home(&mut self) {
        if self.rows.is_empty() { return; }
        self.selected = 0;
        self.ensure_selected_visible();
    }

    fn go_end(&mut self) {
        if self.rows.is_empty() { return; }
        self.selected = self.rows.len().saturating_sub(1);
        self.ensure_selected_visible();
    }

    fn visible_rows(&self) -> usize {
        let viewport = self.viewport_rows.get().max(1);
        viewport.min(self.rows.len().max(1)).min(EFFECTIVE_MAX_ROWS)
    }

    fn ensure_selected_visible(&mut self) {
        let page = self.visible_rows();
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top.saturating_add(page) {
            self.top = self.selected.saturating_sub(page.saturating_sub(1));
        }
    }
}

impl BottomPaneView<'_> for ResumeSelectionView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'_>, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::Home => self.go_home(),
            KeyCode::End => self.go_end(),
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(row) = self.rows.get(self.selected) {
                    match self.action {
                        SessionPickerAction::Resume => {
                            self.app_event_tx.send(AppEvent::ResumeFrom(row.path.clone()));
                        }
                        SessionPickerAction::Fork => {
                            self.app_event_tx.send(AppEvent::ForkFrom(row.path.clone()));
                        }
                    }
                    self.complete = true;
                }
            }
            KeyCode::Esc => self.complete = true,
            _ => {}
        }
    }

    fn is_complete(&self) -> bool { self.complete }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'_>) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }

    fn update_status_text(&mut self, _text: &str) -> ConditionalUpdate { ConditionalUpdate::NoRedraw }

    fn desired_height(&self, _width: u16) -> u16 {
        // Include block borders (+2), optional subtitle (+1), table header (+1),
        // clamped rows, spacer (+1), footer (+1)
        let rows = self.rows.len().clamp(1, EFFECTIVE_MAX_ROWS) as u16;
        let subtitle = u16::from(!self.subtitle.is_empty());
        2 + subtitle + 1 + rows + 1 + 2
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let Some(inner) = render_popup_frame(area, buf, &self.title) else {
            return;
        };

        // Optional subtitle (path, etc.)
        let mut next_y = inner.y;
        if !self.subtitle.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                &self.subtitle,
                crate::colors::style_text_dim(),
            )))
            .render(Rect { x: inner.x.saturating_add(1), y: next_y, width: inner.width.saturating_sub(1), height: 1 }, buf);
            next_y = next_y.saturating_add(1);
        }

        // Reserve one blank spacer line above the footer
        let footer_reserved: u16 = 2;
        let table_area = Rect {
            x: inner.x.saturating_add(1),
            y: next_y,
            width: inner.width.saturating_sub(1),
            height: inner
                .height
                .saturating_sub(footer_reserved.saturating_add(next_y.saturating_sub(inner.y))),
        };

        let header_rows = 1;
        let viewport_capacity = table_area
            .height
            .saturating_sub(header_rows)
            .max(1) as usize;
        self.viewport_rows.set(viewport_capacity);

        // Build rows (windowed to the visible viewport)
        let page = self.visible_rows();
        let start = self.top.min(self.rows.len());
        let end = (start + page).min(self.rows.len());
        let rows_iter = self.rows[start..end].iter().enumerate().map(|(idx, r)| {
            let i = start + idx; // absolute index
            let cells = [
                r.modified.as_str(),
                r.created.as_str(),
                r.user_msgs.as_str(),
                r.branch.as_str(),
                r.last_user_message.as_str(),
            ]
            .into_iter()
            .map(ratatui::widgets::Cell::from);
            let mut row = Row::new(cells).height(1);
            if i == self.selected {
                row = row.style(crate::colors::style_on_selection().add_modifier(Modifier::BOLD));
            }
            row
        });

        // Column constraints roughly match header widths
        let widths = [
            Constraint::Length(10), // Modified
            Constraint::Length(10), // Created
            Constraint::Length(11), // User Msgs
            Constraint::Length(10), // Branch
            Constraint::Min(10),    // Last User Message
        ];

        let header = Row::new(vec!["Modified", "Created", "User Msgs", "Branch", "Session"]).height(1)
            .style(crate::colors::style_text_bright());

        let table = Table::new(rows_iter, widths)
            .header(header)
            .highlight_symbol("")
            .column_spacing(1);
        table.render(table_area, buf);

        // Footer hints
        // Draw a spacer line above footer (implicit by not drawing into that row)
        let footer = Rect { x: inner.x.saturating_add(1), y: inner.y.saturating_add(inner.height.saturating_sub(1)), width: inner.width.saturating_sub(1), height: 1 };
        let footer_line = crate::bottom_pane::settings_ui::hints::shortcut_line(&[
            crate::bottom_pane::settings_ui::hints::KeyHint::new(
                format!("{ud} PgUp PgDn", ud = crate::icons::nav_up_down()),
                " navigate",
            ),
            crate::bottom_pane::settings_ui::hints::hint_enter(" select"),
            crate::bottom_pane::settings_ui::hints::hint_esc(" cancel"),
        ]);
        Paragraph::new(footer_line)
            .style(crate::colors::style_text_on_bg())
            .render(footer, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
    use ratatui::layout::Rect;
    use std::sync::mpsc;

    #[test]
    fn resume_selection_shows_max_rows_for_capacity() {
        let rows = (0..14)
            .map(|i| ResumeRow {
                modified: "m".to_string(),
                created: "c".to_string(),
                user_msgs: "1".to_string(),
                branch: "main".to_string(),
                last_user_message: format!("row-{i}"),
                path: std::path::PathBuf::from(format!("/tmp/sess-{i}")),
            })
            .collect();

        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let view = ResumeSelectionView::new(
            "Resume".to_string(),
            String::new(),
            rows,
            SessionPickerAction::Resume,
            AppEventSender::new(tx),
        );

        let width = 120;
        let height = view.desired_height(width);
        let mut buf = ratatui::buffer::Buffer::empty(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });

        view.render(Rect { x: 0, y: 0, width, height }, &mut buf);

        let inner_width = width.saturating_sub(2);
        let mut row_lines: usize = 0;
        for y in 1..height.saturating_sub(1) {
            let line: String = (0..inner_width)
                .map(|x| {
                    buf[(x.saturating_add(1), y)]
                        .symbol()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .concat();
            if line.contains("row-") {
                row_lines += 1;
            }
        }

        assert_eq!(row_lines, EFFECTIVE_MAX_ROWS);
    }
}
