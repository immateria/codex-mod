use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use code_core::auth;
use code_login::AuthMode;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::Rect;

use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
use crate::components::form_text_field::FormTextField;
use crate::components::mode_guard::ModeGuard;
use crate::bottom_pane::ConditionalUpdate;

use super::super::shared::Feedback;

#[derive(Clone, Debug)]
pub(super) struct AccountRow {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) detail_items: Vec<String>,
    pub(super) mode: AuthMode,
    pub(super) is_active: bool,
}

#[derive(Debug)]
pub(super) enum ViewMode {
    List,
    ConfirmRemove { account_id: String },
    EditStorePaths(Box<StorePathEditorState>),
}

#[derive(Debug)]
pub(super) struct StorePathEditorState {
    pub(super) selected_row: usize,
    pub(super) read_paths_field: FormTextField,
    pub(super) write_path_field: FormTextField,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StorePathEditorAction {
    Save,
    Cancel,
}

pub(crate) struct LoginAccountsState {
    pub(super) code_home: PathBuf,
    pub(super) app_event_tx: AppEventSender,
    pub(super) tail_ticket: BackgroundOrderTicket,
    pub(super) auth_credentials_store_mode: auth::AuthCredentialsStoreMode,
    pub(super) accounts: Vec<AccountRow>,
    pub(super) active_account_id: Option<String>,
    pub(super) selected: usize,
    pub(super) mode: ViewMode,
    pub(super) feedback: Option<Feedback>,
    is_complete: bool,
}

impl LoginAccountsState {
    pub(super) fn new(
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

    pub(super) fn send_tail(&self, message: impl Into<String>) {
        self.app_event_tx
            .send_background_event_with_ticket(&self.tail_ticket, message);
    }

    pub fn weak_handle(state: &Rc<RefCell<Self>>) -> Weak<RefCell<Self>> {
        Rc::downgrade(state)
    }

    pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mut mode_guard = ModeGuard::replace(&mut self.mode, ViewMode::List, |mode| {
            matches!(mode, ViewMode::List)
        });
        match mode_guard.mode_mut() {
            ViewMode::List => self.handle_list_key(key_event),
            ViewMode::ConfirmRemove { account_id } => {
                self.mode = ViewMode::ConfirmRemove {
                    account_id: account_id.clone(),
                };
                let handled = self.handle_confirm_remove_key(key_event);
                if matches!(self.mode, ViewMode::List) {
                    // Keep `List` instead of restoring `ConfirmRemove`.
                    mode_guard.disarm();
                }
                handled
            }
            ViewMode::EditStorePaths(editor) => {
                let (keep_open, handled) = self.handle_store_paths_editor_key(key_event, editor);
                if !keep_open {
                    // Keep `List` instead of restoring `EditStorePaths`.
                    mode_guard.disarm();
                }
                handled
            }
        }
    }

    pub(crate) fn handle_mouse_event(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mut mode_guard = ModeGuard::replace(&mut self.mode, ViewMode::List, |mode| {
            matches!(mode, ViewMode::List)
        });
        match mode_guard.mode_mut() {
            ViewMode::List => self.handle_list_mouse(mouse_event, area),
            ViewMode::ConfirmRemove { account_id } => {
                self.mode = ViewMode::ConfirmRemove {
                    account_id: account_id.clone(),
                };
                let handled = self.handle_confirm_remove_mouse(mouse_event, area);
                if matches!(self.mode, ViewMode::List) {
                    // Keep `List` instead of restoring `ConfirmRemove`.
                    mode_guard.disarm();
                }
                handled
            }
            ViewMode::EditStorePaths(editor) => {
                let (keep_open, handled) =
                    self.handle_store_paths_editor_mouse(mouse_event, area, editor);
                if !keep_open {
                    // Keep `List` instead of restoring `EditStorePaths`.
                    mode_guard.disarm();
                }
                handled
            }
        }
    }

    pub(crate) fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        let mut mode_guard = ModeGuard::replace(&mut self.mode, ViewMode::List, |mode| {
            matches!(mode, ViewMode::List)
        });
        match mode_guard.mode_mut() {
            ViewMode::EditStorePaths(editor) => {
                match editor.selected_row {
                    0 => editor.read_paths_field.handle_paste(text),
                    1 => editor.write_path_field.handle_paste(text),
                    _ => {}
                }
                ConditionalUpdate::NeedsRedraw
            }
            _ => ConditionalUpdate::NoRedraw,
        }
    }

    pub(super) fn add_account_index(&self) -> usize {
        self.accounts.len()
    }

    pub(super) fn store_paths_index(&self) -> usize {
        self.add_account_index().saturating_add(1)
    }

    pub(super) fn is_confirm_remove_mode(&self) -> bool {
        matches!(self.mode, ViewMode::ConfirmRemove { .. })
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(super) fn set_complete(&mut self) {
        self.is_complete = true;
    }

    pub(crate) fn clear_complete(&mut self) {
        self.is_complete = false;
    }
}
