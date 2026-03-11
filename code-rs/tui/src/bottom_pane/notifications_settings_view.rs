use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
    wrap_next,
    wrap_prev,
};
use crate::chatwidget::BackgroundOrderTicket;
use crate::colors;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::hints::{shortcut_line, KeyHint};
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::menu_rows::{selection_id_at as selection_menu_id_at, SettingsMenuRow};
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::rows::StyledText;
use super::settings_ui::toggle;
use super::BottomPane;

#[derive(Clone)]
pub(crate) enum NotificationsMode {
    Toggle { enabled: bool },
    Custom { entries: Vec<String> },
}

pub(crate) struct NotificationsSettingsView {
    mode: NotificationsMode,
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    selected_row: usize,
    is_complete: bool,
}

impl NotificationsSettingsView {
    const SELECTABLE_ROWS: usize = 2;

    pub fn new(
        mode: NotificationsMode,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) -> Self {
        Self {
            mode,
            app_event_tx,
            ticket,
            selected_row: 0,
            is_complete: false,
        }
    }

    fn toggle(&mut self) {
        match &mut self.mode {
            NotificationsMode::Toggle { enabled } => {
                *enabled = !*enabled;
                self.app_event_tx
                    .send(AppEvent::UpdateTuiNotifications(*enabled));
            }
            NotificationsMode::Custom { entries } => {
                let filters = if entries.is_empty() {
                    "(none)".to_string()
                } else {
                    entries.join(", ")
                };
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    format!(
                        "TUI notifications are filtered in config: [{filters}]"
                    ),
                );
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    "Edit ~/.code/config.toml [tui].notifications to change filters.".to_string(),
                );
            }
        }
    }

    fn status_line(&self) -> Line<'static> {
        match &self.mode {
            NotificationsMode::Toggle { enabled } => {
                let mut status = toggle::enabled_word_warning_off(*enabled);
                status.style = status.style.bold();
                Line::from(vec![
                    Span::styled("Status: ", Style::new().fg(colors::text_dim())),
                    Span::styled(status.text, status.style),
                ])
            }
            NotificationsMode::Custom { entries } => {
                let filters = if entries.is_empty() {
                    "<none>".to_string()
                } else {
                    entries.join(", ")
                };
                Line::from(vec![
                    Span::styled("Status: ", Style::new().fg(colors::text_dim())),
                    Span::styled("Custom filter", Style::new().fg(colors::info()).bold()),
                    Span::raw("  "),
                    Span::styled(filters, Style::new().fg(colors::dim())),
                ])
            }
        }
    }

    fn page(&self) -> SettingsMenuPage<'static> {
        let footer_lines = match &self.mode {
            NotificationsMode::Toggle { .. } => vec![shortcut_line(&[
                KeyHint::new("↑↓", " navigate").with_key_style(Style::new().fg(colors::function())),
                KeyHint::new("←→/Space", " toggle").with_key_style(Style::new().fg(colors::success())),
                KeyHint::new("Enter", " toggle/close").with_key_style(Style::new().fg(colors::success())),
                KeyHint::new("Esc", " close").with_key_style(Style::new().fg(colors::error()).bold()),
            ])],
            NotificationsMode::Custom { .. } => vec![Line::from(vec![
                Span::styled("Edit ", Style::new().fg(colors::text_dim())),
                Span::styled("[tui].notifications", Style::new().fg(colors::info())),
                Span::styled(
                    " in ~/.code/config.toml to adjust filters.",
                    Style::new().fg(colors::text_dim()),
                ),
            ])],
        };

        SettingsMenuPage::new(
            "Notifications",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            vec![self.status_line(), Line::from("")],
            footer_lines,
        )
    }

    fn menu_rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        let notifications_row = match &self.mode {
            NotificationsMode::Toggle { enabled } => {
                let mut status = toggle::enabled_word_warning_off(*enabled);
                status.style = status.style.bold();
                SettingsMenuRow::new(0usize, "Notifications").with_value(status)
            }
            NotificationsMode::Custom { entries } => {
                let filters = if entries.is_empty() {
                    "<none>".to_string()
                } else {
                    entries.join(", ")
                };
                SettingsMenuRow::new(0usize, "Notifications")
                    .with_value(StyledText::new(
                        "Custom filter".to_string(),
                        Style::new().fg(colors::info()).bold(),
                    ))
                    .with_detail(StyledText::new(filters, Style::new().fg(colors::dim())))
            }
        };

        vec![notifications_row, SettingsMenuRow::new(1usize, "Close")]
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                self.selected_row = wrap_prev(self.selected_row, Self::SELECTABLE_ROWS);
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                self.selected_row = wrap_next(self.selected_row, Self::SELECTABLE_ROWS);
                true
            }
            KeyEvent { code: KeyCode::Left | KeyCode::Right, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == 0 {
                    self.toggle();
                }
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == 0 {
                    self.toggle();
                } else {
                    self.is_complete = true;
                }
                true
            }
            KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == 0 {
                    self.toggle();
                }
                true
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mut selected = self.selected_row;
        let rows = self.menu_rows();
        let Some(layout) = self.page().layout(area) else {
            return false;
        };
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            rows.len(),
            |x, y| {
                selection_menu_id_at(layout.body, x, y, 0, &rows)
            },
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.selected_row = selected;

        if matches!(result, SelectableListMouseResult::Activated) {
            if self.selected_row == 0 {
                self.toggle();
            } else {
                self.is_complete = true;
            }
        }
        result.handled()
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.process_key_event(key_event)
    }
}

impl<'a> BottomPaneView<'a> for NotificationsSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.process_key_event(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.process_key_event(key_event))
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        9
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let page = self.page();
        let rows = self.menu_rows();
        let _ = page.render_menu_rows(area, buf, 0, Some(self.selected_row), &rows);
    }
}
