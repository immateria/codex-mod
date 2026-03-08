use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

use crate::config_types::MemoriesConfig;
use crate::config_types::MemoriesToml;
use chrono::{DateTime, Utc};
use code_memories_state::MemoriesState;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

mod control;
mod manifest;
mod prompts;
mod storage;

const MEMORIES_DIR: &str = "memories";
const CURRENT_GENERATION_FILENAME: &str = "current";
const MEMORY_SUMMARY_FILENAME: &str = "memory_summary.md";
const RAW_MEMORIES_FILENAME: &str = "raw_memories.md";
const MANIFEST_FILENAME: &str = "manifest.json";
const ROLLOUT_SUMMARIES_SUBDIR: &str = "rollout_summaries";
const SNAPSHOTS_SUBDIR: &str = "snapshots";
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, Default)]
struct RefreshState {
    in_flight: bool,
    last_completed_at: Option<Instant>,
}

struct RefreshAttemptGuard {
    code_home: Option<PathBuf>,
}

impl RefreshAttemptGuard {
    fn new(code_home: PathBuf) -> Self {
        Self {
            code_home: Some(code_home),
        }
    }
}

impl Drop for RefreshAttemptGuard {
    fn drop(&mut self) {
        if let Some(code_home) = self.code_home.take() {
            finish_refresh_attempt(&code_home);
        }
    }
}

static REFRESH_STATES: OnceLock<Mutex<HashMap<PathBuf, RefreshState>>> = OnceLock::new();
static MEMORIES_STATES: OnceLock<AsyncMutex<HashMap<PathBuf, Arc<MemoriesState>>>> =
    OnceLock::new();
static STATUS_CACHE: OnceLock<Mutex<HashMap<PathBuf, MemoriesDbStatus>>> = OnceLock::new();

pub(crate) use prompts::build_memory_tool_developer_instructions;
pub(crate) use storage::refresh_memory_artifacts_from_catalog;
pub(crate) use manifest::current_context_from_runtime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryArtifactStatus {
    pub exists: bool,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoriesArtifactsStatus {
    pub memory_root: PathBuf,
    pub summary: MemoryArtifactStatus,
    pub raw_memories: MemoryArtifactStatus,
    pub rollout_summaries: MemoryArtifactStatus,
    pub rollout_summary_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoriesDbStatus {
    pub db_exists: bool,
    pub thread_count: usize,
    pub stage1_epoch_count: usize,
    pub pending_stage1_count: usize,
    pub running_stage1_count: usize,
    pub dead_lettered_stage1_count: usize,
    pub artifact_job_running: bool,
    pub artifact_dirty: bool,
    pub last_artifact_build_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoriesSettingSource {
    Default,
    Global,
    Profile,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoriesResolvedSources {
    pub no_memories_if_mcp_or_web_search: MemoriesSettingSource,
    pub generate_memories: MemoriesSettingSource,
    pub use_memories: MemoriesSettingSource,
    pub max_raw_memories_for_consolidation: MemoriesSettingSource,
    pub max_rollout_age_days: MemoriesSettingSource,
    pub max_rollouts_per_startup: MemoriesSettingSource,
    pub min_rollout_idle_hours: MemoriesSettingSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoriesStatus {
    pub artifacts: MemoriesArtifactsStatus,
    pub db: MemoriesDbStatus,
    pub effective: MemoriesConfig,
    pub sources: MemoriesResolvedSources,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublishedMemoriesPaths {
    pub base_dir: PathBuf,
    pub summary_path: PathBuf,
    pub raw_memories_path: PathBuf,
    pub manifest_path: PathBuf,
    pub rollout_summaries_dir: PathBuf,
    pub generation: Option<String>,
}

pub(crate) fn memory_root(code_home: &Path) -> PathBuf {
    code_home.join(MEMORIES_DIR)
}

pub(crate) fn current_generation_path(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(CURRENT_GENERATION_FILENAME)
}

pub(crate) fn memories_state_path(code_home: &Path) -> PathBuf {
    code_home.join("memories_state.sqlite")
}

pub(crate) fn snapshots_root(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(SNAPSHOTS_SUBDIR)
}

pub(crate) fn generation_snapshot_dir(code_home: &Path, generation: &str) -> PathBuf {
    snapshots_root(code_home).join(generation)
}

pub(crate) fn snapshot_memory_summary_path(snapshot_dir: &Path) -> PathBuf {
    snapshot_dir.join(MEMORY_SUMMARY_FILENAME)
}

pub(crate) fn snapshot_raw_memories_path(snapshot_dir: &Path) -> PathBuf {
    snapshot_dir.join(RAW_MEMORIES_FILENAME)
}

pub(crate) fn snapshot_manifest_path(snapshot_dir: &Path) -> PathBuf {
    snapshot_dir.join(MANIFEST_FILENAME)
}

pub(crate) fn snapshot_rollout_summaries_dir(snapshot_dir: &Path) -> PathBuf {
    snapshot_dir.join(ROLLOUT_SUMMARIES_SUBDIR)
}

fn legacy_memory_summary_path(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(MEMORY_SUMMARY_FILENAME)
}

fn legacy_raw_memories_path(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(RAW_MEMORIES_FILENAME)
}

fn legacy_manifest_path(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(MANIFEST_FILENAME)
}

fn legacy_rollout_summaries_dir(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(ROLLOUT_SUMMARIES_SUBDIR)
}

fn resolve_current_generation(code_home: &Path) -> io::Result<Option<String>> {
    let current_path = current_generation_path(code_home);
    match std::fs::read_to_string(&current_path) {
        Ok(contents) => {
            let generation = contents.trim().to_string();
            if generation.is_empty() {
                Ok(None)
            } else {
                Ok(Some(generation))
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

async fn resolve_current_generation_async(code_home: &Path) -> io::Result<Option<String>> {
    let current_path = current_generation_path(code_home);
    match tokio::fs::read_to_string(&current_path).await {
        Ok(contents) => {
            let generation = contents.trim().to_string();
            if generation.is_empty() {
                Ok(None)
            } else {
                Ok(Some(generation))
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub(crate) fn published_artifact_paths(code_home: &Path) -> io::Result<PublishedMemoriesPaths> {
    if let Some(generation) = resolve_current_generation(code_home)? {
        let base_dir = generation_snapshot_dir(code_home, &generation);
        return Ok(PublishedMemoriesPaths {
            summary_path: snapshot_memory_summary_path(&base_dir),
            raw_memories_path: snapshot_raw_memories_path(&base_dir),
            manifest_path: snapshot_manifest_path(&base_dir),
            rollout_summaries_dir: snapshot_rollout_summaries_dir(&base_dir),
            base_dir,
            generation: Some(generation),
        });
    }

    let base_dir = memory_root(code_home);
    Ok(PublishedMemoriesPaths {
        summary_path: legacy_memory_summary_path(code_home),
        raw_memories_path: legacy_raw_memories_path(code_home),
        manifest_path: legacy_manifest_path(code_home),
        rollout_summaries_dir: legacy_rollout_summaries_dir(code_home),
        base_dir,
        generation: None,
    })
}

pub(crate) async fn published_artifact_paths_async(
    code_home: &Path,
) -> io::Result<PublishedMemoriesPaths> {
    if let Some(generation) = resolve_current_generation_async(code_home).await? {
        let base_dir = generation_snapshot_dir(code_home, &generation);
        return Ok(PublishedMemoriesPaths {
            summary_path: snapshot_memory_summary_path(&base_dir),
            raw_memories_path: snapshot_raw_memories_path(&base_dir),
            manifest_path: snapshot_manifest_path(&base_dir),
            rollout_summaries_dir: snapshot_rollout_summaries_dir(&base_dir),
            base_dir,
            generation: Some(generation),
        });
    }

    let base_dir = memory_root(code_home);
    Ok(PublishedMemoriesPaths {
        summary_path: legacy_memory_summary_path(code_home),
        raw_memories_path: legacy_raw_memories_path(code_home),
        manifest_path: legacy_manifest_path(code_home),
        rollout_summaries_dir: legacy_rollout_summaries_dir(code_home),
        base_dir,
        generation: None,
    })
}

fn lock_map<'a, T>(mutex: &'a Mutex<T>, name: &str) -> Option<std::sync::MutexGuard<'a, T>> {
    match mutex.lock() {
        Ok(guard) => Some(guard),
        Err(err) => {
            tracing::error!("memories {name} mutex poisoned: {err}");
            None
        }
    }
}

fn try_start_refresh(code_home: &Path, force: bool) -> bool {
    let now = Instant::now();
    let mutex = REFRESH_STATES.get_or_init(|| Mutex::new(HashMap::new()));
    let Some(mut guard) = lock_map(mutex, "refresh state") else {
        return false;
    };
    let state = guard.entry(code_home.to_path_buf()).or_default();
    if state.in_flight {
        return false;
    }
    if !force
        && state
            .last_completed_at
            .is_some_and(|last| now.duration_since(last) < REFRESH_INTERVAL)
    {
        return false;
    }
    state.in_flight = true;
    true
}

fn finish_refresh_attempt(code_home: &Path) {
    let mutex = REFRESH_STATES.get_or_init(|| Mutex::new(HashMap::new()));
    let Some(mut guard) = lock_map(mutex, "refresh state") else {
        return;
    };
    let state = guard.entry(code_home.to_path_buf()).or_default();
    state.in_flight = false;
    state.last_completed_at = Some(Instant::now());
}

fn empty_db_status() -> MemoriesDbStatus {
    MemoriesDbStatus {
        db_exists: false,
        thread_count: 0,
        stage1_epoch_count: 0,
        pending_stage1_count: 0,
        running_stage1_count: 0,
        dead_lettered_stage1_count: 0,
        artifact_job_running: false,
        artifact_dirty: false,
        last_artifact_build_at: None,
    }
}

fn cached_db_status(code_home: &Path) -> Option<MemoriesDbStatus> {
    let mutex = STATUS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let guard = lock_map(mutex, "status cache")?;
    guard.get(code_home).cloned()
}

fn store_cached_db_status(code_home: &Path, status: MemoriesDbStatus) {
    let mutex = STATUS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(mut guard) = lock_map(mutex, "status cache") {
        guard.insert(code_home.to_path_buf(), status);
    }
}

fn clear_cached_db_status(code_home: &Path) {
    let mutex = STATUS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(mut guard) = lock_map(mutex, "status cache") {
        guard.remove(code_home);
    }
}

async fn refresh_cached_db_status(code_home: &Path) {
    if let Err(err) = load_memories_db_status(code_home).await {
        tracing::debug!(
            "failed to refresh cached memories db status for {}: {err}",
            code_home.display()
        );
    }
}

pub(crate) async fn open_memories_state(code_home: &Path) -> io::Result<Arc<MemoriesState>> {
    let code_home = code_home.to_path_buf();
    let mutex = MEMORIES_STATES.get_or_init(|| AsyncMutex::new(HashMap::new()));
    let mut guard = mutex.lock().await;
    if let Some(existing) = guard.get(&code_home) {
        return Ok(existing.clone());
    }

    let state = MemoriesState::open(code_home.clone())
        .await
        .map_err(io::Error::other)?;
    guard.insert(code_home, state.clone());
    Ok(state)
}

async fn maybe_existing_memories_state(code_home: &Path) -> io::Result<Option<Arc<MemoriesState>>> {
    if !tokio::fs::try_exists(memories_state_path(code_home)).await? {
        clear_cached_db_status(code_home);
        return Ok(None);
    }
    open_memories_state(code_home).await.map(Some)
}

pub(crate) fn maybe_spawn_memory_refresh(code_home: PathBuf, settings: MemoriesConfig) {
    if !try_start_refresh(&code_home, false) {
        return;
    }

    tokio::spawn(async move {
        let _refresh_guard = RefreshAttemptGuard::new(code_home.clone());
        if let Err(err) = refresh_memory_artifacts_from_catalog(&code_home, &settings, false).await
        {
            tracing::warn!("memory refresh skipped: {err}");
        }
        refresh_cached_db_status(&code_home).await;
    });
}

pub(crate) async fn ensure_layout(code_home: &Path) -> io::Result<()> {
    tokio::fs::create_dir_all(snapshots_root(code_home)).await
}

pub async fn clear_generated_memory_artifacts(code_home: &Path) -> io::Result<()> {
    let mut refresh_db_status = false;
    if let Some(state) = maybe_existing_memories_state(code_home).await? {
        state.mark_artifact_dirty().await.map_err(io::Error::other)?;
        refresh_db_status = true;
    }
    let result = control::clear_memory_root_contents(&memory_root(code_home)).await;
    if refresh_db_status {
        refresh_cached_db_status(code_home).await;
    }
    result
}

fn refresh_already_running_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::WouldBlock,
        "memories refresh already running",
    )
}

pub async fn refresh_memory_artifacts_now(
    code_home: &Path,
    settings: &MemoriesConfig,
) -> io::Result<()> {
    if !try_start_refresh(code_home, true) {
        return Err(refresh_already_running_error());
    }

    let _refresh_guard = RefreshAttemptGuard::new(code_home.to_path_buf());
    let result = storage::refresh_memory_artifacts_from_catalog(code_home, settings, true).await;
    refresh_cached_db_status(code_home).await;
    result
}

pub async fn note_selected_memories_used(
    code_home: &Path,
    epoch_ids: &[code_memories_state::MemoryEpochId],
) -> io::Result<()> {
    let Some(state) = maybe_existing_memories_state(code_home).await? else {
        return Ok(());
    };
    if epoch_ids.is_empty() {
        return Ok(());
    }
    state
        .record_epoch_usage(epoch_ids)
        .await
        .map_err(io::Error::other)?;
    refresh_cached_db_status(code_home).await;
    Ok(())
}

pub(crate) async fn record_memory_prompt_usage(
    code_home: &Path,
    prompt: &prompts::MemoryPromptInstructions,
) -> io::Result<bool> {
    if prompt.used_fallback_summary {
        return Ok(false);
    }
    note_selected_memories_used(code_home, &prompt.selected_epoch_ids).await?;
    Ok(true)
}

pub async fn set_memories_session_mode(
    code_home: &Path,
    session_id: Uuid,
    mode: crate::rollout::catalog::SessionMemoryMode,
) -> io::Result<()> {
    let state = open_memories_state(code_home).await?;
    state
        .mark_thread_memory_mode(session_id, storage::to_state_memory_mode(mode))
        .await
        .map_err(io::Error::other)?;
    refresh_cached_db_status(code_home).await;
    Ok(())
}

pub fn get_memories_artifacts_status(code_home: &Path) -> io::Result<MemoriesArtifactsStatus> {
    let root = memory_root(code_home);
    let published = published_artifact_paths(code_home)?;
    let summary = artifact_status(&published.summary_path)?;
    let raw_memories = artifact_status(&published.raw_memories_path)?;
    let rollout_dir = published.rollout_summaries_dir;
    let rollout_summaries = artifact_status(&rollout_dir)?;
    let rollout_summary_count = if rollout_dir.is_dir() {
        std::fs::read_dir(&rollout_dir)?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
            .count()
    } else {
        0
    };

    Ok(MemoriesArtifactsStatus {
        memory_root: root,
        summary,
        raw_memories,
        rollout_summaries,
        rollout_summary_count,
    })
}

async fn artifact_status_async(path: &Path) -> io::Result<MemoryArtifactStatus> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => Ok(MemoryArtifactStatus {
            exists: true,
            modified_at: metadata
                .modified()
                .ok()
                .map(|modified| DateTime::<Utc>::from(modified).to_rfc3339()),
        }),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(MemoryArtifactStatus {
            exists: false,
            modified_at: None,
        }),
        Err(err) => Err(err),
    }
}

async fn get_memories_artifacts_status_async(code_home: &Path) -> io::Result<MemoriesArtifactsStatus> {
    let root = memory_root(code_home);
    let published = published_artifact_paths_async(code_home).await?;
    let summary = artifact_status_async(&published.summary_path).await?;
    let raw_memories = artifact_status_async(&published.raw_memories_path).await?;
    let rollout_dir = published.rollout_summaries_dir;
    let rollout_summaries = artifact_status_async(&rollout_dir).await?;
    let rollout_summary_count = match tokio::fs::read_dir(&rollout_dir).await {
        Ok(mut entries) => {
            let mut count = 0;
            while let Some(entry) = entries.next_entry().await? {
                if entry
                    .file_type()
                    .await
                    .is_ok_and(|file_type| file_type.is_file())
                {
                    count += 1;
                }
            }
            count
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => 0,
        Err(err) => return Err(err),
    };

    Ok(MemoriesArtifactsStatus {
        memory_root: root,
        summary,
        raw_memories,
        rollout_summaries,
        rollout_summary_count,
    })
}

fn resolve_sources(
    global: Option<&MemoriesToml>,
    profile: Option<&MemoriesToml>,
    project: Option<&MemoriesToml>,
) -> MemoriesResolvedSources {
    MemoriesResolvedSources {
        no_memories_if_mcp_or_web_search: setting_source(
            global.and_then(|t| t.no_memories_if_mcp_or_web_search),
            profile.and_then(|t| t.no_memories_if_mcp_or_web_search),
            project.and_then(|t| t.no_memories_if_mcp_or_web_search),
        ),
        generate_memories: setting_source(
            global.and_then(|t| t.generate_memories),
            profile.and_then(|t| t.generate_memories),
            project.and_then(|t| t.generate_memories),
        ),
        use_memories: setting_source(
            global.and_then(|t| t.use_memories),
            profile.and_then(|t| t.use_memories),
            project.and_then(|t| t.use_memories),
        ),
        max_raw_memories_for_consolidation: setting_source(
            global.and_then(|t| {
                t.max_raw_memories_for_consolidation
                    .or(t.max_raw_memories_for_global)
            }),
            profile.and_then(|t| {
                t.max_raw_memories_for_consolidation
                    .or(t.max_raw_memories_for_global)
            }),
            project.and_then(|t| {
                t.max_raw_memories_for_consolidation
                    .or(t.max_raw_memories_for_global)
            }),
        ),
        max_rollout_age_days: setting_source(
            global.and_then(|t| t.max_rollout_age_days),
            profile.and_then(|t| t.max_rollout_age_days),
            project.and_then(|t| t.max_rollout_age_days),
        ),
        max_rollouts_per_startup: setting_source(
            global.and_then(|t| t.max_rollouts_per_startup),
            profile.and_then(|t| t.max_rollouts_per_startup),
            project.and_then(|t| t.max_rollouts_per_startup),
        ),
        min_rollout_idle_hours: setting_source(
            global.and_then(|t| t.min_rollout_idle_hours),
            profile.and_then(|t| t.min_rollout_idle_hours),
            project.and_then(|t| t.min_rollout_idle_hours),
        ),
    }
}

fn compose_memories_status(
    code_home: &Path,
    db: MemoriesDbStatus,
    global: Option<&MemoriesToml>,
    profile: Option<&MemoriesToml>,
    project: Option<&MemoriesToml>,
) -> io::Result<MemoriesStatus> {
    let artifacts = get_memories_artifacts_status(code_home)?;
    let effective = crate::config_types::resolve_memories_config(global, profile, project);
    let sources = resolve_sources(global, profile, project);

    Ok(MemoriesStatus {
        artifacts,
        db,
        effective,
        sources,
    })
}

async fn compose_memories_status_async(
    code_home: &Path,
    db: MemoriesDbStatus,
    global: Option<&MemoriesToml>,
    profile: Option<&MemoriesToml>,
    project: Option<&MemoriesToml>,
) -> io::Result<MemoriesStatus> {
    let artifacts = get_memories_artifacts_status_async(code_home).await?;
    let effective = crate::config_types::resolve_memories_config(global, profile, project);
    let sources = resolve_sources(global, profile, project);

    Ok(MemoriesStatus {
        artifacts,
        db,
        effective,
        sources,
    })
}

async fn load_memories_db_status(code_home: &Path) -> io::Result<MemoriesDbStatus> {
    if !tokio::fs::try_exists(memories_state_path(code_home)).await? {
        clear_cached_db_status(code_home);
        return Ok(empty_db_status());
    }

    let state = open_memories_state(code_home).await?;
    let db = state
        .status(crate::rollout::INTERACTIVE_SESSION_SOURCES)
        .await
        .map_err(io::Error::other)?;
    let status = MemoriesDbStatus {
        db_exists: db.db_exists,
        thread_count: db.thread_count,
        stage1_epoch_count: db.stage1_epoch_count,
        pending_stage1_count: db.pending_stage1_count,
        running_stage1_count: db.running_stage1_count,
        dead_lettered_stage1_count: db.dead_lettered_stage1_count,
        artifact_job_running: db.artifact_job_running,
        artifact_dirty: db.artifact_dirty,
        last_artifact_build_at: db.last_artifact_build_at,
    };
    store_cached_db_status(code_home, status.clone());
    Ok(status)
}

pub async fn load_memories_status(
    code_home: &Path,
    global: Option<&MemoriesToml>,
    profile: Option<&MemoriesToml>,
    project: Option<&MemoriesToml>,
) -> io::Result<MemoriesStatus> {
    let db = load_memories_db_status(code_home).await?;
    compose_memories_status_async(code_home, db, global, profile, project).await
}

pub fn get_cached_memories_status(
    code_home: &Path,
    global: Option<&MemoriesToml>,
    profile: Option<&MemoriesToml>,
    project: Option<&MemoriesToml>,
) -> io::Result<Option<MemoriesStatus>> {
    if !memories_state_path(code_home).try_exists()? {
        clear_cached_db_status(code_home);
        return compose_memories_status(code_home, empty_db_status(), global, profile, project)
            .map(Some);
    }

    let Some(db) = cached_db_status(code_home) else {
        return Ok(None);
    };
    compose_memories_status(code_home, db, global, profile, project).map(Some)
}

fn artifact_status(path: &Path) -> io::Result<MemoryArtifactStatus> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(MemoryArtifactStatus {
            exists: true,
            modified_at: metadata
                .modified()
                .ok()
                .map(|modified| DateTime::<Utc>::from(modified).to_rfc3339()),
        }),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(MemoryArtifactStatus {
            exists: false,
            modified_at: None,
        }),
        Err(err) => Err(err),
    }
}

fn setting_source<T>(
    global: Option<T>,
    profile: Option<T>,
    project: Option<T>,
) -> MemoriesSettingSource {
    if project.is_some() {
        MemoriesSettingSource::Project
    } else if profile.is_some() {
        MemoriesSettingSource::Profile
    } else if global.is_some() {
        MemoriesSettingSource::Global
    } else {
        MemoriesSettingSource::Default
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use code_memories_state::{MemoryEpochId, MemoriesState, MemoryThread, SessionMemoryMode as StateMemoryMode, Stage1EpochInput};
    use code_protocol::protocol::SessionSource;
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::clear_generated_memory_artifacts;
    use super::current_generation_path;
    use super::finish_refresh_attempt;
    use super::generation_snapshot_dir;
    use super::get_memories_artifacts_status;
    use super::get_cached_memories_status;
    use super::load_memories_status;
    use super::memory_root;
    use super::prompts::MemoryPromptInstructions;
    use super::open_memories_state;
    use super::published_artifact_paths;
    use super::record_memory_prompt_usage;
    use super::refresh_memory_artifacts_now;
    use super::set_memories_session_mode;
    use super::snapshot_memory_summary_path;
    use super::snapshot_raw_memories_path;
    use super::snapshot_rollout_summaries_dir;
    use super::note_selected_memories_used;
    use super::try_start_refresh;
    use super::MemoriesSettingSource;
    use crate::config_types::{MemoriesConfig, MemoriesToml};
    use crate::rollout::catalog::SessionMemoryMode;

    fn now_epoch() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    fn sample_thread(thread_id: Uuid, updated_at: i64) -> MemoryThread {
        MemoryThread {
            thread_id,
            rollout_path: PathBuf::from(format!("sessions/{thread_id}.jsonl")),
            source: SessionSource::Cli,
            cwd: PathBuf::from("/tmp/workspace"),
            cwd_display: "~/workspace".to_string(),
            updated_at,
            updated_at_label: chrono::DateTime::<chrono::Utc>::from_timestamp(updated_at, 0)
                .expect("valid timestamp")
                .to_rfc3339(),
            archived: false,
            deleted: false,
            memory_mode: StateMemoryMode::Enabled,
            catalog_seen_at: updated_at,
            git_project_root: Some(PathBuf::from("/tmp/workspace")),
            git_branch: Some("main".to_string()),
            last_user_snippet: Some("Investigate regression".to_string()),
        }
    }

    async fn seed_selected_output(
        state: &MemoriesState,
        thread_id: Uuid,
        updated_at: i64,
    ) -> MemoryEpochId {
        state
            .reconcile_threads(&[sample_thread(thread_id, updated_at)])
            .await
            .expect("reconcile thread");
        let epoch = Stage1EpochInput {
            id: MemoryEpochId {
                thread_id,
                epoch_index: 0,
            },
            provenance: code_memories_state::Stage1EpochProvenance::Derived,
            source_updated_at: updated_at,
            generated_at: now_epoch(),
            epoch_start_at: Some(updated_at),
            epoch_end_at: Some(updated_at),
            epoch_start_line: 0,
            epoch_end_line: 0,
            platform_family: code_memories_state::MemoryPlatformFamily::Unix,
            shell_style: code_memories_state::MemoryShellStyle::Zsh,
            shell_program: Some("zsh".to_string()),
            workspace_root: Some("/tmp/workspace".to_string()),
            cwd_display: "~/workspace".to_string(),
            git_branch: Some("main".to_string()),
            raw_memory: "raw memory".to_string(),
            rollout_summary: "rollout summary".to_string(),
            rollout_slug: "memory-slug".to_string(),
        };
        state
            .replace_stage1_epochs(thread_id, &[epoch.clone()])
            .await
            .expect("replace stage1 epochs");
        let lease = state
            .claim_artifact_build_job(true)
            .await
            .expect("claim artifact lease")
            .expect("artifact lease");
        state
            .succeed_artifact_build_job(&lease.ownership_token)
            .await
            .expect("succeed artifact build");
        epoch.id
    }

    #[test]
    fn status_reports_effective_sources() {
        let temp = tempdir().expect("tempdir");
        let global = MemoriesToml {
            generate_memories: Some(true),
            use_memories: Some(false),
            ..MemoriesToml::default()
        };
        let profile = MemoriesToml {
            use_memories: Some(true),
            ..MemoriesToml::default()
        };
        let project = MemoriesToml {
            max_rollouts_per_startup: Some(9),
            ..MemoriesToml::default()
        };

        let status = get_cached_memories_status(
            temp.path(),
            Some(&global),
            Some(&profile),
            Some(&project),
        )
        .expect("memories status")
        .expect("cached status");

        assert_eq!(status.sources.generate_memories, MemoriesSettingSource::Global);
        assert_eq!(status.sources.use_memories, MemoriesSettingSource::Profile);
        assert_eq!(
            status.sources.max_rollouts_per_startup,
            MemoriesSettingSource::Project
        );
        assert!(status.effective.generate_memories);
        assert!(status.effective.use_memories);
        assert_eq!(status.effective.max_rollouts_per_startup, 9);
    }

    #[tokio::test]
    async fn clear_marks_db_artifacts_dirty_without_deleting_db() {
        let temp = tempdir().expect("tempdir");
        let snapshot_dir = generation_snapshot_dir(temp.path(), "20260307T120000Z-test");
        tokio::fs::create_dir_all(snapshot_rollout_summaries_dir(&snapshot_dir))
            .await
            .expect("create snapshot rollout summaries");
        tokio::fs::write(current_generation_path(temp.path()), "20260307T120000Z-test\n")
            .await
            .expect("write current generation");
        tokio::fs::write(snapshot_memory_summary_path(&snapshot_dir), "summary")
            .await
            .expect("write snapshot summary");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let lease = state
            .claim_artifact_build_job(true)
            .await
            .expect("claim artifact lease")
            .expect("lease");
        state
            .succeed_artifact_build_job(&lease.ownership_token)
            .await
            .expect("succeed artifact build");
        let status = state
            .status(crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("status after success");
        assert!(!status.artifact_dirty);

        clear_generated_memory_artifacts(temp.path())
            .await
            .expect("clear artifacts");

        let status = state
            .status(crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("status after clear");
        assert!(status.db_exists);
        assert!(status.artifact_dirty);
        assert_eq!(status.dead_lettered_stage1_count, 0);
        assert!(super::memories_state_path(temp.path()).exists());
        assert!(
            !tokio::fs::try_exists(current_generation_path(temp.path()))
                .await
                .expect("stat current generation")
        );
        assert!(
            !tokio::fs::try_exists(snapshot_dir)
                .await
                .expect("stat cleared snapshot dir")
        );
        let cached = get_cached_memories_status(temp.path(), None, None, None)
            .expect("cached status query")
            .expect("cached status");
        assert!(cached.db.artifact_dirty);
        assert_eq!(cached.db.dead_lettered_stage1_count, 0);
    }

    #[test]
    fn refresh_failure_path_is_still_throttled_until_interval_expires() {
        let temp = tempdir().expect("tempdir");
        assert!(try_start_refresh(temp.path(), false));
        finish_refresh_attempt(temp.path());
        assert!(!try_start_refresh(temp.path(), false));
    }

    #[test]
    fn in_flight_refresh_blocks_duplicate_attempts() {
        let temp = tempdir().expect("tempdir");
        assert!(try_start_refresh(temp.path(), false));
        assert!(!try_start_refresh(temp.path(), false));
        assert!(!try_start_refresh(temp.path(), true));
        finish_refresh_attempt(temp.path());
    }

    #[test]
    fn manual_refresh_completion_still_throttles_background_refresh() {
        let temp = tempdir().expect("tempdir");
        assert!(try_start_refresh(temp.path(), true));
        assert!(!try_start_refresh(temp.path(), true));
        finish_refresh_attempt(temp.path());
        assert!(!try_start_refresh(temp.path(), false));
        assert!(try_start_refresh(temp.path(), true));
        finish_refresh_attempt(temp.path());
    }

    #[tokio::test]
    async fn manual_refresh_returns_already_running_error_when_refresh_is_in_flight() {
        let temp = tempdir().expect("tempdir");
        assert!(try_start_refresh(temp.path(), false));
        let err = refresh_memory_artifacts_now(temp.path(), &MemoriesConfig::default())
            .await
            .expect_err("manual refresh should be blocked");
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(err.to_string(), "memories refresh already running");
        finish_refresh_attempt(temp.path());
    }

    #[tokio::test]
    async fn clear_does_not_reset_refresh_throttle() {
        let temp = tempdir().expect("tempdir");
        assert!(try_start_refresh(temp.path(), false));
        finish_refresh_attempt(temp.path());

        clear_generated_memory_artifacts(temp.path())
            .await
            .expect("clear artifacts");

        assert!(
            !try_start_refresh(temp.path(), false),
            "clear should not reset refresh throttle",
        );
    }

    #[tokio::test]
    async fn repeated_state_opens_reuse_the_same_arc() {
        let temp = tempdir().expect("tempdir");
        let first = open_memories_state(temp.path()).await.expect("open first");
        let second = open_memories_state(temp.path()).await.expect("open second");
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[tokio::test]
    async fn cached_status_is_none_until_db_snapshot_is_loaded() {
        let temp = tempdir().expect("tempdir");
        let _state = MemoriesState::open(temp.path()).await.expect("open state");
        assert!(
            get_cached_memories_status(temp.path(), None, None, None)
                .expect("cached status query")
                .is_none()
        );

        let loaded = load_memories_status(temp.path(), None, None, None)
            .await
            .expect("load status");
        let cached = get_cached_memories_status(temp.path(), None, None, None)
            .expect("cached status query")
            .expect("cached status");

        assert_eq!(cached.db, loaded.db);
    }

    #[tokio::test]
    async fn note_selected_memories_used_populates_cached_snapshot() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread_id = Uuid::new_v4();
        let epoch_id = seed_selected_output(&state, thread_id, now_epoch() - 172_800).await;

        assert!(
            get_cached_memories_status(temp.path(), None, None, None)
                .expect("cached status query")
                .is_none()
        );

        note_selected_memories_used(temp.path(), &[epoch_id])
            .await
            .expect("record usage");

        let cached = get_cached_memories_status(temp.path(), None, None, None)
            .expect("cached status query")
            .expect("cached status");
        assert!(cached.db.db_exists);
        assert_eq!(cached.db.stage1_epoch_count, 1);
        assert_eq!(cached.db.dead_lettered_stage1_count, 0);

        let selected = state
            .select_phase2_epochs(8, 365, crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("selected epochs");
        assert_eq!(selected.len(), 1);
        assert!(selected[0].last_usage.is_some());
    }

    #[tokio::test]
    async fn set_memories_session_mode_refreshes_cached_snapshot() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread_id = Uuid::new_v4();
        state
            .reconcile_threads(&[sample_thread(thread_id, now_epoch() - 172_800)])
            .await
            .expect("reconcile thread");

        let initial = load_memories_status(temp.path(), None, None, None)
            .await
            .expect("initial status");
        assert_eq!(initial.db.pending_stage1_count, 1);

        set_memories_session_mode(temp.path(), thread_id, SessionMemoryMode::Disabled)
            .await
            .expect("mark session disabled");

        let cached = get_cached_memories_status(temp.path(), None, None, None)
            .expect("cached status query")
            .expect("cached status");
        assert!(cached.db.db_exists);
        assert_eq!(cached.db.pending_stage1_count, 0);
        assert_eq!(cached.db.thread_count, 1);
        assert_eq!(cached.db.dead_lettered_stage1_count, 0);
    }

    #[tokio::test]
    async fn record_memory_prompt_usage_records_selected_epochs_once() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread_id = Uuid::new_v4();
        let epoch_id = seed_selected_output(&state, thread_id, now_epoch() - 172_800).await;

        let prompt = MemoryPromptInstructions {
            instructions: "selected manifest memory".to_string(),
            selected_epoch_ids: vec![epoch_id],
            used_fallback_summary: false,
        };

        let recorded = record_memory_prompt_usage(temp.path(), &prompt)
            .await
            .expect("record prompt usage");
        assert!(recorded);

        let selected = state
            .select_phase2_epochs(8, 365, crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("selected epochs");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].usage_count, 1);
        assert!(selected[0].last_usage.is_some());
    }

    #[tokio::test]
    async fn record_memory_prompt_usage_skips_fallback_summary() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread_id = Uuid::new_v4();
        let epoch_id = seed_selected_output(&state, thread_id, now_epoch() - 172_800).await;

        let prompt = MemoryPromptInstructions {
            instructions: "legacy fallback summary".to_string(),
            selected_epoch_ids: vec![epoch_id],
            used_fallback_summary: true,
        };

        let recorded = record_memory_prompt_usage(temp.path(), &prompt)
            .await
            .expect("skip fallback usage");
        assert!(!recorded);

        let selected = state
            .select_phase2_epochs(8, 365, crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("selected epochs");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].usage_count, 0);
        assert!(selected[0].last_usage.is_none());
    }

    #[test]
    fn absent_db_returns_zeroed_cached_status() {
        let temp = tempdir().expect("tempdir");
        let status = get_cached_memories_status(temp.path(), None, None, None)
            .expect("cached status query")
            .expect("status");
        assert!(!status.db.db_exists);
        assert_eq!(status.db.thread_count, 0);
        assert_eq!(status.db.stage1_epoch_count, 0);
        assert_eq!(status.db.dead_lettered_stage1_count, 0);
    }

    #[tokio::test]
    async fn artifacts_status_uses_legacy_root_files_without_current_pointer() {
        let temp = tempdir().expect("tempdir");
        let root = memory_root(temp.path());
        tokio::fs::create_dir_all(root.join("rollout_summaries"))
            .await
            .expect("create legacy rollout summaries");
        tokio::fs::write(root.join("memory_summary.md"), "legacy summary")
            .await
            .expect("write legacy summary");
        tokio::fs::write(root.join("raw_memories.md"), "legacy raw")
            .await
            .expect("write legacy raw");
        tokio::fs::write(root.join("rollout_summaries").join("legacy.md"), "legacy rollout")
            .await
            .expect("write legacy rollout");

        let status = get_memories_artifacts_status(temp.path()).expect("artifact status");

        assert!(status.summary.exists);
        assert!(status.raw_memories.exists);
        assert!(status.rollout_summaries.exists);
        assert_eq!(status.rollout_summary_count, 1);
    }

    #[tokio::test]
    async fn artifacts_status_prefers_active_snapshot_over_legacy_root_files() {
        let temp = tempdir().expect("tempdir");
        let root = memory_root(temp.path());
        let snapshot_dir = generation_snapshot_dir(temp.path(), "20260307T120000Z-test");
        tokio::fs::create_dir_all(root.join("rollout_summaries"))
            .await
            .expect("create legacy rollout summaries");
        tokio::fs::write(root.join("memory_summary.md"), "legacy summary")
            .await
            .expect("write legacy summary");
        tokio::fs::write(root.join("raw_memories.md"), "legacy raw")
            .await
            .expect("write legacy raw");
        tokio::fs::write(root.join("rollout_summaries").join("legacy.md"), "legacy rollout")
            .await
            .expect("write legacy rollout");

        tokio::fs::create_dir_all(snapshot_rollout_summaries_dir(&snapshot_dir))
            .await
            .expect("create snapshot rollout summaries");
        tokio::fs::write(current_generation_path(temp.path()), "20260307T120000Z-test\n")
            .await
            .expect("write current generation");
        tokio::fs::write(snapshot_memory_summary_path(&snapshot_dir), "snapshot summary")
            .await
            .expect("write snapshot summary");
        tokio::fs::write(snapshot_raw_memories_path(&snapshot_dir), "snapshot raw")
            .await
            .expect("write snapshot raw");
        tokio::fs::write(
            snapshot_rollout_summaries_dir(&snapshot_dir).join("snapshot.md"),
            "snapshot rollout",
        )
        .await
        .expect("write snapshot rollout");

        let published = published_artifact_paths(temp.path()).expect("published paths");
        assert_eq!(published.generation.as_deref(), Some("20260307T120000Z-test"));
        assert_eq!(published.base_dir, snapshot_dir);

        let status = get_memories_artifacts_status(temp.path()).expect("artifact status");

        assert!(status.summary.exists);
        assert!(status.raw_memories.exists);
        assert!(status.rollout_summaries.exists);
        assert_eq!(status.rollout_summary_count, 1);
    }
}
