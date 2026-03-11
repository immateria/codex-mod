use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::hints::{shortcut_line, KeyHint};
use crate::ui_interaction::{
    redraw_if,
    wrap_next,
    wrap_prev,
};
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::menu_rows::SettingsMenuRow;
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::toggle;
use super::BottomPane;
// TODO - This is currently unlinked here, on the official CODEX side, etc. figure out what to do later.
/// Interactive UI for GitHub workflow monitoring settings.
/// Shows token status and allows toggling the watcher on/off.
pub(crate) struct GithubSettingsView {
    watcher_enabled: bool,
    token_status: String,
    token_ready: bool,
    app_event_tx: AppEventSender,
    is_complete: bool,
    /// Selection index: 0 = toggle, 1 = close
    selected_row: usize,
}

impl GithubSettingsView {
    const TOGGLE_ROW: usize = 0;
    const CLOSE_ROW: usize = 1;
    const ROW_COUNT: usize = 2;

    pub fn new(watcher_enabled: bool, token_status: String, ready: bool, app_event_tx: AppEventSender) -> Self {
        Self {
            watcher_enabled,
            token_status,
            token_ready: ready,
            app_event_tx,
            is_complete: false,
            selected_row: 0,
        }
    }

    fn toggle(&mut self) {
        self.watcher_enabled = !self.watcher_enabled;
        self.app_event_tx
            .send(AppEvent::UpdateGithubWatcher(self.watcher_enabled));
    }

    fn activate_selected_row(&mut self) {
        if self.selected_row == Self::TOGGLE_ROW {
            self.toggle();
        } else {
            self.is_complete = true;
        }
    }

    fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "GitHub Settings",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            self.header_lines(),
            self.footer_lines(),
        )
    }

    fn header_lines(&self) -> Vec<Line<'static>> {
        let status_line = if self.token_ready {
            Line::from(vec![
                Span::styled("Status: ", Style::new().fg(colors::text_dim())),
                Span::styled("Ready", Style::new().fg(colors::success()).bold()),
                Span::raw("  "),
                Span::styled(self.token_status.clone(), Style::new().fg(colors::dim())),
            ])
        } else {
            Line::from(vec![
                Span::styled("Status: ", Style::new().fg(colors::text_dim())),
                Span::styled("No token", Style::new().fg(colors::warning()).bold()),
                Span::raw("  "),
                Span::styled(
                    "Set GH_TOKEN/GITHUB_TOKEN or run: 'gh auth login'",
                    Style::new().fg(colors::dim()),
                ),
            ])
        };

        vec![status_line, Line::from("")]
    }

    fn footer_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            shortcut_line(&[
                KeyHint::new("↑↓", " Navigate")
                    .with_key_style(Style::new().fg(colors::light_blue())),
                KeyHint::new("←→/Space", " Toggle")
                    .with_key_style(Style::new().fg(colors::success())),
                KeyHint::new("Enter", " Toggle/Close")
                    .with_key_style(Style::new().fg(colors::success())),
                KeyHint::new("Esc", " Cancel")
                    .with_key_style(Style::new().fg(colors::error())),
            ]),
        ]
    }

    fn menu_rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        vec![
            SettingsMenuRow::new(Self::TOGGLE_ROW, "Workflow Monitoring")
                .with_value(toggle::enabled_word(self.watcher_enabled)),
            SettingsMenuRow::new(Self::CLOSE_ROW, "Close"),
        ]
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                self.selected_row = wrap_prev(self.selected_row, Self::ROW_COUNT);
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                self.selected_row = wrap_next(self.selected_row, Self::ROW_COUNT);
                true
            }
            KeyEvent { code: KeyCode::Left | KeyCode::Right, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == Self::TOGGLE_ROW {
                    self.toggle();
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                self.activate_selected_row();
                true
            }
            KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == Self::TOGGLE_ROW {
                    self.toggle();
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }

    pub fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let page = self.page();
        let Some(layout) = page.layout(area) else {
            return false;
        };
        let rows = self.menu_rows();
        let Some(selected) = SettingsMenuPage::selection_menu_id_in_body(
            layout.body,
            mouse_event.column,
            mouse_event.row,
            0,
            &rows,
        ) else {
            return false;
        };
        self.selected_row = selected;
        if matches!(mouse_event.kind, crossterm::event::MouseEventKind::Down(_)) {
            self.activate_selected_row();
            return true;
        }
        true
    }

    pub fn is_view_complete(&self) -> bool {
        self.is_complete
    }
}

impl<'a> BottomPaneView<'a> for GithubSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_direct(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_direct(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_mouse_event_direct(mouse_event, area))
    }

    fn is_complete(&self) -> bool { self.is_complete }

    fn desired_height(&self, _width: u16) -> u16 { 9 }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.menu_rows();
        let _ = self
            .page()
            .render_menu_rows(area, buf, 0, Some(self.selected_row), &rows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use std::sync::mpsc::channel;

    fn left_click(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn github_content_hit_regions_map_to_toggle_and_close() {
        let (tx, _rx) = channel();
        let mut view = GithubSettingsView::new(false, "token missing".to_string(), false, AppEventSender::new(tx));
        let area = Rect::new(0, 0, 40, 9);
        let page = view.page();
        let layout = page.layout(area).expect("layout");
        let rows = view.menu_rows();
        assert_eq!(
            SettingsMenuPage::selection_menu_id_in_body(
                layout.body,
                layout.body.x,
                layout.body.y,
                0,
                &rows,
            ),
            Some(GithubSettingsView::TOGGLE_ROW)
        );
        assert!(view.handle_mouse_event_direct(left_click(layout.body.x, layout.body.y), area));
        assert_eq!(view.selected_row, GithubSettingsView::TOGGLE_ROW);
        assert_eq!(
            SettingsMenuPage::selection_menu_id_in_body(
                layout.body,
                layout.body.x,
                layout.body.y.saturating_add(1),
                0,
                &rows,
            ),
            Some(GithubSettingsView::CLOSE_ROW)
        );
        assert!(view.handle_mouse_event_direct(left_click(layout.body.x, layout.body.y.saturating_add(1)), area));
        assert_eq!(view.selected_row, GithubSettingsView::CLOSE_ROW);
    }
}
