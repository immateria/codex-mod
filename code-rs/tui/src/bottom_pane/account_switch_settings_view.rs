use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::BottomPane;

pub(crate) struct AccountSwitchSettingsView {
    app_event_tx: AppEventSender,
    selected_index: usize,
    auto_switch_enabled: bool,
    api_key_fallback_enabled: bool,
    is_complete: bool,
}

impl AccountSwitchSettingsView {
    pub(crate) fn new(
        app_event_tx: AppEventSender,
        auto_switch_enabled: bool,
        api_key_fallback_enabled: bool,
    ) -> Self {
        Self {
            app_event_tx,
            selected_index: 0,
            auto_switch_enabled,
            api_key_fallback_enabled,
            is_complete: false,
        }
    }

    fn option_count() -> usize {
        3
    }

    fn row_at_position(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        if x < area.x
            || x >= area.x.saturating_add(area.width)
            || y < area.y
            || y >= area.y.saturating_add(area.height)
        {
            return None;
        }
        let rel_y = y.saturating_sub(area.y);
        match rel_y {
            2 | 3 => Some(0),
            4 | 5 => Some(1),
            7 => Some(2),
            _ => None,
        }
    }

    fn toggle_auto_switch(&mut self) {
        self.auto_switch_enabled = !self.auto_switch_enabled;
        self.app_event_tx
            .send(AppEvent::SetAutoSwitchAccountsOnRateLimit(
                self.auto_switch_enabled,
            ));
    }

    fn toggle_api_key_fallback(&mut self) {
        self.api_key_fallback_enabled = !self.api_key_fallback_enabled;
        self.app_event_tx
            .send(AppEvent::SetApiKeyFallbackOnAllAccountsLimited(
                self.api_key_fallback_enabled,
            ));
    }

    fn close(&mut self) {
        self.is_complete = true;
    }

    fn activate_selected(&mut self) {
        match self.selected_index {
            0 => self.toggle_auto_switch(),
            1 => self.toggle_api_key_fallback(),
            2 => self.close(),
            _ => {}
        }
    }

    fn info_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        lines.push(Line::from(vec![Span::styled(
            "Accounts",
            Style::default().add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        let highlight = Style::default()
            .fg(colors::primary())
            .add_modifier(Modifier::BOLD);
        let normal = Style::default().fg(colors::text());
        let dim = Style::default().fg(colors::text_dim());

        let row = |idx: usize, label: &str, enabled: bool| -> Line<'static> {
            let selected = idx == self.selected_index;
            let indicator = if selected { ">" } else { " " };
            let style = if selected { highlight } else { normal };
            let state_style = if enabled {
                Style::default().fg(colors::success())
            } else {
                Style::default().fg(colors::text_dim())
            };
            Line::from(vec![
                Span::styled(format!("{indicator} "), style),
                Span::styled(label.to_string(), style),
                Span::raw("  "),
                Span::styled(
                    format!("[{}]", if enabled { "x" } else { " " }),
                    state_style,
                ),
            ])
        };

        lines.push(row(
            0,
            "Auto-switch on rate/usage limit",
            self.auto_switch_enabled,
        ));
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "Switches to another connected account on 429/usage_limit.",
                dim,
            ),
        ]));

        lines.push(row(
            1,
            "API key fallback when all accounts limited",
            self.api_key_fallback_enabled,
        ));
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "Only used if every connected ChatGPT account is limited.",
                dim,
            ),
        ]));

        lines.push(Line::from(""));

        let close_selected = self.selected_index == 2;
        let close_style = if close_selected { highlight } else { normal };
        let indicator = if close_selected { ">" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(format!("{indicator} "), close_style),
            Span::styled("Close", close_style),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(" Up/Down", Style::default().fg(colors::function())),
            Span::styled(" Navigate  ", dim),
            Span::styled("Enter", Style::default().fg(colors::success())),
            Span::styled(" Toggle  ", dim),
            Span::styled("Esc", Style::default().fg(colors::error())),
            Span::styled(" Close", dim),
        ]));

        lines
    }

    pub(crate) fn render_without_frame(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        Paragraph::new(self.info_lines())
            .wrap(Wrap { trim: true })
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .render(area, buf);
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc => self.close(),
            KeyCode::Up => {
                self.selected_index = self.selected_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Tab => {
                self.selected_index = (self.selected_index + 1) % Self::option_count();
            }
            KeyCode::BackTab => {
                if self.selected_index == 0 {
                    self.selected_index = Self::option_count() - 1;
                } else {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_selected(),
            _ => {}
        }
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match mouse_event.kind {
            MouseEventKind::Moved => {
                let Some(row) =
                    self.row_at_position(area, mouse_event.column, mouse_event.row)
                else {
                    return false;
                };
                if self.selected_index == row {
                    return false;
                }
                self.selected_index = row;
                true
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(row) =
                    self.row_at_position(area, mouse_event.column, mouse_event.row)
                else {
                    return false;
                };
                self.selected_index = row;
                self.activate_selected();
                true
            }
            MouseEventKind::ScrollUp => {
                if self.selected_index == 0 {
                    self.selected_index = Self::option_count() - 1;
                } else {
                    self.selected_index -= 1;
                }
                true
            }
            MouseEventKind::ScrollDown => {
                self.selected_index = (self.selected_index + 1) % Self::option_count();
                true
            }
            _ => false,
        }
    }

    pub(crate) fn is_view_complete(&self) -> bool {
        self.is_complete
    }
}

impl<'a> BottomPaneView<'a> for AccountSwitchSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        self.handle_key_event_direct(key_event);
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
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
        12
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_without_frame(area, buf);
    }
}
