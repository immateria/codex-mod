use code_core::config_types::{AutoResolveAttemptLimit, ReasoningEffort};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use std::cell::Cell;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::frame::{compute_settings_frame_layout, render_settings_frame};
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
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

    fn build_rows(&self) -> (Vec<RowData>, Vec<usize>, Vec<SelectionKind>) {
        let rows = vec![
            RowData::SectionReview,
            RowData::ReviewEnabled,
            RowData::ReviewModel,
            RowData::ReviewResolveModel,
            RowData::ReviewAttempts,
            RowData::SectionAutoReview,
            RowData::AutoReviewEnabled,
            RowData::AutoReviewModel,
            RowData::AutoReviewResolveModel,
            RowData::AutoReviewAttempts,
        ];
        let selection_rows = vec![1, 2, 3, 4, 6, 7, 8, 9];
        let selection_kinds = vec![
            SelectionKind::ReviewEnabled,
            SelectionKind::ReviewModel,
            SelectionKind::ReviewResolveModel,
            SelectionKind::ReviewAttempts,
            SelectionKind::AutoReviewEnabled,
            SelectionKind::AutoReviewModel,
            SelectionKind::AutoReviewResolveModel,
            SelectionKind::AutoReviewAttempts,
        ];
        (rows, selection_rows, selection_kinds)
    }

    fn visible_budget(&self, total: usize) -> usize {
        if total == 0 {
            return 1;
        }
        let hint = self.viewport_rows.get();
        let target = if hint == 0 { DEFAULT_VISIBLE_ROWS } else { hint };
        target.clamp(1, total)
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
        let mut lines = vec![Line::from(vec![
            Span::styled("↑↓", Style::default().fg(colors::function())),
            Span::styled(" Navigate  ", Style::default().fg(colors::text_dim())),
            Span::styled("Enter", Style::default().fg(colors::success())),
            Span::styled(" Select  ", Style::default().fg(colors::text_dim())),
            Span::styled("Space", Style::default().fg(colors::success())),
            Span::styled(" Toggle  ", Style::default().fg(colors::text_dim())),
            Span::styled("←→", Style::default().fg(colors::function())),
            Span::styled(" Adjust  ", Style::default().fg(colors::text_dim())),
            Span::styled("Esc", Style::default().fg(colors::error())),
            Span::styled(" Close", Style::default().fg(colors::text_dim())),
        ])];
        if let Some(notice) = &self.pending_notice {
            lines.push(Line::from(vec![Span::styled(
                notice.clone(),
                Style::default().fg(colors::warning()),
            )]));
        }
        lines
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
                let status_span = if self.review_auto_resolve_enabled {
                    Span::styled("On", Style::default().fg(colors::success()))
                } else {
                    Span::styled("Off", Style::default().fg(colors::text_dim()))
                };
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
                let status_span = if self.auto_review_enabled {
                    Span::styled("On", Style::default().fg(colors::success()))
                } else {
                    Span::styled("Off", Style::default().fg(colors::text_dim()))
                };
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

    fn selection_index_at(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        let header_lines = self.render_header_lines();
        let footer_lines = self.render_footer_lines();
        let layout =
            compute_settings_frame_layout(
                area,
                " Review Settings ",
                header_lines.len(),
                footer_lines.len(),
            )?;
        if !layout.body.contains(ratatui::layout::Position { x, y }) {
            return None;
        }
        let row_idx = y.saturating_sub(layout.body.y) as usize;
        let (_, selection_rows, _) = self.build_rows();
        selection_rows.iter().position(|&row| row == row_idx)
    }

    pub fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let (_, _, selection_kinds) = self.build_rows();
        let total = selection_kinds.len();
        if total == 0 {
            return false;
        }

        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);

        let mut selected = self.state.selected_idx.unwrap_or(0);
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            total,
            |x, y| self.selection_index_at(area, x, y),
            SelectableListMouseConfig::default(),
        );
        self.state.selected_idx = Some(selected);

        if matches!(result, SelectableListMouseResult::Activated)
            && let Some(kind) = selection_kinds.get(selected).copied() {
                self.activate_selection_kind(kind);
            }
        if result.handled() {
            let visible_budget = self.visible_budget(total);
            self.state.ensure_visible(total, visible_budget);
        }

        result.handled()
    }

    fn handle_key_event_impl(&mut self, key_event: KeyEvent) -> bool {
        let (_, _, selection_kinds) = self.build_rows();
        let mut total = selection_kinds.len();
        if total == 0 {
            if matches!(key_event.code, KeyCode::Esc) {
                self.is_complete = true;
                return true;
            }
            return false;
        }
        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        let visible_budget = self.visible_budget(total);
        self.state.ensure_visible(total, visible_budget);
        let current_kind = self
            .state
            .selected_idx
            .and_then(|sel| selection_kinds.get(sel))
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

        let (_, _, selection_kinds) = self.build_rows();
        total = selection_kinds.len();
        if total == 0 {
            self.state.selected_idx = None;
            self.state.scroll_top = 0;
        } else {
            self.state.clamp_selection(total);
            let visible_budget = self.visible_budget(total);
            self.state.ensure_visible(total, visible_budget);
        }
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
        let header_lines = self.render_header_lines();
        let footer_lines = self.render_footer_lines();
        let Some(layout) = render_settings_frame(
            area,
            buf,
            " Review Settings ",
            header_lines,
            footer_lines,
        ) else {
            return;
        };
        let visible_slots = layout.visible_rows;
        self.viewport_rows.set(visible_slots);

        let (rows, selection_rows, _) = self.build_rows();
        let selection_count = selection_rows.len();
        let selected_idx = self.state.selected_idx.unwrap_or(0).min(selection_count.saturating_sub(1));
        let selected_row_index = selection_rows.get(selected_idx).copied().unwrap_or(0);

        let mut visible_lines: Vec<Line> = Vec::new();

        let mut remaining = visible_slots;
        let mut row_index = 0;
        while remaining > 0 && row_index < rows.len() {
            let is_selected = row_index == selected_row_index;
            visible_lines.push(self.render_row(&rows[row_index], is_selected));
            remaining = remaining.saturating_sub(1);
            row_index += 1;
        }

        Paragraph::new(visible_lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .render(layout.body, buf);
    }
}
