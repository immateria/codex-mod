use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use super::super::limits_overlay::{LimitsOverlay, LimitsOverlayContent};
use super::SettingsContent;
use crate::util::buffer::fill_rect;
use unicode_width::UnicodeWidthStr;

pub(crate) struct LimitsSettingsContent {
    overlay: LimitsOverlay,
}

impl LimitsSettingsContent {
    pub(crate) fn new(content: LimitsOverlayContent) -> Self {
        Self {
            overlay: LimitsOverlay::new(content),
        }
    }

    pub(crate) fn set_content(&mut self, content: LimitsOverlayContent) {
        self.overlay.set_content(content);
    }

    fn render_tabs(&self, area: Rect, buf: &mut Buffer) {
        use ratatui::widgets::Paragraph;

        if area.width == 0 || area.height == 0 {
            return;
        }

        if let Some(tabs) = self.overlay.tabs() {
            let mut spans = Vec::new();
            for (idx, tab) in tabs.iter().enumerate() {
                let selected = idx == self.overlay.selected_tab();
                let style = if selected {
                    Style::default()
                        .fg(crate::colors::text())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(crate::colors::text_dim())
                };
                spans.push(Span::styled(format!(" {} ", tab.title), style));
                spans.push(Span::raw(" "));
            }
            Paragraph::new(Line::from(spans))
                .style(Style::default().bg(crate::colors::background()))
                .render(area, buf);
        }
    }

    fn render_body(&self, area: Rect, buf: &mut Buffer) {
        use ratatui::widgets::Paragraph;
        use ratatui::widgets::Wrap;

        if area.width == 0 || area.height == 0 {
            self.overlay.set_visible_rows(0);
            self.overlay.set_max_scroll(0);
            return;
        }

        self.overlay.set_visible_rows(area.height);

        let lines = self.overlay.lines_for_width(area.width);
        let max_scroll = lines.len().saturating_sub(area.height as usize) as u16;
        self.overlay.set_max_scroll(max_scroll);

        let start = self.overlay.scroll() as usize;
        let end = (start + area.height as usize).min(lines.len());
        let viewport = if start < end {
            lines[start..end].to_vec()
        } else {
            Vec::new()
        };

        Paragraph::new(viewport)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(area, buf);
    }

    fn render_hint_row(&self, area: Rect, buf: &mut Buffer) {
        use ratatui::widgets::Paragraph;

        if area.width == 0 || area.height == 0 {
            return;
        }

        let hint_style = Style::default().fg(crate::colors::text_dim());
        let accent_style = Style::default().fg(crate::colors::function());
        let line = Line::from(vec![
            Span::styled("↑↓", accent_style),
            Span::styled(" scroll  ", hint_style),
            Span::styled("PgUp/PgDn", accent_style),
            Span::styled(" page  ", hint_style),
            Span::styled("◂ ▸", accent_style),
            Span::styled(" change tab", hint_style),
        ]);

        Paragraph::new(line)
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text_dim()))
            .render(area, buf);
    }

    fn tab_at(&self, area: Rect, mouse_event: MouseEvent) -> Option<usize> {
        if self.overlay.tab_count() <= 1 {
            return None;
        }
        if area.width == 0 || area.height == 0 {
            return None;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(2), Constraint::Fill(1)])
            .split(area);
        let tabs_area = chunks[1];
        if tabs_area.width == 0 || tabs_area.height == 0 {
            return None;
        }
        if mouse_event.column < tabs_area.x
            || mouse_event.column >= tabs_area.x.saturating_add(tabs_area.width)
            || mouse_event.row < tabs_area.y
            || mouse_event.row >= tabs_area.y.saturating_add(tabs_area.height)
        {
            return None;
        }

        let tabs = self.overlay.tabs()?;
        let mut x = tabs_area.x;
        for (idx, tab) in tabs.iter().enumerate() {
            let tab_width = UnicodeWidthStr::width(tab.title.as_str()) as u16 + 2;
            let start = x;
            let end = start.saturating_add(tab_width);
            if mouse_event.column >= start && mouse_event.column < end {
                return Some(idx);
            }
            x = end.saturating_add(1);
            if x >= tabs_area.x.saturating_add(tabs_area.width) {
                break;
            }
        }
        None
    }
}

impl SettingsContent for LimitsSettingsContent {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            self.overlay.set_visible_rows(0);
            self.overlay.set_max_scroll(0);
            return;
        }

        fill_rect(
            buf,
            area,
            Some(' '),
            Style::default().bg(crate::colors::background()),
        );

        let has_tabs = self.overlay.tab_count() > 1;
        let constraints = if has_tabs {
            vec![Constraint::Length(1), Constraint::Length(2), Constraint::Fill(1)]
        } else {
            vec![Constraint::Length(1), Constraint::Fill(1)]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        let hint_area = chunks[0];
        let (tabs_area, body_area) = if has_tabs {
            (Some(chunks[1]), chunks[2])
        } else {
            (None, chunks[1])
        };

        self.render_hint_row(hint_area, buf);

        if let Some(tabs_rect) = tabs_area {
            self.render_tabs(tabs_rect, buf);
        }

        self.render_body(body_area, buf);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => {
                let current = self.overlay.scroll();
                if current > 0 {
                    self.overlay.set_scroll(current - 1);
                }
                true
            }
            KeyCode::Down => {
                let current = self.overlay.scroll();
                let next = current.saturating_add(1).min(self.overlay.max_scroll());
                self.overlay.set_scroll(next);
                true
            }
            KeyCode::PageUp => {
                let step = self.overlay.visible_rows().max(1);
                let current = self.overlay.scroll();
                self.overlay.set_scroll(current.saturating_sub(step));
                true
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                let step = self.overlay.visible_rows().max(1);
                let current = self.overlay.scroll();
                let next = current.saturating_add(step).min(self.overlay.max_scroll());
                self.overlay.set_scroll(next);
                true
            }
            KeyCode::Home => {
                self.overlay.set_scroll(0);
                true
            }
            KeyCode::End => {
                self.overlay.set_scroll(self.overlay.max_scroll());
                true
            }
            KeyCode::Left | KeyCode::Char('[') => self.overlay.select_prev_tab(),
            KeyCode::Right | KeyCode::Char(']') => self.overlay.select_next_tab(),
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.overlay.select_prev_tab()
                } else {
                    self.overlay.select_next_tab()
                }
            }
            KeyCode::BackTab => self.overlay.select_prev_tab(),
            _ => false,
        }
    }

    fn is_complete(&self) -> bool {
        false
    }

    fn handle_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(tab_idx) = self.tab_at(area, mouse_event) else {
                    return false;
                };
                self.overlay.select_tab(tab_idx)
            }
            MouseEventKind::ScrollUp => {
                let current = self.overlay.scroll();
                if current > 0 {
                    self.overlay.set_scroll(current - 1);
                    true
                } else {
                    false
                }
            }
            MouseEventKind::ScrollDown => {
                let current = self.overlay.scroll();
                let next = current.saturating_add(1).min(self.overlay.max_scroll());
                if next != current {
                    self.overlay.set_scroll(next);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
