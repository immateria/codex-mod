use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use super::actions::BrowserAction;
use super::{BrowserScreenshotRecord, BrowserSessionCell, MAX_ACTIONS, MAX_CONSOLE, MAX_SCREENSHOT_HISTORY};

impl Clone for BrowserSessionCell {
    fn clone(&self) -> Self {
        Self {
            url: self.url.clone(),
            title: self.title.clone(),
            actions: self.actions.clone(),
            console_messages: self.console_messages.clone(),
            screenshot_path: self.screenshot_path.clone(),
            screenshot_history: self.screenshot_history.clone(),
            total_duration: self.total_duration,
            completed: self.completed,
            cell_key: self.cell_key.clone(),
            parent_call_id: self.parent_call_id.clone(),
            headless: self.headless,
            status_code: self.status_code.clone(),
            cached_picker: Rc::clone(&self.cached_picker),
            cached_image_protocol: Rc::clone(&self.cached_image_protocol),
        }
    }
}

impl Default for BrowserSessionCell {
    fn default() -> Self {
        Self {
            url: None,
            title: None,
            actions: Vec::new(),
            console_messages: Vec::new(),
            screenshot_path: None,
            screenshot_history: Vec::new(),
            total_duration: Duration::ZERO,
            completed: false,
            cell_key: None,
            parent_call_id: None,
            headless: None,
            status_code: None,
            cached_picker: Rc::new(RefCell::new(None)),
            cached_image_protocol: Rc::new(RefCell::new(None)),
        }
    }
}

impl BrowserSessionCell {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set_url(&mut self, url: impl Into<String>) {
        self.url = Some(url.into());
    }

    pub(crate) fn current_url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub(crate) fn record_action(
        &mut self,
        timestamp: Duration,
        duration: Duration,
        action: String,
        target: Option<String>,
        value: Option<String>,
        outcome: Option<String>,
    ) {
        if self.actions.last().is_some_and(|last| {
            last.action == action
                && last.target == target
                && last.value == value
                && last.outcome == outcome
        }) {
            return;
        }
        let action_entry = BrowserAction {
            action,
            target,
            value,
            outcome: outcome.clone(),
            timestamp,
        };
        self.actions.push(action_entry);
        if self.actions.len() > MAX_ACTIONS {
            let overflow = self.actions.len() - MAX_ACTIONS;
            self.actions.drain(0..overflow);
        }
        let finish = timestamp.saturating_add(duration);
        if finish > self.total_duration {
            self.total_duration = finish;
        }
        if let Some(outcome) = outcome
            && let Some(code) = extract_status_code(&outcome)
        {
            self.status_code = Some(code);
        }
    }

    pub(crate) fn add_console_message(&mut self, message: String) {
        if self.console_messages.last() == Some(&message) {
            return;
        }
        self.console_messages.push(message);
        if self.console_messages.len() > MAX_CONSOLE {
            let overflow = self.console_messages.len() - MAX_CONSOLE;
            self.console_messages.drain(0..overflow);
        }
    }

    pub(crate) fn record_screenshot(&mut self, timestamp: Duration, path: PathBuf, url: Option<String>) {
        let display_path = path.display().to_string();
        self.screenshot_path = Some(display_path);
        self.screenshot_history.push(BrowserScreenshotRecord {
            path,
            url,
            timestamp,
        });
        if self.screenshot_history.len() > MAX_SCREENSHOT_HISTORY {
            let overflow = self.screenshot_history.len() - MAX_SCREENSHOT_HISTORY;
            self.screenshot_history.drain(0..overflow);
        }
        self.cached_image_protocol.borrow_mut().take();
    }

    pub(crate) fn set_headless(&mut self, headless: Option<bool>) {
        self.headless = headless;
    }

    pub(crate) fn set_status_code(&mut self, code: Option<String>) {
        self.status_code = code;
    }

    pub(crate) fn set_cell_key(&mut self, key: Option<String>) {
        self.cell_key = key;
    }

    pub(crate) fn cell_key(&self) -> Option<&str> {
        self.cell_key.as_deref()
    }

    pub(crate) fn screenshot_history(&self) -> &[BrowserScreenshotRecord] {
        &self.screenshot_history
    }

    pub(crate) fn total_duration(&self) -> Duration {
        self.total_duration
    }
}

fn extract_status_code(outcome: &str) -> Option<String> {
    let trimmed = outcome.trim();
    if trimmed.len() < 3 {
        return None;
    }
    let code: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    if code.len() == 3 {
        Some(code)
    } else {
        None
    }
}

