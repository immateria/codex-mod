use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use code_core::auth;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::{BottomPane, BottomPaneView, ConditionalUpdate};
use crate::chatwidget::BackgroundOrderTicket;
use crate::ui_interaction::redraw_if;

use super::state::LoginAccountsState;

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
        let handled = state.handle_key_event(key_event);
        handled || state.is_complete()
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
        self.state.borrow().is_complete()
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.state.borrow().desired_height(width)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.state.borrow().render(area, buf);
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        self.state.borrow_mut().handle_paste(text)
    }
}

