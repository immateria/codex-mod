use code_login::AuthMode;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::bottom_pane::settings_ui::list_detail_page::{SettingsListDetailMode, SettingsListDetailPage};

use super::{
    LoginAccountsState,
    ViewMode,
    ACCOUNTS_LIST_PANE_PERCENT,
    ACCOUNTS_TWO_PANE_MIN_HEIGHT,
    ACCOUNTS_TWO_PANE_MIN_WIDTH,
};

impl LoginAccountsState {
    pub(super) fn accounts_page(&self) -> SettingsListDetailPage<'static> {
        SettingsListDetailPage::new(
            "Manage Accounts",
            super::super::panel_style(),
            self.account_header_lines().len(),
            self.account_footer_lines().len(),
            ACCOUNTS_TWO_PANE_MIN_WIDTH,
            ACCOUNTS_TWO_PANE_MIN_HEIGHT,
            ACCOUNTS_LIST_PANE_PERCENT,
            "Accounts",
            "Details",
        )
    }

    pub(super) fn desired_height(&self, _width: u16) -> u16 {
        const MIN_HEIGHT: usize = 9;
        if matches!(self.mode, ViewMode::EditStorePaths(_)) {
            return 18;
        }
        let content_lines = self.content_line_count();
        let total = content_lines + 2; // account for top/bottom borders
        u16::try_from(total.max(MIN_HEIGHT)).unwrap_or(u16::MAX)
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

    fn account_header_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if let Some(feedback) = &self.feedback {
            let style = if feedback.is_error {
                crate::colors::style_error_bold()
            } else {
                Style::default()
                    .fg(crate::colors::success())
                    .add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(vec![Span::styled(feedback.message.clone(), style)]));
        }

        let status = if self.accounts.is_empty() {
            "No connected accounts".to_owned()
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
            Span::raw("  "),
            Span::styled(status, crate::colors::style_text_dim()),
        ]));
        lines
    }

    fn account_footer_lines(&self) -> Vec<Line<'static>> {
        use crate::bottom_pane::settings_ui::hints::{hint_enter, hint_esc, hint_nav, shortcut_line, KeyHint};
        let mut lines = Vec::new();
        lines.push(shortcut_line(&[
            hint_nav(" navigate"),
            hint_enter(" select"),
            KeyHint::new("a", " re-auth"),
            KeyHint::new("d", " disconnect"),
            KeyHint::new("p", " paths"),
            hint_esc(" close"),
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
                    crate::colors::style_text_dim(),
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

    pub(super) fn render_account_list_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let s_text_dim = crate::colors::style_text_dim();
        let s_primary = crate::colors::style_primary();
        let s_primary_bold = crate::colors::style_primary_bold();
        let c_success = crate::colors::success();
        let s_text = crate::colors::style_text();
        if self.accounts.is_empty() {
            lines.push(Line::from(Span::styled(
                "No accounts connected yet.",
                s_text_dim,
            )));
        } else {
            for (idx, account) in self.accounts.iter().enumerate() {
                let selected = idx == self.selected;
                let arrow_style = if selected {
                    s_primary
                } else {
                    s_text_dim
                };
                let label_style = if selected {
                    s_primary_bold
                } else if account.is_active {
                    Style::default()
                        .fg(c_success)
                        .add_modifier(Modifier::BOLD)
                } else {
                    s_text
                };

                let mut spans = vec![
                    Span::styled(crate::icons::selection_prefix(selected), arrow_style),
                    Span::styled(account.label.clone(), label_style),
                    Span::styled(
                        format!("  [{}]", Self::account_mode_badge(account.mode)),
                        s_text_dim,
                    ),
                ];

                if account.is_active {
                    spans.push(Span::styled(
                        " (current)",
                        Style::default()
                            .fg(c_success)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                if account.needs_reauth {
                    spans.push(Span::styled(
                        " ⚠ re-auth",
                        Style::default()
                            .fg(crate::colors::warning())
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
                crate::icons::selection_prefix(add_selected),
                if add_selected {
                    s_primary
                } else {
                    s_text_dim
                },
            ),
            Span::styled(
                "Add account…",
                if add_selected {
                    s_primary_bold
                } else {
                    s_text
                },
            ),
        ]));

        let store_selected = self.selected == self.store_paths_index();
        lines.push(Line::from(vec![
            Span::styled(
                crate::icons::selection_prefix(store_selected),
                if store_selected {
                    s_primary
                } else {
                    s_text_dim
                },
            ),
            Span::styled(
                "Account store paths…",
                if store_selected {
                    s_primary_bold
                } else {
                    s_text
                },
            ),
        ]));

        lines
    }

    fn render_selected_details_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let s_text_dim = crate::colors::style_text_dim();

        if let Some(account) = self.accounts.get(self.selected) {
            lines.push(Line::from(vec![Span::styled(
                "Selected Account",
                Style::default().add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(vec![
                Span::styled(
                    account.label.clone(),
                    crate::colors::style_primary_bold(),
                ),
                Span::styled(
                    format!("  [{}]", Self::account_mode_badge(account.mode)),
                    s_text_dim,
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
                    s_text_dim,
                )));
            } else {
                for item in &account.detail_items {
                    lines.push(Line::from(vec![
                        Span::styled("• ", s_text_dim),
                        Span::styled(item.clone(), s_text_dim),
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
                s_text_dim,
            )));
            lines.push(Line::from(Span::styled(
                "Useful when your current account is near usage limits.",
                s_text_dim,
            )));
        } else {
            lines.push(Line::from(vec![Span::styled(
                "Account Store Paths",
                Style::default().add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(Span::styled(
                "Control where account records are loaded from and saved to.",
                s_text_dim,
            )));
            lines.push(Line::from(Span::styled(
                "Supports multiple read paths with a dedicated write target.",
                s_text_dim,
            )));
        }

        lines
    }

    fn render_accounts_compact(&self, area: Rect, buf: &mut Buffer) {
        let list_lines = self.render_account_list_lines();
        let list_height = u16::try_from(list_lines.len().min(area.height as usize))
            .unwrap_or(area.height);
        let base = crate::colors::style_text_on_bg();

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
        let base = crate::colors::style_text_on_bg();

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
}
