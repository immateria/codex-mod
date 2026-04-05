use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
use std::sync::mpsc::{Receiver, Sender as StdSender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::KeyCode;
use portable_pty::MasterPty;
use ratatui::buffer::Buffer;
use ratatui::prelude::Size;

use crate::app_event::{AppEvent, TerminalRunController};
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::{ChatWidget, GhostState};
use crate::file_search::FileSearchManager;
use crate::history::state::HistorySnapshot;
use crate::onboarding::onboarding_screen::OnboardingScreen;
use crate::tui::TerminalInfo;
use code_core::config::Config;
use code_core::config_types::ThemeName;
use code_core::ConversationManager;
use code_login::ShutdownHandle;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

#[cfg(unix)]
use signal_hook::SigId;

mod app_impl;
mod buffer_diff_profiler;
mod frame_timer;
mod timing_stats;

#[cfg(test)]
mod tests;

/// Time window for debouncing redraw requests.
///
/// Temporarily widened to ~30 FPS (33 ms) to coalesce bursts of updates while
/// we smooth out per-frame hotspots; keeps redraws responsive without pegging
/// the main thread.
pub(super) const REDRAW_DEBOUNCE: Duration = Duration::from_millis(33);
// Prevent bulk events (Codex output/tool completions) from being starved behind a
// continuous stream of high-priority events (e.g., redraw scheduling).
pub(super) const HIGH_EVENT_BURST_MAX: u32 = 32;
/// After this many consecutive backpressure skips, force a non‑blocking draw so
/// buffered output can catch up even if POLLOUT never flips true (e.g., tmux
/// reattach or XON/XOFF throttling).
pub(super) const BACKPRESSURE_FORCED_DRAW_SKIPS: u32 = 4;
pub(super) const DEFAULT_PTY_ROWS: u16 = 24;
pub(super) const DEFAULT_PTY_COLS: u16 = 80;
const FRAME_TIMER_LOG_THROTTLE_SECS: u64 = 5;

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Top-level application state: which full-screen view is currently active.
pub(super) enum AppState<'a> {
    Onboarding {
        screen: Box<OnboardingScreen>,
    },
    /// The main chat UI is visible.
    Chat {
        /// Boxed to avoid a large enum variant and reduce the overall size of
        /// `AppState`.
        widget: Box<ChatWidget<'a>>,
    },
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ThemeSplitPreview {
    pub(super) current: ThemeName,
    pub(super) preview: ThemeName,
}

type WriterTx = Arc<Mutex<Option<StdSender<Vec<u8>>>>>;
type PtyHandle = Arc<Mutex<Box<dyn MasterPty + Send>>>;

pub(super) struct TerminalRunState {
    pub(super) command: Vec<String>,
    pub(super) display: String,
    pub(super) cancel_tx: Option<oneshot::Sender<()>>,
    pub(super) running: bool,
    pub(super) controller: Option<TerminalRunController>,
    pub(super) writer_tx: Option<WriterTx>,
    pub(super) pty: Option<PtyHandle>,
}

pub(super) struct FrameTimer {
    state: Mutex<FrameTimerState>,
    cv: Condvar,
    last_limit_log_secs: AtomicU64,
    suppressed_limit_logs: AtomicUsize,
}

struct FrameTimerState {
    deadlines: BinaryHeap<Reverse<Instant>>,
    worker_running: bool,
}

pub(super) struct LoginFlowState {
    pub(super) shutdown: Option<ShutdownHandle>,
    pub(super) join_handle: JoinHandle<()>,
}

pub(crate) struct App<'a> {
    pub(super) _server: Arc<ConversationManager>,
    pub(super) app_event_tx: AppEventSender,
    // Split event receivers: high‑priority (input) and bulk (streaming)
    pub(super) app_event_rx_high: Receiver<AppEvent>,
    pub(super) app_event_rx_bulk: Receiver<AppEvent>,
    pub(super) consecutive_high_events: u32,
    pub(super) app_state: AppState<'a>,

    /// Config is stored here so we can recreate ChatWidgets as needed.
    pub(super) config: Config,
    pub(super) cli_kv_overrides: Vec<(String, toml::Value)>,
    pub(super) config_overrides: code_core::config::ConfigOverrides,

    /// Latest available release version (if detected) so new widgets can surface it.
    pub(super) latest_upgrade_version: Option<String>,

    pub(super) file_search: FileSearchManager,

    /// True when a redraw has been scheduled but not yet executed (debounce window).
    pub(super) pending_redraw: Arc<AtomicBool>,
    /// Tracks whether a frame is currently queued or being drawn. Used to coalesce
    /// rapid-fire redraw requests without dropping the final state.
    pub(super) redraw_inflight: Arc<AtomicBool>,
    /// Set if a redraw request arrived while another frame was in flight. Ensures we
    /// queue one more frame immediately after the current draw completes.
    pub(super) post_frame_redraw: Arc<AtomicBool>,
    /// Count of consecutive redraws skipped because stdout/PTY was not writable.
    pub(super) stdout_backpressure_skips: u32,
    /// Shared scheduler for future animation frames. Ensures the shortest
    /// requested interval wins while preserving later deadlines.
    pub(super) frame_timer: Arc<FrameTimer>,
    /// Controls the input reader thread spawned at startup.
    pub(super) input_running: Arc<AtomicBool>,
    /// Temporarily pause input reader while external editor owns the terminal.
    pub(super) input_suspended: Arc<AtomicBool>,

    pub(super) enhanced_keys_supported: bool,
    /// Tracks keys seen as pressed when keyboard enhancements are unavailable
    /// so duplicate release events can be filtered and release-only terminals
    /// still synthesize a press.
    pub(super) non_enhanced_pressed_keys: HashSet<KeyCode>,

    /// Debug flag for logging LLM requests/responses
    pub(super) _debug: bool,
    /// Show per-cell ordering overlay when true
    pub(super) show_order_overlay: bool,

    /// Controls the animation thread that sends CommitTick events.
    pub(super) commit_anim_running: Arc<AtomicBool>,

    /// Terminal information queried at startup
    pub(super) terminal_info: TerminalInfo,

    #[cfg(unix)]
    pub(super) sigterm_guard: Option<SigId>,
    #[cfg(unix)]
    pub(super) sigterm_flag: Arc<AtomicBool>,

    /// Perform a hard clear on the first frame to ensure the entire buffer
    /// starts with our theme background. This avoids terminals that may show
    /// profile defaults until all cells are explicitly painted.
    pub(super) clear_on_first_frame: bool,

    /// Pending ghost snapshot state to apply after a conversation fork completes.
    pub(super) pending_jump_back_ghost_state: Option<GhostState>,
    /// Pending history snapshot to seed the next widget after a jump-back fork.
    pub(super) pending_jump_back_history_snapshot: Option<HistorySnapshot>,
    /// When set, render the entire frame as a left/right split where each half
    /// is drawn using a different theme.
    pub(super) theme_split_preview: Option<ThemeSplitPreview>,

    /// Track last known terminal size. If it changes (true resize or a
    /// tab switch that altered the viewport), perform a full clear on the next
    /// draw to avoid ghost cells from the previous size. This is cheap and
    /// happens rarely, but fixes Windows/macOS terminals that don't fully
    /// repaint after focus/size changes until a manual resize occurs.
    pub(super) last_frame_size: Option<Size>,

    // Double‑Esc timing for undo timeline
    pub(super) last_esc_time: Option<Instant>,

    /// If true, enable lightweight timing collection and report on exit.
    pub(super) timing_enabled: bool,
    pub(super) timing: TimingStats,

    pub(super) buffer_diff_profiler: BufferDiffProfiler,

    /// True when TUI is currently rendering in the terminal's alternate screen.
    pub(super) alt_screen_active: bool,

    pub(super) terminal_runs: HashMap<u64, TerminalRunState>,

    pub(super) terminal_title_override: Option<String>,
    pub(super) login_flow: Option<LoginFlowState>,
}

/// Aggregate parameters needed to create a `ChatWidget`, as creation may be
/// deferred until after the Git warning screen is dismissed.
#[derive(Clone, Debug)]
pub(crate) struct ChatWidgetArgs {
    pub(crate) config: Config,
    pub(crate) initial_prompt: Option<String>,
    pub(crate) initial_images: Vec<PathBuf>,
    pub(crate) terminal_info: TerminalInfo,
    pub(crate) show_order_overlay: bool,
    pub(crate) enable_perf: bool,
    pub(crate) resume_picker: bool,
    pub(crate) fork_picker: bool,
    pub(crate) fork_source_path: Option<PathBuf>,
    pub(crate) latest_upgrade_version: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct AppInitArgs {
    pub(crate) config: Config,
    pub(crate) cli_kv_overrides: Vec<(String, toml::Value)>,
    pub(crate) config_overrides: code_core::config::ConfigOverrides,
    pub(crate) initial_prompt: Option<String>,
    pub(crate) initial_images: Vec<PathBuf>,
    pub(crate) show_trust_screen: bool,
    pub(crate) debug: bool,
    pub(crate) show_order_overlay: bool,
    pub(crate) terminal_info: TerminalInfo,
    pub(crate) enable_perf: bool,
    pub(crate) resume_picker: bool,
    pub(crate) fork_picker: bool,
    pub(crate) fork_source_path: Option<PathBuf>,
    pub(crate) startup_footer_notice: Option<String>,
    pub(crate) latest_upgrade_version: Option<String>,
}

pub(super) struct BufferDiffProfiler {
    enabled: bool,
    prev: Option<Buffer>,
    frame_seq: u64,
    log_every: usize,
    min_changed: usize,
    min_percent: f64,
}

// (legacy tests removed)
#[derive(Default, Clone, Debug)]
pub(super) struct TimingStats {
    frames_drawn: u64,
    redraw_events: u64,
    key_events: u64,
    draw_ns: Vec<u64>,
    key_to_frame_ns: Vec<u64>,
    last_key_event: Option<Instant>,
    key_waiting_for_frame: bool,
}
