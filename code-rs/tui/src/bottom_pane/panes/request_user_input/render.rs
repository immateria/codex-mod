use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};

use crate::components::selection_popup_common::{render_rows, GenericDisplayRow};

use super::RequestUserInputView;

impl RequestUserInputView {
    pub(super) fn desired_height_for_width(&self, width: u16) -> u16 {
        let inner_width = width.saturating_sub(2);
        let question = self.current_question();

        let prompt_lines = question
            .map(|q| {
                let wrapped = textwrap::wrap(&q.question, inner_width.max(1) as usize);
                u16::try_from(wrapped.len()).unwrap_or(u16::MAX)
            })
            .unwrap_or(1)
            .clamp(1, 3);

        let options_len = question
            .map(|q| {
                let base_len = q.options.as_ref().map_or(0, std::vec::Vec::len);
                if base_len > 0 && q.is_other {
                    base_len + 1
                } else {
                    base_len
                }
            })
            .unwrap_or(0);

        let options_lines = if options_len > 0 {
            u16::try_from(options_len.min(6)).unwrap_or(6).max(2)
        } else {
            3
        };

        // Borders (2) + progress (1) + header (1) + prompt (N) + content (M) + footer (1)
        // + 1 to account for BottomPane's reserved bottom padding line.
        (2 + 1 + 1 + prompt_lines + options_lines + 1 + 1).max(8)
    }

    pub(super) fn render_direct(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title("User input")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let question_count = self.question_count();
        let (header, prompt, options) = self
            .current_question()
            .map(|q| (q.header.as_str(), q.question.as_str(), q.options.as_ref()))
            .unwrap_or(("No questions", "", None));
        let is_secret = self.current_question().is_some_and(|q| q.is_secret);
        let mask_secret = |value: &str| "*".repeat(value.chars().count());

        let mut y = inner.y;
        let progress = if question_count > 0 {
            format!("Question {}/{}", self.current_idx + 1, question_count)
        } else {
            "Question 0/0".to_string()
        };
        Paragraph::new(Line::from(progress).dim()).render(
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
            buf,
        );
        y = y.saturating_add(1);

        Paragraph::new(Line::from(header.bold())).render(
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
            buf,
        );
        y = y.saturating_add(1);

        let footer_height = 1u16;
        let available = inner
            .height
            .saturating_sub((y - inner.y).saturating_add(footer_height));
        let has_options = options.is_some_and(|opts| !opts.is_empty());
        let accepts_freeform = self.current_accepts_freeform();
        let desired_prompt_height = if prompt.trim().is_empty() {
            0u16
        } else {
            let wrapped = textwrap::wrap(prompt, inner.width.max(1) as usize);
            u16::try_from(wrapped.len()).unwrap_or(u16::MAX).clamp(1, 3)
        };

        // Always reserve at least 2 rows so option pickers are usable.
        let (prompt_height, content_height) = if available == 0 {
            (0, 0)
        } else if available <= 2 {
            (0, available)
        } else {
            let prompt_budget = available.saturating_sub(2);
            let prompt_height = desired_prompt_height.min(prompt_budget);
            let content_height = available.saturating_sub(prompt_height);
            (prompt_height, content_height)
        };

        if prompt_height > 0 {
            let prompt_rect = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: prompt_height,
            };
            Paragraph::new(prompt)
                .wrap(Wrap { trim: true })
                .render(prompt_rect, buf);
            y = y.saturating_add(prompt_height);
        }

        let content_rect = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: content_height,
        };

        if content_rect.height > 0 {
            if let Some(options) = options.filter(|opts| !opts.is_empty()) {
                let state = self
                    .current_answer()
                    .map(|answer| answer.option_state)
                    .unwrap_or_default();
                let selected = state.selected_idx;
                let hovered = self.current_answer().and_then(|answer| answer.hover_option_idx);
                let mut rows = options
                    .iter()
                    .enumerate()
                    .map(|(idx, opt)| {
                        let prefix = if selected.is_some_and(|sel| sel == idx) {
                            "(x)"
                        } else {
                            "( )"
                        };
                        GenericDisplayRow {
                            name: format!("{prefix} {}", opt.label),
                            description: Some(opt.description.clone()),
                            match_indices: None,
                            is_current: hovered.is_some_and(|hovered| hovered == idx),
                            name_color: None,
                        }
                    })
                    .collect::<Vec<_>>();
                if self.current_is_other() {
                    let other_idx = options.len();
                    let is_selected = selected.is_some_and(|sel| sel == other_idx);
                    let other_value = self
                        .current_answer()
                        .map(|answer| answer.freeform.trim_end())
                        .unwrap_or("");
                    let other_value = if is_secret && !other_value.is_empty() {
                        mask_secret(other_value)
                    } else {
                        other_value.to_string()
                    };
                    let other_label = if is_selected && !other_value.is_empty() {
                        format!("Other: {other_value}")
                    } else {
                        "Other".to_string()
                    };
                    let prefix = if is_selected { "(x)" } else { "( )" };
                    rows.push(GenericDisplayRow {
                        name: format!("{prefix} {other_label}"),
                        description: Some("Provide a custom answer".to_string()),
                        match_indices: None,
                        is_current: hovered.is_some_and(|hovered| hovered == other_idx),
                        name_color: None,
                    });
                }
                render_rows(content_rect, buf, &rows, &state, rows.len().max(1), false);
            } else {
                let text = self.current_answer().map_or("", |a| a.freeform.as_str());
                let display_text = if is_secret && !text.is_empty() {
                    mask_secret(text)
                } else {
                    text.to_string()
                };
                let placeholder = "Type your answer…";
                let display = if display_text.is_empty() {
                    Line::from(placeholder).dim()
                } else {
                    Line::from(display_text)
                };
                Paragraph::new(display)
                    .wrap(Wrap { trim: true })
                    .render(content_rect, buf);
            }
        }

        let footer_y = inner.y.saturating_add(inner.height).saturating_sub(1);
        let is_last = question_count > 0 && self.current_idx + 1 >= question_count;
        let enter_label = if is_last { "submit" } else { "next" };
        let footer = if has_options {
            if accepts_freeform {
                format!(
                    "↑/↓ select | Type other answer | Enter {enter_label} | Esc type in composer | PgUp/PgDn prev/next"
                )
            } else {
                format!(
                    "↑/↓ select | Enter {enter_label} | Esc type in composer | PgUp/PgDn prev/next"
                )
            }
        } else {
            format!(
                "Type answer | Enter {enter_label} | Esc type in composer | PgUp/PgDn prev/next"
            )
        };
        Paragraph::new(Line::from(vec![Span::raw(footer)]).dim()).render(
            Rect {
                x: inner.x,
                y: footer_y,
                width: inner.width,
                height: 1,
            },
            buf,
        );
    }

    pub(super) fn should_handle_key_event(&self, key_event: &KeyEvent) -> bool {
        matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
    }
}

