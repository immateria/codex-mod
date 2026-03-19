use code_core::auth;
use crossterm::event::{KeyCode, KeyEvent};

use crate::app_event::AppEvent;
use crate::bottom_pane::ConditionalUpdate;
use crate::components::form_text_field::FormTextField;

use super::{AddStep, DeviceCodeStep, LoginAddAccountState, ADD_ACCOUNT_CHOICES};
use super::super::shared::Feedback;

pub(super) fn handle_key_event(state: &mut LoginAddAccountState, key_event: KeyEvent) -> bool {
    match &mut state.step {
        AddStep::Choose { selected } => match key_event.code {
            KeyCode::Esc => {
                finish_and_show_accounts(state);
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
                    state.feedback = Some(Feedback {
                        message: "Opening browser for ChatGPT sign-in…".to_string(),
                        is_error: false,
                    });
                    state.step = AddStep::Waiting { auth_url: None };
                    state.app_event_tx.send(AppEvent::LoginStartChatGpt);
                } else {
                    state.feedback = None;
                    state.step = AddStep::ApiKey {
                        field: FormTextField::new_single_line(),
                    };
                }
                true
            }
            _ => false,
        },
        AddStep::ApiKey { field } => match key_event.code {
            KeyCode::Esc => {
                finish_and_show_accounts(state);
                true
            }
            KeyCode::Enter => {
                let key = field.text().trim().to_string();
                if key.is_empty() {
                    state.feedback = Some(Feedback {
                        message: "API key cannot be empty".to_string(),
                        is_error: true,
                    });
                } else {
                    match auth::login_with_api_key_with_store_mode(
                        &state.code_home,
                        &key,
                        state.auth_credentials_store_mode,
                    ) {
                        Ok(()) => {
                            state.feedback = Some(Feedback {
                                message: "API key connected".to_string(),
                                is_error: false,
                            });
                            state.send_tail("Added API key account".to_string());
                            state.app_event_tx.send(AppEvent::LoginUsingChatGptChanged {
                                using_chatgpt_auth: false,
                            });
                            finish_and_show_accounts(state);
                        }
                        Err(err) => {
                            state.feedback = Some(Feedback {
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
                state.app_event_tx.send(AppEvent::LoginCancelChatGpt);
                true
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                state.feedback = Some(Feedback {
                    message: "Switching to code authentication…".to_string(),
                    is_error: false,
                });
                state.step = AddStep::DeviceCode(DeviceCodeStep::Generating);
                state.app_event_tx.send(AppEvent::LoginStartDeviceCode);
                true
            }
            _ => false,
        },
        AddStep::DeviceCode(_) => {
            if matches!(key_event.code, KeyCode::Esc) {
                state.app_event_tx.send(AppEvent::LoginCancelChatGpt);
                true
            } else {
                false
            }
        }
    }
}

pub(super) fn handle_paste(state: &mut LoginAddAccountState, text: String) -> ConditionalUpdate {
    if let AddStep::ApiKey { field } = &mut state.step {
        field.handle_paste(text);
        ConditionalUpdate::NeedsRedraw
    } else {
        ConditionalUpdate::NoRedraw
    }
}

pub(super) fn acknowledge_chatgpt_started(state: &mut LoginAddAccountState, auth_url: String) {
    state.step = AddStep::Waiting {
        auth_url: Some(auth_url),
    };
    state.feedback = Some(Feedback {
        message: "Browser opened. Complete sign-in to finish.".to_string(),
        is_error: false,
    });
}

pub(super) fn acknowledge_chatgpt_failed(state: &mut LoginAddAccountState, error: String) {
    state.step = AddStep::Choose { selected: 0 };
    state.feedback = Some(Feedback {
        message: error,
        is_error: true,
    });
}

pub(super) fn begin_device_code_flow(state: &mut LoginAddAccountState) {
    if !matches!(state.step, AddStep::DeviceCode(_)) {
        state.step = AddStep::DeviceCode(DeviceCodeStep::Generating);
    }
    state.feedback = Some(Feedback {
        message: "Use the on-screen code to finish signing in.".to_string(),
        is_error: false,
    });
}

pub(super) fn set_device_code_ready(
    state: &mut LoginAddAccountState,
    authorize_url: String,
    user_code: String,
) {
    state.step = AddStep::DeviceCode(DeviceCodeStep::WaitingForApproval {
        authorize_url,
        user_code,
    });
    state.feedback = Some(Feedback {
        message: "Enter the code in your browser to continue.".to_string(),
        is_error: false,
    });
}

pub(super) fn on_device_code_failed(state: &mut LoginAddAccountState, error: String) {
    state.step = AddStep::Choose { selected: 0 };
    state.feedback = Some(Feedback {
        message: error,
        is_error: true,
    });
}

pub(super) fn on_chatgpt_complete(state: &mut LoginAddAccountState, result: Result<(), String>) {
    match result {
        Ok(()) => {
            state.feedback = Some(Feedback {
                message: "ChatGPT account connected".to_string(),
                is_error: false,
            });
            state.send_tail("ChatGPT account connected".to_string());
            state.app_event_tx
                .send(AppEvent::LoginUsingChatGptChanged { using_chatgpt_auth: true });
            finish_and_show_accounts(state);
        }
        Err(err) => {
            state.step = AddStep::Choose { selected: 0 };
            state.feedback = Some(Feedback {
                message: err,
                is_error: true,
            });
        }
    }
}

pub(super) fn cancel_active_flow(state: &mut LoginAddAccountState) {
    let message = match state.step {
        AddStep::DeviceCode(_) => "Cancelled code authentication",
        AddStep::Waiting { .. } => "Cancelled ChatGPT login",
        _ => "Cancelled login",
    };
    state.step = AddStep::Choose { selected: 0 };
    state.feedback = Some(Feedback {
        message: message.to_string(),
        is_error: false,
    });
}

pub(super) fn clear_complete(state: &mut LoginAddAccountState) {
    state.is_complete = false;
    state.step = AddStep::Choose { selected: 0 };
    state.feedback = None;
}

fn finish_and_show_accounts(state: &mut LoginAddAccountState) {
    state.is_complete = true;
    state.app_event_tx.send(AppEvent::ShowLoginAccounts);
}

