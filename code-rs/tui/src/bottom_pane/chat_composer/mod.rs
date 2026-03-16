use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, StatefulWidgetRef, WidgetRef};
use code_core::config_types::ContextMode;
use code_core::protocol::AutoContextPhase;
use code_core::protocol::TokenUsage;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use crossterm::event::MouseButton;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;

use self::history::ChatComposerHistory;
use self::paste_burst::PasteBurst;
use self::popups::{CommandItem, CommandPopup, FileSearchPopup};
use crate::slash_command::{parse_slash_name, SlashCommand};
use code_protocol::custom_prompts::CustomPrompt;
use code_protocol::custom_prompts::PROMPTS_CMD_PREFIX;
use code_core::model_family::EXTENDED_CONTEXT_WINDOW_1M;

use crate::app_event_sender::AppEventSender;
use crate::auto_drive_style::{BorderGradient, ComposerStyle};
use crate::chatwidget::AutoReviewIndicatorStatus;
use crate::thread_spawner;
use crate::components::textarea::TextArea;
use crate::components::textarea::TextAreaState;
use crate::clipboard_paste::normalize_pasted_path;
use crate::clipboard_paste::paste_image_to_temp_png;
use crate::clipboard_paste::try_decode_base64_image_to_temp_png;
use code_file_search::FileMatch;
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;

mod footer;
mod history;
mod input;
mod paste_burst;
mod popups;
mod render;

// Dynamic placeholder rendered when the composer is empty.
/// If the pasted content exceeds this number of characters, replace it with a
/// placeholder in the UI.
const LARGE_PASTE_CHAR_THRESHOLD: usize = 1000;

struct PostPasteSpaceGuard {
    expires_at: Instant,
    cursor_pos: usize,
}

struct AnimationThread {
    running: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl AnimationThread {
    fn stop(self) {
        self.running.store(false, Ordering::Release);
        drop(self.handle);
    }
}

struct TokenCursorContext<'a> {
    text: &'a str,
    safe_cursor: usize,
    after_cursor: &'a str,
    start_idx: usize,
    end_idx: usize,
}

/// Result returned when the user interacts with the text area.
#[derive(Debug, PartialEq)]
pub enum InputResult {
    Submitted(String),
    Command(SlashCommand),
    ScrollUp,
    ScrollDown,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposerRenderMode {
    Full,
    FooterOnly,
}

struct TokenUsageInfo {
    last_token_usage: TokenUsage,
    model_context_window: Option<u64>,
    context_mode: Option<ContextMode>,
    auto_context_phase: Option<AutoContextPhase>,
    /// Baseline token count present in the context before the user's first
    /// message content is considered. This is used to normalize the
    /// "context left" percentage so it reflects the portion the user can
    /// influence rather than fixed prompt overhead (system prompt, tool
    /// instructions, etc.).
    ///
    /// Preferred source is `cached_input_tokens` from the first turn (when
    /// available), otherwise we fall back to 0.
    initial_prompt_tokens: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoReviewPhase {
    Reviewing,
    Resolving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AutoReviewFooterStatus {
    pub(crate) status: AutoReviewIndicatorStatus,
    pub(crate) findings: Option<usize>,
    pub(crate) phase: AutoReviewPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentHintLabel {
    Agents,
    Review,
}

pub(crate) struct ChatComposer {
    textarea: TextArea,
    textarea_state: RefCell<TextAreaState>,
    active_popup: ActivePopup,
    app_event_tx: AppEventSender,
    history: ChatComposerHistory,
    ctrl_c_quit_hint: bool,
    dismissed_file_popup_token: Option<String>,
    current_file_query: Option<String>,
    // Tracks a one-off Tab-triggered file search. When set, we will only
    // create/show a popup if the results are non-empty to avoid flicker.
    pending_tab_file_query: Option<String>,
    file_popup_origin: Option<FilePopupOrigin>,
    pending_pastes: Vec<(String, String)>,
    token_usage_info: Option<TokenUsageInfo>,
    has_focus: bool,
    has_chat_history: bool,
    /// Tracks whether the user has typed or pasted any content since startup.
    typed_anything: bool,
    is_task_running: bool,
    // Current status message to display when task is running
    status_message: String,
    show_auto_drive_goal_title: bool,
    // Animation thread for spinning icon when task is running
    animation_running: Option<AnimationThread>,
    using_chatgpt_auth: bool,
    custom_prompts: Vec<CustomPrompt>,
    subagent_commands: Vec<String>,
    // Ephemeral footer notice and its expiry
    footer_notice: Option<(String, std::time::Instant)>,
    // Persistent hint for specific modes (e.g., standard terminal mode)
    standard_terminal_hint: Option<String>,
    // Auto Review status displayed in the footer
    auto_review_status: Option<AutoReviewFooterStatus>,
    // Agent hint label to display alongside Auto Review footer state
    agent_hint_label: AgentHintLabel,
    // Persistent/ephemeral access-mode indicator shown on the left
    access_mode_label: Option<String>,
    access_mode_label_expiry: Option<std::time::Instant>,
    access_mode_hint_expiry: Option<std::time::Instant>,
    // Footer hint visibility flags
    show_reasoning_hint: bool,
    show_diffs_hint: bool,
    reasoning_shown: bool,
    // Sticky flag: after a chat ScrollUp, make the very next Down trigger
    // chat ScrollDown instead of moving within the textarea, unless another
    // key is pressed in between.
    next_down_scrolls_history: bool,
    // Detect and coalesce paste bursts for smoother UX
    paste_burst: PasteBurst,
    post_paste_space_guard: Option<PostPasteSpaceGuard>,
    footer_hint_override: Option<Vec<(String, String)>>,
    embedded_mode: bool,
    render_mode: ComposerRenderMode,
    auto_drive_active: bool,
    auto_drive_style: Option<ComposerStyle>,
    /// Last rendered textarea rect, used for mouse click-to-cursor positioning
    last_textarea_rect: RefCell<Option<Rect>>,
}

/// Popup state – at most one can be visible at any time.
enum ActivePopup {
    None,
    Command(CommandPopup),
    File(FileSearchPopup),
}

enum FilePopupOrigin {
    Auto,
    Manual { token: String },
}

const RESERVED_SUBAGENT_NAMES: &[&str] = &["plan", "solve", "code"];

fn is_reserved_subagent_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    RESERVED_SUBAGENT_NAMES
        .iter()
        .any(|reserved| *reserved == lower)
}

impl ChatComposer {
    pub(crate) const DEFAULT_FOOTER_NOTICE_DURATION: std::time::Duration =
        std::time::Duration::from_secs(2);

    pub fn new(
        has_input_focus: bool,
        app_event_tx: AppEventSender,
        using_chatgpt_auth: bool,
    ) -> Self {
        Self {
            textarea: TextArea::new(),
            textarea_state: RefCell::new(TextAreaState::default()),
            active_popup: ActivePopup::None,
            app_event_tx,
            history: ChatComposerHistory::new(),
            ctrl_c_quit_hint: false,
            dismissed_file_popup_token: None,
            current_file_query: None,
            pending_tab_file_query: None,
            file_popup_origin: None,
            pending_pastes: Vec::new(),
            token_usage_info: None,
            has_focus: has_input_focus,
            has_chat_history: false,
            typed_anything: false,
            // no double‑Esc handling here; App manages Esc policy
            is_task_running: false,
            status_message: String::from("Coding"),
            show_auto_drive_goal_title: false,
            animation_running: None,
            using_chatgpt_auth,
            custom_prompts: Vec::new(),
            subagent_commands: Vec::new(),
            footer_notice: None,
            standard_terminal_hint: None,
            auto_review_status: None,
            agent_hint_label: AgentHintLabel::Agents,
            access_mode_label: None,
            access_mode_label_expiry: None,
            access_mode_hint_expiry: None,
            show_reasoning_hint: false,
            show_diffs_hint: false,
            reasoning_shown: false,
            next_down_scrolls_history: false,
            paste_burst: PasteBurst::default(),
            post_paste_space_guard: None,
            footer_hint_override: None,
            embedded_mode: false,
            render_mode: ComposerRenderMode::Full,
            auto_drive_active: false,
            auto_drive_style: None,
            last_textarea_rect: RefCell::new(None),
        }
    }

    pub fn set_using_chatgpt_auth(&mut self, using: bool) {
        self.using_chatgpt_auth = using;
    }

    pub(crate) fn set_subagent_commands(&mut self, mut names: Vec<String>) {
        names.retain(|n| !is_reserved_subagent_name(n));
        names.sort();
        self.subagent_commands = names;
        if let ActivePopup::Command(popup) = &mut self.active_popup {
            popup.set_subagent_commands(self.subagent_commands.clone());
        }
    }

    pub(crate) fn set_auto_review_status(&mut self, status: Option<AutoReviewFooterStatus>) {
        self.auto_review_status = status;
    }

    pub(crate) fn set_agent_hint_label(&mut self, label: AgentHintLabel) {
        self.agent_hint_label = label;
    }

    #[cfg(test)]
    pub(crate) fn auto_review_status(&self) -> Option<AutoReviewFooterStatus> {
        self.auto_review_status
    }

    /// Returns true if the input starts with a slash command and the cursor
    /// is positioned within the command head (i.e., before the first
    /// whitespace on the first line). Used to decide whether to keep the
    /// slash-command popup active and to suppress file completion.
    fn is_cursor_in_slash_command_head(&self) -> bool {
        let text = self.textarea.text();
        if text.is_empty() { return false; }
        let cursor = self.textarea.cursor();
        let first_line_end = text.find('\n').unwrap_or(text.len());
        let first_line = &text[..first_line_end];
        if !first_line.starts_with('/') { return false; }
        let head_end = first_line
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(i, _)| i)
            .unwrap_or(first_line_end);
        cursor <= head_end
    }

    pub fn set_has_chat_history(&mut self, has_history: bool) {
        self.has_chat_history = has_history;
    }

    pub fn set_task_running(&mut self, running: bool) {
        self.is_task_running = running;

        if running {
            // Start animation thread if not already running
            if self.animation_running.is_none() {
                let animation_flag = Arc::new(AtomicBool::new(true));
                let animation_flag_clone = Arc::clone(&animation_flag);
                let app_event_tx_clone = self.app_event_tx.clone();

                // Drive redraws at the spinner's native cadence with a
                // phase‑aligned, monotonic scheduler to minimize drift and
                // reduce perceived frame skipping under load. We purposely
                // avoid very small intervals to keep CPU impact low.
                let fallback_tx = self.app_event_tx.clone();
                if let Some(handle) = thread_spawner::spawn_lightweight("composer-anim", move || {
                    use std::time::Instant;
                    // Default to ~120ms if spinner state is not yet initialized
                    let default_ms: u64 = 120;
                    // Clamp to a sane floor so we never busy loop if a custom spinner
                    // has an extremely small interval configured.
                    let min_ms: u64 = 60; // ~16 FPS upper bound for this thread

                    // Determine the target period. If the user changes the spinner
                    // while running, we'll still get correct visual output because
                    // frames are time‑based at render; this cadence simply requests
                    // redraws.
                    let period_ms = crate::spinner::current_spinner()
                        .interval_ms
                        .max(min_ms)
                        .max(1);
                    let period = Duration::from_millis(period_ms); // fallback uses default below if needed

                    let mut next = Instant::now()
                        .checked_add(if period_ms == 0 { Duration::from_millis(default_ms) } else { period })
                        .unwrap_or_else(Instant::now);

                    while animation_flag_clone.load(Ordering::Acquire) {
                        let now = Instant::now();
                        if now < next {
                            let sleep_dur = next - now;
                            thread::sleep(sleep_dur);
                        } else {
                            // If we're late (system busy), request a redraw immediately.
                            app_event_tx_clone.send(crate::app_event::AppEvent::RequestRedraw);
                            // Step the schedule forward by whole periods to avoid
                            // bursty catch‑up redraws.
                            let mut target = next;
                            while target <= now {
                                if let Some(t) = target.checked_add(period) { target = t; } else { break; }
                            }
                            next = target;
                        }
                    }
                }) {
                    self.animation_running = Some(AnimationThread {
                        running: animation_flag,
                        handle,
                    });
                } else {
                    fallback_tx.send(crate::app_event::AppEvent::RequestRedraw);
                }
            }
        } else {
            // Stop animation thread
            if let Some(animation_thread) = self.animation_running.take() {
                animation_thread.stop();
            }
        }
    }

    pub fn update_status_message(&mut self, message: String) {
        self.show_auto_drive_goal_title =
            message.to_ascii_lowercase().contains("auto drive goal");
        self.status_message = Self::map_status_message(&message);
    }

    pub fn status_message(&self) -> Option<&str> {
        let trimmed = self.status_message.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    pub fn flash_footer_notice(&mut self, text: String) {
        let expiry = std::time::Instant::now() + Self::DEFAULT_FOOTER_NOTICE_DURATION;
        self.footer_notice = Some((text, expiry));
    }

    pub(crate) fn set_embedded_mode(&mut self, enabled: bool) {
        if self.embedded_mode != enabled {
            self.embedded_mode = enabled;
        }
    }

    pub(crate) fn set_render_mode(&mut self, mode: ComposerRenderMode) {
        if self.render_mode != mode {
            self.render_mode = mode;
        }
    }

    pub(crate) fn set_auto_drive_active(&mut self, active: bool) {
        self.auto_drive_active = active;
    }

    pub(crate) fn set_auto_drive_style(&mut self, style: Option<ComposerStyle>) {
        self.auto_drive_style = style;
    }

    /// Override the footer hint line with a simple key/label list.
    /// When set, we skip the standard reasoning/diff/help hints and render the
    /// provided items using our theme colors. Inspired by upstream's
    /// `FooterProps`, but routed through our single-line composer footer to
    /// preserve custom fork styling.
    pub(crate) fn set_footer_hint_override(
        &mut self,
        items: Option<Vec<(String, String)>>,
    ) {
        self.footer_hint_override = items.map(|values| {
            values
                .into_iter()
                .map(|(key, label)| (key.trim().to_string(), label.trim().to_string()))
                .collect()
        });
    }

    pub(crate) fn has_footer_hint_override(&self) -> bool {
        self.footer_hint_override.is_some()
    }

    /// Show a footer notice for a specific duration.
    pub fn flash_footer_notice_for(&mut self, text: String, dur: std::time::Duration) {
        let expiry = std::time::Instant::now() + dur;
        self.footer_notice = Some((text, expiry));
    }

    // Control footer hint visibility
    pub fn set_show_reasoning_hint(&mut self, show: bool) {
        if self.show_reasoning_hint != show {
            self.show_reasoning_hint = show;
        }
    }

    pub fn set_show_diffs_hint(&mut self, show: bool) {
        if self.show_diffs_hint != show {
            self.show_diffs_hint = show;
        }
    }

    pub fn set_access_mode_label(&mut self, label: Option<String>) {
        self.access_mode_label = label;
        self.access_mode_label_expiry = None;
        self.access_mode_hint_expiry = None;
    }
    pub fn set_access_mode_label_ephemeral(&mut self, label: String, dur: std::time::Duration) {
        self.access_mode_label = Some(label);
        let expiry = std::time::Instant::now() + dur;
        self.access_mode_label_expiry = Some(expiry);
        self.access_mode_hint_expiry = Some(expiry);
    }
    pub fn set_access_mode_hint_for(&mut self, dur: std::time::Duration) {
        self.access_mode_hint_expiry = Some(std::time::Instant::now() + dur);
    }

    pub fn set_reasoning_state(&mut self, shown: bool) {
        self.reasoning_shown = shown;
    }

    // Map technical status messages to user-friendly ones
    pub(crate) fn map_status_message(technical_message: &str) -> String {
        if technical_message.trim().is_empty() {
            return String::new();
        }

        let lower = technical_message.to_ascii_lowercase();

        // Auto Review: preserve the phase text so the footer shows
        // "Auto Review: Reviewing/Resolving" instead of a generic label.
        if lower.contains("auto review") {
            let cleaned = technical_message.trim();
            if cleaned.is_empty() {
                "Auto Review".to_string()
            } else {
                cleaned.to_string()
            }
        } else if lower.contains("auto drive goal") {
            "Auto Drive Goal".to_string()
        } else if lower.contains("auto drive") {
            "Auto Drive".to_string()
        }
        // Thinking/reasoning patterns
        else if lower.contains("reasoning")
            || lower.contains("thinking")
            || lower.contains("planning")
            || lower.contains("waiting for model")
        {
            "Thinking".to_string()
        }
        // Tool/command execution patterns
        else if lower.contains("tool")
            || lower.contains("command")
            || lower.contains("running")
            || lower.contains("bash")
            || lower.contains("shell")
        {
            "Using tools".to_string()
        }
        // Browser activity
        else if lower.contains("browser")
            || lower.contains("chrome")
            || lower.contains("cdp")
            || lower.contains("navigate to")
            || lower.contains("open url")
            || lower.contains("load url")
            || lower.contains("screenshot")
        {
            "Browsing".to_string()
        }
        // Multi-agent orchestration
        else if lower.contains("agent")
            || lower.contains("orchestrating")
            || lower.contains("coordinating")
        {
            "Agents".to_string()
        }
        // Response generation patterns
        else if lower.contains("generating")
            || lower.contains("responding")
            || lower.contains("streaming")
            || lower.contains("writing response")
            || lower.contains("assistant")
            || lower.contains("chat completions")
            || lower.contains("completion")
        {
            "Responding".to_string()
        }
        // Transient network/stream retry patterns → keep spinner visible with a
        // clear reconnecting message so the user knows we are still working.
        else if lower.contains("retrying")
            || lower.contains("reconnecting")
            || lower.contains("disconnected")
            || lower.contains("stream error")
            || lower.contains("stream closed")
            || lower.contains("timeout")
            || lower.contains("transport")
            || lower.contains("network")
            || lower.contains("connection")
        {
            "Reconnecting".to_string()
        }
        // File/code editing patterns
        else if lower.contains("editing")
            || lower.contains("writing")
            || lower.contains("modifying")
            || lower.contains("creating file")
            || lower.contains("updating")
            || lower.contains("patch")
        {
            "Coding".to_string()
        }
        // Catch some common technical terms
        else if lower.contains("processing") || lower.contains("analyzing") {
            "Thinking".to_string()
        } else if lower == "search" || lower.contains("searching") {
            "Searching".to_string()
        } else if lower.contains("reading") {
            "Reading".to_string()
        } else {
            // Default fallback - use "working" for unknown status
            "Working".to_string()
        }
    }


    /// Returns true if the composer currently contains no user input.
    pub(crate) fn is_empty(&self) -> bool {
        self.textarea.is_empty()
    }

    /// Update the cached *context-left* percentage and refresh the placeholder
    /// text. The UI relies on the placeholder to convey the remaining
    /// context when the composer is empty.
    pub(crate) fn set_token_usage(
        &mut self,
        last_token_usage: TokenUsage,
        model_context_window: Option<u64>,
        context_mode: Option<ContextMode>,
    ) {
        let initial_prompt_tokens = self
            .token_usage_info
            .as_ref()
            .map(|info| info.initial_prompt_tokens)
            .unwrap_or_else(|| last_token_usage.cached_input_tokens);

        self.token_usage_info = Some(TokenUsageInfo {
            last_token_usage,
            model_context_window,
            context_mode,
            auto_context_phase: self
                .token_usage_info
                .as_ref()
                .and_then(|info| info.auto_context_phase),
            initial_prompt_tokens,
        });
    }

    pub(crate) fn set_auto_context_phase(&mut self, phase: Option<AutoContextPhase>) {
        if let Some(info) = self.token_usage_info.as_mut() {
            info.auto_context_phase = phase;
        }
    }

    /// Record the history metadata advertised by `SessionConfiguredEvent` so
    /// that the composer can navigate cross-session history.
    pub(crate) fn set_history_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.history.set_metadata(log_id, entry_count);
    }
}

impl Drop for ChatComposer {
    fn drop(&mut self) {
        if let Some(animation_thread) = self.animation_running.take() {
            animation_thread.stop();
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn auto_review_status_stays_left_with_auto_drive_footer() {
        let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
        let app_tx = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, app_tx, true);

        composer.auto_drive_active = true;
        composer.standard_terminal_hint = Some("Esc stop\tCtrl+S settings".to_string());
        composer.set_auto_review_status(Some(AutoReviewFooterStatus {
            status: AutoReviewIndicatorStatus::Running,
            findings: None,
            phase: AutoReviewPhase::Reviewing,
        }));

        let area = Rect {
            x: 0,
            y: 0,
            width: 64,
            height: 1,
        };
        let mut buf = Buffer::empty(area);
        composer.render_footer(area, &mut buf);

        let line: String = (0..area.width)
            .map(|x| buf[(area.x + x, area.y)].symbol().to_string())
            .collect();

        let auto_idx = line
            .find("Auto Review")
            .expect("footer should show auto review text");
        let esc_idx = line.find("Esc stop").unwrap_or(line.len());

        assert!(auto_idx < esc_idx, "Auto Review status should be left-most");
    }

    #[test]
    fn footer_shows_1m_context_suffix_when_extended_context_is_active() {
        let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
        let app_tx = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, app_tx, true);

        let token_usage = TokenUsage {
            input_tokens: 13_290,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 13_290,
        };
        composer.set_token_usage(
            token_usage,
            Some(EXTENDED_CONTEXT_WINDOW_1M),
            Some(ContextMode::OneM),
        );

        let area = Rect {
            x: 0,
            y: 0,
            width: 96,
            height: 1,
        };
        let mut buf = Buffer::empty(area);
        composer.render_footer(area, &mut buf);

        let line: String = (0..area.width)
            .map(|x| buf[(area.x + x, area.y)].symbol().to_string())
            .collect();

        assert!(line.contains("13,290 tokens"));
        assert!(line.contains("1M Context"));
    }

    #[test]
    fn footer_shows_1m_auto_suffix_when_auto_context_is_active() {
        let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
        let app_tx = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, app_tx, true);

        let token_usage = TokenUsage {
            input_tokens: 13_290,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 13_290,
        };
        composer.set_token_usage(
            token_usage,
            Some(EXTENDED_CONTEXT_WINDOW_1M),
            Some(ContextMode::Auto),
        );

        let area = Rect {
            x: 0,
            y: 0,
            width: 96,
            height: 1,
        };
        let mut buf = Buffer::empty(area);

        composer.render_footer(area, &mut buf);

        let line: String = (0..area.width)
            .map(|x| buf[(area.x + x, area.y)].symbol().to_string())
            .collect();
        assert!(line.contains("1M Auto"));
    }

    #[test]
    fn footer_shows_checking_context_while_auto_context_check_is_running() {
        let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
        let app_tx = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, app_tx, true);

        let token_usage = TokenUsage {
            input_tokens: 13_290,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 13_290,
        };
        composer.set_token_usage(
            token_usage,
            Some(EXTENDED_CONTEXT_WINDOW_1M),
            Some(ContextMode::Auto),
        );
        composer.set_auto_context_phase(Some(AutoContextPhase::Checking));

        let area = Rect {
            x: 0,
            y: 0,
            width: 96,
            height: 1,
        };
        let mut buf = Buffer::empty(area);

        composer.render_footer(area, &mut buf);

        let line: String = (0..area.width)
            .map(|x| buf[(area.x + x, area.y)].symbol().to_string())
            .collect();
        assert!(line.contains("Checking context..."));
    }

    #[test]
    fn footer_shows_compacting_while_auto_context_compact_is_running() {
        let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
        let app_tx = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, app_tx, true);

        let token_usage = TokenUsage {
            input_tokens: 13_290,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 13_290,
        };
        composer.set_token_usage(
            token_usage,
            Some(EXTENDED_CONTEXT_WINDOW_1M),
            Some(ContextMode::Auto),
        );
        composer.set_auto_context_phase(Some(AutoContextPhase::Compacting));

        let area = Rect {
            x: 0,
            y: 0,
            width: 96,
            height: 1,
        };
        let mut buf = Buffer::empty(area);

        composer.render_footer(area, &mut buf);

        let line: String = (0..area.width)
            .map(|x| buf[(area.x + x, area.y)].symbol().to_string())
            .collect();
        assert!(line.contains("Compacting..."));
    }

    #[test]
    fn map_status_message_shows_searching_for_search_status() {
        assert_eq!(
            ChatComposer::map_status_message("Search"),
            "Searching".to_string()
        );
        assert_eq!(
            ChatComposer::map_status_message("searching files"),
            "Searching".to_string()
        );
        assert_eq!(
            ChatComposer::map_status_message("waiting for user input"),
            "Working".to_string()
        );
        assert_eq!(
            ChatComposer::map_status_message("chat completions model"),
            "Responding".to_string()
        );
    }

    #[test]
    fn subagent_popup_prefill_does_not_record_submission_history() {
        let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
        let app_tx = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, app_tx, true);
        composer.set_subagent_commands(vec!["qwertyagent".to_string()]);
        composer.textarea.set_text("/qwe");
        composer.sync_command_popup();

        let (result, handled) = composer.confirm_slash_popup_selection();

        assert_eq!(result, InputResult::None);
        assert!(handled);
        assert_eq!(composer.textarea.text(), "/qwertyagent ");
        composer.textarea.set_text("");
        assert!(!composer.try_history_up());
    }

    #[test]
    fn footer_only_mode_uses_footer_height_and_hides_cursor() {
        let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
        let app_tx = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, app_tx, true);
        composer.set_render_mode(ComposerRenderMode::FooterOnly);
        composer.standard_terminal_hint = Some("Terminal mode".to_string());
        composer.active_popup = ActivePopup::Command(CommandPopup::new_with_filter(true));

        assert_eq!(composer.footer_height(), 1);
        assert_eq!(composer.desired_height(80), 1);
        assert_eq!(
            composer.cursor_pos(Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 3,
            }),
            None
        );
    }

    #[test]
    fn insert_selected_path_quotes_and_escapes_internal_quotes() {
        let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
        let app_tx = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, app_tx, true);
        composer.textarea.set_text("@fi");
        composer.textarea.set_cursor(3);

        composer.insert_selected_path("/tmp/my \"quoted\" file.txt");

        assert_eq!(
            composer.textarea.text(),
            "\"/tmp/my \\\"quoted\\\" file.txt\" "
        );
    }
}
