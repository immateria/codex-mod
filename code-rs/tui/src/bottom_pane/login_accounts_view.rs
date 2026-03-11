use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use base64::Engine;
use chrono::{DateTime, Utc};
use code_core::auth;
use code_core::auth_accounts::{self, StoredAccount};
use code_core::config::{load_config_as_toml, set_account_store_paths};
use code_login::AuthMode;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Margin, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use textwrap::Options as TwOptions;

use crate::account_label::{account_display_label, account_mode_priority};
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
use serde_json::Value as JsonValue;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use crate::components::form_text_field::FormTextField;
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    wrap_next,
    wrap_prev,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use super::BottomPane;
use super::settings_ui::action_page::SettingsActionPage;
use super::settings_ui::buttons::{standard_button_specs, SettingsButtonKind, StandardButtonSpec};
use super::settings_ui::editor_page::SettingsEditorPage;
use super::settings_ui::form_page::{SettingsFormPage, SettingsFormSection};
use super::settings_ui::hints::{status_and_shortcuts, title_line, KeyHint};
use super::settings_ui::list_detail_page::{SettingsListDetailMode, SettingsListDetailPage};
use super::settings_ui::message_page::SettingsMessagePage;
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::menu_rows::SettingsMenuRow;
use super::settings_ui::panel::SettingsPanelStyle;

const CHATGPT_REFRESH_INTERVAL_DAYS: i64 = 28;
const ACCOUNTS_TWO_PANE_MIN_WIDTH: u16 = 96;
const ACCOUNTS_TWO_PANE_MIN_HEIGHT: u16 = 10;
const ACCOUNTS_LIST_PANE_PERCENT: u16 = 42;

/// Interactive view shown for `/login` to manage stored accounts.
pub(crate) struct LoginAccountsView {
    state: Rc<RefCell<LoginAccountsState>>,
}

impl LoginAccountsView {
    pub fn new(
        code_home: PathBuf,
        app_event_tx: AppEventSender,
        tail_ticket: BackgroundOrderTicket,
        auth_credentials_store_mode: auth::AuthCredentialsStoreMode,
    ) -> (Self, Rc<RefCell<LoginAccountsState>>) {
        let state = Rc::new(RefCell::new(LoginAccountsState::new(
            code_home,
            app_event_tx,
            tail_ticket,
            auth_credentials_store_mode,
        )));
        (Self { state: state.clone() }, state)
    }

    fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        let mut state = self.state.borrow_mut();
        let mut handled = state.handle_key_event(key_event);
        if state.should_close() {
            state.set_complete();
            handled = true;
        }
        handled
    }

    fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.state.borrow_mut().handle_mouse_event(mouse_event, area)
    }
}

impl<'a> BottomPaneView<'a> for LoginAccountsView {
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
        self.state.borrow().is_complete
    }

    fn desired_height(&self, width: u16) -> u16 {
        let state = self.state.borrow();
        state.desired_height(width)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let state = self.state.borrow();
        state.render(area, buf);
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        let mut state = self.state.borrow_mut();
        state.handle_paste(text)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AccountRow {
    id: String,
    label: String,
    detail_items: Vec<String>,
    mode: AuthMode,
    is_active: bool,
}

#[derive(Clone, Debug)]
struct Feedback {
    message: String,
    is_error: bool,
}

#[derive(Debug)]
enum ViewMode {
    List,
    ConfirmRemove { account_id: String },
    EditStorePaths(Box<StorePathEditorState>),
}

#[derive(Debug)]
struct StorePathEditorState {
    selected_row: usize,
    read_paths_field: FormTextField,
    write_path_field: FormTextField,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorePathEditorAction {
    Save,
    Cancel,
}

impl StorePathEditorState {
    fn new(read_paths_text: &str, write_path_text: &str) -> Self {
        let mut read_paths_field = FormTextField::new_multi_line();
        read_paths_field.set_placeholder("auth_accounts.json\nlegacy/auth_accounts.json");
        read_paths_field.set_text(read_paths_text);

        let mut write_path_field = FormTextField::new_single_line();
        write_path_field.set_placeholder("auth_accounts.json");
        write_path_field.set_text(write_path_text);

        Self {
            selected_row: 0,
            read_paths_field,
            write_path_field,
        }
    }

    fn parse_read_paths(&self) -> Vec<String> {
        self.read_paths_field
            .text()
            .lines()
            .flat_map(|line| line.split(','))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(std::string::ToString::to_string)
            .collect()
    }

    fn write_path(&self) -> Option<String> {
        let trimmed = self.write_path_field.text().trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

pub(crate) struct LoginAccountsState {
    code_home: PathBuf,
    app_event_tx: AppEventSender,
    tail_ticket: BackgroundOrderTicket,
    auth_credentials_store_mode: auth::AuthCredentialsStoreMode,
    accounts: Vec<AccountRow>,
    active_account_id: Option<String>,
    selected: usize,
    mode: ViewMode,
    feedback: Option<Feedback>,
    is_complete: bool,
}

impl LoginAccountsState {
    fn new(
        code_home: PathBuf,
        app_event_tx: AppEventSender,
        tail_ticket: BackgroundOrderTicket,
        auth_credentials_store_mode: auth::AuthCredentialsStoreMode,
    ) -> Self {
        let mut state = Self {
            code_home,
            app_event_tx,
            tail_ticket,
            auth_credentials_store_mode,
            accounts: Vec::new(),
            active_account_id: None,
            selected: 0,
            mode: ViewMode::List,
            feedback: None,
            is_complete: false,
        };
        state.sync_account_store_from_auth();
        state.reload_accounts();
        state
    }

    fn send_tail(&self, message: impl Into<String>) {
        self.app_event_tx
            .send_background_event_with_ticket(&self.tail_ticket, message);
    }

    pub fn weak_handle(state: &Rc<RefCell<Self>>) -> Weak<RefCell<Self>> {
        Rc::downgrade(state)
    }

    fn reload_accounts(&mut self) {
        let previously_selected_id = self
            .accounts
            .get(self.selected)
            .map(|row| row.id.clone());

        match auth_accounts::list_accounts(&self.code_home) {
            Ok(raw_accounts) => {
                let active_id = auth_accounts::get_active_account_id(&self.code_home).ok().flatten();
                self.active_account_id = active_id.clone();
                self.accounts = raw_accounts
                    .into_iter()
                    .map(|account| AccountRow::from(account, active_id.as_deref()))
                    .collect();

                self.accounts.sort_by(|a, b| {
                    let priority = account_mode_priority;
                    let a_priority = priority(a.mode);
                    let b_priority = priority(b.mode);
                    a_priority
                        .cmp(&b_priority)
                        .then_with(|| a.label.to_ascii_lowercase().cmp(&b.label.to_ascii_lowercase()))
                        .then_with(|| a.label.cmp(&b.label))
                        .then_with(|| a.id.cmp(&b.id))
                });

                let mut selected_idx = previously_selected_id
                    .and_then(|id| self.accounts.iter().position(|row| row.id == id))
                    .or_else(|| {
                        active_id
                            .as_ref()
                            .and_then(|id| self.accounts.iter().position(|row| &row.id == id))
                    });

                if self.accounts.is_empty() {
                    self.selected = 0;
                } else {
                    if selected_idx.is_none() {
                        selected_idx = Some(0);
                    }
                    self.selected = selected_idx.unwrap_or(0).min(self.accounts.len() - 1);
                }
            }
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to read accounts: {err}"),
                    is_error: true,
                });
                self.accounts.clear();
                self.selected = 0;
                self.active_account_id = None;
            }
        }
    }

    fn sync_account_store_from_auth(&mut self) {
        let auth_json = match auth::load_auth_dot_json(&self.code_home, self.auth_credentials_store_mode)
        {
            Ok(Some(auth)) => auth,
            Ok(None) => return,
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to read current auth: {err}"),
                    is_error: true,
                });
                return;
            }
        };

        if let Some(tokens) = auth_json.tokens.clone() {
            let last_refresh = auth_json.last_refresh.unwrap_or_else(Utc::now);
            let email = tokens.id_token.email.clone();
            if let Err(err) = auth_accounts::upsert_chatgpt_account(
                &self.code_home,
                tokens,
                last_refresh,
                email,
                true,
            ) {
                self.feedback = Some(Feedback {
                    message: format!("Failed to record ChatGPT login: {err}"),
                    is_error: true,
                });
            }
            return;
        }

        if let Some(api_key) = auth_json.openai_api_key.as_ref()
            && let Err(err) = auth_accounts::upsert_api_key_account(
                &self.code_home,
                api_key.clone(),
                None,
                true,
            ) {
                self.feedback = Some(Feedback {
                    message: format!("Failed to record API key login: {err}"),
                    is_error: true,
                });
            }
    }

    fn panel_style() -> SettingsPanelStyle {
        SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0))
    }

    fn accounts_page(&self) -> SettingsListDetailPage<'static> {
        SettingsListDetailPage::new(
            "Manage Accounts",
            Self::panel_style(),
            self.account_header_lines().len(),
            self.account_footer_lines().len(),
            ACCOUNTS_TWO_PANE_MIN_WIDTH,
            ACCOUNTS_TWO_PANE_MIN_HEIGHT,
            ACCOUNTS_LIST_PANE_PERCENT,
            "Accounts",
            "Details",
        )
    }

    pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::List);
        match mode {
            ViewMode::List => {
                self.mode = ViewMode::List;
                self.handle_list_key(key_event)
            }
            ViewMode::ConfirmRemove { account_id } => {
                self.mode = ViewMode::ConfirmRemove { account_id };
                self.handle_confirm_remove_key(key_event)
            }
            ViewMode::EditStorePaths(mut editor) => {
                let (keep_open, handled) =
                    self.handle_store_paths_editor_key(key_event, &mut editor);
                if keep_open {
                    self.mode = ViewMode::EditStorePaths(editor);
                } else {
                    self.mode = ViewMode::List;
                }
                handled
            }
        }
    }

    pub(crate) fn handle_mouse_event(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::List);
        match mode {
            ViewMode::List => {
                self.mode = ViewMode::List;
                self.handle_list_mouse(mouse_event, area)
            }
            ViewMode::ConfirmRemove { account_id } => {
                self.mode = ViewMode::ConfirmRemove { account_id };
                self.handle_confirm_remove_mouse(mouse_event, area)
            }
            ViewMode::EditStorePaths(mut editor) => {
                let (keep_open, handled) =
                    self.handle_store_paths_editor_mouse(mouse_event, area, &mut editor);
                if keep_open {
                    self.mode = ViewMode::EditStorePaths(editor);
                } else {
                    self.mode = ViewMode::List;
                }
                handled
            }
        }
    }

    fn selectable_row_count(&self) -> usize {
        self.accounts.len().saturating_add(2)
    }

    fn select_previous_row(&mut self) {
        let total_rows = self.selectable_row_count();
        self.selected = wrap_prev(self.selected, total_rows);
    }

    fn select_next_row(&mut self) {
        let total_rows = self.selectable_row_count();
        self.selected = wrap_next(self.selected, total_rows);
    }

    fn activate_selected_row(&mut self) {
        let account_count = self.accounts.len();
        if self.selected < account_count {
            if let Some(account) = self.accounts.get(self.selected) {
                let label = account.label.clone();
                let mode = account.mode;
                if self.activate_account(account.id.clone(), mode) {
                    self.mode = ViewMode::List;
                    self.send_tail(format!("Switched to {label}"));
                    self.is_complete = true;
                }
            }
        } else if self.selected == account_count {
            self.is_complete = true;
            self.app_event_tx.send(AppEvent::ShowLoginAddAccount);
        } else {
            self.open_store_paths_editor();
        }
    }

    fn list_hit_area_for_mouse(&self, area: Rect) -> Option<Rect> {
        let layout = self.accounts_page().layout(area)?;
        match layout.mode {
            SettingsListDetailMode::Split { list_inner, .. } => Some(list_inner),
            SettingsListDetailMode::Compact { content } => Some(content),
        }
    }

    fn list_selection_for_position(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        if x < area.x
            || x >= area.x.saturating_add(area.width)
            || y < area.y
            || y >= area.y.saturating_add(area.height)
        {
            return None;
        }
        let rel_y = y.saturating_sub(area.y);

        if self.accounts.is_empty() {
            match rel_y {
                3 => Some(self.add_account_index()),
                4 => Some(self.store_paths_index()),
                _ => None,
            }
        } else {
            let account_count = self.accounts.len() as u16;
            if rel_y < account_count {
                Some(rel_y as usize)
            } else if rel_y == account_count.saturating_add(2) {
                Some(self.add_account_index())
            } else if rel_y == account_count.saturating_add(3) {
                Some(self.store_paths_index())
            } else {
                None
            }
        }
    }

    fn handle_list_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let Some(list_area) = self.list_hit_area_for_mouse(area) else {
            return false;
        };

        let mut selected = self.selected;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.selectable_row_count(),
            |x, y| self.list_selection_for_position(list_area, x, y),
            SelectableListMouseConfig {
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );
        self.selected = selected;

        if matches!(result, SelectableListMouseResult::Activated) {
            self.activate_selected_row();
        }
        result.handled()
    }

    fn handle_confirm_remove_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(list_area) = self.list_hit_area_for_mouse(area) else {
                    return false;
                };
                if self
                    .list_selection_for_position(list_area, mouse_event.column, mouse_event.row)
                    .is_some()
                {
                    self.mode = ViewMode::List;
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn handle_list_key(&mut self, key_event: KeyEvent) -> bool {
        let account_count = self.accounts.len();

        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.is_complete = true;
                true
            }
            KeyCode::Up => {
                self.select_previous_row();
                true
            }
            KeyCode::Down => {
                self.select_next_row();
                true
            }
            KeyCode::Char('d') => {
                if self.selected < account_count
                    && let Some(account) = self.accounts.get(self.selected) {
                        self.mode = ViewMode::ConfirmRemove { account_id: account.id.clone() };
                        return true;
                    }
                false
            }
            KeyCode::Char('r') => {
                self.reload_accounts();
                true
            }
            KeyCode::Char('p') => {
                self.open_store_paths_editor();
                true
            }
            KeyCode::Enter => {
                self.activate_selected_row();
                true
            }
            _ => false,
        }
    }

    fn handle_confirm_remove_key(&mut self, key_event: KeyEvent) -> bool {
        let account_id = if let ViewMode::ConfirmRemove { account_id } = &self.mode {
            account_id.clone()
        } else {
            return false;
        };

        match key_event.code {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.mode = ViewMode::List;
                true
            }
            KeyCode::Enter | KeyCode::Char('y') => {
                self.remove_account(account_id);
                true
            }
            _ => false,
        }
    }

    fn load_store_path_inputs(&self) -> (String, String) {
        let mut read_paths = vec!["auth_accounts.json".to_string()];
        let mut write_path = "auth_accounts.json".to_string();

        if let Ok(root) = load_config_as_toml(&self.code_home)
            && let Some(accounts) = root.get("accounts").and_then(|value| value.as_table())
        {
            if let Some(values) = accounts.get("read_paths").and_then(|value| value.as_array())
                {
                    let parsed = values
                        .iter()
                        .filter_map(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<_>>();
                    if !parsed.is_empty() {
                        read_paths = parsed;
                    }
                }

            if let Some(value) = accounts.get("write_path").and_then(|value| value.as_str()) {
                    let trimmed = value.trim();
                    if !trimmed.is_empty() {
                        write_path = trimmed.to_string();
                    }
                }
        }

        (read_paths.join("\n"), write_path)
    }

    fn open_store_paths_editor(&mut self) {
        let (read_paths, write_path) = self.load_store_path_inputs();
        self.mode = ViewMode::EditStorePaths(Box::new(StorePathEditorState::new(
            &read_paths,
            &write_path,
        )));
    }

    fn save_store_paths_editor(&mut self, editor: &StorePathEditorState) -> bool {
        let read_paths = editor.parse_read_paths();
        let write_path = editor.write_path();

        match set_account_store_paths(&self.code_home, &read_paths, write_path.as_deref()) {
            Ok(()) => {
                self.feedback = Some(Feedback {
                    message: "Account store paths updated".to_string(),
                    is_error: false,
                });
                self.reload_accounts();
                true
            }
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to save account store paths: {err}"),
                    is_error: true,
                });
                false
            }
        }
    }

    fn handle_store_paths_editor_key(
        &mut self,
        key_event: KeyEvent,
        editor: &mut StorePathEditorState,
    ) -> (bool, bool) {
        const ROW_COUNT: usize = 4;
        match key_event.code {
            KeyCode::Esc => (false, true),
            KeyCode::Up => {
                if editor.selected_row == 0 {
                    editor.selected_row = ROW_COUNT - 1;
                } else {
                    editor.selected_row = editor.selected_row.saturating_sub(1);
                }
                (true, true)
            }
            KeyCode::Down | KeyCode::Tab => {
                editor.selected_row = (editor.selected_row + 1) % ROW_COUNT;
                (true, true)
            }
            KeyCode::BackTab => {
                if editor.selected_row == 0 {
                    editor.selected_row = ROW_COUNT - 1;
                } else {
                    editor.selected_row = editor.selected_row.saturating_sub(1);
                }
                (true, true)
            }
            KeyCode::Enter => match editor.selected_row {
                0 | 1 => {
                    editor.selected_row = (editor.selected_row + 1) % ROW_COUNT;
                    (true, true)
                }
                2 => {
                    if self.save_store_paths_editor(editor) {
                        (false, true)
                    } else {
                        (true, true)
                    }
                }
                3 => (false, true),
                _ => (true, false),
            },
            KeyCode::Char('s') | KeyCode::Char('S') if editor.selected_row >= 2 => {
                if self.save_store_paths_editor(editor) {
                    (false, true)
                } else {
                    (true, true)
                }
            }
            _ => match editor.selected_row {
                0 => {
                    (true, editor.read_paths_field.handle_key(key_event))
                }
                1 => {
                    (true, editor.write_path_field.handle_key(key_event))
                }
                _ => (true, false),
            },
        }
    }

    fn activate_account(&mut self, account_id: String, mode: AuthMode) -> bool {
        match auth::activate_account_with_store_mode(
            &self.code_home,
            &account_id,
            self.auth_credentials_store_mode,
        ) {
            Ok(()) => {
                self.feedback = Some(Feedback {
                    message: if mode.is_chatgpt() {
                        "ChatGPT account selected".to_string()
                    } else {
                        "API key selected".to_string()
                    },
                    is_error: false,
                });
                self.reload_accounts();
                self.app_event_tx
                    .send(AppEvent::LoginUsingChatGptChanged { using_chatgpt_auth: mode.is_chatgpt() });
                true
            }
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to activate account: {err}"),
                    is_error: true,
                });
                false
            }
        }
    }

    fn remove_account(&mut self, account_id: String) {
        match auth_accounts::remove_account(&self.code_home, &account_id) {
            Ok(Some(_)) => {
                let removed_active = self
                    .active_account_id
                    .as_ref()
                    .is_some_and(|id| id == &account_id);
                if removed_active {
                    let _ =
                        auth::logout_with_store_mode(&self.code_home, self.auth_credentials_store_mode);
                }
                self.feedback = Some(Feedback {
                    message: "Account disconnected".to_string(),
                    is_error: false,
                });
                self.mode = ViewMode::List;
                self.reload_accounts();
                let using_chatgpt = self
                    .active_account_id
                    .as_ref()
                    .and_then(|id| auth_accounts::find_account(&self.code_home, id).ok().flatten())
                    .map(|acc| acc.mode.is_chatgpt())
                    .unwrap_or(false);
                self.app_event_tx
                    .send(AppEvent::LoginUsingChatGptChanged { using_chatgpt_auth: using_chatgpt });
            }
            Ok(None) => {
                self.feedback = Some(Feedback {
                    message: "Account no longer exists".to_string(),
                    is_error: true,
                });
                self.mode = ViewMode::List;
                self.reload_accounts();
            }
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to remove account: {err}"),
                    is_error: true,
                });
                self.mode = ViewMode::List;
            }
        }
    }

    pub(crate) fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        let mode = std::mem::replace(&mut self.mode, ViewMode::List);
        match mode {
            ViewMode::EditStorePaths(mut editor) => {
                match editor.selected_row {
                    0 => editor.read_paths_field.handle_paste(text),
                    1 => editor.write_path_field.handle_paste(text),
                    _ => {}
                }
                self.mode = ViewMode::EditStorePaths(editor);
                ConditionalUpdate::NeedsRedraw
            }
            other => {
                self.mode = other;
                ConditionalUpdate::NoRedraw
            }
        }
    }

    fn desired_height(&self, _width: u16) -> u16 {
        const MIN_HEIGHT: usize = 9;
        if matches!(self.mode, ViewMode::EditStorePaths(_)) {
            return 18;
        }
        let content_lines = self.content_line_count();
        let total = content_lines + 2; // account for top/bottom borders
        total.max(MIN_HEIGHT) as u16
    }

    fn content_line_count(&self) -> usize {
        let mut lines = 0usize;

        if self.feedback.is_some() {
            lines += 2; // message + blank spacer
        }

        lines += 2; // heading + blank spacer after heading

        if self.accounts.is_empty() {
            lines += 1;
        } else {
            lines += self.accounts.len();
        }

        lines += 1; // blank before add row
        lines += 1; // add account row
        lines += 1; // account store paths row
        lines += 2; // blank + key hints row

        if matches!(self.mode, ViewMode::ConfirmRemove { .. }) {
            lines += 3; // blank, question, instruction
        }

        lines
    }

    fn add_account_index(&self) -> usize {
        self.accounts.len()
    }

    fn store_paths_index(&self) -> usize {
        self.add_account_index().saturating_add(1)
    }

    fn is_confirm_remove_mode(&self) -> bool {
        matches!(self.mode, ViewMode::ConfirmRemove { .. })
    }

    fn account_header_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if let Some(feedback) = &self.feedback {
            let style = if feedback.is_error {
                Style::default()
                    .fg(crate::colors::error())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(crate::colors::success())
                    .add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(vec![Span::styled(feedback.message.clone(), style)]));
        }

        let status = if self.accounts.is_empty() {
            "No connected accounts".to_string()
        } else {
            format!(
                "{} connected {}",
                self.accounts.len(),
                if self.accounts.len() == 1 {
                    "account"
                } else {
                    "accounts"
                }
            )
        };
        lines.push(Line::from(vec![
            Span::styled(
                "Connected Accounts",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(status, Style::default().fg(crate::colors::text_dim())),
        ]));
        lines
    }

    fn account_footer_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(crate::colors::function())),
            Span::styled(" Navigate  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::styled(" Select  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled(
                "d",
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Disconnect  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled(
                "p",
                Style::default()
                    .fg(crate::colors::info())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Paths  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(crate::colors::error())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Close", Style::default().fg(crate::colors::text_dim())),
        ]));

        if self.is_confirm_remove_mode() {
            lines.push(Line::from(vec![
                Span::styled(
                    "Disconnect selected account?",
                    Style::default()
                        .fg(crate::colors::warning())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  Enter confirms, Esc cancels.",
                    Style::default().fg(crate::colors::text_dim()),
                ),
            ]));
        }

        lines
    }

    fn account_mode_badge(mode: AuthMode) -> &'static str {
        if mode.is_chatgpt() {
            "ChatGPT"
        } else {
            "API Key"
        }
    }

    fn render_account_list_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if self.accounts.is_empty() {
            lines.push(Line::from(Span::styled(
                "No accounts connected yet.",
                Style::default().fg(crate::colors::text_dim()),
            )));
        } else {
            for (idx, account) in self.accounts.iter().enumerate() {
                let selected = idx == self.selected;
                let arrow_style = if selected {
                    Style::default().fg(crate::colors::primary())
                } else {
                    Style::default().fg(crate::colors::text_dim())
                };
                let label_style = if selected {
                    Style::default()
                        .fg(crate::colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else if account.is_active {
                    Style::default()
                        .fg(crate::colors::success())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(crate::colors::text())
                };

                let mut spans = vec![
                    Span::styled(if selected { "› " } else { "  " }, arrow_style),
                    Span::styled(account.label.clone(), label_style),
                    Span::styled(
                        format!("  [{}]", Self::account_mode_badge(account.mode)),
                        Style::default().fg(crate::colors::text_dim()),
                    ),
                ];

                if account.is_active {
                    spans.push(Span::styled(
                        " (current)",
                        Style::default()
                            .fg(crate::colors::success())
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                lines.push(Line::from(spans));
            }
        }

        let add_selected = self.selected == self.add_account_index();
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Actions",
            Style::default().add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(vec![
            Span::styled(
                if add_selected { "› " } else { "  " },
                if add_selected {
                    Style::default().fg(crate::colors::primary())
                } else {
                    Style::default().fg(crate::colors::text_dim())
                },
            ),
            Span::styled(
                "Add account…",
                if add_selected {
                    Style::default()
                        .fg(crate::colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(crate::colors::text())
                },
            ),
        ]));

        let store_selected = self.selected == self.store_paths_index();
        lines.push(Line::from(vec![
            Span::styled(
                if store_selected { "› " } else { "  " },
                if store_selected {
                    Style::default().fg(crate::colors::primary())
                } else {
                    Style::default().fg(crate::colors::text_dim())
                },
            ),
            Span::styled(
                "Account store paths…",
                if store_selected {
                    Style::default()
                        .fg(crate::colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(crate::colors::text())
                },
            ),
        ]));

        lines
    }

    fn render_selected_details_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        if let Some(account) = self.accounts.get(self.selected) {
            lines.push(Line::from(vec![Span::styled(
                "Selected Account",
                Style::default().add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(vec![
                Span::styled(
                    account.label.clone(),
                    Style::default()
                        .fg(crate::colors::primary())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  [{}]", Self::account_mode_badge(account.mode)),
                    Style::default().fg(crate::colors::text_dim()),
                ),
                Span::styled(
                    if account.is_active { "  (current)" } else { "" },
                    Style::default()
                        .fg(crate::colors::success())
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(""));

            if account.detail_items.is_empty() {
                lines.push(Line::from(Span::styled(
                    "No metadata available for this account.",
                    Style::default().fg(crate::colors::text_dim()),
                )));
            } else {
                for item in &account.detail_items {
                    lines.push(Line::from(vec![
                        Span::styled("• ", Style::default().fg(crate::colors::text_dim())),
                        Span::styled(item.clone(), Style::default().fg(crate::colors::text_dim())),
                    ]));
                }
            }
        } else if self.selected == self.add_account_index() {
            lines.push(Line::from(vec![Span::styled(
                "Add Account",
                Style::default().add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(Span::styled(
                "Connect another ChatGPT or API-key account.",
                Style::default().fg(crate::colors::text_dim()),
            )));
            lines.push(Line::from(Span::styled(
                "Useful when your current account is near usage limits.",
                Style::default().fg(crate::colors::text_dim()),
            )));
        } else {
            lines.push(Line::from(vec![Span::styled(
                "Account Store Paths",
                Style::default().add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(Span::styled(
                "Control where account records are loaded from and saved to.",
                Style::default().fg(crate::colors::text_dim()),
            )));
            lines.push(Line::from(Span::styled(
                "Supports multiple read paths with a dedicated write target.",
                Style::default().fg(crate::colors::text_dim()),
            )));
        }

        lines
    }

    fn render_accounts_compact(&self, area: Rect, buf: &mut Buffer) {
        let list_lines = self.render_account_list_lines();
        let list_height = (list_lines.len()).min(area.height as usize) as u16;
        let base = Style::default().bg(crate::colors::background()).fg(crate::colors::text());

        Paragraph::new(list_lines)
            .alignment(Alignment::Left)
            .style(base)
            .render(
                Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: list_height,
                },
                buf,
            );

        let remaining_height = area.height.saturating_sub(list_height);
        if remaining_height == 0 {
            return;
        }

        let mut detail_lines = Vec::new();
        detail_lines.push(Line::from(""));
        detail_lines.extend(self.render_selected_details_lines());
        Paragraph::new(detail_lines)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left)
            .style(base)
            .render(
                Rect {
                    x: area.x,
                    y: area.y.saturating_add(list_height),
                    width: area.width,
                    height: remaining_height,
                },
                buf,
            );
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        if let ViewMode::EditStorePaths(editor) = &self.mode {
            self.render_store_paths_editor(area, buf, editor.as_ref());
            return;
        }

        let header_lines = self.account_header_lines();
        let footer_lines = self.account_footer_lines();
        let page = self.accounts_page();
        let Some(layout) = page.render(area, buf) else {
            return;
        };
        let base = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());

        Paragraph::new(header_lines)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left)
            .style(base)
            .render(layout.header, buf);

        match layout.mode {
            SettingsListDetailMode::Split {
                list_inner,
                detail_inner,
                ..
            } => {
                Paragraph::new(self.render_account_list_lines())
                    .alignment(Alignment::Left)
                    .style(base)
                    .render(list_inner, buf);

                Paragraph::new(self.render_selected_details_lines())
                    .wrap(Wrap { trim: true })
                    .alignment(Alignment::Left)
                    .style(base)
                    .render(detail_inner, buf);
            }
            SettingsListDetailMode::Compact { content } => {
                self.render_accounts_compact(content, buf);
            }
        }

        Paragraph::new(footer_lines)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left)
            .style(base)
            .render(layout.footer, buf);
    }

    fn render_store_paths_editor(&self, area: Rect, buf: &mut Buffer, editor: &StorePathEditorState) {
        let page = self.store_paths_editor_form_page();
        let buttons = self.store_paths_editor_button_specs(editor.selected_row);
        let Some(_layout) = page.render_with_standard_actions_end(
            area,
            buf,
            &[&editor.read_paths_field, &editor.write_path_field],
            &buttons,
        ) else {
            return;
        };
    }

    fn store_paths_editor_page(&self) -> SettingsActionPage<'static> {
        let header_lines = vec![
            title_line("Account Store Paths"),
            Line::from("Set where account records are read/written."),
            Line::from(""),
        ];
        let status = self.feedback.as_ref().map(|feedback| {
            let style = if feedback.is_error {
                Style::new().fg(crate::colors::error()).bold()
            } else {
                Style::new().fg(crate::colors::success()).bold()
            };
            super::settings_ui::rows::StyledText::new(feedback.message.clone(), style)
        });
        let footer_lines = status_and_shortcuts(
            status,
            &[
                KeyHint::new("Tab", " next"),
                KeyHint::new("S", " save")
                    .with_key_style(Style::new().fg(crate::colors::success()).bold()),
                KeyHint::new("Esc", " back")
                    .with_key_style(Style::new().fg(crate::colors::error()).bold()),
            ],
        );

        SettingsActionPage::new("Manage Accounts", Self::panel_style(), header_lines, footer_lines)
            .with_action_rows(1)
            .with_min_body_rows(6)
    }

    fn store_paths_editor_form_page(&self) -> SettingsFormPage<'static> {
        SettingsFormPage::new(
            self.store_paths_editor_page(),
            vec![
                SettingsFormSection::new(
                    "Read paths (one per line)",
                    false,
                    Constraint::Length(4),
                ),
                SettingsFormSection::new("Write path", false, Constraint::Length(1)),
            ],
        )
        .with_section_gap_rows(1)
    }

    fn store_paths_editor_button_specs(
        &self,
        selected_row: usize,
    ) -> Vec<StandardButtonSpec<StorePathEditorAction>> {
        standard_button_specs(
            &[
                (StorePathEditorAction::Save, SettingsButtonKind::Save),
                (StorePathEditorAction::Cancel, SettingsButtonKind::Cancel),
            ],
            match selected_row {
                2 => Some(StorePathEditorAction::Save),
                3 => Some(StorePathEditorAction::Cancel),
                _ => None,
            },
            None,
        )
    }

    fn handle_store_paths_editor_mouse(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        editor: &mut StorePathEditorState,
    ) -> (bool, bool) {
        let page = self.store_paths_editor_form_page();
        let Some(layout) = page.layout(area) else {
            return (true, false);
        };
        let buttons = self.store_paths_editor_button_specs(editor.selected_row);

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(action) = page.standard_action_at_end(
                    &layout,
                    mouse_event.column,
                    mouse_event.row,
                    &buttons,
                ) {
                    return match action {
                        StorePathEditorAction::Save => {
                            if self.save_store_paths_editor(editor) {
                                (false, true)
                            } else {
                                (true, true)
                            }
                        }
                        StorePathEditorAction::Cancel => (false, true),
                    };
                }
                if let Some(section_idx) =
                    page.field_index_at(&layout, mouse_event.column, mouse_event.row)
                {
                    match section_idx {
                        0 => {
                            editor.selected_row = 0;
                            return (
                                true,
                                editor.read_paths_field.handle_mouse_click(
                                    mouse_event.column,
                                    mouse_event.row,
                                    layout.sections[0].inner,
                                ),
                            );
                        }
                        1 => {
                            editor.selected_row = 1;
                            return (
                                true,
                                editor.write_path_field.handle_mouse_click(
                                    mouse_event.column,
                                    mouse_event.row,
                                    layout.sections[1].inner,
                                ),
                            );
                        }
                        _ => {}
                    }
                }
                (true, false)
            }
            MouseEventKind::ScrollUp => {
                let pos = ratatui::layout::Position {
                    x: mouse_event.column,
                    y: mouse_event.row,
                };
                if layout.sections[0].outer.contains(pos) {
                    editor.selected_row = 0;
                    return (true, editor.read_paths_field.handle_mouse_scroll(false));
                }
                if layout.sections[1].outer.contains(pos) {
                    editor.selected_row = 1;
                    return (true, editor.write_path_field.handle_mouse_scroll(false));
                }
                (true, false)
            }
            MouseEventKind::ScrollDown => {
                let pos = ratatui::layout::Position {
                    x: mouse_event.column,
                    y: mouse_event.row,
                };
                if layout.sections[0].outer.contains(pos) {
                    editor.selected_row = 0;
                    return (true, editor.read_paths_field.handle_mouse_scroll(true));
                }
                if layout.sections[1].outer.contains(pos) {
                    editor.selected_row = 1;
                    return (true, editor.write_path_field.handle_mouse_scroll(true));
                }
                (true, false)
            }
            _ => (true, false),
        }
    }

    fn should_close(&self) -> bool {
        self.is_complete
    }

    fn set_complete(&mut self) {
        self.is_complete = true;
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn clear_complete(&mut self) {
        self.is_complete = false;
    }
}

fn wrap_url_segments(url: &str, available_width: u16) -> Vec<String> {
    let width = available_width.max(1) as usize;
    let mut opts = TwOptions::new(width);
    opts.break_words = true;
    textwrap::wrap(url, opts)
        .into_iter()
        .map(std::borrow::Cow::into_owned)
        .collect()
}

pub(crate) struct LoginAddAccountView {
    state: Rc<RefCell<LoginAddAccountState>>,
}

impl LoginAddAccountView {
    pub fn new(
        code_home: PathBuf,
        app_event_tx: AppEventSender,
        tail_ticket: BackgroundOrderTicket,
        auth_credentials_store_mode: auth::AuthCredentialsStoreMode,
    ) -> (Self, Rc<RefCell<LoginAddAccountState>>) {
        let state = Rc::new(RefCell::new(LoginAddAccountState::new(
            code_home,
            app_event_tx,
            tail_ticket,
            auth_credentials_store_mode,
        )));
        (Self { state: state.clone() }, state)
    }

    fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.state.borrow_mut().handle_key_event(key_event)
    }
}

impl<'a> BottomPaneView<'a> for LoginAddAccountView {
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

    fn is_complete(&self) -> bool {
        self.state.borrow().is_complete()
    }

    fn desired_height(&self, _width: u16) -> u16 {
        self.state.borrow().desired_height() as u16
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.state.borrow().render(area, buf);
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        self.state.borrow_mut().handle_paste(text)
    }
}

#[derive(Debug)]
enum AddStep {
    Choose { selected: usize },
    ApiKey { field: FormTextField },
    Waiting { auth_url: Option<String> },
    DeviceCode(DeviceCodeState),
}

#[derive(Debug)]
struct DeviceCodeState {
    authorize_url: Option<String>,
    user_code: Option<String>,
    status: DeviceCodeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceCodeStatus {
    Generating,
    WaitingForApproval,
}

pub(crate) struct LoginAddAccountState {
    code_home: PathBuf,
    app_event_tx: AppEventSender,
    tail_ticket: BackgroundOrderTicket,
    auth_credentials_store_mode: auth::AuthCredentialsStoreMode,
    step: AddStep,
    feedback: Option<Feedback>,
    is_complete: bool,
}

impl LoginAddAccountState {
    fn new(
        code_home: PathBuf,
        app_event_tx: AppEventSender,
        tail_ticket: BackgroundOrderTicket,
        auth_credentials_store_mode: auth::AuthCredentialsStoreMode,
    ) -> Self {
        Self {
            code_home,
            app_event_tx,
            tail_ticket,
            auth_credentials_store_mode,
            step: AddStep::Choose { selected: 0 },
            feedback: None,
            is_complete: false,
        }
    }

    fn send_tail(&self, message: impl Into<String>) {
        self.app_event_tx
            .send_background_event_with_ticket(&self.tail_ticket, message);
    }

    pub fn weak_handle(state: &Rc<RefCell<Self>>) -> Weak<RefCell<Self>> {
        Rc::downgrade(state)
    }

    pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) -> bool {
        match &mut self.step {
            AddStep::Choose { selected } => match key_event.code {
                KeyCode::Esc => {
                    self.finish_and_show_accounts();
                    true
                }
                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                    *selected = if *selected == 0 { 1 } else { 0 };
                    true
                }
                KeyCode::Enter => {
                    if *selected == 0 {
                        self.feedback = Some(Feedback {
                            message: "Opening browser for ChatGPT sign-in…".to_string(),
                            is_error: false,
                        });
                        self.step = AddStep::Waiting { auth_url: None };
                        self.app_event_tx.send(AppEvent::LoginStartChatGpt);
                    } else {
                        self.feedback = None;
                        self.step = AddStep::ApiKey { field: FormTextField::new_single_line() };
                    }
                    true
                }
                _ => false,
            },
            AddStep::ApiKey { field } => match key_event.code {
                KeyCode::Esc => {
                    self.finish_and_show_accounts();
                    true
                }
                KeyCode::Enter => {
                    let key = field.text().trim().to_string();
                    if key.is_empty() {
                        self.feedback = Some(Feedback {
                            message: "API key cannot be empty".to_string(),
                            is_error: true,
                        });
                    } else {
                        match auth::login_with_api_key_with_store_mode(
                            &self.code_home,
                            &key,
                            self.auth_credentials_store_mode,
                        ) {
                            Ok(()) => {
                                self.feedback = Some(Feedback {
                                    message: "API key connected".to_string(),
                                    is_error: false,
                                });
                                self.send_tail("Added API key account".to_string());
                                self.app_event_tx
                                    .send(AppEvent::LoginUsingChatGptChanged { using_chatgpt_auth: false });
                                self.finish_and_show_accounts();
                            }
                            Err(err) => {
                                self.feedback = Some(Feedback {
                                    message: format!("Failed to store API key: {err}"),
                                    is_error: true,
                                });
                            }
                        }
                    }
                    true
                }
                _ => field.handle_key(key_event),
            },
            AddStep::Waiting { .. } => match key_event.code {
                KeyCode::Esc => {
                    self.app_event_tx.send(AppEvent::LoginCancelChatGpt);
                    true
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    self.feedback = Some(Feedback {
                        message: "Switching to code authentication…".to_string(),
                        is_error: false,
                    });
                    self.step = AddStep::DeviceCode(DeviceCodeState::generating());
                    self.app_event_tx.send(AppEvent::LoginStartDeviceCode);
                    true
                }
                _ => false,
            },
            AddStep::DeviceCode(_) => {
                if matches!(key_event.code, KeyCode::Esc) {
                    self.app_event_tx.send(AppEvent::LoginCancelChatGpt);
                    true
                } else {
                    false
                }
            }
        }
    }

    pub(crate) fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        if let AddStep::ApiKey { field } = &mut self.step {
            field.handle_paste(text);
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    fn desired_height(&self) -> usize {
        let mut lines = 5; // title + spacing baseline
        if self.feedback.is_some() {
            lines += 2;
        }

        match &self.step {
            AddStep::Choose { .. } => {
                lines += 4; // options + spacing
            }
            AddStep::ApiKey { .. } => {
                lines += 4; // instructions + input + spacing
            }
            AddStep::Waiting { auth_url } => {
                lines += 3; // instructions + cancel hint
                if auth_url.is_some() {
                    lines += 1;
                }
            }
            AddStep::DeviceCode(_) => {
                lines += 6;
            }
        }

        lines.max(10) + 2
    }

    fn feedback_styled_text(&self) -> Option<super::settings_ui::rows::StyledText<'static>> {
        self.feedback.as_ref().map(|feedback| {
            let style = if feedback.is_error {
                Style::new().fg(crate::colors::error()).bold()
            } else {
                Style::new().fg(crate::colors::success()).bold()
            };
            super::settings_ui::rows::StyledText::new(feedback.message.clone(), style)
        })
    }

    fn footer_status_and_shortcuts(&self, hints: &[KeyHint<'_>]) -> Vec<Line<'static>> {
        status_and_shortcuts(self.feedback_styled_text(), hints)
    }

    fn auth_progress_message_page(
        &self,
        body_lines: Vec<Line<'static>>,
        footer_lines: Vec<Line<'static>>,
    ) -> SettingsMessagePage<'static> {
        SettingsMessagePage::new(
            "Add Account",
            LoginAccountsState::panel_style(),
            Vec::new(),
            body_lines,
            footer_lines,
        )
        .with_min_body_rows(3)
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        match &self.step {
            AddStep::Choose { selected } => {
                let header_lines = vec![Line::from("Choose how you’d like to add an account:")];
                let footer_lines = self.footer_status_and_shortcuts(&[
                    KeyHint::new("↑↓", " navigate").with_key_style(Style::new().fg(crate::colors::function())),
                    KeyHint::new("Enter", " select").with_key_style(Style::new().fg(crate::colors::success())),
                    KeyHint::new("Esc", " back").with_key_style(Style::new().fg(crate::colors::error()).bold()),
                ]);
                let page = SettingsMenuPage::new(
                    "Add Account",
                    LoginAccountsState::panel_style(),
                    header_lines,
                    footer_lines,
                );
                let rows = vec![
                    SettingsMenuRow::new(0usize, "ChatGPT sign-in"),
                    SettingsMenuRow::new(1usize, "API key"),
                ];
                let _ = page.render_menu_rows(area, buf, 0, Some(*selected), &rows);
            }
            AddStep::ApiKey { field } => {
                let pre_lines = vec![Line::from("Paste your OpenAI API key:")];
                let post_lines = self.footer_status_and_shortcuts(&[
                    KeyHint::new("Enter", " save")
                        .with_key_style(Style::new().fg(crate::colors::success())),
                    KeyHint::new("Esc", " cancel")
                        .with_key_style(Style::new().fg(crate::colors::error()).bold()),
                ]);
                let page = SettingsEditorPage::new(
                    "Add Account",
                    LoginAccountsState::panel_style(),
                    "OpenAI API key",
                    pre_lines,
                    post_lines,
                );
                let _ = page.render(area, buf, field);
            }
            AddStep::Waiting { auth_url } => {
                let footer_lines = self.footer_status_and_shortcuts(&[
                    KeyHint::new("Esc", " cancel login")
                        .with_key_style(Style::new().fg(crate::colors::error()).bold()),
                ]);
                let content_width = self
                    .auth_progress_message_page(Vec::new(), footer_lines.clone())
                    .layout(area)
                    .map(|layout| layout.body.width.max(1))
                    .unwrap_or(area.width.max(1));
                let mut body_lines = vec![
                    Line::from("Finish signing in with ChatGPT in your browser."),
                    Line::from(vec![
                        Span::styled("Not seeing a browser? ", Style::default().fg(crate::colors::text_dim())),
                        Span::styled(
                            "Press C to switch to code authentication.",
                            Style::default().fg(crate::colors::primary()),
                        ),
                    ]),
                ];
                if let Some(url) = auth_url {
                    for chunk in wrap_url_segments(url, content_width) {
                        body_lines.push(Line::from(vec![Span::styled(
                            chunk,
                            Style::default().fg(crate::colors::primary()),
                        )]));
                    }
                }
                let page = self.auth_progress_message_page(body_lines, footer_lines);
                let _ = page.render(area, buf);
            }
            AddStep::DeviceCode(state) => {
                let footer_lines = self.footer_status_and_shortcuts(&[
                    KeyHint::new("Esc", " cancel login")
                        .with_key_style(Style::new().fg(crate::colors::error()).bold()),
                ]);
                let content_width = self
                    .auth_progress_message_page(Vec::new(), footer_lines.clone())
                    .layout(area)
                    .map(|layout| layout.body.width.max(1))
                    .unwrap_or(area.width.max(1));
                let mut body_lines = vec![Line::from("Complete sign-in using a verification code.")];
                match state.status {
                    DeviceCodeStatus::Generating => {
                        body_lines.push(Line::from("Generating a secure code and link…"));
                    }
                    DeviceCodeStatus::WaitingForApproval => {
                        if let Some(code) = &state.user_code {
                            body_lines.push(Line::from(vec![
                                Span::styled("Code: ", Style::default().fg(crate::colors::text_dim())),
                                Span::styled(
                                    code.clone(),
                                    Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD),
                                ),
                            ]));
                        }
                        if let Some(url) = &state.authorize_url {
                            body_lines.push(Line::from("Visit this link on any device:"));
                            for chunk in wrap_url_segments(url, content_width) {
                                body_lines.push(Line::from(vec![Span::styled(
                                    chunk,
                                    Style::default().fg(crate::colors::info()),
                                )]));
                            }
                        }
                        body_lines.push(Line::from("Keep this code private. It expires after 15 minutes."));
                    }
                }
                let page = self.auth_progress_message_page(body_lines, footer_lines);
                let _ = page.render(area, buf);
            }
        }
    }

    pub fn acknowledge_chatgpt_started(&mut self, auth_url: String) {
        self.step = AddStep::Waiting { auth_url: Some(auth_url) };
        self.feedback = Some(Feedback {
            message: "Browser opened. Complete sign-in to finish.".to_string(),
            is_error: false,
        });
    }

    pub fn acknowledge_chatgpt_failed(&mut self, error: String) {
        self.step = AddStep::Choose { selected: 0 };
        self.feedback = Some(Feedback { message: error, is_error: true });
    }

    pub fn begin_device_code_flow(&mut self) {
        if !matches!(self.step, AddStep::DeviceCode(_)) {
            self.step = AddStep::DeviceCode(DeviceCodeState::generating());
        }
        self.feedback = Some(Feedback {
            message: "Use the on-screen code to finish signing in.".to_string(),
            is_error: false,
        });
    }

    pub fn set_device_code_ready(&mut self, authorize_url: String, user_code: String) {
        self.step = AddStep::DeviceCode(DeviceCodeState::with_details(authorize_url, user_code));
        self.feedback = Some(Feedback {
            message: "Enter the code in your browser to continue.".to_string(),
            is_error: false,
        });
    }

    pub fn on_device_code_failed(&mut self, error: String) {
        self.step = AddStep::Choose { selected: 0 };
        self.feedback = Some(Feedback { message: error, is_error: true });
    }

    pub fn on_chatgpt_complete(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
        self.feedback = Some(Feedback { message: "ChatGPT account connected".to_string(), is_error: false });
        self.send_tail("ChatGPT account connected".to_string());
                self.app_event_tx
                    .send(AppEvent::LoginUsingChatGptChanged { using_chatgpt_auth: true });
                self.finish_and_show_accounts();
            }
            Err(err) => {
                self.step = AddStep::Choose { selected: 0 };
                self.feedback = Some(Feedback { message: err, is_error: true });
            }
        }
    }

    pub fn cancel_active_flow(&mut self) {
        let message = match self.step {
            AddStep::DeviceCode(_) => "Cancelled code authentication",
            AddStep::Waiting { .. } => "Cancelled ChatGPT login",
            _ => "Cancelled login",
        };
        self.step = AddStep::Choose { selected: 0 };
        self.feedback = Some(Feedback { message: message.to_string(), is_error: false });
    }

    fn finish_and_show_accounts(&mut self) {
        self.is_complete = true;
        self.app_event_tx.send(AppEvent::ShowLoginAccounts);
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn clear_complete(&mut self) {
        self.is_complete = false;
    }
}

impl DeviceCodeState {
    fn generating() -> Self {
        Self {
            authorize_url: None,
            user_code: None,
            status: DeviceCodeStatus::Generating,
        }
    }

    fn with_details(authorize_url: String, user_code: String) -> Self {
        Self {
            authorize_url: Some(authorize_url),
            user_code: Some(user_code),
            status: DeviceCodeStatus::WaitingForApproval,
        }
    }
}

impl AccountRow {
    fn from(account: StoredAccount, active_id: Option<&str>) -> Self {
        let id = account.id.clone();
        let label = account_display_label(&account);
        let mode = account.mode;
        let mut detail_parts: Vec<String> = Vec::new();
        let now = Utc::now();

        if mode.is_chatgpt()
            && let Some(plan) = account
                .tokens
                .as_ref()
                .and_then(|t| t.id_token.get_chatgpt_plan_type())
            {
                detail_parts.push(format!("Plan: {plan}"));
            }

        if let Some(email) = account
            .tokens
            .as_ref()
            .and_then(|token| token.id_token.email.as_ref())
        {
            detail_parts.push(format!("Email: {email}"));
        }

        if let Some(last_refresh) = account.last_refresh {
            detail_parts.push(format!("Refreshed: {}", format_timestamp(last_refresh)));
            let refresh_due = last_refresh + chrono::Duration::days(CHATGPT_REFRESH_INTERVAL_DAYS);
            detail_parts.push(format!(
                "Renews: {} ({})",
                format_timestamp(refresh_due),
                format_relative_time(refresh_due, now)
            ));
        }

        if let Some(access_expires_at) = account
            .tokens
            .as_ref()
            .and_then(|token| jwt_expiry(&token.access_token))
        {
            detail_parts.push(format!(
                "Access expires: {} ({})",
                format_timestamp(access_expires_at),
                format_relative_time(access_expires_at, now)
            ));
        }

        if mode == AuthMode::ApiKey {
            detail_parts.push("Type: API key account".to_string());
        }

        if let Some(created_at) = account.created_at {
            detail_parts.push(format!("Connected: {}", format_timestamp(created_at)));
        }

        let is_active = active_id.is_some_and(|candidate| candidate == id);

        Self {
            id,
            label,
            detail_items: detail_parts,
            mode,
            is_active,
        }
    }
}

fn format_timestamp(ts: DateTime<Utc>) -> String {
    ts.with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

fn format_relative_time(target: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let delta = target - now;
    let seconds = delta.num_seconds();
    if seconds >= 0 {
        if seconds < 60 {
            "in <1m".to_string()
        } else if seconds < 3600 {
            format!("in {}m", seconds / 60)
        } else if seconds < 86_400 {
            format!("in {}h", seconds / 3600)
        } else {
            format!("in {}d", seconds / 86_400)
        }
    } else {
        let past = -seconds;
        if past < 60 {
            "<1m ago".to_string()
        } else if past < 3600 {
            format!("{}m ago", past / 60)
        } else if past < 86_400 {
            format!("{}h ago", past / 3600)
        } else {
            format!("{}d ago", past / 86_400)
        }
    }
}

fn jwt_expiry(token: &str) -> Option<DateTime<Utc>> {
    let mut parts = token.split('.');
    let (_header, payload, _sig) = match (parts.next(), parts.next(), parts.next()) {
        (Some(header), Some(payload), Some(sig))
            if !header.is_empty() && !payload.is_empty() && !sig.is_empty() =>
        {
            (header, payload, sig)
        }
        _ => return None,
    };

    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let payload_json = serde_json::from_slice::<JsonValue>(&payload_bytes).ok()?;
    let exp = payload_json.get("exp")?.as_i64()?;
    DateTime::from_timestamp(exp, 0)
}
