fn history_cell_logging_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if let Ok(value) = std::env::var("CODEX_TRACE_HISTORY") {
        let trimmed = value.trim();
        if !matches!(trimmed, "" | "0") {
            return true;
        }
    }
    *ENABLED.get_or_init(|| {
        if let Ok(value) = std::env::var("CODE_BUFFER_DIFF_TRACE_CELLS") {
            return !matches!(value.trim(), "" | "0");
        }
        if let Ok(value) = std::env::var("CODE_BUFFER_DIFF_METRICS") {
            return !matches!(value.trim(), "" | "0");
        }
        false
    })
}

pub(crate) fn is_test_mode() -> bool {
    #[cfg(any(test, feature = "test-helpers"))]
    {
        static FLAG: OnceLock<bool> = OnceLock::new();
        *FLAG.get_or_init(|| match std::env::var("CODE_TUI_TEST_MODE") {
            Ok(raw) => {
                let val = raw.trim().to_ascii_lowercase();
                matches!(val.as_str(), "1" | "true" | "yes" | "on")
            }
            Err(_) => true,
        })
    }
    #[cfg(not(any(test, feature = "test-helpers")))]
    {
        static FLAG: OnceLock<bool> = OnceLock::new();
        *FLAG.get_or_init(|| match std::env::var("CODE_TUI_TEST_MODE") {
            Ok(raw) => {
                let val = raw.trim().to_ascii_lowercase();
                matches!(val.as_str(), "1" | "true" | "yes" | "on")
            }
            Err(_) => false,
        })
    }
}
use tracing::{debug, info, warn};
// use image::GenericImageView;

const TOKENS_PER_MILLION: f64 = 1_000_000.0;
const INPUT_COST_PER_MILLION_USD: f64 = 1.25;
const CACHED_INPUT_COST_PER_MILLION_USD: f64 = 0.125;
const OUTPUT_COST_PER_MILLION_USD: f64 = 10.0;
const STATUS_LABEL_INDENT: &str = "   ";
const STATUS_LABEL_TARGET_WIDTH: usize = 7;
const STATUS_LABEL_GAP: usize = 2;
const STATUS_CONTENT_PREFIX: &str = "    ";
const RESUME_PLACEHOLDER_MESSAGE: &str = "Resuming previous session...";
const RESUME_NO_HISTORY_NOTICE: &str =
    "No saved messages for this session. Start typing to continue.";
const ENABLE_WARP_STRIPES: bool = false;

fn auto_continue_from_config(mode: AutoDriveContinueMode) -> AutoContinueMode {
    match mode {
        AutoDriveContinueMode::Immediate => AutoContinueMode::Immediate,
        AutoDriveContinueMode::TenSeconds => AutoContinueMode::TenSeconds,
        AutoDriveContinueMode::SixtySeconds => AutoContinueMode::SixtySeconds,
        AutoDriveContinueMode::Manual => AutoContinueMode::Manual,
    }
}

fn auto_continue_to_config(mode: AutoContinueMode) -> AutoDriveContinueMode {
    match mode {
        AutoContinueMode::Immediate => AutoDriveContinueMode::Immediate,
        AutoContinueMode::TenSeconds => AutoDriveContinueMode::TenSeconds,
        AutoContinueMode::SixtySeconds => AutoDriveContinueMode::SixtySeconds,
        AutoContinueMode::Manual => AutoDriveContinueMode::Manual,
    }
}

fn status_field_prefix(label: &str) -> String {
    let padding = STATUS_LABEL_GAP
        .saturating_add(STATUS_LABEL_TARGET_WIDTH.saturating_sub(label.len()));
    format!(
        "{indent}{label}:{spaces}",
        indent = STATUS_LABEL_INDENT,
        label = label,
        spaces = " ".repeat(padding)
    )
}

fn status_content_prefix() -> String {
    STATUS_CONTENT_PREFIX.to_string()
}

fn describe_cloud_error(err: &CloudTaskError) -> String {
    match err {
        CloudTaskError::Msg(message) => message.clone(),
        other => other.to_string(),
    }
}

use crate::account_label::{account_display_label, account_mode_priority};
use crate::app_event::{
    AppEvent,
    AutoContinueMode,
    BackgroundPlacement,
    GitInitResume,
    ModelSelectionKind,
    TerminalAfter,
    TerminalCommandGate,
    TerminalLaunch,
    TerminalRunController,
};
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::CustomPromptView;
use crate::bottom_pane::list_selection_view::{ListSelectionView, SelectionItem};
use crate::bottom_pane::CloudTasksView;
use crate::bottom_pane::validation_settings_view;
use crate::bottom_pane::validation_settings_view::{GroupStatus, ToolRow};
use crate::bottom_pane::model_selection_view::ModelSelectionTarget;
use crate::bottom_pane::BottomPane;
use crate::bottom_pane::BottomPaneParams;
use crate::bottom_pane::{UndoTimelineEntry, UndoTimelineEntryKind, UndoTimelineView};
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::InputResult;
use crate::bottom_pane::LoginAccountsState;
use crate::bottom_pane::LoginAccountsView;
use crate::bottom_pane::LoginAddAccountState;
use crate::bottom_pane::LoginAddAccountView;
use crate::bottom_pane::UpdateSharedState;
use crate::height_manager::HeightEvent;
use crate::height_manager::HeightManager;
use crate::history_cell;
use crate::history_cell::clean_wait_command;
#[cfg(target_os = "macos")]
use crate::agent_install_helpers::macos_brew_formula_for_command;
use crate::history_cell::ExecCell;
use crate::history_cell::FrozenHistoryCell;
use crate::history_cell::HistoryCell;
use crate::history_cell::HistoryCellType;
use crate::history_cell::PatchEventType;
use crate::history_cell::PlainHistoryCell;
use crate::history_cell::PlanUpdateCell;
use crate::history_cell::DiffCell;
use crate::history_cell::BrowserSessionCell;
use crate::history_cell::{AutoDriveActionKind, AutoDriveStatus};
use sha2::{Digest, Sha256};
use crate::history::state::PatchEventType as HistoryPatchEventType;
use crate::history::state::{
    AssistantMessageState,
    AssistantStreamDelta,
    AssistantStreamState,
    DiffLineKind,
    DiffRecord,
    ExecStatus,
    ExecWaitNote,
    HistoryDomainEvent,
    HistoryDomainRecord,
    HistoryId,
    HistoryRecord,
    HistoryMutation,
    HistorySnapshot,
    HistoryState,
    InlineSpan,
    MessageLine,
    MessageLineKind,
    MessageHeader,
    ImageRecord,
    PlainMessageKind,
    PlainMessageRole,
    PlainMessageState,
    MessageMetadata,
    OrderKeySnapshot,
    PatchFailureMetadata,
    PatchRecord,
    RateLimitLegendEntry,
    RateLimitsRecord,
    TextTone,
    TextEmphasis,
    ToolArgument,
    ToolStatus,
};
use crate::cloud_tasks_service::CloudEnvironment;
use crate::sanitize::{sanitize_for_tui, Mode as SanitizeMode, Options as SanitizeOptions};
use crate::slash_command::{ProcessedCommand, SlashCommand};
use crate::live_wrap::RowBuilder;
use crate::streaming::StreamKind;
use crate::streaming::controller::AppEventHistorySink;
use crate::util::buffer::fill_rect;
use crate::user_approval_widget::ApprovalRequest;
use code_ansi_escape::ansi_escape_line;
pub(crate) use self::terminal::{
    PendingCommand,
    PendingCommandAction,
    PendingManualTerminal,
    TerminalOverlay,
    TerminalState,
};
use code_browser::BrowserManager;
use code_core::config::find_code_home;
use code_core::config::resolve_code_path_for_read;
use code_core::config::set_github_actionlint_on_patch;
use code_core::config::set_validation_group_enabled;
use code_core::config::set_validation_tool_enabled;
use code_file_search::FileMatch;
use code_cloud_tasks_client::{ApplyOutcome, CloudTaskError, CreatedTask, TaskSummary};
use code_protocol::models::ContentItem;
use code_protocol::models::ResponseItem;
use code_core::config_types::{validation_tool_category, ValidationCategory};
use code_core::protocol::RateLimitSnapshotEvent;
use code_core::protocol::ValidationGroup;
use crate::rate_limits_view::{
    build_limits_view, RateLimitDisplayConfig, RateLimitResetInfo, DEFAULT_DISPLAY_CONFIG,
    DEFAULT_GRID_CONFIG,
};
use crate::session_log;
use code_core::review_format::format_review_findings_block;
use chrono::{DateTime, Datelike, Duration as ChronoDuration, Local, TimeZone, Timelike, Utc};
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use ratatui::style::Stylize;
use ratatui::symbols::scrollbar as scrollbar_symbols;
use ratatui::text::Span;
use ratatui::text::Text as RtText;
use textwrap::wrap;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Scrollbar;
use ratatui::widgets::ScrollbarOrientation;
use ratatui::widgets::ScrollbarState;
use ratatui::widgets::StatefulWidget;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize)]
struct CachedConnection {
    port: Option<u16>,
    ws: Option<String>,
}

async fn read_cached_connection() -> Option<(Option<u16>, Option<String>)> {
    let code_home = find_code_home().ok()?;
    let path = resolve_code_path_for_read(&code_home, std::path::Path::new("cache.json"));
    let bytes = tokio::fs::read(path).await.ok()?;
    let parsed: CachedConnection = serde_json::from_slice(&bytes).ok()?;
    Some((parsed.port, parsed.ws))
}

async fn write_cached_connection(port: Option<u16>, ws: Option<String>) -> std::io::Result<()> {
    if port.is_none() && ws.is_none() {
        return Ok(());
    }
    if let Ok(code_home) = find_code_home() {
        let path = code_home.join("cache.json");
        let obj = CachedConnection { port, ws };
        let data = serde_json::to_vec_pretty(&obj).unwrap_or_else(|_| b"{}".to_vec());
        if let Some(dir) = path.parent() {
            let _ = tokio::fs::create_dir_all(dir).await;
        }
        tokio::fs::write(path, data).await?;
    }
    Ok(())
}

struct RunningCommand {
    command: Vec<String>,
    parsed: Vec<ParsedCommand>,
    // Index of the in-history Exec cell for this call, if inserted
    history_index: Option<usize>,
    history_id: Option<HistoryId>,
    // Aggregated exploration entry (history index, entry index) when grouped
    explore_entry: Option<(usize, usize)>,
    stdout_offset: usize,
    stderr_offset: usize,
    wait_total: Option<Duration>,
    wait_active: bool,
    wait_notes: Vec<(String, bool)>,
}

const RATE_LIMIT_WARNING_THRESHOLDS: [f64; 3] = [50.0, 75.0, 90.0];
const RATE_LIMIT_REFRESH_INTERVAL: chrono::Duration = chrono::Duration::minutes(10);

const MAX_TRACKED_GHOST_COMMITS: usize = 20;
const GHOST_SNAPSHOT_NOTICE_THRESHOLD: Duration = Duration::from_secs(4);
const GHOST_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct RateLimitWarning {
    scope: RateLimitWarningScope,
    threshold: f64,
    message: String,
}

#[derive(Default)]
struct RateLimitWarningState {
    weekly_index: usize,
    hourly_index: usize,
}

impl RateLimitWarningState {
    fn take_warnings(
        &mut self,
        secondary_used_percent: f64,
        primary_used_percent: f64,
    ) -> Vec<RateLimitWarning> {
        let mut warnings = Vec::new();

        let mut next_weekly_index = self.weekly_index;
        while next_weekly_index < RATE_LIMIT_WARNING_THRESHOLDS.len()
            && secondary_used_percent >= RATE_LIMIT_WARNING_THRESHOLDS[next_weekly_index]
        {
            next_weekly_index += 1;
        }
        if next_weekly_index > self.weekly_index {
            let threshold = RATE_LIMIT_WARNING_THRESHOLDS[next_weekly_index - 1];
            warnings.push(RateLimitWarning {
                scope: RateLimitWarningScope::Secondary,
                threshold,
                message: format!(
                    "Secondary usage exceeded {threshold:.0}% of the limit. Run /limits for detailed usage."
                ),
            });
            self.weekly_index = next_weekly_index;
        }

        let mut next_hourly_index = self.hourly_index;
        while next_hourly_index < RATE_LIMIT_WARNING_THRESHOLDS.len()
            && primary_used_percent >= RATE_LIMIT_WARNING_THRESHOLDS[next_hourly_index]
        {
            next_hourly_index += 1;
        }
        if next_hourly_index > self.hourly_index {
            let threshold = RATE_LIMIT_WARNING_THRESHOLDS[next_hourly_index - 1];
            warnings.push(RateLimitWarning {
                scope: RateLimitWarningScope::Primary,
                threshold,
                message: format!(
                    "Hourly usage exceeded {threshold:.0}% of the limit. Run /limits for detailed usage."
                ),
            });
            self.hourly_index = next_hourly_index;
        }

        warnings
    }

    fn reset(&mut self) {
        self.weekly_index = 0;
        self.hourly_index = 0;
    }
}

#[derive(Clone)]
struct GhostSnapshotsDisabledReason {
    message: String,
    hint: Option<String>,
}

#[derive(Clone, Copy)]
struct ConversationSnapshot {
    user_turns: usize,
    assistant_turns: usize,
    history_len: usize,
    order_len: usize,
    order_dbg_len: usize,
}

impl ConversationSnapshot {
    fn new(user_turns: usize, assistant_turns: usize) -> Self {
        Self {
            user_turns,
            assistant_turns,
            history_len: 0,
            order_len: 0,
            order_dbg_len: 0,
        }
    }
}

#[derive(Clone)]
pub(crate) struct GhostState {
    snapshots: Vec<GhostSnapshot>,
    disabled: bool,
    disabled_reason: Option<GhostSnapshotsDisabledReason>,
    queue: VecDeque<(u64, GhostSnapshotRequest)>,
    active: Option<(u64, GhostSnapshotRequest)>,
    next_id: u64,
    queued_user_messages: VecDeque<UserMessage>,
}

#[cfg(any(test, feature = "test-helpers"))]
#[allow(dead_code)]
struct AutoReviewCommitScope {
    commit: String,
    file_count: usize,
}

#[cfg(any(test, feature = "test-helpers"))]
#[allow(dead_code)]
enum AutoReviewOutcome {
    Skip,
    Workspace,
    Commit(AutoReviewCommitScope),
}

#[cfg(test)]
pub(super) type CaptureAutoTurnCommitStub = Box<
    dyn Fn(&'static str, Option<String>) -> Result<GhostCommit, GitToolingError> + Send + Sync,
>;

#[cfg(test)]
pub(super) static CAPTURE_AUTO_TURN_COMMIT_STUB: Lazy<Mutex<Option<CaptureAutoTurnCommitStub>>> =
    Lazy::new(|| Mutex::new(None));

#[cfg(test)]
pub(super) type GitDiffNameOnlyBetweenStub =
    Box<dyn Fn(String, String) -> Result<Vec<String>, String> + Send + Sync>;

#[cfg(test)]
pub(super) static GIT_DIFF_NAME_ONLY_BETWEEN_STUB: Lazy<Mutex<Option<GitDiffNameOnlyBetweenStub>>> =
    Lazy::new(|| Mutex::new(None));

#[cfg(test)]
pub(super) static AUTO_STUB_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Deserialize)]
struct AutoResolveDecision {
    status: String,
    #[serde(default)]
    rationale: Option<String>,
}

const AGENTS_OVERVIEW_STATIC_ROWS: usize = 2; // spacer + "Add new agent" row

#[derive(Clone)]
struct PendingAgentUpdate {
    id: uuid::Uuid,
    cfg: AgentConfig,
}

impl PendingAgentUpdate {
    fn key(&self) -> String { format!("{}:{}", self.cfg.name.to_ascii_lowercase(), self.id) }
}

#[derive(Clone, Debug)]
struct BackgroundReviewState {
    worktree_path: std::path::PathBuf,
    branch: String,
    agent_id: Option<String>,
    snapshot: Option<String>,
    base: Option<GhostCommit>,
    last_seen: std::time::Instant,
}

#[derive(Clone, Debug)]
struct PendingAutoReviewRange {
    base: GhostCommit,
    defer_until_turn: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoReviewIndicatorStatus {
    Running,
    Clean,
    Fixed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AutoReviewStatus {
    status: AutoReviewIndicatorStatus,
    findings: Option<usize>,
    phase: AutoReviewPhase,
}

fn detect_auto_review_phase(progress: Option<&str>) -> AutoReviewPhase {
    let text = progress.unwrap_or_default().to_ascii_lowercase();
    // Prefer explicit phase markers emitted by exec when available.
    if text.contains("[auto-review] phase: resolving") {
        return AutoReviewPhase::Resolving;
    }
    if text.contains("[auto-review] phase: reviewing") {
        return AutoReviewPhase::Reviewing;
    }

    AutoReviewPhase::Reviewing
}

const SKIP_REVIEW_PROGRESS_SENTINEL: &str = "Another review is already running; skipping this /review.";
const AUTO_REVIEW_SHARED_WORKTREE: &str = "auto-review";
const AUTO_REVIEW_FALLBACK_PREFIX: &str = "auto-review-";
const AUTO_REVIEW_BASELINE_FILENAME: &str = "auto-review-baseline";
const AUTO_REVIEW_FALLBACK_MAX: usize = 3;
const AUTO_REVIEW_FALLBACK_MAX_AGE_SECS: u64 = 12 * 60 * 60; // 12h
const AUTO_REVIEW_STALE_SECS: u64 = 5 * 60;

fn auto_review_repo_dir(git_root: &Path) -> Result<PathBuf, String> {
    let repo_name = git_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo");
    let code_home = code_core::config::find_code_home()
        .map_err(|e| format!("failed to locate code home: {e}"))?;
    let repo_dir = code_home.join("working").join(repo_name);
    std::fs::create_dir_all(&repo_dir)
        .map_err(|e| format!("failed to create auto review repo dir: {e}"))?;
    Ok(repo_dir)
}

fn auto_review_branches_dir(git_root: &Path) -> Result<PathBuf, String> {
    let branches_dir = auto_review_repo_dir(git_root)?.join("branches");
    std::fs::create_dir_all(&branches_dir)
        .map_err(|e| format!("failed to create branches dir: {e}"))?;
    Ok(branches_dir)
}

fn auto_review_baseline_path_for_repo(git_root: &Path) -> Result<PathBuf, String> {
    Ok(auto_review_repo_dir(git_root)?.join(AUTO_REVIEW_BASELINE_FILENAME))
}

fn resolve_auto_review_worktree_path(git_root: &Path, branch: &str) -> Option<PathBuf> {
    if branch.is_empty() {
        return None;
    }

    let branches_dir = auto_review_branches_dir(git_root).ok()?;
    let candidate = branches_dir.join(branch);
    candidate.exists().then_some(candidate)
}

async fn remove_worktree_path(git_root: &Path, path: &Path) -> Result<(), String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| "invalid worktree path".to_string())?;
    let remove = tokio::process::Command::new("git")
        .current_dir(git_root)
        .args(["worktree", "remove", "-f", path_str])
        .output()
        .await
        .map_err(|e| format!("failed to remove worktree: {e}"))?;
    if !remove.status.success() {
        let stderr = String::from_utf8_lossy(&remove.stderr);
        tracing::warn!("failed to remove fallback worktree via git: {}", stderr.trim());
    }
    if path.exists()
        && let Err(e) = tokio::fs::remove_dir_all(path).await {
            tracing::warn!("failed to delete fallback worktree dir {:?}: {}", path, e);
        }
    Ok(())
}

async fn cleanup_fallback_worktrees(git_root: &Path) -> Result<(), String> {
    let branches_dir = auto_review_branches_dir(git_root)?;
    let mut entries: Vec<(PathBuf, SystemTime)> = Vec::new();
    if let Ok(read_dir) = fs::read_dir(&branches_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry
                .file_name()
                .into_string()
                .unwrap_or_default();
            if !name.starts_with(AUTO_REVIEW_FALLBACK_PREFIX) || name == AUTO_REVIEW_SHARED_WORKTREE {
                continue;
            }
            let meta = entry.metadata().ok();
            let mtime = meta
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            entries.push((path, mtime));
        }
    }

    // Age-based prune
    let now = SystemTime::now();
    for (path, mtime) in entries.iter() {
        if let Ok(elapsed) = now.duration_since(*mtime)
            && elapsed.as_secs() > AUTO_REVIEW_FALLBACK_MAX_AGE_SECS
                && let Ok(Some(g)) = try_acquire_lock("review-fallback", path) {
                    drop(g);
                    let _ = remove_worktree_path(git_root, path).await;
                }
    }

    // Count-based prune
    let mut remaining: Vec<(PathBuf, SystemTime)> = entries
        .into_iter()
        .filter(|(p, _)| p.exists())
        .collect();
    remaining.sort_by_key(|(_, t)| *t);
    while remaining.len() > AUTO_REVIEW_FALLBACK_MAX {
        if let Some((path, _)) = remaining.first().cloned() {
            if let Ok(Some(g)) = try_acquire_lock("review-fallback", &path) {
                drop(g);
                let _ = remove_worktree_path(git_root, &path).await;
                remaining.remove(0);
            } else {
                // Busy; skip pruning this one
                break;
            }
        }
    }

    Ok(())
}

async fn allocate_fallback_auto_review_worktree(
    git_root: &Path,
    snapshot_id: &str,
) -> Result<(PathBuf, String, ReviewGuard), String> {
    cleanup_fallback_worktrees(git_root).await?;
    let branches_dir = auto_review_branches_dir(git_root)?;
    let short = snapshot_id.chars().take(8).collect::<String>();

    for attempt in 0..AUTO_REVIEW_FALLBACK_MAX {
        let suffix = if attempt == 0 { String::new() } else { format!("-{}", attempt + 1) };
        let name = format!("{AUTO_REVIEW_FALLBACK_PREFIX}{short}{suffix}");
        let path = branches_dir.join(&name);

        match try_acquire_lock("review-fallback", &path) {
            Ok(Some(guard)) => {
                let worktree_path = code_core::git_worktree::prepare_reusable_worktree(
                    git_root,
                    &name,
                    snapshot_id,
                    true,
                )
                .await
                .map_err(|e| format!("failed to prepare fallback worktree: {e}"))?;
                return Ok((worktree_path, name, guard));
            }
            Ok(None) => continue, // in use, try next suffix
            Err(err) => return Err(format!("could not acquire fallback review lock: {err}")),
        }
    }

    Err("Auto review fallback pool is busy; try again soon.".to_string())
}

#[derive(Clone, Debug)]
struct AutoReviewNotice {
    history_id: HistoryId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TurnOrigin {
    User,
    Developer,
}

#[derive(Clone, Debug)]
struct PendingRequestUserInput {
    turn_id: String,
    call_id: String,
    anchor_key: OrderKey,
    questions: Vec<code_protocol::request_user_input::RequestUserInputQuestion>,
}

#[derive(Clone)]
struct RenderRequestSeed {
    history_id: HistoryId,
    use_cache: bool,
    fallback_lines: Option<Rc<Vec<Line<'static>>>>,
    kind: RenderRequestKind,
}

/// Actions that can be triggered by clicking on UI elements
#[derive(Clone, Debug, PartialEq, Eq)]
enum ClickableAction {
    ShowModelSelector,
    ShowShellSelector,
    ShowReasoningSelector,
    #[allow(dead_code)]
    ExecuteCommand(String),
}

/// A clickable region with its screen coordinates and associated action
#[derive(Clone, Debug)]
struct ClickableRegion {
    rect: ratatui::layout::Rect,
    action: ClickableAction,
}

pub(crate) struct ChatWidget<'a> {
    app_event_tx: AppEventSender,
    code_op_tx: UnboundedSender<Op>,
    bottom_pane: BottomPane<'a>,
    auth_manager: Arc<AuthManager>,
    login_view_state: Option<Weak<RefCell<LoginAccountsState>>>,
    login_add_view_state: Option<Weak<RefCell<LoginAddAccountState>>>,
    active_exec_cell: Option<ExecCell>,
    history_cells: Vec<Box<dyn HistoryCell>>, // Store all history in memory
    history_cell_ids: Vec<Option<HistoryId>>,
    history_live_window: Option<(usize, usize)>,
    history_frozen_width: u16,
    history_frozen_count: usize,
    history_render: HistoryRenderState,
    last_render_settings: Cell<RenderSettings>,
    history_virtualization_sync_pending: Cell<bool>,
    render_request_cache: RefCell<Vec<RenderRequestSeed>>,
    render_request_cache_dirty: Cell<bool>,
    history_prefix_append_only: Cell<bool>,
    render_theme_epoch: u64,
    history_state: HistoryState,
    history_snapshot_dirty: bool,
    history_snapshot_last_flush: Option<Instant>,
    context_cell_id: Option<HistoryId>,
    context_summary: Option<ContextSummary>,
    context_last_sequence: Option<u64>,
    context_browser_sequence: Option<u64>,
    config: Config,
    mcp_tool_catalog_by_id: HashMap<String, mcp_types::Tool>,
    mcp_tools_by_server: HashMap<String, Vec<String>>,
    mcp_disabled_tools_by_server: HashMap<String, Vec<String>>,
    mcp_server_failures: HashMap<String, McpServerFailure>,
    /// Startup-only MCP init error summary. We keep this out of history so the
    /// welcome intro doesn't jump when MCP status changes.
    startup_mcp_error_summary: Option<String>,

    /// Optional remote-merged presets list delivered asynchronously.
    /// When absent, the TUI falls back to built-in presets.
    remote_model_presets: Option<Vec<ModelPreset>>,
    /// Whether remote defaults may be applied to this session.
    /// Captured at startup so later config changes don't retroactively enable it.
    allow_remote_default_at_startup: bool,
    /// Tracks whether the user explicitly selected a chat model in this session.
    chat_model_selected_explicitly: bool,

    planning_restore: Option<(String, ReasoningEffort)>,
    history_debug_events: Option<RefCell<Vec<String>>>,
    latest_upgrade_version: Option<String>,
    reconnect_notice_active: bool,
    initial_user_message: Option<UserMessage>,
    total_token_usage: TokenUsage,
    last_token_usage: TokenUsage,
    rate_limit_snapshot: Option<RateLimitSnapshotEvent>,
    rate_limit_warnings: RateLimitWarningState,
    rate_limit_fetch_inflight: bool,
    rate_limit_last_fetch_at: Option<DateTime<Utc>>,
    rate_limit_primary_next_reset_at: Option<DateTime<Utc>>,
    rate_limit_secondary_next_reset_at: Option<DateTime<Utc>>,
    rate_limit_refresh_scheduled_for: Option<DateTime<Utc>>,
    rate_limit_refresh_schedule_id: Arc<AtomicU64>,
    content_buffer: String,
    // Buffer for streaming assistant answer text; we do not surface partial
    // We wait for the final AgentMessage event and then emit the full text
    // at once into scrollback so the history contains a single message.
    // Cache of the last finalized assistant message to suppress immediate duplicates
    last_assistant_message: Option<String>,
    // Track the most recent finalized Answer output item within the current turn.
    // When a new Answer stream id arrives, we retroactively mark the previous
    // assistant message as a mid-turn update for styling.
    last_answer_stream_id_in_turn: Option<String>,
    last_answer_history_id_in_turn: Option<HistoryId>,
    // Track the most recent Answer stream id we've *seen* in this turn (delta or final).
    // Used to label earlier answers as mid-turn even if their final cell hasn't
    // been inserted yet.
    last_seen_answer_stream_id_in_turn: Option<String>,
    mid_turn_answer_ids_in_turn: HashSet<String>,
    // Cache of the last user text we submitted (for context passing to review/resolve agents)
    last_user_message: Option<String>,
    // Cache of the last developer/system note we injected (hidden messages)
    last_developer_message: Option<String>,
    pending_turn_origin: Option<TurnOrigin>,
    pending_request_user_input: Option<PendingRequestUserInput>,
    current_turn_origin: Option<TurnOrigin>,
    // Tracks whether lingering running exec/tool cells have been cleared for the
    // current turn. Reset on TaskStarted; set after the first assistant message
    // (delta or final) arrives, which is more reliable than TaskComplete.
    cleared_lingering_execs_this_turn: bool,
    // Track the ID of the current streaming message to prevent duplicates
    // Track the ID of the current streaming reasoning to prevent duplicates
    exec: ExecState,
    tools_state: ToolState,
    live_builder: RowBuilder,
    header_wave: HeaderWaveEffect,
    browser_overlay_visible: bool,
    browser_overlay_state: BrowserOverlayState,
    // Store pending image paths keyed by their placeholder text
    pending_images: HashMap<String, PathBuf>,
    // (removed) pending non-image files are no longer tracked; non-image paths remain as plain text
    welcome_shown: bool,
    test_mode: bool,
    // Path to the latest browser screenshot and URL for display
    latest_browser_screenshot: Arc<Mutex<Option<(PathBuf, String)>>>,
    browser_autofix_requested: Arc<AtomicBool>,
    // Cached image protocol to avoid recreating every frame (path, area, protocol)
    cached_image_protocol:
        std::cell::RefCell<Option<(PathBuf, Rect, ratatui_image::protocol::Protocol)>>,
    // Cached picker to avoid recreating every frame
    cached_picker: std::cell::RefCell<Option<Picker>>,

    // Cached cell size (width,height) in pixels
    cached_cell_size: std::cell::OnceCell<(u16, u16)>,
    git_branch_cache: RefCell<GitBranchCache>,

    // Terminal information from startup
    terminal_info: crate::tui::TerminalInfo,
    // Agent tracking for multi-agent tasks
    active_agents: Vec<AgentInfo>,
    agents_ready_to_start: bool,
    last_agent_prompt: Option<String>,
    agent_context: Option<String>,
    agent_task: Option<String>,
    recent_agent_hint: Option<String>,
    suppress_next_agent_hint: bool,
    active_review_hint: Option<String>,
    active_review_prompt: Option<String>,
    auto_resolve_state: Option<AutoResolveState>,
    auto_resolve_attempts_baseline: u32,
    turn_had_code_edits: bool,
    background_review: Option<BackgroundReviewState>,
    auto_review_status: Option<AutoReviewStatus>,
    auto_review_notice: Option<AutoReviewNotice>,
    auto_review_baseline: Option<GhostCommit>,
    auto_review_reviewed_marker: Option<GhostCommit>,
    pending_auto_review_range: Option<PendingAutoReviewRange>,
    turn_sequence: u64,
    review_guard: Option<ReviewGuard>,
    background_review_guard: Option<ReviewGuard>,
    processed_auto_review_agents: HashSet<String>,
    // New: coordinator-provided hints for the next Auto turn
    pending_turn_descriptor: Option<TurnDescriptor>,
    pending_auto_turn_config: Option<TurnConfig>,
    overall_task_status: String,
    active_plan_title: Option<String>,
    /// Runtime timing per-agent (by id) to improve visibility in the HUD
    agent_runtime: HashMap<String, AgentRuntime>,
    pending_agent_updates: HashMap<String, PendingAgentUpdate>,
    // Sparkline data for showing agent activity (using RefCell for interior mutability)
    // Each tuple is (value, is_completed) where is_completed indicates if any agent was complete at that time
    sparkline_data: std::cell::RefCell<Vec<(u64, bool)>>,
    last_sparkline_update: std::cell::RefCell<std::time::Instant>,
    // Stream controller for managing streaming content
    stream: crate::streaming::controller::StreamController,
    // Stream lifecycle state (kind, closures, sequencing, cancel)
    stream_state: StreamState,
    // Interrupt manager for handling cancellations
    interrupts: interrupts::InterruptManager,
    // Guard to avoid spamming flush timers while interrupts wait behind a stalled stream
    interrupt_flush_scheduled: bool,

    // Guard for out-of-order exec events: track call_ids that already ended
    ended_call_ids: HashSet<ExecCallId>,
    /// Exec call_ids that were explicitly cancelled by user interrupt. Used to
    /// drop any late ExecEnd events so we don't render duplicate cells.
    canceled_exec_call_ids: HashSet<ExecCallId>,

    // Accumulated diff/session state
    diffs: DiffsState,

    // Help overlay state
    help: HelpState,

    // Settings overlay state
    settings: SettingsState,
    // When a standalone picker (model selection) closes, optionally reopen the settings overlay
    pending_settings_return: Option<SettingsSection>,

    // Limits overlay state
    limits: LimitsState,

    // Terminal overlay state
    terminal: TerminalState,
    pending_manual_terminal: HashMap<u64, PendingManualTerminal>,

    // Persisted selection for Agents overview
    agents_overview_selected_index: usize,

    // State for the Agents Terminal view
    agents_terminal: AgentsTerminalState,

    pending_git_init_resume: Option<GitInitResume>,
    git_init_inflight: bool,
    git_init_declined: bool,

    pending_upgrade_notice: Option<(u64, String)>,

    // Cached visible rows for the diff overlay body to clamp scrolling (kept within diffs)

    // Centralized height manager (always enabled)
    height_manager: RefCell<HeightManager>,

    // Aggregated layout and scroll state
    layout: LayoutState,

    // True when connected to external Chrome via CDP; affects HUD titles
    browser_is_external: bool,

    // Most recent theme snapshot used to retint pre-rendered lines
    last_theme: crate::theme::Theme,

    // Performance tracing (opt-in via /perf)
    perf_state: PerfState,
    // Current session id (from SessionConfigured)
    session_id: Option<uuid::Uuid>,

    // Pending diagnostics integration
    next_cli_text_format: Option<TextFormat>,

    // Pending jump-back state (reversible until submit)

    // Track active task ids so we don't drop the working status while any
    // agent/sub‑agent is still running (long‑running sessions can interleave).
    active_task_ids: HashSet<String>,

    // --- Queued user message support ---
    // Messages typed while a task is running are kept here and rendered
    // at the bottom as "(queued)" until the next turn begins. At that
    // point we submit one queued message and move its cell into the
    // normal history within the new turn window.
    queued_user_messages: std::collections::VecDeque<UserMessage>,
    pending_dispatched_user_messages: std::collections::VecDeque<String>,
    // Number of user prompts we pre-pended to history just before starting
    // a new turn; used to anchor the next turn window so assistant output
    // appears after them.
    pending_user_prompts_for_next_turn: usize,
    ghost_snapshots: Vec<GhostSnapshot>,
    ghost_snapshots_disabled: bool,
    ghost_snapshots_disabled_reason: Option<GhostSnapshotsDisabledReason>,
    ghost_snapshot_queue: VecDeque<(u64, GhostSnapshotRequest)>,
    active_ghost_snapshot: Option<(u64, GhostSnapshotRequest)>,
    next_ghost_snapshot_id: u64,
    queue_block_started_at: Option<Instant>,

    auto_drive_card_sequence: u64,
    auto_drive_variant: AutoDriveVariant,
    auto_state: AutoDriveController,
    auto_goal_escape_state: AutoGoalEscState,
    auto_handle: Option<AutoCoordinatorHandle>,
    auto_drive_pid_guard: Option<AutoDrivePidFile>,
    auto_history: AutoDriveHistory,
    auto_compaction_overlay: Option<AutoCompactionOverlay>,
    auto_turn_review_state: Option<AutoTurnReviewState>,
    auto_pending_goal_request: bool,
    auto_goal_bootstrap_done: bool,
    cloud_tasks_selected_env: Option<CloudEnvironment>,
    cloud_tasks_environments: Vec<CloudEnvironment>,
    cloud_tasks_last_tasks: Vec<TaskSummary>,
    cloud_tasks_best_of_n: usize,
    cloud_tasks_creation_inflight: bool,
    cloud_task_apply_tickets: HashMap<(String, bool), BackgroundOrderTicket>,
    cloud_task_create_ticket: Option<BackgroundOrderTicket>,

    // Event sequencing to preserve original order across streaming/tool events
    // and stream-related flags moved into stream_state

    // Strict global ordering for history: every cell has a required key
    // (req, out, seq). No unordered inserts and no turn windows.
    cell_order_seq: Vec<OrderKey>,
    // Debug: per-cell order info string rendered in the UI to diagnose ordering.
    cell_order_dbg: Vec<Option<String>>,
    // Routing for reasoning stream ids -> existing CollapsibleReasoningCell index
    reasoning_index: HashMap<String, usize>,
    // Stable per-(kind, stream_id) ordering, derived from OrderMeta.
    stream_order_seq: HashMap<(StreamKind, String), OrderKey>,
    // Resume-aware bias applied to provider request ordinals for restored sessions.
    order_request_bias: u64,
    resume_expected_next_request: Option<u64>,
    resume_provider_baseline: Option<u64>,
    // Track last provider request_ordinal seen so internal messages can be
    // assigned request_index = last_seen + 1 (with out = -1).
    last_seen_request_index: u64,
    // Synthetic request index used for internal-only messages; always >= last_seen_request_index
    current_request_index: u64,
    // Monotonic seq for internal messages to keep intra-request order stable
    internal_seq: u64,
    // Show order overlay when true (from --order)
    show_order_overlay: bool,

    // One-time hint to teach input history navigation
    scroll_history_hint_shown: bool,

    // Track and manage the access-mode background status cell so mode changes
    // replace the existing status instead of stacking multiple entries.
    access_status_idx: Option<usize>,
    /// When true, render without the top status bar and HUD so the normal
    /// terminal scrollback remains usable (Ctrl+T standard terminal mode).
    pub(crate) standard_terminal_mode: bool,
    // Pending system notes to inject into the agent's conversation history
    // before the next user turn. Each entry is sent in order ahead of the
    // user's visible prompt.
    pending_agent_notes: Vec<String>,

    // Stable synthetic request bucket for pre‑turn system notices (set on first use)
    synthetic_system_req: Option<u64>,
    // Map of system notice ids to their history index for in-place replacement
    system_cell_by_id: std::collections::HashMap<String, usize>,
    // Per-request counters for UI-issued background order metadata
    ui_background_seq_counters: HashMap<u64, Arc<AtomicU64>>,
    // Track the largest order key we have assigned so far to keep tail inserts monotonic
    last_assigned_order: Option<OrderKey>,
    replay_history_depth: usize,
    resume_placeholder_visible: bool,
    resume_picker_loading: bool,
    // Clickable regions for mouse interaction (tracked during render, checked on click)
    clickable_regions: RefCell<Vec<ClickableRegion>>,
    // Current hovered header action (for hover styling on top status line).
    hovered_clickable_action: RefCell<Option<ClickableAction>>,
}

#[derive(Clone, Debug, Default)]
struct ContextSummary {
    cwd: Option<String>,
    git_branch: Option<String>,
    reasoning_effort: Option<String>,
    browser_session_active: bool,
    deltas: Vec<ContextDeltaRecord>,
    browser_snapshot: Option<ContextBrowserSnapshotRecord>,
    expanded: bool,
}

#[derive(Clone, Debug)]
struct AutoCompactionOverlay {
    /// Snapshot of the conversation prefix (including the latest compact summary)
    /// that should be injected ahead of any history-derived tail when exporting
    /// the next Auto Drive request.
    prefix_items: Vec<code_protocol::models::ResponseItem>,
    /// History cell index that marks the beginning of the still-live tail that
    /// we continue to mirror directly from the UI.
    tail_start_cell: usize,
}

#[derive(Clone)]
pub(crate) struct BackgroundOrderTicket {
    request_ordinal: u64,
    seq_counter: Arc<AtomicU64>,
}

impl BackgroundOrderTicket {
    pub(crate) fn next_order(&self) -> code_core::protocol::OrderMeta {
        let seq = self.seq_counter.fetch_add(1, Ordering::SeqCst);
        code_core::protocol::OrderMeta {
            request_ordinal: self.request_ordinal,
            output_index: Some(i32::MAX as u32),
            sequence_number: Some(seq),
        }
    }
}

#[derive(Clone)]
struct GhostSnapshot {
    commit: GhostCommit,
    captured_at: DateTime<Local>,
    summary: Option<String>,
    conversation: ConversationSnapshot,
    history: HistorySnapshot,
}

#[derive(Clone, Copy)]
enum UndoPreviewRole {
    User,
    Assistant,
}

impl GhostSnapshot {
    fn new(
        commit: GhostCommit,
        summary: Option<String>,
        conversation: ConversationSnapshot,
        history: HistorySnapshot,
    ) -> Self {
        let summary = summary.and_then(|text| {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        Self {
            commit,
            captured_at: Local::now(),
            summary,
            conversation,
            history,
        }
    }

    fn commit(&self) -> &GhostCommit {
        &self.commit
    }

    fn short_id(&self) -> String {
        self.commit.id().chars().take(8).collect()
    }

    fn summary_snippet(&self, max_len: usize) -> Option<String> {
        let summary = self.summary.as_ref()?;
        let mut snippet = String::new();
        let mut truncated = false;
        for word in summary.split_whitespace() {
            if !snippet.is_empty() {
                snippet.push(' ');
            }
            snippet.push_str(word);
            if snippet.chars().count() > max_len {
                truncated = true;
                break;
            }
        }

        if snippet.chars().count() > max_len {
            truncated = true;
            snippet = snippet.chars().take(max_len).collect();
        }

        if truncated {
            snippet.push('…');
        }

        Some(snippet)
    }

    fn age_from(&self, now: DateTime<Local>) -> Option<std::time::Duration> {
        now.signed_duration_since(self.captured_at).to_std().ok()
    }
}

#[derive(Clone)]
struct GhostSnapshotRequest {
    summary: Option<String>,
    conversation: ConversationSnapshot,
    history: HistorySnapshot,
    started_at: Instant,
}

impl GhostSnapshotRequest {
    fn new(
        summary: Option<String>,
        conversation: ConversationSnapshot,
        history: HistorySnapshot,
    ) -> Self {
        Self {
            summary,
            conversation,
            history,
            started_at: Instant::now(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GhostSnapshotJobHandle {
    Scheduled(u64),
    Skipped,
}

#[derive(Default)]
struct GitBranchCache {
    value: Option<String>,
    last_head_mtime: Option<SystemTime>,
    last_refresh: Option<Instant>,
}

#[derive(Debug, Clone, Default)]
struct AgentRuntime {
    /// First time this agent entered Running
    started_at: Option<Instant>,
    /// Time of the latest status update we observed
    last_update: Option<Instant>,
    /// Time the agent reached a terminal state (Completed/Failed)
    completed_at: Option<Instant>,
}

#[derive(Debug, Clone)]
struct AgentTerminalEntry {
    name: String,
    batch_id: Option<String>,
    batch_label: Option<String>,
    batch_prompt: Option<String>,
    batch_context: Option<String>,
    model: Option<String>,
    status: AgentStatus,
    source_kind: Option<AgentSourceKind>,
    last_progress: Option<String>,
    result: Option<String>,
    error: Option<String>,
    logs: Vec<AgentLogEntry>,
}

impl AgentTerminalEntry {
    fn new(
        name: String,
        model: Option<String>,
        status: AgentStatus,
        batch_id: Option<String>,
    ) -> Self {
        Self {
            name,
            batch_id,
            batch_label: None,
            batch_prompt: None,
            batch_context: None,
            model,
            status,
            source_kind: None,
            last_progress: None,
            result: None,
            error: None,
            logs: Vec::new(),
        }
    }

    fn push_log(&mut self, kind: AgentLogKind, message: impl Into<String>) {
        let msg = message.into();
        if self
            .logs
            .last()
            .map(|entry| entry.kind == kind && entry.message == msg)
            .unwrap_or(false)
        {
            return;
        }
        self.logs.push(AgentLogEntry {
            timestamp: Local::now(),
            kind,
            message: msg,
        });
        const MAX_HISTORY: usize = 500;
        if self.logs.len() > MAX_HISTORY {
            let excess = self.logs.len() - MAX_HISTORY;
            self.logs.drain(0..excess);
        }
    }
}

#[derive(Debug, Clone)]
struct AgentLogEntry {
    timestamp: DateTime<Local>,
    kind: AgentLogKind,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentLogKind {
    Status,
    Progress,
    Result,
    Error,
}

struct AgentsTerminalState {
    active: bool,
    selected_index: usize,
    order: Vec<String>,
    entries: HashMap<String, AgentTerminalEntry>,
    scroll_offsets: HashMap<String, u16>,
    // Last scroll offset used to render the detail view (bottom-anchored)
    last_render_scroll: std::cell::Cell<u16>,
    saved_scroll_offset: u16,
    shared_context: Option<String>,
    shared_task: Option<String>,
    pending_stop: Option<PendingAgentStop>,
    focus: AgentsTerminalFocus,
    active_tab: AgentsTerminalTab,
    sort_mode: AgentsSortMode,
    highlights_collapsed: bool,
    actions_collapsed: bool,
}

#[derive(Clone, Debug)]
struct PendingAgentStop {
    agent_id: String,
    agent_name: String,
}

#[derive(Default, Clone)]
struct AgentBatchMetadata {
    label: Option<String>,
    prompt: Option<String>,
    context: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum AgentsSidebarEntry {
    Agent(String),
}

#[derive(Clone, Debug)]
struct AgentsSidebarGroup {
    batch_id: Option<String>,
    label: String,
    agent_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentsTerminalTab {
    All,
    Running,
    Failed,
    Completed,
    Review,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentsSortMode {
    Recent,
    RunningFirst,
    Name,
}

fn short_batch_label(batch_id: &str) -> String {
    let compact: String = batch_id.chars().filter(|c| *c != '-').collect();
    let source = if compact.is_empty() { batch_id } else { compact.as_str() };
    let short: String = source.chars().take(8).collect();
    if short.is_empty() {
        "Batch".to_string()
    } else {
        format!("Batch {short}")
    }
}

impl AgentsSidebarEntry {
    fn scroll_key(&self) -> String {
        match self {
            AgentsSidebarEntry::Agent(id) => format!("agent:{id}"),
        }
    }
}

impl AgentsTerminalState {
    fn new() -> Self {
        Self {
            active: false,
            selected_index: 0,
            order: Vec::new(),
            entries: HashMap::new(),
            scroll_offsets: HashMap::new(),
            last_render_scroll: std::cell::Cell::new(0),
            saved_scroll_offset: 0,
            shared_context: None,
            shared_task: None,
            pending_stop: None,
            focus: AgentsTerminalFocus::Sidebar,
            active_tab: AgentsTerminalTab::All,
            sort_mode: AgentsSortMode::Recent,
            highlights_collapsed: false,
            actions_collapsed: false,
        }
    }

    fn reset(&mut self) {
        self.selected_index = 0;
        self.order.clear();
        self.entries.clear();
        self.scroll_offsets.clear();
        self.last_render_scroll.set(0);
        self.shared_context = None;
        self.shared_task = None;
        self.pending_stop = None;
        self.focus = AgentsTerminalFocus::Sidebar;
        self.active_tab = AgentsTerminalTab::All;
    }

    fn current_sidebar_entry(&self) -> Option<AgentsSidebarEntry> {
        let entries = self.sidebar_entries();
        entries.get(self.selected_index).cloned()
    }

    fn focus_sidebar(&mut self) {
        self.focus = AgentsTerminalFocus::Sidebar;
    }

    fn focus_detail(&mut self) {
        self.focus = AgentsTerminalFocus::Detail;
    }

    fn focus(&self) -> AgentsTerminalFocus {
        self.focus
    }

    fn set_stop_prompt(&mut self, agent_id: String, agent_name: String) {
        self.pending_stop = Some(PendingAgentStop { agent_id, agent_name });
    }

    fn clear_stop_prompt(&mut self) {
        self.pending_stop = None;
    }

    fn clamp_selected_index(&mut self) {
        let entries = self.sidebar_entries();
        if entries.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= entries.len() {
            self.selected_index = entries.len().saturating_sub(1);
        }
    }

    fn reselect_entry(&mut self, entry: Option<AgentsSidebarEntry>) {
        if let Some(target) = entry
            && let Some(idx) = self
                .sidebar_entries()
                .iter()
                .position(|candidate| *candidate == target)
            {
                self.selected_index = idx;
                return;
            }
        self.clamp_selected_index();
    }

    fn cycle_sort_mode(&mut self) {
        self.sort_mode = match self.sort_mode {
            AgentsSortMode::Recent => AgentsSortMode::RunningFirst,
            AgentsSortMode::RunningFirst => AgentsSortMode::Name,
            AgentsSortMode::Name => AgentsSortMode::Recent,
        };
    }

    fn toggle_highlights(&mut self) {
        self.highlights_collapsed = !self.highlights_collapsed;
    }

    fn toggle_actions(&mut self) {
        self.actions_collapsed = !self.actions_collapsed;
    }

    fn tab_allows(&self, entry: &AgentTerminalEntry) -> bool {
        match self.active_tab {
            AgentsTerminalTab::All => true,
            AgentsTerminalTab::Running =>
                matches!(entry.status, AgentStatus::Pending | AgentStatus::Running),
            AgentsTerminalTab::Failed => matches!(entry.status, AgentStatus::Failed),
            AgentsTerminalTab::Completed =>
                matches!(entry.status, AgentStatus::Completed | AgentStatus::Cancelled),
            AgentsTerminalTab::Review => matches!(entry.source_kind, Some(AgentSourceKind::AutoReview)),
        }
    }

    fn filtered_order(&self) -> Vec<String> {
        let mut filtered: Vec<String> = self
            .order
            .iter()
            .filter(|id| {
                self.entries
                    .get(*id)
                    .map(|entry| self.tab_allows(entry))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        match self.sort_mode {
            AgentsSortMode::Recent => {
                // keep insertion order
            }
            AgentsSortMode::RunningFirst => {
                let mut positions: HashMap<String, usize> = HashMap::new();
                for (idx, id) in self.order.iter().enumerate() {
                    positions.insert(id.clone(), idx);
                }
                filtered.sort_by(|a, b| {
                    let sa = self
                        .entries
                        .get(a)
                        .map(|e| agent_running_priority(e.status.clone()))
                        .unwrap_or(usize::MAX);
                    let sb = self
                        .entries
                        .get(b)
                        .map(|e| agent_running_priority(e.status.clone()))
                        .unwrap_or(usize::MAX);
                    sa.cmp(&sb).then_with(|| positions[a].cmp(&positions[b]))
                });
            }
            AgentsSortMode::Name => {
                filtered.sort_by(|a, b| {
                    let left = self
                        .entries
                        .get(a)
                        .and_then(|e| e.name.split_whitespace().next())
                        .unwrap_or("")
                        .to_lowercase();
                    let right = self
                        .entries
                        .get(b)
                        .and_then(|e| e.name.split_whitespace().next())
                        .unwrap_or("")
                        .to_lowercase();
                    left.cmp(&right).then_with(|| a.cmp(b))
                });
            }
        }

        filtered
    }

    fn sidebar_entries(&self) -> Vec<AgentsSidebarEntry> {
        let mut out = Vec::new();
        for group in self.sidebar_groups() {
            for agent_id in group.agent_ids {
                out.push(AgentsSidebarEntry::Agent(agent_id));
            }
        }
        out
    }

    fn sidebar_groups(&self) -> Vec<AgentsSidebarGroup> {
        let mut groups: Vec<AgentsSidebarGroup> = Vec::new();
        let mut group_lookup: HashMap<Option<String>, usize> = HashMap::new();
        for id in self.filtered_order() {
            if let Some(entry) = self.entries.get(&id) {
                let key = entry.batch_id.clone();
                let idx = if let Some(idx) = group_lookup.get(&key) {
                    *idx
                } else {
                    let label = entry
                        .batch_label
                        .as_ref()
                        .and_then(|value| {
                            let trimmed = value.trim();
                            (!trimmed.is_empty()).then(|| trimmed.to_string())
                        })
                        .or_else(|| {
                            key.as_ref().map(|batch| short_batch_label(batch))
                        })
                        .unwrap_or_else(|| "Ad-hoc Agents".to_string());
                    let idx = groups.len();
                    group_lookup.insert(key.clone(), idx);
                    groups.push(AgentsSidebarGroup {
                        batch_id: key.clone(),
                        label,
                        agent_ids: Vec::new(),
                    });
                    idx
                };
                if let Some(group) = groups.get_mut(idx) {
                    group.agent_ids.push(id.clone());
                }
            }
        }
        groups
    }

    fn set_tab(&mut self, tab: AgentsTerminalTab) {
        if self.active_tab != tab {
            self.active_tab = tab;
            self.selected_index = 0;
        }
        self.clear_stop_prompt();
        self.clamp_selected_index();
    }

    fn jump_batch(&mut self, delta: isize) {
        let groups = self.sidebar_groups();
        if groups.is_empty() {
            return;
        }
        let current_batch = match self.current_sidebar_entry() {
            Some(AgentsSidebarEntry::Agent(id)) => self
                .entries
                .get(id.as_str())
                .and_then(|entry| entry.batch_id.clone()),
            None => None,
        };
        let mut idx: isize = groups
            .iter()
            .position(|group| group.batch_id == current_batch)
            .unwrap_or(0) as isize;
        let len = groups.len() as isize;
        if len == 0 {
            return;
        }
        idx = (idx + delta).rem_euclid(len);
        if let Some(target) = groups.get(idx as usize)
            && let Some(first_agent) = target.agent_ids.first()
                && let Some(pos) = self
                    .sidebar_entries()
                    .iter()
                    .position(|entry| matches!(entry, AgentsSidebarEntry::Agent(id) if id == first_agent))
                {
                    self.selected_index = pos;
                    self.focus_sidebar();
                    self.clear_stop_prompt();
                }
        self.clamp_selected_index();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentsTerminalFocus {
    Sidebar,
    Detail,
}

// ---------- Stable ordering & routing helpers ----------
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OrderKey {
    req: u64,
    out: i32,
    seq: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BrowserSessionOrderKey {
    req: u64,
    out: i32,
}

impl BrowserSessionOrderKey {
    fn from_order_meta(meta: &code_core::protocol::OrderMeta) -> Self {
        let out = meta
            .output_index
            .map(|value| {
                if value > i32::MAX as u32 {
                    i32::MAX
                } else {
                    value as i32
                }
            })
            .unwrap_or(i32::MAX);
        Self {
            req: meta.request_ordinal,
            out,
        }
    }
}

impl Ord for OrderKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.req.cmp(&other.req) {
            std::cmp::Ordering::Equal => match self.out.cmp(&other.out) {
                std::cmp::Ordering::Equal => self.seq.cmp(&other.seq),
                o => o,
            },
            o => o,
        }
    }
}

impl PartialOrd for OrderKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl From<OrderKeySnapshot> for OrderKey {
    fn from(snapshot: OrderKeySnapshot) -> Self {
        Self {
            req: snapshot.req,
            out: snapshot.out,
            seq: snapshot.seq,
        }
    }
}

impl From<OrderKey> for OrderKeySnapshot {
    fn from(key: OrderKey) -> Self {
        OrderKeySnapshot {
            req: key.req,
            out: key.out,
            seq: key.seq,
        }
    }
}

// Removed legacy turn-window logic; ordering is strictly global.

// Global guard to prevent overlapping background screenshot captures and to rate-limit them
static BG_SHOT_IN_FLIGHT: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));
static BACKGROUND_REVIEW_LOCKS: Lazy<Mutex<HashMap<String, code_core::review_coord::ReviewGuard>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static BG_SHOT_LAST_START_MS: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(0));
static MERGE_LOCKS: Lazy<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static WORKTREE_ROOT_HINTS: Lazy<Mutex<HashMap<PathBuf, PathBuf>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static CWD_HISTORY: Lazy<Mutex<Vec<PathBuf>>> = Lazy::new(|| Mutex::new(Vec::new()));
const CWD_HISTORY_LIMIT: usize = 16;

fn remember_worktree_root_hint(worktree: &Path, git_root: &Path) {
    let mut hints = WORKTREE_ROOT_HINTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = git_root.to_path_buf();
    hints.insert(worktree.to_path_buf(), root.clone());
    if let Ok(real) = std::fs::canonicalize(worktree) {
        hints.insert(real, root);
    }
}

fn worktree_root_hint_for(path: &Path) -> Option<PathBuf> {
    let hints = WORKTREE_ROOT_HINTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    hints.get(path).cloned()
}

fn remember_cwd_history(path: &Path) {
    let mut history = CWD_HISTORY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if history.last().is_some_and(|p| p == path) {
        return;
    }
    history.push(path.to_path_buf());
    if history.len() > CWD_HISTORY_LIMIT {
        history.remove(0);
    }
}

fn last_existing_cwd(except: &Path) -> Option<PathBuf> {
    let history = CWD_HISTORY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    history
        .iter()
        .rev()
        .filter(|p| p.as_path() != except)
        .find(|p| p.exists())
        .cloned()
}

use self::diff_ui::DiffBlock;
use self::diff_ui::DiffConfirm;
use self::diff_ui::DiffOverlay;
use self::settings_overlay::{
    AgentOverviewRow,
    AccountsSettingsContent,
    AutoDriveSettingsContent,
    AgentsSettingsContent,
    LimitsSettingsContent,
    ChromeSettingsContent,
    McpSettingsContent,
    ModelSettingsContent,
    PlanningSettingsContent,
    NotificationsSettingsContent,
    PromptsSettingsContent,
    SkillsSettingsContent,
    ReviewSettingsContent,
    ThemeSettingsContent,
    UpdatesSettingsContent,
    ValidationSettingsContent,
    SettingsOverlayView,
    SettingsOverviewRow,
};
use ratatui::text::Line as RtLine;
use ratatui::text::Span as RtSpan;


use self::perf::PerfStats;

#[derive(Debug, Clone)]
struct AgentInfo {
    // Stable id to correlate updates
    id: String,
    // Display name
    name: String,
    // Current status
    status: AgentStatus,
    // Source of the agent (e.g., Auto Review)
    source_kind: Option<AgentSourceKind>,
    // Batch identifier reported by the core (if any)
    batch_id: Option<String>,
    // Optional model name
    model: Option<String>,
    // Final success message when completed
    result: Option<String>,
    // Final error message when failed
    error: Option<String>,
    // Most recent progress line from core
    last_progress: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum AgentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

fn agent_status_from_str(status: &str) -> AgentStatus {
    match status {
        "pending" => AgentStatus::Pending,
        "running" => AgentStatus::Running,
        "completed" => AgentStatus::Completed,
        "failed" => AgentStatus::Failed,
        "cancelled" => AgentStatus::Cancelled,
        _ => AgentStatus::Pending,
    }
}

fn agent_status_label(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Pending => "Pending",
        AgentStatus::Running => "Running",
        AgentStatus::Completed => "Completed",
        AgentStatus::Failed => "Failed",
        AgentStatus::Cancelled => "Cancelled",
    }
}

fn agent_status_icon(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Completed => "✔",
        AgentStatus::Running => "▶",
        AgentStatus::Pending => "…",
        AgentStatus::Failed => "✖",
        AgentStatus::Cancelled => "⏹",
    }
}

fn agent_running_priority(status: AgentStatus) -> usize {
    match status {
        AgentStatus::Running => 0,
        AgentStatus::Pending => 1,
        AgentStatus::Failed => 2,
        AgentStatus::Completed => 3,
        AgentStatus::Cancelled => 4,
    }
}

fn agent_status_color(status: AgentStatus) -> ratatui::style::Color {
    match status {
        AgentStatus::Pending => crate::colors::warning(),
        AgentStatus::Running => crate::colors::info(),
        AgentStatus::Completed => crate::colors::success(),
        AgentStatus::Failed => crate::colors::error(),
        AgentStatus::Cancelled => crate::colors::warning(),
    }
}

fn agent_log_label(kind: AgentLogKind) -> &'static str {
    match kind {
        AgentLogKind::Status => "status",
        AgentLogKind::Progress => "progress",
        AgentLogKind::Result => "result",
        AgentLogKind::Error => "error",
    }
}

fn agent_log_color(kind: AgentLogKind) -> ratatui::style::Color {
    match kind {
        AgentLogKind::Status => crate::colors::info(),
        AgentLogKind::Progress => crate::colors::primary(),
        AgentLogKind::Result => crate::colors::success(),
        AgentLogKind::Error => crate::colors::error(),
    }
}

use self::message::create_initial_user_message;

// Newtype IDs for clarity across exec/tools/streams
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ExecCallId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ToolCallId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct StreamId(pub String);

impl From<String> for ExecCallId {
    fn from(s: String) -> Self {
        ExecCallId(s)
    }
}
impl From<&str> for ExecCallId {
    fn from(s: &str) -> Self {
        ExecCallId(s.to_string())
    }
}

fn wait_target_from_params(params: Option<&String>, call_id: &str) -> String {
    if let Some(raw) = params
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(raw) {
            if let Some(for_value) = json.get("for").and_then(|v| v.as_str()) {
                let cleaned = clean_wait_command(for_value);
                if !cleaned.is_empty() {
                    return cleaned;
                }
            }
            if let Some(cid) = json.get("call_id").and_then(|v| v.as_str()) {
                return format!("call {cid}");
            }
        }
    format!("call {call_id}")
}

fn wait_exec_call_id_from_params(params: Option<&String>) -> Option<ExecCallId> {
    params
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|json| json.get("call_id").and_then(|v| v.as_str()).map(|s| ExecCallId(s.to_string())))
}

fn wait_result_missing_background_job(message: &str) -> bool {
    let trimmed = message.trim();
    trimmed.starts_with("No background job found for call_id=")
        || trimmed == "No completed background job found"
}

fn wait_result_interrupted(message: &str) -> bool {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.contains("wait ended due to new user message")
        || lower.contains("wait ended because the session was interrupted")
        || lower.contains("wait interrupted so the assistant can adapt")
        || (lower.contains("background job") && lower.contains("still running"))
}

fn image_mime_from_path(path: &Path) -> Option<String> {
    let ext = path.extension().and_then(|ext| ext.to_str())?;
    let mime = match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        _ => return None,
    };
    Some(mime.to_string())
}

fn image_record_from_path(path: &Path) -> Option<ImageRecord> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!("Failed to read image {}: {err}", path.display());
            return None;
        }
    };
    let (width, height) = match image::image_dimensions(path) {
        Ok((w, h)) => (
            w.min(u16::MAX as u32) as u16,
            h.min(u16::MAX as u32) as u16,
        ),
        Err(err) => {
            tracing::warn!("Failed to read image dimensions for {}: {err}", path.display());
            (0, 0)
        }
    };
    let sha_hex = format!("{:x}", Sha256::digest(&bytes));
    let byte_len = bytes.len().min(u32::MAX as usize) as u32;
    Some(ImageRecord {
        id: HistoryId::ZERO,
        source_path: Some(path.to_path_buf()),
        alt_text: None,
        width,
        height,
        sha256: Some(sha_hex),
        mime_type: image_mime_from_path(path),
        byte_len: Some(byte_len),
    })
}

fn image_view_path_from_params(params: &serde_json::Value, cwd: &Path) -> Option<PathBuf> {
    let path = params.get("path").and_then(|value| value.as_str())?;
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut resolved = PathBuf::from(trimmed);
    if resolved.is_relative() {
        resolved = cwd.join(&resolved);
    }
    if let Ok(canon) = resolved.canonicalize() {
        resolved = canon;
    }
    Some(resolved)
}

impl std::fmt::Display for ExecCallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl AsRef<str> for ExecCallId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for ToolCallId {
    fn from(s: String) -> Self {
        ToolCallId(s)
    }
}
impl From<&str> for ToolCallId {
    fn from(s: &str) -> Self {
        ToolCallId(s.to_string())
    }
}
impl std::fmt::Display for ToolCallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl AsRef<str> for ToolCallId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StreamId {
    fn from(s: String) -> Self {
        StreamId(s)
    }
}
impl From<&str> for StreamId {
    fn from(s: &str) -> Self {
        StreamId(s.to_string())
    }
}
impl std::fmt::Display for StreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl AsRef<str> for StreamId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ---- System notice ordering helpers ----
#[derive(Copy, Clone)]
enum SystemPlacement {
    /// Place near the top of the current request (before most provider output)
    Early,
    /// Place at the end of the current request window (after provider output)
    Tail,
    /// Place before the first user prompt of the very first request
    /// (used for pre-turn UI confirmations like theme/spinner changes)
    PrePrompt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoDriveRole {
    User,
    Assistant,
}

pub(crate) struct ChatWidgetInit {
    pub(crate) config: Config,
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) initial_prompt: Option<String>,
    pub(crate) initial_images: Vec<PathBuf>,
    pub(crate) enhanced_keys_supported: bool,
    pub(crate) terminal_info: crate::tui::TerminalInfo,
    pub(crate) show_order_overlay: bool,
    pub(crate) latest_upgrade_version: Option<String>,
}

pub(crate) struct ForkedChatWidgetInit {
    pub(crate) config: Config,
    pub(crate) conversation: Arc<code_core::CodexConversation>,
    pub(crate) session_configured: SessionConfiguredEvent,
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) enhanced_keys_supported: bool,
    pub(crate) terminal_info: crate::tui::TerminalInfo,
    pub(crate) show_order_overlay: bool,
    pub(crate) latest_upgrade_version: Option<String>,
    pub(crate) auth_manager: Arc<AuthManager>,
    pub(crate) show_welcome: bool,
}

pub(crate) struct BackgroundReviewFinishedEvent {
    pub(crate) worktree_path: std::path::PathBuf,
    pub(crate) branch: String,
    pub(crate) has_findings: bool,
    pub(crate) findings: usize,
    pub(crate) summary: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) agent_id: Option<String>,
    pub(crate) snapshot: Option<String>,
}

pub(crate) struct AutoLaunchRequest {
    pub(crate) goal: String,
    pub(crate) derive_goal_from_history: bool,
    pub(crate) review_enabled: bool,
    pub(crate) subagents_enabled: bool,
    pub(crate) cross_check_enabled: bool,
    pub(crate) qa_automation_enabled: bool,
    pub(crate) continue_mode: AutoContinueMode,
}

pub(crate) struct AutoDecisionEvent {
    pub(crate) seq: u64,
    pub(crate) status: AutoCoordinatorStatus,
    pub(crate) status_title: Option<String>,
    pub(crate) status_sent_to_user: Option<String>,
    pub(crate) goal: Option<String>,
    pub(crate) cli: Option<AutoTurnCliAction>,
    pub(crate) agents_timing: Option<AutoTurnAgentsTiming>,
    pub(crate) agents: Vec<AutoTurnAgentsAction>,
    pub(crate) transcript: Vec<code_protocol::models::ResponseItem>,
}

pub(crate) struct AgentUpdateRequest {
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) args_ro: Option<Vec<String>>,
    pub(crate) args_wr: Option<Vec<String>>,
    pub(crate) instructions: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) command: String,
}
