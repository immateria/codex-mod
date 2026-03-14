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
        let list_height = u16::try_from(list_lines.len().min(area.height as usize))
            .unwrap_or(area.height);
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
}
