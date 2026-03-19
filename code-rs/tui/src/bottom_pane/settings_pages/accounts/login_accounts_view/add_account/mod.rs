use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use code_core::auth;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::ConditionalUpdate;
use crate::chatwidget::BackgroundOrderTicket;
use crate::components::form_text_field::FormTextField;

use super::shared::Feedback;

mod input;
mod pane_impl;
mod render;

const ADD_ACCOUNT_CHOICES: usize = 2;

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

#[derive(Debug)]
enum AddStep {
    Choose { selected: usize },
    ApiKey { field: FormTextField },
    Waiting { auth_url: Option<String> },
    DeviceCode(DeviceCodeStep),
}

#[derive(Debug)]
enum DeviceCodeStep {
    Generating,
    WaitingForApproval { authorize_url: String, user_code: String },
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
        input::handle_key_event(self, key_event)
    }

    pub(crate) fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        input::handle_paste(self, text)
    }

    fn desired_height(&self) -> usize {
        render::desired_height(self)
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        render::render(self, area, buf);
    }

    pub fn acknowledge_chatgpt_started(&mut self, auth_url: String) {
        input::acknowledge_chatgpt_started(self, auth_url);
    }

    pub fn acknowledge_chatgpt_failed(&mut self, error: String) {
        input::acknowledge_chatgpt_failed(self, error);
    }

    pub fn begin_device_code_flow(&mut self) {
        input::begin_device_code_flow(self);
    }

    pub fn set_device_code_ready(&mut self, authorize_url: String, user_code: String) {
        input::set_device_code_ready(self, authorize_url, user_code);
    }

    pub fn on_device_code_failed(&mut self, error: String) {
        input::on_device_code_failed(self, error);
    }

    pub fn on_chatgpt_complete(&mut self, result: Result<(), String>) {
        input::on_chatgpt_complete(self, result);
    }

    pub fn cancel_active_flow(&mut self) {
        input::cancel_active_flow(self);
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn clear_complete(&mut self) {
        input::clear_complete(self);
    }
}
