use super::*;

impl ChatComposer {
    pub(super) fn build_auto_drive_hint_spans(
        text: &str,
        key_style: Style,
        label_style: Style,
    ) -> Vec<Span<'static>> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let leading_bullet = text.trim_start().starts_with('•');
        for (index, part) in text
            .split('•')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .enumerate()
        {
            if index > 0 {
                spans.push(Span::from("  • ").style(label_style));
            } else if leading_bullet {
                spans.push(Span::from("• ").style(label_style));
            }
            let (key, label) = Self::split_auto_drive_key_label(part);
            if let Some(key) = key {
                spans.push(Span::from(key).style(key_style));
                if !label.is_empty() {
                    let mut spaced_label = String::with_capacity(label.len() + 1);
                    spaced_label.push(' ');
                    spaced_label.push_str(&label);
                    spans.push(Span::from(spaced_label).style(label_style));
                }
            } else if !label.is_empty() {
                spans.push(Span::from(label).style(label_style));
            }
        }

        spans
    }

    fn split_auto_drive_key_label(part: &str) -> (Option<String>, String) {
        if part.is_empty() {
            return (None, String::new());
        }
        if let Some((first, rest)) = part.split_once(' ') {
            let key = first.trim();
            let remainder = rest.trim_start();
            if Self::is_auto_drive_key(key) {
                return (Some(key.to_string()), remainder.to_string());
            }
        }
        (None, part.to_string())
    }

    fn is_auto_drive_key(token: &str) -> bool {
        let normalized = token.trim();
        if normalized.is_empty() {
            return false;
        }
        if normalized.contains('+') {
            let mut parts = normalized.split('+');
            let Some(first) = parts.next() else {
                return false;
            };
            if !matches!(first, "Ctrl" | "Alt" | "Shift" | "Meta" | "Cmd") {
                return false;
            }
            let mut last = first;
            let mut saw_extra = false;
            for part in parts {
                if part.is_empty()
                    || part.chars().count() > 10
                    || !part.chars().all(|ch| ch.is_ascii_alphanumeric())
                {
                    return false;
                }
                last = part;
                saw_extra = true;
            }
            return saw_extra && !matches!(last, "Ctrl" | "Alt" | "Shift" | "Meta" | "Cmd");
        }
        matches!(
            normalized,
            "Esc"
                | "Enter"
                | "Tab"
                | "Space"
                | "Backspace"
                | "Delete"
                | "Insert"
                | "Home"
                | "End"
                | "PageUp"
                | "PageDown"
                | "Up"
                | "Down"
                | "Left"
                | "Right"
                | "F1"
                | "F2"
                | "F3"
                | "F4"
                | "F5"
                | "F6"
                | "F7"
                | "F8"
                | "F9"
                | "F10"
                | "F11"
                | "F12"
        )
    }

    pub(super) fn auto_review_footer_sections(
        status: AutoReviewFooterStatus,
        agent_hint_label: AgentHintLabel,
    ) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        let key_hint_style = Style::default().fg(crate::colors::function());
        let label_style = Style::default().fg(crate::colors::text_dim());

        let agent_hint_label_text = match agent_hint_label {
            AgentHintLabel::Review => " show review",
            AgentHintLabel::Agents => " show agents",
        };

        let agent_hint_spans = match status.status {
            AutoReviewIndicatorStatus::Failed => Vec::new(),
            _ => vec![
                Span::styled("Ctrl+A", key_hint_style),
                Span::from(agent_hint_label_text).style(label_style),
            ],
        };

        let status_spans = match status.status {
            AutoReviewIndicatorStatus::Running => {
                let phase_label = match status.phase {
                    AutoReviewPhase::Resolving => "Resolving",
                    AutoReviewPhase::Reviewing => "Reviewing",
                };
                vec![
                    Span::styled("Auto Review: ", label_style),
                    Span::styled("•", key_hint_style),
                    Span::from(" "),
                    Span::styled(phase_label, key_hint_style),
                ]
            }
            AutoReviewIndicatorStatus::Clean => {
                vec![
                    Span::styled("Auto Review: ", label_style),
                    Span::styled("✓", key_hint_style),
                    Span::from(" "),
                    Span::styled("Correct", key_hint_style),
                ]
            }
            AutoReviewIndicatorStatus::Fixed => {
                let icon_style = Style::default().fg(crate::colors::success());
                let text = if let Some(count) = status.findings {
                    let plural = if count == 1 { "Issue" } else { "Issues" };
                    format!("{count} {plural} Fixed")
                } else {
                    "Issues Fixed".to_string()
                };
                vec![
                    Span::styled("Auto Review: ", label_style),
                    Span::styled("✓", icon_style),
                    Span::from(" "),
                    Span::styled(text, icon_style),
                ]
            }
            AutoReviewIndicatorStatus::Failed => {
                let icon_style = Style::default().fg(crate::colors::error());
                vec![
                    Span::styled("Auto Review: ", label_style),
                    Span::styled("✗", icon_style),
                    Span::from(" "),
                    Span::styled("Failed", icon_style),
                ]
            }
        };

        (status_spans, agent_hint_spans)
    }
}
