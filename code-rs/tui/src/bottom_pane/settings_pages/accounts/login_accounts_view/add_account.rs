use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use code_core::auth;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use textwrap::Options as TwOptions;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::{BottomPane, BottomPaneView, ConditionalUpdate};
use crate::bottom_pane::chrome::ChromeMode;
use crate::chatwidget::BackgroundOrderTicket;
use crate::components::form_text_field::FormTextField;
use crate::ui_interaction::redraw_if;

use crate::bottom_pane::settings_ui::editor_page::SettingsEditorPage;
use crate::bottom_pane::settings_ui::hints::{status_and_shortcuts, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::message_page::SettingsMessagePage;
use crate::bottom_pane::settings_ui::rows::StyledText;

use super::shared::Feedback;

const ADD_ACCOUNT_CHOICES: usize = 2;

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
        u16::try_from(self.state.borrow().desired_height()).unwrap_or(u16::MAX)
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
        match &mut self.step {
            AddStep::Choose { selected } => match key_event.code {
                KeyCode::Esc => {
                    self.finish_and_show_accounts();
                    true
                }
                KeyCode::Up => {
                    *selected = selected
                        .checked_sub(1)
                        .unwrap_or(ADD_ACCOUNT_CHOICES.saturating_sub(1));
                    true
                }
                KeyCode::Down => {
                    *selected = selected.saturating_add(1) % ADD_ACCOUNT_CHOICES.max(1);
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
                    self.step = AddStep::DeviceCode(DeviceCodeStep::Generating);
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
        const BASE_LINES: usize = 5; // title + spacing baseline
        const FEEDBACK_LINES: usize = 2;
        const CHOOSE_STEP_LINES: usize = 4; // options + spacing
        const API_KEY_STEP_LINES: usize = 4; // instructions + input + spacing
        const WAITING_STEP_LINES: usize = 3; // instructions + cancel hint
        const DEVICE_CODE_STEP_LINES: usize = 6;
        const MIN_LINES: usize = 10;
        const FRAME_PADDING_LINES: usize = 2;

        let mut lines = BASE_LINES;
        if self.feedback.is_some() {
            lines += FEEDBACK_LINES;
        }

        match &self.step {
            AddStep::Choose { .. } => {
                lines += CHOOSE_STEP_LINES;
            }
            AddStep::ApiKey { .. } => {
                lines += API_KEY_STEP_LINES;
            }
            AddStep::Waiting { auth_url } => {
                lines += WAITING_STEP_LINES;
                if auth_url.is_some() {
                    lines += 1;
                }
            }
            AddStep::DeviceCode(_) => {
                lines += DEVICE_CODE_STEP_LINES;
            }
        }

        lines.max(MIN_LINES) + FRAME_PADDING_LINES
    }

    fn feedback_styled_text(&self) -> Option<StyledText<'static>> {
        self.feedback.as_ref().map(|feedback| {
            let style = if feedback.is_error {
                Style::new().fg(crate::colors::error()).bold()
            } else {
                Style::new().fg(crate::colors::success()).bold()
            };
            StyledText::new(feedback.message.clone(), style)
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
            super::panel_style(),
            Vec::new(),
            body_lines,
            footer_lines,
        )
        .with_min_body_rows(3)
    }

    fn auth_progress_body_width(&self, area: Rect) -> u16 {
        self.auth_progress_message_page(Vec::new(), Vec::new())
            .framed()
            .layout(area)
            .map(|layout| layout.body.width.max(1))
            .unwrap_or(area.width.max(1))
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
                    super::panel_style(),
                    header_lines,
                    footer_lines,
                );
                let rows = vec![
                    SettingsMenuRow::new(0usize, "ChatGPT sign-in"),
                    SettingsMenuRow::new(1usize, "API key"),
                ];
                let _ = page.render_menu_rows_in_chrome(
                    ChromeMode::Framed,
                    area,
                    buf,
                    0,
                    Some(*selected),
                    &rows,
                );
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
                    super::panel_style(),
                    "OpenAI API key",
                    pre_lines,
                    post_lines,
                );
                let _ = page.render_in_chrome(ChromeMode::Framed, area, buf, field);
            }
            AddStep::Waiting { auth_url } => {
                let footer_lines = self.footer_status_and_shortcuts(&[
                    KeyHint::new("Esc", " cancel login")
                        .with_key_style(Style::new().fg(crate::colors::error()).bold()),
                ]);
                let content_width = self.auth_progress_body_width(area);
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
                let _ = page.render_in_chrome(ChromeMode::Framed, area, buf);
            }
            AddStep::DeviceCode(state) => {
                let footer_lines = self.footer_status_and_shortcuts(&[
                    KeyHint::new("Esc", " cancel login")
                        .with_key_style(Style::new().fg(crate::colors::error()).bold()),
                ]);
                let content_width = self.auth_progress_body_width(area);
                let mut body_lines = vec![Line::from("Complete sign-in using a verification code.")];
                match state {
                    DeviceCodeStep::Generating => {
                        body_lines.push(Line::from("Generating a secure code and link…"));
                    }
                    DeviceCodeStep::WaitingForApproval { authorize_url, user_code } => {
                        body_lines.push(Line::from(vec![
                            Span::styled(
                                "Code: ",
                                Style::default().fg(crate::colors::text_dim()),
                            ),
                            Span::styled(
                                user_code.clone(),
                                Style::new().fg(crate::colors::primary()).bold(),
                            ),
                        ]));
                        body_lines.push(Line::from("Visit this link on any device:"));
                        for chunk in wrap_url_segments(authorize_url, content_width) {
                            body_lines.push(Line::from(vec![Span::styled(
                                chunk,
                                Style::default().fg(crate::colors::info()),
                            )]));
                        }
                        body_lines.push(Line::from("Keep this code private. It expires after 15 minutes."));
                    }
                }
                let page = self.auth_progress_message_page(body_lines, footer_lines);
                let _ = page.render_in_chrome(ChromeMode::Framed, area, buf);
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
            self.step = AddStep::DeviceCode(DeviceCodeStep::Generating);
        }
        self.feedback = Some(Feedback {
            message: "Use the on-screen code to finish signing in.".to_string(),
            is_error: false,
        });
    }

    pub fn set_device_code_ready(&mut self, authorize_url: String, user_code: String) {
        self.step = AddStep::DeviceCode(DeviceCodeStep::WaitingForApproval { authorize_url, user_code });
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
                self.feedback = Some(Feedback {
                    message: "ChatGPT account connected".to_string(),
                    is_error: false,
                });
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
        self.step = AddStep::Choose { selected: 0 };
        self.feedback = None;
    }
}
