use code_core::config_types::{AutoResolveAttemptLimit, ReasoningEffort};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::cell::Cell;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::hints::{shortcut_line, KeyHint};
use super::settings_ui::line_runs::{selection_id_at, SelectableLineRun};
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::toggle;
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
    scroll_top_to_keep_visible,
};
use crate::components::scroll_state::ScrollState;
use super::BottomPane;

const DEFAULT_VISIBLE_ROWS: usize = 8;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SelectionKind {
    ReviewEnabled,
    ReviewModel,
    ReviewResolveModel,
    ReviewAttempts,
    AutoReviewEnabled,
    AutoReviewModel,
    AutoReviewResolveModel,
    AutoReviewAttempts,
}

enum RowData {
    SectionReview,
    ReviewEnabled,
    ReviewModel,
    ReviewResolveModel,
    ReviewAttempts,
    SectionAutoReview,
    AutoReviewEnabled,
    AutoReviewModel,
    AutoReviewResolveModel,
    AutoReviewAttempts,
}

#[derive(Clone, Debug)]
struct ReviewListModel {
    runs: Vec<SelectableLineRun<'static, usize>>,
    /// Selection index -> semantic kind.
    selection_kinds: Vec<SelectionKind>,
    /// Selection index -> absolute line index within the flattened run list.
    selection_line: Vec<usize>,
    /// Selection index -> inclusive (section_start_line, section_end_line).
    section_bounds: Vec<(usize, usize)>,
    /// Total line count across all runs.
    total_lines: usize,
}

pub(crate) struct ReviewSettingsView {
    review_use_chat_model: bool,
    review_model: String,
    review_reasoning: ReasoningEffort,
    review_resolve_use_chat_model: bool,
    review_resolve_model: String,
    review_resolve_reasoning: ReasoningEffort,
    review_auto_resolve_enabled: bool,
    review_followups: u32,
    review_followups_index: usize,

    auto_review_enabled: bool,
    auto_review_use_chat_model: bool,
    auto_review_model: String,
    auto_review_reasoning: ReasoningEffort,
    auto_review_resolve_use_chat_model: bool,
    auto_review_resolve_model: String,
    auto_review_resolve_reasoning: ReasoningEffort,
    auto_review_followups: u32,
    auto_review_followups_index: usize,

    app_event_tx: AppEventSender,
    state: ScrollState,
    is_complete: bool,
    viewport_rows: Cell<usize>,
    pending_notice: Option<String>,
}

pub(crate) struct ReviewSettingsInit {
    pub review_use_chat_model: bool,
    pub review_model: String,
    pub review_reasoning: ReasoningEffort,
    pub review_resolve_use_chat_model: bool,
    pub review_resolve_model: String,
    pub review_resolve_reasoning: ReasoningEffort,
    pub review_auto_resolve_enabled: bool,
    pub review_followups: u32,
    pub auto_review_enabled: bool,
    pub auto_review_use_chat_model: bool,
    pub auto_review_model: String,
    pub auto_review_reasoning: ReasoningEffort,
    pub auto_review_resolve_use_chat_model: bool,
    pub auto_review_resolve_model: String,
    pub auto_review_resolve_reasoning: ReasoningEffort,
    pub auto_review_followups: u32,
    pub app_event_tx: AppEventSender,
}

impl ReviewSettingsView {
    pub fn set_review_model(&mut self, model: String, effort: ReasoningEffort) {
        self.review_model = model;
        self.review_reasoning = effort;
    }

    pub fn set_review_use_chat_model(&mut self, use_chat: bool) {
        self.review_use_chat_model = use_chat;
    }

    pub fn set_review_resolve_model(&mut self, model: String, effort: ReasoningEffort) {
        self.review_resolve_model = model;
        self.review_resolve_reasoning = effort;
    }

    pub fn set_review_resolve_use_chat_model(&mut self, use_chat: bool) {
        self.review_resolve_use_chat_model = use_chat;
    }

    pub fn set_auto_review_model(&mut self, model: String, effort: ReasoningEffort) {
        self.auto_review_model = model;
        self.auto_review_reasoning = effort;
    }

    pub fn set_auto_review_use_chat_model(&mut self, use_chat: bool) {
        self.auto_review_use_chat_model = use_chat;
    }

    pub fn set_auto_review_resolve_model(&mut self, model: String, effort: ReasoningEffort) {
        self.auto_review_resolve_model = model;
        self.auto_review_resolve_reasoning = effort;
    }

    pub fn set_auto_review_resolve_use_chat_model(&mut self, use_chat: bool) {
        self.auto_review_resolve_use_chat_model = use_chat;
    }

    pub fn set_review_followups(&mut self, attempts: u32) {
        if let Some(idx) = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == attempts)
        {
            self.review_followups_index = idx;
        }
        self.review_followups = attempts;
    }

    pub fn set_auto_review_followups(&mut self, attempts: u32) {
        if let Some(idx) = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == attempts)
        {
            self.auto_review_followups_index = idx;
        }
        self.auto_review_followups = attempts;
    }

    pub fn new(init: ReviewSettingsInit) -> Self {
        let ReviewSettingsInit {
            review_use_chat_model,
            review_model,
            review_reasoning,
            review_resolve_use_chat_model,
            review_resolve_model,
            review_resolve_reasoning,
            review_auto_resolve_enabled,
            review_followups,
            auto_review_enabled,
            auto_review_use_chat_model,
            auto_review_model,
            auto_review_reasoning,
            auto_review_resolve_use_chat_model,
            auto_review_resolve_model,
            auto_review_resolve_reasoning,
            auto_review_followups,
            app_event_tx,
        } = init;
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);

        let default_index = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == AutoResolveAttemptLimit::DEFAULT)
            .unwrap_or(0);

        let review_followups_index = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == review_followups)
            .unwrap_or(default_index);

        let auto_review_followups_index = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == auto_review_followups)
            .unwrap_or(default_index);

        Self {
            review_use_chat_model,
            review_model,
            review_reasoning,
            review_resolve_use_chat_model,
            review_resolve_model,
            review_resolve_reasoning,
            review_auto_resolve_enabled,
            review_followups,
            review_followups_index,
            auto_review_enabled,
            auto_review_use_chat_model,
            auto_review_model,
            auto_review_reasoning,
            auto_review_resolve_use_chat_model,
            auto_review_resolve_model,
            auto_review_resolve_reasoning,
            auto_review_followups,
            auto_review_followups_index,
            app_event_tx,
            state,
            is_complete: false,
            viewport_rows: Cell::new(0),
            pending_notice: None,
        }
    }

    fn toggle_review_auto_resolve(&mut self) {
        self.review_auto_resolve_enabled = !self.review_auto_resolve_enabled;
        self.app_event_tx
            .send(AppEvent::UpdateReviewAutoResolveEnabled(self.review_auto_resolve_enabled));
    }

    fn adjust_review_followups(&mut self, forward: bool) {
        let allowed = AutoResolveAttemptLimit::ALLOWED;
        if allowed.is_empty() {
            return;
        }

        let len = allowed.len();
        let mut next = self.review_followups_index;
        next = if forward {
            (next + 1) % len
        } else if next == 0 {
            len.saturating_sub(1)
        } else {
            next - 1
        };

        if next == self.review_followups_index {
            return;
        }

        self.review_followups_index = next;
        self.review_followups = allowed[next];
        self.app_event_tx
            .send(AppEvent::UpdateReviewAutoResolveAttempts(self.review_followups));
    }

    fn adjust_auto_review_followups(&mut self, forward: bool) {
        let allowed = AutoResolveAttemptLimit::ALLOWED;
        if allowed.is_empty() {
            return;
        }

        let len = allowed.len();
        let mut next = self.auto_review_followups_index;
        next = if forward {
            (next + 1) % len
        } else if next == 0 {
            len.saturating_sub(1)
        } else {
            next - 1
        };

        if next == self.auto_review_followups_index {
            return;
        }

        self.auto_review_followups_index = next;
        self.auto_review_followups = allowed[next];
        self.app_event_tx
            .send(AppEvent::UpdateAutoReviewFollowupAttempts(self.auto_review_followups));
    }

    fn toggle_auto_review(&mut self) {
        self.auto_review_enabled = !self.auto_review_enabled;
        self.app_event_tx
            .send(AppEvent::UpdateAutoReviewEnabled(self.auto_review_enabled));
    }

    fn open_review_model_selector(&self) {
        self.app_event_tx
            .send(AppEvent::ShowReviewModelSelector);
    }

    fn open_review_resolve_model_selector(&self) {
        self.app_event_tx
            .send(AppEvent::ShowReviewResolveModelSelector);
    }

    fn open_auto_review_model_selector(&self) {
        self.app_event_tx
            .send(AppEvent::ShowAutoReviewModelSelector);
    }

    fn open_auto_review_resolve_model_selector(&self) {
        self.app_event_tx
            .send(AppEvent::ShowAutoReviewResolveModelSelector);
    }

    fn build_model(&self, selected_idx: usize) -> ReviewListModel {
        let mut runs = Vec::new();
        let mut selection_kinds = Vec::new();
        let mut selection_line = Vec::new();
        let mut section_bounds = Vec::new();

        let mut current_line = 0usize;

        let review_section_start = current_line;
        runs.push(SelectableLineRun::plain(vec![self.render_row(
            &RowData::SectionReview,
            false,
        )]));
        current_line = current_line.saturating_add(1);

        let review_enabled_idx = selection_kinds.len();
        selection_kinds.push(SelectionKind::ReviewEnabled);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        runs.push(
            SelectableLineRun::selectable(
                review_enabled_idx,
                vec![self.render_row(&RowData::ReviewEnabled, review_enabled_idx == selected_idx)],
            )
            .with_style(if review_enabled_idx == selected_idx {
                Style::new().bg(colors::selection())
            } else {
                Style::new()
            }),
        );
        current_line = current_line.saturating_add(1);

        let review_model_idx = selection_kinds.len();
        selection_kinds.push(SelectionKind::ReviewModel);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        runs.push(
            SelectableLineRun::selectable(
                review_model_idx,
                vec![self.render_row(&RowData::ReviewModel, review_model_idx == selected_idx)],
            )
            .with_style(if review_model_idx == selected_idx {
                Style::new().bg(colors::selection())
            } else {
                Style::new()
            }),
        );
        current_line = current_line.saturating_add(1);

        let review_resolve_idx = selection_kinds.len();
        selection_kinds.push(SelectionKind::ReviewResolveModel);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        runs.push(
            SelectableLineRun::selectable(
                review_resolve_idx,
                vec![self.render_row(
                    &RowData::ReviewResolveModel,
                    review_resolve_idx == selected_idx,
                )],
            )
            .with_style(if review_resolve_idx == selected_idx {
                Style::new().bg(colors::selection())
            } else {
                Style::new()
            }),
        );
        current_line = current_line.saturating_add(1);

        let review_attempts_idx = selection_kinds.len();
        selection_kinds.push(SelectionKind::ReviewAttempts);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        runs.push(
            SelectableLineRun::selectable(
                review_attempts_idx,
                vec![self.render_row(&RowData::ReviewAttempts, review_attempts_idx == selected_idx)],
            )
            .with_style(if review_attempts_idx == selected_idx {
                Style::new().bg(colors::selection())
            } else {
                Style::new()
            }),
        );
        current_line = current_line.saturating_add(1);

        let review_section_end = current_line.saturating_sub(1);
        for idx in 0..selection_kinds.len() {
            section_bounds[idx] = (review_section_start, review_section_end);
        }

        let auto_review_section_start = current_line;
        runs.push(SelectableLineRun::plain(vec![self.render_row(
            &RowData::SectionAutoReview,
            false,
        )]));
        current_line = current_line.saturating_add(1);

        let auto_review_enabled_idx = selection_kinds.len();
        selection_kinds.push(SelectionKind::AutoReviewEnabled);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        runs.push(
            SelectableLineRun::selectable(
                auto_review_enabled_idx,
                vec![self.render_row(
                    &RowData::AutoReviewEnabled,
                    auto_review_enabled_idx == selected_idx,
                )],
            )
            .with_style(if auto_review_enabled_idx == selected_idx {
                Style::new().bg(colors::selection())
            } else {
                Style::new()
            }),
        );
        current_line = current_line.saturating_add(1);

        let auto_review_model_idx = selection_kinds.len();
        selection_kinds.push(SelectionKind::AutoReviewModel);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        runs.push(
            SelectableLineRun::selectable(
                auto_review_model_idx,
                vec![self.render_row(
                    &RowData::AutoReviewModel,
                    auto_review_model_idx == selected_idx,
                )],
            )
            .with_style(if auto_review_model_idx == selected_idx {
                Style::new().bg(colors::selection())
            } else {
                Style::new()
            }),
        );
        current_line = current_line.saturating_add(1);

        let auto_review_resolve_idx = selection_kinds.len();
        selection_kinds.push(SelectionKind::AutoReviewResolveModel);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        runs.push(
            SelectableLineRun::selectable(
                auto_review_resolve_idx,
                vec![self.render_row(
                    &RowData::AutoReviewResolveModel,
                    auto_review_resolve_idx == selected_idx,
                )],
            )
            .with_style(if auto_review_resolve_idx == selected_idx {
                Style::new().bg(colors::selection())
            } else {
                Style::new()
            }),
        );
        current_line = current_line.saturating_add(1);

        let auto_review_attempts_idx = selection_kinds.len();
        selection_kinds.push(SelectionKind::AutoReviewAttempts);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        runs.push(
            SelectableLineRun::selectable(
                auto_review_attempts_idx,
                vec![self.render_row(
                    &RowData::AutoReviewAttempts,
                    auto_review_attempts_idx == selected_idx,
                )],
            )
            .with_style(if auto_review_attempts_idx == selected_idx {
                Style::new().bg(colors::selection())
            } else {
                Style::new()
            }),
        );
        current_line = current_line.saturating_add(1);

        let auto_review_section_end = current_line.saturating_sub(1);
        for idx in review_attempts_idx.saturating_add(1)..selection_kinds.len() {
            section_bounds[idx] = (auto_review_section_start, auto_review_section_end);
        }

        ReviewListModel {
            runs,
            selection_kinds,
            selection_line,
            section_bounds,
            total_lines: current_line,
        }
    }

    fn reasoning_label(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::XHigh => "XHigh",
            ReasoningEffort::High => "High",
            ReasoningEffort::Medium => "Medium",
            ReasoningEffort::Low => "Low",
            ReasoningEffort::Minimal => "Minimal",
            ReasoningEffort::None => "None",
        }
    }

    fn format_model_label(model: &str) -> String {
        let mut parts = Vec::new();
        for (idx, part) in model.split('-').enumerate() {
            if idx == 0 {
                parts.push(part.to_ascii_uppercase());
                continue;
            }
            let mut chars = part.chars();
            let formatted = match chars.next() {
                Some(first) if first.is_ascii_alphabetic() => {
                    let mut s = String::new();
                    s.push(first.to_ascii_uppercase());
                    s.push_str(chars.as_str());
                    s
                }
                Some(first) => {
                    let mut s = String::new();
                    s.push(first);
                    s.push_str(chars.as_str());
                    s
                }
                None => String::new(),
            };
            parts.push(formatted);
        }
        parts.join("-")
    }

    fn render_header_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                "Configure /review and Auto Review models, resolve models, and follow-ups.",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Use ↑↓ to navigate · Enter select/open · Space toggle · ←→ adjust values · Esc close",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(""),
        ]
    }

    fn render_footer_lines(&self) -> Vec<Line<'static>> {
        let shortcuts = shortcut_line(&[
            KeyHint::new("↑↓", " Navigate").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter", " Select").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Space", " Toggle").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("←→", " Adjust").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Esc", " Close").with_key_style(Style::new().fg(colors::error())),
        ]);

        let notice_line = match &self.pending_notice {
            Some(notice) => Line::from(Span::styled(
                notice.clone(),
                Style::new().fg(colors::warning()),
            )),
            None => Line::default(),
        };

        vec![shortcuts, notice_line]
    }

    fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Review Settings",
            SettingsPanelStyle::bottom_pane(),
            self.render_header_lines(),
            self.render_footer_lines(),
        )
    }

    fn render_row(&self, row: &RowData, selected: bool) -> Line<'static> {
        let arrow = if selected { "› " } else { "  " };
        let arrow_style = if selected {
            Style::default().fg(colors::primary())
        } else {
            Style::default().fg(colors::text_dim())
        };
        match row {
            RowData::SectionReview => {
                Line::from(vec![Span::styled(
                    " /review (manual) ",
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                )])
            }
            RowData::SectionAutoReview => {
                Line::from(vec![Span::styled(
                    " Auto Review (background) ",
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                )])
            }
            RowData::ReviewEnabled => {
                let label_style = if selected {
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                };
                let status = toggle::on_off_word(self.review_auto_resolve_enabled);
                let status_span = Span::styled(status.text, status.style);
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Enabled", label_style),
                    Span::raw("  "),
                    status_span,
                    Span::raw("  (auto-resolve /review)"),
                ];
                if selected {
                    let hint = if self.review_auto_resolve_enabled {
                        "(press Enter to disable)"
                    } else {
                        "(press Enter to enable)"
                    };
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(hint, Style::default().fg(colors::text_dim())));
                }
                Line::from(spans)
            }
            RowData::ReviewModel => {
                let label_style = if selected {
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                };
                let value_style = if selected {
                    Style::default()
                        .fg(colors::function())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text())
                };
                let value_text = if self.review_use_chat_model {
                    "Follow Chat".to_string()
                } else {
                    format!(
                        "{} ({})",
                        Self::format_model_label(&self.review_model),
                        Self::reasoning_label(self.review_reasoning)
                    )
                };
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Review Model", label_style),
                    Span::raw("  "),
                    Span::styled(value_text, value_style),
                ];
                if selected {
                    spans.push(Span::raw("  Enter to change"));
                }
                Line::from(spans)
            }
            RowData::ReviewResolveModel => {
                let label_style = if selected {
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                };
                let value_style = if selected {
                    Style::default()
                        .fg(colors::function())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text())
                };
                let value_text = if self.review_resolve_use_chat_model {
                    "Follow Chat".to_string()
                } else {
                    format!(
                        "{} ({})",
                        Self::format_model_label(&self.review_resolve_model),
                        Self::reasoning_label(self.review_resolve_reasoning)
                    )
                };
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Resolve Model", label_style),
                    Span::raw("  "),
                    Span::styled(value_text, value_style),
                ];
                if selected {
                    spans.push(Span::raw("  Enter to change"));
                }
                Line::from(spans)
            }
            RowData::ReviewAttempts => {
                let label_style = if selected {
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else if self.review_auto_resolve_enabled {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text_dim()).add_modifier(Modifier::BOLD)
                };
                let value_style = if selected {
                    Style::default().fg(colors::function()).add_modifier(Modifier::BOLD)
                } else if self.review_followups == 0 {
                    Style::default().fg(colors::text_dim())
                } else {
                    Style::default().fg(colors::text())
                };
                let value_label = if self.review_followups == 0 {
                    "0 (no re-reviews)".to_string()
                } else if self.review_followups == 1 {
                    "1 re-review".to_string()
                } else {
                    format!("{} re-reviews", self.review_followups)
                };
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Max follow-up reviews", label_style),
                    Span::raw("  "),
                    Span::styled(value_label, value_style),
                ];
                if selected {
                    spans.push(Span::raw("  (←→ to adjust)"));
                }
                Line::from(spans)
            }
            RowData::AutoReviewEnabled => {
                let label_style = if selected {
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                };
                let status = toggle::on_off_word(self.auto_review_enabled);
                let status_span = Span::styled(status.text, status.style);
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Enabled", label_style),
                    Span::raw("  "),
                    status_span,
                    Span::raw("  (background auto review)"),
                ];
                if selected {
                    let hint = if self.auto_review_enabled {
                        "(press Enter to disable)"
                    } else {
                        "(press Enter to enable)"
                    };
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(hint, Style::default().fg(colors::text_dim())));
                }
                Line::from(spans)
            }
            RowData::AutoReviewModel => {
                let label_style = if selected {
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                };
                let value_style = if selected {
                    Style::default()
                        .fg(colors::function())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text())
                };
                let value_text = if self.auto_review_use_chat_model {
                    "Follow Chat".to_string()
                } else {
                    format!(
                        "{} ({})",
                        Self::format_model_label(&self.auto_review_model),
                        Self::reasoning_label(self.auto_review_reasoning)
                    )
                };
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Review Model", label_style),
                    Span::raw("  "),
                    Span::styled(value_text, value_style),
                ];
                if selected {
                    spans.push(Span::raw("  Enter to change"));
                }
                Line::from(spans)
            }
            RowData::AutoReviewResolveModel => {
                let label_style = if selected {
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                };
                let value_style = if selected {
                    Style::default()
                        .fg(colors::function())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text())
                };
                let value_text = if self.auto_review_resolve_use_chat_model {
                    "Follow Chat".to_string()
                } else {
                    format!(
                        "{} ({})",
                        Self::format_model_label(&self.auto_review_resolve_model),
                        Self::reasoning_label(self.auto_review_resolve_reasoning)
                    )
                };
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Resolve Model", label_style),
                    Span::raw("  "),
                    Span::styled(value_text, value_style),
                ];
                if selected {
                    spans.push(Span::raw("  Enter to change"));
                }
                Line::from(spans)
            }
            RowData::AutoReviewAttempts => {
                let label_style = if selected {
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else if self.auto_review_enabled {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text_dim()).add_modifier(Modifier::BOLD)
                };
                let value_style = if selected {
                    Style::default().fg(colors::function()).add_modifier(Modifier::BOLD)
                } else if self.auto_review_followups == 0 {
                    Style::default().fg(colors::text_dim())
                } else {
                    Style::default().fg(colors::text())
                };
                let value_label = if self.auto_review_followups == 0 {
                    "0 (no follow-ups)".to_string()
                } else if self.auto_review_followups == 1 {
                    "1 follow-up".to_string()
                } else {
                    format!("{} follow-ups", self.auto_review_followups)
                };
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Max follow-up reviews", label_style),
                    Span::raw("  "),
                    Span::styled(value_label, value_style),
                ];
                if selected {
                    spans.push(Span::raw("  (←→ to adjust)"));
                }
                Line::from(spans)
            }
        }
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.handle_key_event_impl(key_event)
    }

    fn activate_selection_kind(&mut self, kind: SelectionKind) {
        match kind {
            SelectionKind::ReviewEnabled => self.toggle_review_auto_resolve(),
            SelectionKind::ReviewAttempts => self.adjust_review_followups(true),
            SelectionKind::ReviewModel => self.open_review_model_selector(),
            SelectionKind::ReviewResolveModel => self.open_review_resolve_model_selector(),
            SelectionKind::AutoReviewEnabled => self.toggle_auto_review(),
            SelectionKind::AutoReviewModel => self.open_auto_review_model_selector(),
            SelectionKind::AutoReviewResolveModel => self.open_auto_review_resolve_model_selector(),
            SelectionKind::AutoReviewAttempts => self.adjust_auto_review_followups(true),
        }
    }

    fn ensure_selected_visible(&mut self, model: &ReviewListModel, body_height: usize) {
        if body_height == 0 || model.total_lines == 0 || model.selection_kinds.is_empty() {
            self.state.scroll_top = 0;
            return;
        }

        let total = model.selection_kinds.len();
        self.state.clamp_selection(total);
        let Some(sel_idx) = self.state.selected_idx else {
            self.state.scroll_top = 0;
            return;
        };
        let sel_idx = sel_idx.min(total.saturating_sub(1));
        self.state.selected_idx = Some(sel_idx);

        let selected_line = model
            .selection_line
            .get(sel_idx)
            .copied()
            .unwrap_or(0)
            .min(model.total_lines.saturating_sub(1));
        let (section_start, section_end) = model
            .section_bounds
            .get(sel_idx)
            .copied()
            .unwrap_or((0, model.total_lines.saturating_sub(1)));
        let section_end = section_end.min(model.total_lines.saturating_sub(1));
        let section_start = section_start.min(section_end);
        let section_len = section_end.saturating_sub(section_start).saturating_add(1);

        // If the full section fits, pin it to the top so the section header is visible.
        if section_len <= body_height {
            self.state.scroll_top = section_start;
            return;
        }

        // If we can show the section header while keeping the selection visible, do so.
        if selected_line <= section_start.saturating_add(body_height.saturating_sub(1)) {
            self.state.scroll_top = section_start;
            return;
        }

        let max_scroll_top = section_end.saturating_add(1).saturating_sub(body_height);
        let current_scroll_top = self.state.scroll_top.clamp(section_start, max_scroll_top);
        let next = scroll_top_to_keep_visible(
            current_scroll_top,
            max_scroll_top,
            body_height,
            selected_line,
            1,
        );

        self.state.scroll_top = next.clamp(section_start, max_scroll_top);
    }

    pub fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let page = self.page();
        let Some(layout) = page.layout(area) else {
            return false;
        };

        let mut model = self.build_model(self.state.selected_idx.unwrap_or(0));
        let total = model.selection_kinds.len();
        if total == 0 {
            return false;
        }

        self.state.clamp_selection(total);
        model = self.build_model(self.state.selected_idx.unwrap_or(0));
        self.ensure_selected_visible(&model, layout.body.height as usize);

        let mut selected = self.state.selected_idx.unwrap_or(0);
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            total,
            |x, y| selection_id_at(layout.body, x, y, self.state.scroll_top, &model.runs),
            SelectableListMouseConfig::default(),
        );
        self.state.selected_idx = Some(selected);

        if matches!(result, SelectableListMouseResult::Activated)
            && let Some(kind) = model.selection_kinds.get(selected).copied() {
                self.activate_selection_kind(kind);
            }
        if result.handled() {
            self.ensure_selected_visible(&model, layout.body.height as usize);
        }

        result.handled()
    }

    fn handle_key_event_impl(&mut self, key_event: KeyEvent) -> bool {
        let mut model = self.build_model(self.state.selected_idx.unwrap_or(0));
        let total = model.selection_kinds.len();
        if total == 0 {
            if matches!(key_event.code, KeyCode::Esc) {
                self.is_complete = true;
                return true;
            }
            return false;
        }
        self.state.clamp_selection(total);
        let body_height_hint = match self.viewport_rows.get() {
            0 => DEFAULT_VISIBLE_ROWS,
            other => other,
        };
        model = self.build_model(self.state.selected_idx.unwrap_or(0));
        self.ensure_selected_visible(&model, body_height_hint);
        let current_kind = self
            .state
            .selected_idx
            .and_then(|sel| model.selection_kinds.get(sel))
            .copied();
        let handled = match key_event {
            KeyEvent { code: KeyCode::Up, .. } => {
                self.state.move_up_wrap(total);
                true
            }
            KeyEvent { code: KeyCode::Down, .. } => {
                self.state.move_down_wrap(total);
                true
            }
            KeyEvent { code: KeyCode::Left, .. } => {
                if let Some(kind) = current_kind {
                    match kind {
                        SelectionKind::ReviewEnabled => self.toggle_review_auto_resolve(),
                        SelectionKind::ReviewAttempts => self.adjust_review_followups(false),
                        SelectionKind::AutoReviewEnabled => self.toggle_auto_review(),
                        SelectionKind::AutoReviewAttempts => {
                            self.adjust_auto_review_followups(false)
                        }
                        SelectionKind::ReviewModel
                        | SelectionKind::ReviewResolveModel
                        | SelectionKind::AutoReviewModel
                        | SelectionKind::AutoReviewResolveModel => {}
                    }
                }
                true
            }
            KeyEvent { code: KeyCode::Right, .. } => {
                if let Some(kind) = current_kind {
                    match kind {
                        SelectionKind::ReviewEnabled => self.toggle_review_auto_resolve(),
                        SelectionKind::ReviewAttempts => self.adjust_review_followups(true),
                        SelectionKind::AutoReviewEnabled => self.toggle_auto_review(),
                        SelectionKind::AutoReviewAttempts => {
                            self.adjust_auto_review_followups(true)
                        }
                        SelectionKind::ReviewModel
                        | SelectionKind::ReviewResolveModel
                        | SelectionKind::AutoReviewModel
                        | SelectionKind::AutoReviewResolveModel => {}
                    }
                }
                true
            }
            KeyEvent { code: KeyCode::Char(' '), .. }
            | KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                if let Some(kind) = current_kind {
                    self.activate_selection_kind(kind);
                }
                true
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        };

        self.state.clamp_selection(total);
        self.ensure_selected_visible(&model, body_height_hint);
        handled
    }
}

impl<'a> BottomPaneView<'a> for ReviewSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_impl(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_impl(key_event))
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
        12
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let page = self.page();
        let model = self.build_model(self.state.selected_idx.unwrap_or(0));
        let mut rects = Vec::new();
        let Some(layout) =
            page.render_runs(area, buf, self.state.scroll_top, &model.runs, &mut rects)
        else {
            return;
        };
        let visible_slots = layout.body.height as usize;
        self.viewport_rows.set(visible_slots);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn make_view() -> ReviewSettingsView {
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        ReviewSettingsView::new(ReviewSettingsInit {
            review_use_chat_model: false,
            review_model: "gpt-5.4".to_string(),
            review_reasoning: ReasoningEffort::Medium,
            review_resolve_use_chat_model: false,
            review_resolve_model: "gpt-5.4".to_string(),
            review_resolve_reasoning: ReasoningEffort::Medium,
            review_auto_resolve_enabled: true,
            review_followups: AutoResolveAttemptLimit::DEFAULT,
            auto_review_enabled: true,
            auto_review_use_chat_model: false,
            auto_review_model: "gpt-5.4".to_string(),
            auto_review_reasoning: ReasoningEffort::Medium,
            auto_review_resolve_use_chat_model: false,
            auto_review_resolve_model: "gpt-5.4".to_string(),
            auto_review_resolve_reasoning: ReasoningEffort::Medium,
            auto_review_followups: AutoResolveAttemptLimit::DEFAULT,
            app_event_tx: AppEventSender::new(tx),
        })
    }

    #[test]
    fn selection_index_to_kind_order_is_stable() {
        let view = make_view();
        let model = view.build_model(0);
        assert_eq!(
            model.selection_kinds,
            vec![
                SelectionKind::ReviewEnabled,
                SelectionKind::ReviewModel,
                SelectionKind::ReviewResolveModel,
                SelectionKind::ReviewAttempts,
                SelectionKind::AutoReviewEnabled,
                SelectionKind::AutoReviewModel,
                SelectionKind::AutoReviewResolveModel,
                SelectionKind::AutoReviewAttempts,
            ]
        );
    }

    #[test]
    fn ensure_selected_visible_clamps_scroll_within_section() {
        let mut view = make_view();
        view.state.selected_idx = Some(3);
        view.state.scroll_top = 0;
        let model = view.build_model(view.state.selected_idx.unwrap_or(0));
        view.ensure_selected_visible(&model, 3);
        assert_eq!(view.state.scroll_top, 2);
    }

    #[test]
    fn selection_id_at_matches_run_geometry_with_scroll() {
        let view = make_view();
        let model = view.build_model(0);
        let area = Rect::new(2, 4, 20, 10);

        assert_eq!(selection_id_at(area, 3, 4, 0, &model.runs), None);
        assert_eq!(selection_id_at(area, 3, 5, 0, &model.runs), Some(0));
        assert_eq!(selection_id_at(area, 3, 6, 0, &model.runs), Some(1));
        assert_eq!(selection_id_at(area, 3, 4, 2, &model.runs), Some(1));
    }
}
