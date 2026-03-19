use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use textwrap::Options as TwOptions;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::editor_page::SettingsEditorPage;
use crate::bottom_pane::settings_ui::hints::{status_and_shortcuts, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::message_page::SettingsMessagePage;
use crate::bottom_pane::settings_ui::rows::StyledText;

use super::{AddStep, DeviceCodeStep, LoginAddAccountState};

pub(super) fn desired_height(state: &LoginAddAccountState) -> usize {
    const BASE_LINES: usize = 5; // title + spacing baseline
    const FEEDBACK_LINES: usize = 2;
    const CHOOSE_STEP_LINES: usize = 4; // options + spacing
    const API_KEY_STEP_LINES: usize = 4; // instructions + input + spacing
    const WAITING_STEP_LINES: usize = 3; // instructions + cancel hint
    const DEVICE_CODE_STEP_LINES: usize = 6;
    const MIN_LINES: usize = 10;
    const FRAME_PADDING_LINES: usize = 2;

    let mut lines = BASE_LINES;
    if state.feedback.is_some() {
        lines += FEEDBACK_LINES;
    }

    match &state.step {
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

pub(super) fn render(state: &LoginAddAccountState, area: Rect, buf: &mut Buffer) {
    match &state.step {
        AddStep::Choose { selected } => {
            let header_lines = vec![Line::from("Choose how you’d like to add an account:")];
            let footer_lines = footer_status_and_shortcuts(state, &[
                KeyHint::new("↑↓", " navigate")
                    .with_key_style(Style::new().fg(crate::colors::function())),
                KeyHint::new("Enter", " select")
                    .with_key_style(Style::new().fg(crate::colors::success())),
                KeyHint::new("Esc", " back")
                    .with_key_style(Style::new().fg(crate::colors::error()).bold()),
            ]);
            let page = SettingsMenuPage::new(
                "Add Account",
                super::super::panel_style(),
                header_lines,
                footer_lines,
            );
            let rows = vec![
                SettingsMenuRow::new(0usize, "ChatGPT sign-in"),
                SettingsMenuRow::new(1usize, "API key"),
            ];
            let _ = page.render_menu_rows_in_chrome(ChromeMode::Framed, area, buf, 0, Some(*selected), &rows);
        }
        AddStep::ApiKey { field } => {
            let pre_lines = vec![Line::from("Paste your OpenAI API key:")];
            let post_lines = footer_status_and_shortcuts(state, &[
                KeyHint::new("Enter", " save")
                    .with_key_style(Style::new().fg(crate::colors::success())),
                KeyHint::new("Esc", " cancel")
                    .with_key_style(Style::new().fg(crate::colors::error()).bold()),
            ]);
            let page = SettingsEditorPage::new(
                "Add Account",
                super::super::panel_style(),
                "OpenAI API key",
                pre_lines,
                post_lines,
            );
            let _ = page.render_in_chrome(ChromeMode::Framed, area, buf, field);
        }
        AddStep::Waiting { auth_url } => {
            let footer_lines = footer_status_and_shortcuts(state, &[
                KeyHint::new("Esc", " cancel login")
                    .with_key_style(Style::new().fg(crate::colors::error()).bold()),
            ]);
            let content_width = auth_progress_body_width(state, area);
            let mut body_lines = vec![
                Line::from("Finish signing in with ChatGPT in your browser."),
                Line::from(vec![
                    Span::styled(
                        "Not seeing a browser? ",
                        Style::default().fg(crate::colors::text_dim()),
                    ),
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
            let page = auth_progress_message_page(body_lines, footer_lines);
            let _ = page.render_in_chrome(ChromeMode::Framed, area, buf);
        }
        AddStep::DeviceCode(step) => {
            let footer_lines = footer_status_and_shortcuts(state, &[
                KeyHint::new("Esc", " cancel login")
                    .with_key_style(Style::new().fg(crate::colors::error()).bold()),
            ]);
            let content_width = auth_progress_body_width(state, area);
            let mut body_lines = vec![Line::from("Complete sign-in using a verification code.")];
            match step {
                DeviceCodeStep::Generating => {
                    body_lines.push(Line::from("Generating a secure code and link…"));
                }
                DeviceCodeStep::WaitingForApproval { authorize_url, user_code } => {
                    body_lines.push(Line::from(vec![
                        Span::styled("Code: ", Style::default().fg(crate::colors::text_dim())),
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
                    body_lines.push(Line::from(
                        "Keep this code private. It expires after 15 minutes.",
                    ));
                }
            }
            let page = auth_progress_message_page(body_lines, footer_lines);
            let _ = page.render_in_chrome(ChromeMode::Framed, area, buf);
        }
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

fn feedback_styled_text(state: &LoginAddAccountState) -> Option<StyledText<'static>> {
    state.feedback.as_ref().map(|feedback| {
        let style = if feedback.is_error {
            Style::new().fg(crate::colors::error()).bold()
        } else {
            Style::new().fg(crate::colors::success()).bold()
        };
        StyledText::new(feedback.message.clone(), style)
    })
}

fn footer_status_and_shortcuts(state: &LoginAddAccountState, hints: &[KeyHint<'_>]) -> Vec<Line<'static>> {
    status_and_shortcuts(feedback_styled_text(state), hints)
}

fn auth_progress_message_page(
    body_lines: Vec<Line<'static>>,
    footer_lines: Vec<Line<'static>>,
) -> SettingsMessagePage<'static> {
    SettingsMessagePage::new(
        "Add Account",
        super::super::panel_style(),
        Vec::new(),
        body_lines,
        footer_lines,
    )
    .with_min_body_rows(3)
}

fn auth_progress_body_width(_state: &LoginAddAccountState, area: Rect) -> u16 {
    auth_progress_message_page(Vec::new(), Vec::new())
        .framed()
        .layout(area)
        .map(|layout| layout.body.width.max(1))
        .unwrap_or(area.width.max(1))
}

