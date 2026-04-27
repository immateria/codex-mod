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
            .map_or(i32::MAX, |value| {
                if value > code_core::protocol::BACKGROUND_OUTPUT_INDEX {
                    i32::MAX
                } else {
                    value as i32
                }
            });
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
#[cfg(feature = "browser-automation")]
static BG_SHOT_IN_FLIGHT: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));
static BACKGROUND_REVIEW_LOCKS: Lazy<Mutex<HashMap<String, code_core::review_coord::ReviewGuard>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
#[cfg(feature = "browser-automation")]
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

#[cfg(feature = "browser-automation")]
use self::settings_overlay::ChromeSettingsContent;
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum AgentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

fn agent_status_from_str(status: &str) -> AgentStatus {
    match status {
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
        AgentStatus::Completed => crate::icons::agent_completed(),
        AgentStatus::Running => crate::icons::agent_running(),
        AgentStatus::Pending => crate::icons::agent_pending(),
        AgentStatus::Failed => crate::icons::agent_failed(),
        AgentStatus::Cancelled => crate::icons::agent_cancelled(),
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
        AgentStatus::Pending | AgentStatus::Cancelled => crate::colors::warning(),
        AgentStatus::Running => crate::colors::info(),
        AgentStatus::Completed => crate::colors::success(),
        AgentStatus::Failed => crate::colors::error(),
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
        ExecCallId(s.to_owned())
    }
}

fn wait_target_from_params(params: Option<&str>, call_id: &str) -> String {
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

fn wait_exec_call_id_from_params(params: Option<&str>) -> Option<ExecCallId> {
    params
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|json| json.get("call_id").and_then(|v| v.as_str()).map(|s| ExecCallId(s.to_owned())))
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
    Some(mime.to_owned())
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
            w.min(u32::from(u16::MAX)) as u16,
            h.min(u32::from(u16::MAX)) as u16,
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

fn image_generation_replay_artifact_path(
    code_home: &Path,
    session_id: uuid::Uuid,
    call_id: &str,
) -> PathBuf {
    fn sanitize(value: &str) -> String {
        let sanitized: String = value
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        if sanitized.is_empty() {
            "generated_image".to_string()
        } else {
            sanitized
        }
    }

    code_home
        .join("generated_images")
        .join(sanitize(&session_id.to_string()))
        .join(format!("{}.png", sanitize(call_id)))
}

fn ensure_replayed_image_generation_artifact(
    code_home: &Path,
    session_id: Option<uuid::Uuid>,
    call_id: &str,
    result: &str,
) -> Option<PathBuf> {
    let session_id = session_id?;
    let path = image_generation_replay_artifact_path(code_home, session_id, call_id);
    if path.exists() {
        return Some(path);
    }
    if result.starts_with("data:") {
        return None;
    }
    let bytes = match BASE64_STANDARD.decode(result.trim().as_bytes()) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!("failed to decode replayed image generation result: {err}");
            return None;
        }
    };
    if let Some(parent) = path.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(
            "failed to create generated image replay dir {}: {err}",
            parent.display()
        );
        return None;
    }
    if let Err(err) = std::fs::write(&path, bytes) {
        tracing::warn!(
            "failed to write replayed image generation artifact {}: {err}",
            path.display()
        );
        return None;
    }
    Some(path)
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
        ToolCallId(s.to_owned())
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
        StreamId(s.to_owned())
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
