use std::time::Duration;

use code_common::elapsed::format_duration_digital;

use super::BrowserSessionCell;

#[derive(Clone)]
pub(super) struct ActionEntry {
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) time_label: String,
}

pub(super) enum ActionDisplayLine {
    Entry(ActionEntry),
    Ellipsis,
}

#[derive(Clone)]
pub(super) struct BrowserAction {
    pub(super) action: String,
    pub(super) target: Option<String>,
    pub(super) value: Option<String>,
    pub(super) outcome: Option<String>,
    pub(super) timestamp: Duration,
}

impl BrowserSessionCell {
    pub(crate) fn full_action_entries(&self) -> Vec<(String, String, String)> {
        let show_minutes = self.total_duration.as_secs() >= 60;
        let mut entries: Vec<(String, String, String)> = Vec::new();
        if self.actions.is_empty() {
            if let Some(url) = self.url.as_ref() {
                let time_label =
                    format!(" {}", Self::format_elapsed_label(Duration::ZERO, show_minutes));
                entries.push((time_label, "Opened".to_string(), url.clone()));
            }
            return entries;
        }

        for action in &self.actions {
            let time_label = format!(
                " {}",
                Self::format_elapsed_label(action.timestamp, show_minutes)
            );
            let entry = format_action_entry(action, time_label);
            entries.push((entry.time_label, entry.label, entry.detail));
        }

        entries
    }

    pub(super) fn formatted_action_display(&self, show_minutes: bool) -> Vec<ActionDisplayLine> {
        let mut entries: Vec<ActionEntry> = Vec::new();
        let has_actions = !self.actions.is_empty();
        if !has_actions
            && let Some(url) = self.url.as_ref()
        {
            entries.push(ActionEntry {
                label: "Opened".to_string(),
                detail: url.clone(),
                time_label: format!(
                    " {}",
                    Self::format_elapsed_label(Duration::ZERO, show_minutes)
                ),
            });
        }

        entries.extend(self.actions.iter().map(|action| {
            let time_label = format!(
                " {}",
                Self::format_elapsed_label(action.timestamp, show_minutes)
            );
            format_action_entry(action, time_label)
        }));

        if entries.is_empty() {
            return Vec::new();
        }

        if entries.len() > super::ACTION_DISPLAY_HEAD + super::ACTION_DISPLAY_TAIL {
            let mut display: Vec<ActionDisplayLine> = Vec::new();
            for entry in entries.iter().take(super::ACTION_DISPLAY_HEAD) {
                display.push(ActionDisplayLine::Entry(entry.clone()));
            }
            display.push(ActionDisplayLine::Ellipsis);
            for entry in entries
                .iter()
                .rev()
                .take(super::ACTION_DISPLAY_TAIL)
                .rev()
                .cloned()
            {
                display.push(ActionDisplayLine::Entry(entry));
            }
            display
        } else {
            entries
                .into_iter()
                .map(ActionDisplayLine::Entry)
                .collect()
        }
    }

    pub(crate) fn format_elapsed_label(duration: Duration, _show_minutes: bool) -> String {
        format_duration_digital(duration)
    }
}

fn format_action_summary(action: &BrowserAction) -> String {
    match (&action.target, &action.value, &action.outcome) {
        (Some(target), Some(value), Some(outcome)) => {
            format!(
                "{} {} → {}",
                action.action,
                target,
                outcome_for_display(outcome, value)
            )
        }
        (Some(target), Some(value), None) => format!("{} {} = {}", action.action, target, value),
        (Some(target), None, Some(outcome)) => format!("{} {} → {}", action.action, target, outcome),
        (Some(target), None, None) => format!("{} {}", action.action, target),
        (None, Some(value), Some(outcome)) => format!("{} {} → {}", action.action, value, outcome),
        (None, Some(value), None) => format!("{} {}", action.action, value),
        (None, None, Some(outcome)) => format!("{} → {}", action.action, outcome),
        _ => action.action.clone(),
    }
}

fn format_action_entry(action: &BrowserAction, time_label: String) -> ActionEntry {
    let action_lower = action.action.to_ascii_lowercase();
    match action_lower.as_str() {
        "click" | "mouse_click" => {
            let target = action.target.as_deref().unwrap_or("").trim();
            let detail = if target.starts_with('(') && target.ends_with(')') {
                format!("at {target}")
            } else if !target.is_empty() {
                target.to_string()
            } else if let Some(value) = action.value.as_deref() {
                value.trim().to_string()
            } else if let Some(outcome) = action.outcome.as_deref() {
                outcome.trim().to_string()
            } else {
                String::new()
            };
            ActionEntry {
                label: "Clicked".to_string(),
                detail,
                time_label,
            }
        }
        "press_key" | "key" | "press" => {
            let key_raw = action
                .value
                .as_deref()
                .or(action.outcome.as_deref())
                .or(action.target.as_deref())
                .unwrap_or("?")
                .trim();
            let key = sanitize_pressed_detail(key_raw);
            ActionEntry {
                label: "Pressed".to_string(),
                detail: key,
                time_label,
            }
        }
        "type" | "input" | "enter_text" | "fill" | "insert_text" => {
            let typed = action
                .value
                .as_deref()
                .or(action.outcome.as_deref())
                .unwrap_or("?")
                .trim()
                .to_string();
            ActionEntry {
                label: "Typed".to_string(),
                detail: typed,
                time_label,
            }
        }
        "navigate" | "open" | "nav" => {
            let dest = action
                .target
                .as_deref()
                .map(sanitize_nav_text)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    action
                        .value
                        .as_deref()
                        .map(sanitize_nav_text)
                        .filter(|s| !s.is_empty())
                })
                .or_else(|| {
                    action
                        .outcome
                        .as_deref()
                        .map(sanitize_nav_text)
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or_default();
            ActionEntry {
                label: "Opened".to_string(),
                detail: dest,
                time_label,
            }
        }
        other if other.starts_with("scroll") => {
            let detail = action
                .value
                .as_deref()
                .filter(|v| !v.trim().is_empty())
                .map(|v| v.trim().to_string())
                .or_else(|| {
                    action
                        .outcome
                        .as_deref()
                        .filter(|o| !o.trim().is_empty())
                        .map(|o| o.trim().to_string())
                })
                .or_else(|| {
                    action
                        .target
                        .as_deref()
                        .filter(|t| !t.trim().is_empty())
                        .map(|t| t.trim().to_string())
                })
                .unwrap_or_else(|| {
                    format_action_summary(action)
                        .strip_prefix(other)
                        .map(|suffix| suffix.trim_start_matches([' ', ':', '-']))
                        .filter(|suffix| !suffix.is_empty())
                        .map(std::string::ToString::to_string)
                        .unwrap_or_else(|| format_action_summary(action))
                });
            ActionEntry {
                label: "Scrolled".to_string(),
                detail,
                time_label,
            }
        }
        _ => {
            let summary = format_action_summary(action);
            let label = titleize_action(action.action.as_str());
            let trimmed = summary
                .strip_prefix(action.action.as_str())
                .map(|suffix| suffix.trim_start_matches([' ', ':', '-']))
                .filter(|suffix| !suffix.is_empty())
                .map(std::string::ToString::to_string)
                .unwrap_or_else(|| summary.clone());
            ActionEntry {
                label,
                detail: trimmed,
                time_label,
            }
        }
    }
}

fn titleize_action(raw: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for segment in raw.split(['_', '-']).filter(|part| !part.is_empty()) {
        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            let first_upper = first.to_uppercase().collect::<String>();
            let rest = chars.as_str().to_ascii_lowercase();
            words.push(format!("{first_upper}{rest}"));
        }
    }
    if words.is_empty() {
        raw.to_string()
    } else {
        words.join(" ")
    }
}

fn sanitize_pressed_detail(raw: &str) -> String {
    let mut candidate = raw;
    const PREFIXES: &[&str] = &["pressed key:", "press key:", "key pressed:", "key:"];
    for prefix in PREFIXES {
        if let Some(rest) = strip_prefix_ignore_case(candidate, prefix) {
            candidate = rest;
            break;
        }
    }
    let cleaned = candidate.trim();
    if cleaned.is_empty() {
        raw.trim().to_string()
    } else {
        cleaned.to_string()
    }
}

fn sanitize_nav_text(raw: &str) -> String {
    let mut candidate = raw;
    const PREFIXES: &[&str] = &[
        "browser opened to:",
        "opened to:",
        "navigated to",
        "nav to:",
        "opened:",
    ];
    for prefix in PREFIXES {
        if let Some(rest) = strip_prefix_ignore_case(candidate, prefix) {
            candidate = rest;
            break;
        }
    }
    let cleaned = candidate.trim().trim_start_matches(':').trim();
    cleaned.to_string()
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let text_bytes = text.as_bytes();
    let prefix_bytes = prefix.as_bytes();
    if text_bytes.len() < prefix_bytes.len() {
        return None;
    }
    for (idx, prefix_byte) in prefix_bytes.iter().enumerate() {
        if !text_bytes[idx].eq_ignore_ascii_case(prefix_byte) {
            return None;
        }
    }
    Some(text.get(prefix.len()..)?.trim_start())
}

pub(super) fn format_action_line(action: &BrowserAction) -> String {
    let action_lower = action.action.to_ascii_lowercase();
    let target = action.target.as_deref().unwrap_or("?");
    let value = action.value.as_deref();
    let outcome = action.outcome.as_deref();

    match action_lower.as_str() {
        "click" | "mouse_click" => {
            let display = target.trim();
            if display.starts_with('(') {
                format!("Clicked at {display}")
            } else if !display.is_empty() {
                format!("Clicked {display}")
            } else {
                "Clicked".to_string()
            }
        }
        "press_key" | "key" | "press" => {
            let key = value.or(outcome).unwrap_or("?");
            format!("Pressed key: {key}")
        }
        "type" | "input" | "enter_text" | "fill" | "insert_text" => {
            let typed = value.or(outcome).unwrap_or("?");
            format!("Typed: {typed}")
        }
        "navigate" | "open" => {
            let dest = value.or(action.target.as_deref()).or(outcome).unwrap_or("?");
            format!("Navigated to {dest}")
        }
        other => {
            let summary = format_action_summary(action);
            if summary.is_empty() {
                other.to_string()
            } else {
                summary
            }
        }
    }
}

fn outcome_for_display(outcome: &str, value: &str) -> String {
    if outcome == "value set" {
        value.to_string()
    } else {
        outcome.to_string()
    }
}

