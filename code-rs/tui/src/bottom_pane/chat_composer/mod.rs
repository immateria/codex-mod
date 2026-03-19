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
mod status;
mod subagents;
mod task_running;
mod token_usage;

#[cfg(test)]
mod tests;

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
    /// Returns true if the composer currently contains no user input.
    pub(crate) fn is_empty(&self) -> bool {
        self.textarea.is_empty()
    }
}
