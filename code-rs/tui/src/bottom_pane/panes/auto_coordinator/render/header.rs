use std::borrow::Cow;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::prelude::Widget;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::auto_drive_strings;
use crate::colors;
use crate::glitch_animation::{gradient_multi, mix_rgb};
use crate::spinner;

use super::super::{style, AutoActiveViewModel, AutoCoordinatorView};
use super::{HeaderRenderParams, IntroState};

pub(super) fn intro_state<'a>(header_text: &'a str, model: &AutoActiveViewModel) -> IntroState<'a> {
    const LETTER_INTERVAL_MS: u64 = 32;
    const BODY_DELAY_MS: u64 = 90;
    const MIN_FRAME_MS: u64 = 16;

    if header_text.is_empty() || model.intro_reduced_motion {
        return IntroState {
            header_text: Cow::Borrowed(header_text),
            body_visible: true,
            schedule_next_in: None,
        };
    }

    let Some(started) = model.intro_started_at else {
        return IntroState {
            header_text: Cow::Borrowed(header_text),
            body_visible: true,
            schedule_next_in: None,
        };
    };

    let total_chars = header_text.chars().count();
    if total_chars == 0 {
        return IntroState {
            header_text: Cow::Borrowed(header_text),
            body_visible: true,
            schedule_next_in: None,
        };
    }

    let now = Instant::now();
    let elapsed = now.saturating_duration_since(started);
    let interval_ms = LETTER_INTERVAL_MS as u128;
    let stage = (elapsed.as_millis() / interval_ms) as usize;
    let mut visible = stage.saturating_add(1);
    if visible > total_chars {
        visible = total_chars;
    }

    let header_completion_ms = if total_chars <= 1 {
        0
    } else {
        LETTER_INTERVAL_MS * (total_chars as u64 - 1)
    };
    let header_completion = Duration::from_millis(header_completion_ms);
    let body_delay = Duration::from_millis(BODY_DELAY_MS);
    let header_done = elapsed >= header_completion;
    let body_visible = header_done && elapsed >= header_completion + body_delay;

    let header_text = if visible >= total_chars {
        Cow::Borrowed(header_text)
    } else {
        Cow::Owned(header_text.chars().take(visible).collect())
    };

    let mut schedule_next_in = None;
    if !body_visible {
        let next_target = if visible < total_chars {
            Duration::from_millis(LETTER_INTERVAL_MS * visible as u64)
        } else {
            header_completion + body_delay
        };

        let mut remaining = if next_target > elapsed {
            next_target - elapsed
        } else {
            Duration::from_millis(0)
        };

        if remaining == Duration::from_millis(0) {
            remaining = Duration::from_millis(MIN_FRAME_MS);
        }

        let min_delay = Duration::from_millis(MIN_FRAME_MS);
        schedule_next_in = Some(remaining.max(min_delay));
    }

    IntroState {
        header_text,
        body_visible,
        schedule_next_in,
    }
}

pub(super) fn effective_elapsed(model: &AutoActiveViewModel) -> Option<Duration> {
    if let Some(duration) = model.elapsed {
        Some(duration)
    } else {
        model.started_at
            .map(|started| Instant::now().saturating_duration_since(started))
    }
}

fn status_label(model: &AutoActiveViewModel) -> &'static str {
    if model.waiting_for_review {
        "Awaiting review"
    } else if model.awaiting_submission {
        "Waiting"
    } else if model.coordinator_waiting {
        "Creating prompt"
    } else if model.cli_running {
        "Running"
    } else if model.waiting_for_response {
        "Thinking"
    } else if model.started_at.is_some() {
        "Running"
    } else if model.elapsed.is_some() && model.started_at.is_none() {
        "Stopped"
    } else {
        "Ready"
    }
}

fn is_generic_status_message(message: &str) -> bool {
    matches!(message, "Auto Drive" | "Auto Drive Goal")
}

pub(super) fn resolve_display_message(view: &AutoCoordinatorView, model: &AutoActiveViewModel) -> String {
    if let Some(message) = view
        .status_message
        .as_ref()
        .map(|msg| msg.trim())
        .filter(|msg| !msg.is_empty())
        .filter(|msg| !is_generic_status_message(msg))
    {
        return message.to_string();
    }

    if let Some(current) = model.status_title.as_ref() {
        let trimmed = current.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if model.awaiting_submission {
        if let Some(countdown) = &model.countdown {
            return format!("Awaiting confirmation ({}s)", countdown.remaining);
        }
        if let Some(button) = &model.button {
            let trimmed = button.label.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    for status in &model.status_lines {
        let trimmed = status.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    auto_drive_strings::next_auto_drive_phrase().to_string()
}

pub(super) fn runtime_text(_view: &AutoCoordinatorView, model: &AutoActiveViewModel) -> String {
    let label = status_label(model);
    let mut details: Vec<String> = Vec::new();
    if let Some(duration) = effective_elapsed(model) && duration.as_secs() > 0 {
        details.push(AutoCoordinatorView::format_elapsed(duration));
    }
    if let Some(tokens) = model.session_tokens {
        details.push(AutoCoordinatorView::format_tokens(tokens));
    }
    if model.turns_completed > 0 {
        details.push(AutoCoordinatorView::format_turns(model.turns_completed));
    }
    if details.is_empty() {
        label.to_string()
    } else {
        format!("{} ({})", label, details.join(", "))
    }
}

pub(super) fn render_header(view: &AutoCoordinatorView, buf: &mut Buffer, params: HeaderRenderParams<'_>) {
    let HeaderRenderParams {
        area,
        model,
        frame_style,
        display_message,
        header_label,
        full_title,
        intro,
    } = params;
    if area.width == 0 || area.height == 0 {
        return;
    }

    let animating = intro.schedule_next_in.is_some() && !model.intro_reduced_motion;
    let mut base_spans: Vec<Span<'static>> = Vec::new();
    base_spans.push(Span::raw(" "));

    let border_gradient = view.style.composer.border_gradient;
    let (text_left, text_right) = border_gradient
        .map(style::text_gradient_colors)
        .unwrap_or((colors::primary(), colors::text_dim()));
    let fallback_color = if border_gradient.is_some() {
        text_left
    } else {
        frame_style
            .border_style
            .fg
            .or(frame_style.title_style.fg)
            .unwrap_or(text_left)
    };

    if animating {
        let total_chars = full_title.chars().count().max(1);
        let visible_chars: Vec<char> = header_label.chars().collect();
        if !visible_chars.is_empty() {
            for (idx, ch) in visible_chars.iter().enumerate() {
                let gradient_position = if total_chars > 1 {
                    idx as f32 / (total_chars as f32 - 1.0)
                } else {
                    0.0
                };
                let mut color = gradient_multi(gradient_position);
                if visible_chars.len() == total_chars {
                    color = mix_rgb(color, fallback_color, 0.65);
                } else if idx == visible_chars.len().saturating_sub(1) {
                    #[allow(clippy::disallowed_methods)]
                    {
                        color = mix_rgb(color, Color::Rgb(255, 255, 255), 0.35);
                    }
                }
                base_spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ));
            }
        }
    } else {
        let mut title_style = frame_style.title_style;
        title_style.fg = Some(fallback_color);
        title_style = title_style.add_modifier(Modifier::BOLD);
        base_spans.push(Span::styled(header_label.to_string(), title_style));
    }

    base_spans.push(Span::styled(" > ", Style::default().fg(colors::text_dim())));
    let message_style = Style::default().fg(colors::text());
    let default_message_span = Span::styled(display_message.to_string(), message_style);
    let base_line = {
        let mut spans = base_spans.clone();
        spans.push(default_message_span.clone());
        Line::from(spans)
    };

    let runtime_text = runtime_text(view, model);
    let runtime_color = if border_gradient.is_some() {
        text_right
    } else {
        colors::text_dim()
    };
    let mut right_spans: Vec<Span<'static>> = Vec::new();
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let spinner_frame = spinner::frame_at_time(spinner::current_spinner(), now_ms);
    right_spans.push(Span::raw(" "));
    right_spans.push(Span::styled(spinner_frame, Style::default().fg(runtime_color)));
    if !runtime_text.is_empty() {
        right_spans.push(Span::raw(" "));
        right_spans.push(Span::styled(runtime_text, Style::default().fg(runtime_color)));
    }
    let right_line = Line::from(right_spans.clone());
    let right_width = right_line.width().min(area.width as usize) as u16;
    let constraints = if right_width == 0 {
        vec![Constraint::Fill(1)]
    } else {
        vec![Constraint::Fill(1), Constraint::Length(right_width)]
    };
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    let left_available = chunks[0].width;
    let mut left_line = base_line.clone();
    let show_progress_hint = model.cli_running
        || model.awaiting_submission
        || (model.waiting_for_response && !model.coordinator_waiting);
    if show_progress_hint && left_available > 0 {
        let (status_sent_to_user, status_title) = status_labels(model);
        let status_style = Style::default().fg(colors::text_dim());

        let try_apply = |content: &str| -> Option<Line<'static>> {
            let mut candidate_spans = base_spans.clone();
            candidate_spans.push(Span::styled(content.to_string(), status_style));
            let candidate_line = Line::from(candidate_spans.clone());
            if candidate_line.width() <= left_available as usize {
                Some(candidate_line)
            } else {
                None
            }
        };

        match (status_title.as_ref(), status_sent_to_user.as_ref()) {
            (Some(title), Some(sent)) => {
                if let Some(line) = try_apply(&format!("{title} · {sent}")) {
                    left_line = line;
                } else if let Some(line) = try_apply(title) {
                    left_line = line;
                } else if let Some(line) = try_apply(sent) {
                    left_line = line;
                }
            }
            (Some(title), None) => {
                if let Some(line) = try_apply(title) {
                    left_line = line;
                }
            }
            (None, Some(sent)) => {
                if let Some(line) = try_apply(sent) {
                    left_line = line;
                }
            }
            (None, None) => {}
        }
    }

    Paragraph::new(left_line).render(chunks[0], buf);

    if right_width > 0 {
        Paragraph::new(right_line)
            .alignment(Alignment::Right)
            .render(chunks[chunks.len() - 1], buf);
    }
}

pub(super) fn status_labels(model: &AutoActiveViewModel) -> (Option<String>, Option<String>) {
    let sent_to_user = model.status_sent_to_user.as_ref().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    let title = model.status_title.as_ref().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    (sent_to_user, title)
}

pub(super) fn compose_status_line(model: &AutoActiveViewModel) -> Option<String> {
    let (sent_to_user, title) = status_labels(model);
    match (title, sent_to_user) {
        (Some(title), Some(sent)) => Some(format!("{title} · {sent}")),
        (Some(title), None) => Some(title),
        (None, Some(sent)) => Some(sent),
        (None, None) => None,
    }
}
