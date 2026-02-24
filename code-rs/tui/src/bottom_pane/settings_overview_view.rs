use super::bottom_pane_view::ConditionalUpdate;
use super::settings_panel::{panel_content_rect, render_panel, PanelFrameStyle};
use super::{BottomPane, BottomPaneView, SettingsSection};
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;
use crate::ui_interaction::{
    route_selectable_list_mouse_with_config, ScrollSelectionBehavior, SelectableListMouseConfig,
    SelectableListMouseResult,
};
use crate::util::buffer::{fill_rect, write_line};
use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

#[derive(Debug, Clone)]
pub(crate) struct SettingsMenuRow {
    pub(crate) section: SettingsSection,
    pub(crate) summary: Option<String>,
}

pub(crate) struct SettingsOverviewView {
    rows: Vec<SettingsMenuRow>,
    scroll: ScrollState,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

impl SettingsOverviewView {
    pub(crate) fn new(
        rows: Vec<SettingsMenuRow>,
        initial_section: SettingsSection,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut scroll = ScrollState::new();
        if !rows.is_empty() {
            let selected = rows
                .iter()
                .position(|row| row.section == initial_section)
                .unwrap_or(0);
            scroll.selected_idx = Some(selected);
        }
        Self {
            rows,
            scroll,
            app_event_tx,
            is_complete: false,
        }
    }

    fn selected_index(&self) -> usize {
        self.scroll.selected_idx.unwrap_or(0)
    }

    fn selected_section(&self) -> Option<SettingsSection> {
        self.rows.get(self.selected_index()).map(|row| row.section)
    }

    fn move_up(&mut self, visible_rows: usize) {
        self.scroll.move_up_wrap_visible(self.rows.len(), visible_rows);
    }

    fn move_down(&mut self, visible_rows: usize) {
        self.scroll.move_down_wrap_visible(self.rows.len(), visible_rows);
    }

    fn open_selected(&mut self) {
        let Some(section) = self.selected_section() else {
            return;
        };
        self.app_event_tx
            .send(AppEvent::OpenSettings { section: Some(section) });
        self.is_complete = true;
    }

    fn cancel(&mut self) {
        self.is_complete = true;
    }

    fn visible_layout(area: Rect) -> Option<(Rect, Rect, Rect)> {
        if area.width == 0 || area.height == 0 {
            return None;
        }

        let style = PanelFrameStyle::bottom_pane().with_margin(Margin::new(1, 0));
        let content = panel_content_rect(area, style);
        if content.width == 0 || content.height < 3 {
            return None;
        }

        let header = Rect::new(content.x, content.y, content.width, 1);
        let footer = Rect::new(
            content.x,
            content.y.saturating_add(content.height.saturating_sub(1)),
            content.width,
            1,
        );
        let body = Rect::new(
            content.x,
            content.y.saturating_add(1),
            content.width,
            content.height.saturating_sub(2),
        );

        Some((header, body, footer))
    }

    fn process_key_event(&mut self, key_event: KeyEvent, visible_rows: usize) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.cancel();
                true
            }
            KeyCode::Enter => {
                self.open_selected();
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up(visible_rows);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down(visible_rows);
                true
            }
            KeyCode::Home => {
                if !self.rows.is_empty() {
                    self.scroll.selected_idx = Some(0);
                    self.scroll.scroll_top = 0;
                }
                true
            }
            KeyCode::End => {
                let len = self.rows.len();
                if len > 0 {
                    self.scroll.selected_idx = Some(len - 1);
                    self.scroll.ensure_visible(len, visible_rows.max(1));
                }
                true
            }
            _ => false,
        }
    }

    fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let Some((_header_area, list_area, _footer_area)) = Self::visible_layout(area) else {
            return false;
        };

        if self.rows.is_empty() || list_area.width == 0 || list_area.height == 0 {
            return false;
        }

        let visible_rows = list_area.height as usize;
        let mut selected = self.selected_index();
        let scroll_top = self.scroll.scroll_top.min(self.rows.len().saturating_sub(1));

        let row_at_position = |x: u16, y: u16| {
            if x < list_area.x
                || x >= list_area.x.saturating_add(list_area.width)
                || y < list_area.y
                || y >= list_area.y.saturating_add(list_area.height)
            {
                return None;
            }
            let rel = y.saturating_sub(list_area.y) as usize;
            Some(scroll_top.saturating_add(rel))
        };

        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.rows.len(),
            row_at_position,
            SelectableListMouseConfig {
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );

        match result {
            SelectableListMouseResult::Ignored => false,
            SelectableListMouseResult::SelectionChanged => {
                self.scroll.selected_idx = Some(selected);
                self.scroll.ensure_visible(self.rows.len(), visible_rows.max(1));
                true
            }
            SelectableListMouseResult::Activated => {
                self.scroll.selected_idx = Some(selected);
                self.open_selected();
                true
            }
        }
    }

    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let line = Line::from(vec![
            Span::styled("↑↓/jk", Style::default().fg(crate::colors::function())),
            Span::styled(" move  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Enter", Style::default().fg(crate::colors::function())),
            Span::styled(" open  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Esc", Style::default().fg(crate::colors::function())),
            Span::styled(" close", Style::default().fg(crate::colors::text_dim())),
        ]);
        write_line(
            buf,
            area.x,
            area.y,
            area.width,
            &line,
            Style::default().bg(crate::colors::background()),
        );
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let help = self
            .selected_section()
            .map(SettingsSection::help_line)
            .unwrap_or("");
        let line = Line::from(vec![Span::styled(
            help.to_string(),
            Style::default().fg(crate::colors::text_dim()),
        )]);
        write_line(
            buf,
            area.x,
            area.y,
            area.width,
            &line,
            Style::default().bg(crate::colors::background()),
        );
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let len = self.rows.len();
        if len == 0 {
            let line = Line::from(vec![Span::styled(
                "No settings sections available.".to_string(),
                Style::default().fg(crate::colors::text_dim()),
            )]);
            write_line(
                buf,
                area.x,
                area.y,
                area.width,
                &line,
                Style::default().bg(crate::colors::background()),
            );
            return;
        }

        let visible = area.height as usize;
        let scroll_top = self.scroll.scroll_top.min(len.saturating_sub(1));
        let selected = self.selected_index().min(len.saturating_sub(1));
        let label_width: usize = 18;
        let max_width = area.width;

        for row_idx in 0..visible {
            let idx = scroll_top.saturating_add(row_idx);
            let y = area.y.saturating_add(row_idx as u16);
            let row_area = Rect::new(area.x, y, area.width, 1);

            if idx >= len {
                fill_rect(buf, row_area, Some(' '), Style::default().bg(crate::colors::background()));
                continue;
            }

            let row = &self.rows[idx];
            let is_selected = idx == selected;
            let base = if is_selected {
                Style::default()
                    .bg(crate::colors::selection())
                    .fg(crate::colors::text_bright())
            } else {
                Style::default().bg(crate::colors::background()).fg(crate::colors::text())
            };
            fill_rect(buf, row_area, Some(' '), base);

            let prefix = if is_selected { "> " } else { "  " };
            let label = row.section.label();
            let mut label_buf = String::with_capacity(prefix.len() + label_width);
            label_buf.push_str(prefix);
            label_buf.push_str(label);
            if label_buf.len() < label_width {
                label_buf.push_str(&" ".repeat(label_width - label_buf.len()));
            } else if label_buf.len() > label_width {
                label_buf.truncate(label_width);
            }

            let summary = row.summary.as_deref().unwrap_or("");
            let summary_style = if is_selected {
                base.add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text_dim())
            };
            let line = Line::from(vec![
                Span::styled(label_buf, base.add_modifier(Modifier::BOLD)),
                Span::styled(summary.to_string(), summary_style),
            ]);
            write_line(buf, area.x, y, max_width, &line, base);
        }
    }
}

impl<'a> BottomPaneView<'a> for SettingsOverviewView {
    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        let visible_rows = 12usize;
        if self.process_key_event(key_event, visible_rows) {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        // Ignore move events when the terminal doesn't support them.
        if matches!(mouse_event.kind, MouseEventKind::Moved) && area.width == 0 {
            return ConditionalUpdate::NoRedraw;
        }
        if self.handle_mouse_event_direct(mouse_event, area) {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let visible = self.rows.len().clamp(1, 12) as u16;
        // border (2) + header (1) + visible rows + footer (1)
        2 + 1 + visible + 1
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let style = PanelFrameStyle::bottom_pane().with_margin(Margin::new(1, 0));
        render_panel(area, buf, "Settings", style, |_content, buf| {
            let Some((header_area, list_area, footer_area)) = Self::visible_layout(area) else {
                return;
            };
            self.render_header(header_area, buf);
            self.render_list(list_area, buf);
            self.render_footer(footer_area, buf);
        });
    }
}
