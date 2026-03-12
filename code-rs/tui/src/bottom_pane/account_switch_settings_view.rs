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
use crate::colors;
use code_core::config_types::AuthCredentialsStoreMode;
use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::hints::{shortcut_line, KeyHint};
use super::settings_ui::line_runs::selection_id_at as selection_run_id_at;
use super::settings_ui::line_runs::SelectableLineRun;
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::menu_rows::{selection_id_at as selection_menu_id_at, SettingsMenuRow};
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::rows::StyledText;
use super::settings_ui::toggle;
use super::BottomPane;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewMode {
    Main,
    ConfirmStoreChange {
        target: AuthCredentialsStoreMode,
        selected: usize,
    },
}

pub(crate) struct AccountSwitchSettingsView {
    app_event_tx: AppEventSender,
    selected_index: usize,
    auto_switch_enabled: bool,
    api_key_fallback_enabled: bool,
    auth_credentials_store_mode: AuthCredentialsStoreMode,
    view_mode: ViewMode,
    is_complete: bool,
}

impl AccountSwitchSettingsView {
    const MAIN_OPTION_COUNT: usize = 6;

    const CONFIRM_OPTION_COUNT: usize = 3;

    pub(crate) fn new(
        app_event_tx: AppEventSender,
        auto_switch_enabled: bool,
        api_key_fallback_enabled: bool,
        auth_credentials_store_mode: AuthCredentialsStoreMode,
    ) -> Self {
        Self {
            app_event_tx,
            selected_index: 0,
            auto_switch_enabled,
            api_key_fallback_enabled,
            auth_credentials_store_mode,
            view_mode: ViewMode::Main,
            is_complete: false,
        }
    }

    fn auth_store_mode_label(mode: AuthCredentialsStoreMode) -> &'static str {
        match mode {
            AuthCredentialsStoreMode::File => "file",
            AuthCredentialsStoreMode::Keyring => "keyring",
            AuthCredentialsStoreMode::Auto => "auto",
            AuthCredentialsStoreMode::Ephemeral => "ephemeral",
        }
    }

    fn next_auth_store_mode(mode: AuthCredentialsStoreMode) -> AuthCredentialsStoreMode {
        match mode {
            AuthCredentialsStoreMode::File => AuthCredentialsStoreMode::Keyring,
            AuthCredentialsStoreMode::Keyring => AuthCredentialsStoreMode::Auto,
            AuthCredentialsStoreMode::Auto => AuthCredentialsStoreMode::Ephemeral,
            AuthCredentialsStoreMode::Ephemeral => AuthCredentialsStoreMode::File,
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

    fn show_login_accounts(&self) {
        self.app_event_tx.send(AppEvent::ShowLoginAccounts);
    }

    fn show_login_add_account(&self) {
        self.app_event_tx.send(AppEvent::ShowLoginAddAccount);
    }

    fn request_store_mode_change(&mut self, target: AuthCredentialsStoreMode, migrate_existing: bool) {
        self.app_event_tx.send(AppEvent::RequestSetAuthCredentialsStoreMode {
            mode: target,
            migrate_existing,
        });
    }

    fn open_store_mode_confirm(&mut self) {
        let target = Self::next_auth_store_mode(self.auth_credentials_store_mode);
        self.view_mode = ViewMode::ConfirmStoreChange { target, selected: 0 };
    }

    fn activate_selected_main(&mut self) {
        match self.selected_index {
            0 => self.toggle_auto_switch(),
            1 => self.toggle_api_key_fallback(),
            2 => self.open_store_mode_confirm(),
            3 => self.show_login_accounts(),
            4 => self.show_login_add_account(),
            5 => self.close(),
            _ => {}
        }
    }

    fn confirm_selected_index(&self) -> usize {
        match self.view_mode {
            ViewMode::ConfirmStoreChange { selected, .. } => selected,
            ViewMode::Main => 0,
        }
    }

    fn set_confirm_selected_index(&mut self, selected: usize) {
        if let ViewMode::ConfirmStoreChange { target, .. } = self.view_mode {
            self.view_mode = ViewMode::ConfirmStoreChange { target, selected };
        }
    }

    fn activate_selected_confirm(&mut self) {
        let ViewMode::ConfirmStoreChange { target, selected } = self.view_mode else {
            return;
        };

        match selected {
            0 => {
                self.request_store_mode_change(target, true);
                self.view_mode = ViewMode::Main;
            }
            1 => {
                self.request_store_mode_change(target, false);
                self.view_mode = ViewMode::Main;
            }
            2 => {
                self.view_mode = ViewMode::Main;
            }
            _ => {}
        }
    }

    fn main_page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Accounts",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            Vec::new(),
            vec![shortcut_line(&[
                KeyHint::new("↑↓/Tab", " navigate").with_key_style(Style::new().fg(colors::function())),
                KeyHint::new("Enter/Space", " activate").with_key_style(Style::new().fg(colors::success())),
                KeyHint::new("Esc", " close").with_key_style(Style::new().fg(colors::error()).bold()),
            ])],
        )
    }

    fn main_runs(&self, selected_id: Option<usize>) -> Vec<SelectableLineRun<'static, usize>> {
        let bool_value = |enabled: bool| toggle::checkbox_marker(enabled);

        let mut runs = Vec::new();

        let mut auto = SettingsMenuRow::new(0usize, "Auto-switch on rate/usage limit")
            .with_value(bool_value(self.auto_switch_enabled))
            .with_selected_hint("Enter to toggle")
            .into_run(selected_id);
        auto.lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "Switches to another connected account on 429/usage_limit.",
                Style::new().fg(colors::text_dim()),
            ),
        ]));
        runs.push(auto);

        let mut fallback = SettingsMenuRow::new(1usize, "API key fallback when all accounts limited")
            .with_value(bool_value(self.api_key_fallback_enabled))
            .with_selected_hint("Enter to toggle")
            .into_run(selected_id);
        fallback.lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "Only used if every connected ChatGPT account is limited.",
                Style::new().fg(colors::text_dim()),
            ),
        ]));
        runs.push(fallback);

        let store_mode = Self::auth_store_mode_label(self.auth_credentials_store_mode);
        let store_detail = match self.auth_credentials_store_mode {
            AuthCredentialsStoreMode::Ephemeral => {
                "In-memory only (will not persist across restarts)."
            }
            _ => "Where Code stores CLI auth credentials (auth.json payload).",
        };
        let mut store = SettingsMenuRow::new(2usize, "Credential store")
            .with_value(StyledText::new(
                format!("[{store_mode}]"),
                Style::new().fg(colors::primary()).bold(),
            ))
            .with_selected_hint("Enter to change")
            .into_run(selected_id);
        store.lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(store_detail, Style::new().fg(colors::text_dim())),
        ]));
        runs.push(store);

        runs.push(SelectableLineRun::plain(vec![Line::from("")]));

        let mut manage = SettingsMenuRow::new(3usize, "Manage connected accounts")
            .with_selected_hint("Enter to open")
            .into_run(selected_id);
        manage.lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "View, switch, and remove stored accounts.",
                Style::new().fg(colors::text_dim()),
            ),
        ]));
        runs.push(manage);

        let mut add = SettingsMenuRow::new(4usize, "Add account")
            .with_selected_hint("Enter to open")
            .into_run(selected_id);
        add.lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "Start ChatGPT or API-key account setup.",
                Style::new().fg(colors::text_dim()),
            ),
        ]));
        runs.push(add);

        runs.push(SelectableLineRun::plain(vec![Line::from("")]));

        runs.push(
            SettingsMenuRow::new(5usize, "Close")
                .with_selected_hint("Enter to close")
                .into_run(selected_id),
        );

        runs
    }

    fn confirm_page(&self, target: AuthCredentialsStoreMode) -> SettingsMenuPage<'static> {
        let current = Self::auth_store_mode_label(self.auth_credentials_store_mode);
        let next = Self::auth_store_mode_label(target);
        let header_lines = vec![
            Line::from(vec![
                Span::styled("Current: ", Style::new().fg(colors::text_dim())),
                Span::styled(current, Style::new().fg(colors::text())),
                Span::styled("   New: ", Style::new().fg(colors::text_dim())),
                Span::styled(next, Style::new().fg(colors::primary()).bold()),
            ]),
            Line::from(""),
        ];
        let footer_lines = vec![shortcut_line(&[
            KeyHint::new("↑↓/Tab", " select").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter/Space", " apply").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " back").with_key_style(Style::new().fg(colors::error()).bold()),
        ])];

        SettingsMenuPage::new(
            "Credential store",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            header_lines,
            footer_lines,
        )
    }

    fn confirm_rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        vec![
            SettingsMenuRow::new(0usize, "Apply + migrate existing credentials"),
            SettingsMenuRow::new(1usize, "Apply (do not migrate)  (may log you out)"),
            SettingsMenuRow::new(2usize, "Cancel"),
        ]
    }

    pub(crate) fn render_without_frame(&self, area: Rect, buf: &mut Buffer) {
        match self.view_mode {
            ViewMode::Main => {
                let page = self.main_page();
                let runs = self.main_runs(Some(self.selected_index));
                let _ = page.content_only().render_runs(area, buf, 0, &runs);
            }
            ViewMode::ConfirmStoreChange { target, selected } => {
                let page = self.confirm_page(target);
                let rows = self.confirm_rows();
                let _ = page
                    .content_only()
                    .render_menu_rows(area, buf, 0, Some(selected), &rows);
            }
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match self.view_mode {
            ViewMode::Main => match key_event.code {
                KeyCode::Esc => {
                    self.close();
                    true
                }
                KeyCode::Up => {
                    self.selected_index =
                        wrap_prev(self.selected_index, Self::MAIN_OPTION_COUNT);
                    true
                }
                KeyCode::Down | KeyCode::Tab => {
                    self.selected_index =
                        wrap_next(self.selected_index, Self::MAIN_OPTION_COUNT);
                    true
                }
                KeyCode::BackTab => {
                    self.selected_index =
                        wrap_prev(self.selected_index, Self::MAIN_OPTION_COUNT);
                    true
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.activate_selected_main();
                    true
                }
                _ => false,
            },
            ViewMode::ConfirmStoreChange { .. } => match key_event.code {
                KeyCode::Esc => {
                    self.view_mode = ViewMode::Main;
                    true
                }
                KeyCode::Up => {
                    let next = wrap_prev(
                        self.confirm_selected_index(),
                        Self::CONFIRM_OPTION_COUNT,
                    );
                    self.set_confirm_selected_index(next);
                    true
                }
                KeyCode::Down | KeyCode::Tab => {
                    let next = wrap_next(
                        self.confirm_selected_index(),
                        Self::CONFIRM_OPTION_COUNT,
                    );
                    self.set_confirm_selected_index(next);
                    true
                }
                KeyCode::BackTab => {
                    let next = wrap_prev(
                        self.confirm_selected_index(),
                        Self::CONFIRM_OPTION_COUNT,
                    );
                    self.set_confirm_selected_index(next);
                    true
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.activate_selected_confirm();
                    true
                }
                _ => false,
            },
        }
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match self.view_mode {
            ViewMode::Main => {
                let page = self.main_page();
                let runs = self.main_runs(None);
                let Some(layout) = page.content_only().layout(area) else {
                    return false;
                };
                let mut selected = self.selected_index;
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    Self::MAIN_OPTION_COUNT,
                    |x, y| selection_run_id_at(layout.body, x, y, 0, &runs),
                    SelectableListMouseConfig {
                        hover_select: false,
                        scroll_select: false,
                        ..SelectableListMouseConfig::default()
                    },
                );
                self.selected_index = selected;

                if matches!(result, SelectableListMouseResult::Activated) {
                    self.activate_selected_main();
                }
                result.handled()
            }
            ViewMode::ConfirmStoreChange { target, .. } => {
                let page = self.confirm_page(target);
                let rows = self.confirm_rows();
                let Some(layout) = page.content_only().layout(area) else {
                    return false;
                };
                let mut selected = self.confirm_selected_index();
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    Self::CONFIRM_OPTION_COUNT,
                    |x, y| selection_menu_id_at(layout.body, x, y, 0, &rows),
                    SelectableListMouseConfig {
                        hover_select: false,
                        scroll_select: false,
                        ..SelectableListMouseConfig::default()
                    },
                );
                self.set_confirm_selected_index(selected);

                if matches!(result, SelectableListMouseResult::Activated) {
                    self.activate_selected_confirm();
                }
                result.handled()
            }
        }
    }

    pub(crate) fn is_view_complete(&self) -> bool {
        self.is_complete
    }
}

impl<'a> BottomPaneView<'a> for AccountSwitchSettingsView {
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

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        match self.view_mode {
            ViewMode::Main => 18,
            ViewMode::ConfirmStoreChange { .. } => 10,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_without_frame(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use std::sync::mpsc::channel;

    fn mouse_left_click(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn mouse_click_on_main_row_toggles_auto_switch() {
        let (tx, rx) = channel();
        let mut view = AccountSwitchSettingsView::new(
            AppEventSender::new(tx),
            false,
            false,
            AuthCredentialsStoreMode::File,
        );
        let area = Rect::new(0, 0, 80, 18);
        let layout = view.main_page().content_only().layout(area).expect("layout");

        assert!(view.handle_mouse_event_direct(
            mouse_left_click(layout.body.x + 1, layout.body.y),
            area,
        ));
        assert_eq!(view.selected_index, 0);
        assert!(view.auto_switch_enabled);
        match rx.recv().expect("auto-switch event") {
            AppEvent::SetAutoSwitchAccountsOnRateLimit(enabled) => assert!(enabled),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
