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
mod prompts;
mod storage;

const MEMORIES_DIR: &str = "memories";
const MEMORY_SUMMARY_FILENAME: &str = "memory_summary.md";
const RAW_MEMORIES_FILENAME: &str = "raw_memories.md";
const ROLLOUT_SUMMARIES_SUBDIR: &str = "rollout_summaries";
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, Default)]
struct RefreshState {
    in_flight: bool,
    last_completed_at: Option<Instant>,
}

static REFRESH_STATES: OnceLock<Mutex<HashMap<PathBuf, RefreshState>>> = OnceLock::new();
static MEMORIES_STATES: OnceLock<AsyncMutex<HashMap<PathBuf, Arc<MemoriesState>>>> =
    OnceLock::new();
static STATUS_CACHE: OnceLock<Mutex<HashMap<PathBuf, MemoriesDbStatus>>> = OnceLock::new();

pub(crate) use prompts::build_memory_tool_developer_instructions;
pub(crate) use storage::refresh_memory_artifacts_from_catalog;

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
    pub stage1_output_count: usize,
    pub pending_stage1_count: usize,
    pub running_stage1_count: usize,
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

pub(crate) fn memory_root(code_home: &Path) -> PathBuf {
    code_home.join(MEMORIES_DIR)
}

pub(crate) fn memories_state_path(code_home: &Path) -> PathBuf {
    code_home.join("memories_state.sqlite")
}

pub(crate) fn memory_summary_path(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(MEMORY_SUMMARY_FILENAME)
}

pub(crate) fn raw_memories_path(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(RAW_MEMORIES_FILENAME)
}

pub(crate) fn rollout_summaries_dir(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(ROLLOUT_SUMMARIES_SUBDIR)
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
        stage1_output_count: 0,
        pending_stage1_count: 0,
        running_stage1_count: 0,
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
        if let Err(err) = refresh_memory_artifacts_from_catalog(&code_home, &settings, false).await
        {
            tracing::warn!("memory refresh skipped: {err}");
        }
        refresh_cached_db_status(&code_home).await;
        finish_refresh_attempt(&code_home);
    });
}

pub(crate) async fn ensure_layout(code_home: &Path) -> io::Result<()> {
    tokio::fs::create_dir_all(rollout_summaries_dir(code_home)).await
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

    let result = storage::refresh_memory_artifacts_from_catalog(code_home, settings, true).await;
    refresh_cached_db_status(code_home).await;
    finish_refresh_attempt(code_home);
    result
}

pub async fn note_selected_memories_used(code_home: &Path) -> io::Result<()> {
    let Some(state) = maybe_existing_memories_state(code_home).await? else {
        return Ok(());
    };
    let ids: Vec<Uuid> = state
        .current_selected_outputs(crate::rollout::INTERACTIVE_SESSION_SOURCES)
        .await
        .map_err(io::Error::other)?
        .into_iter()
        .map(|row| row.thread_id)
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    state.record_usage(&ids).await.map_err(io::Error::other)?;
    refresh_cached_db_status(code_home).await;
    Ok(())
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
    let summary_path = memory_summary_path(code_home);
    let summary = artifact_status(&summary_path)?;
    let raw_memories_path = raw_memories_path(code_home);
    let raw_memories = artifact_status(&raw_memories_path)?;
    let rollout_dir = rollout_summaries_dir(code_home);
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
        stage1_output_count: db.stage1_output_count,
        pending_stage1_count: db.pending_stage1_count,
        running_stage1_count: db.running_stage1_count,
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
    compose_memories_status(code_home, db, global, profile, project)
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

    use code_memories_state::{MemoriesState, MemoryThread, SessionMemoryMode as StateMemoryMode, Stage1OutputInput};
    use code_protocol::protocol::SessionSource;
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::clear_generated_memory_artifacts;
    use super::finish_refresh_attempt;
    use super::get_cached_memories_status;
    use super::load_memories_status;
    use super::open_memories_state;
    use super::refresh_memory_artifacts_now;
    use super::set_memories_session_mode;
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
            git_branch: Some("main".to_string()),
            last_user_snippet: Some("Investigate regression".to_string()),
        }
    }

    async fn seed_selected_output(
        state: &MemoriesState,
        thread_id: Uuid,
        updated_at: i64,
    ) {
        state
            .reconcile_threads(&[sample_thread(thread_id, updated_at)])
            .await
            .expect("reconcile thread");
        state
            .upsert_stage1_output(&Stage1OutputInput {
                thread_id,
                source_updated_at: updated_at,
                generated_at: now_epoch(),
                raw_memory: "raw memory".to_string(),
                rollout_summary: "rollout summary".to_string(),
                rollout_slug: "memory-slug".to_string(),
            })
            .await
            .expect("upsert stage1 output");
        let selected = state
            .select_phase2_inputs(8, 365, crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("select phase2 inputs");
        let lease = state
            .claim_artifact_build_job(true)
            .await
            .expect("claim artifact lease")
            .expect("artifact lease");
        state
            .succeed_artifact_build_job(&lease.ownership_token, &selected)
            .await
            .expect("succeed artifact build");
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
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let lease = state
            .claim_artifact_build_job(true)
            .await
            .expect("claim artifact lease")
            .expect("lease");
        state
            .succeed_artifact_build_job(&lease.ownership_token, &[])
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
        assert!(super::memories_state_path(temp.path()).exists());
        let cached = get_cached_memories_status(temp.path(), None, None, None)
            .expect("cached status query")
            .expect("cached status");
        assert!(cached.db.artifact_dirty);
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
        seed_selected_output(&state, thread_id, now_epoch() - 172_800).await;

        assert!(
            get_cached_memories_status(temp.path(), None, None, None)
                .expect("cached status query")
                .is_none()
        );

        note_selected_memories_used(temp.path())
            .await
            .expect("record usage");

        let cached = get_cached_memories_status(temp.path(), None, None, None)
            .expect("cached status query")
            .expect("cached status");
        assert!(cached.db.db_exists);
        assert_eq!(cached.db.stage1_output_count, 1);

        let selected = state
            .current_selected_outputs(crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("selected outputs");
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
    }

    #[test]
    fn absent_db_returns_zeroed_cached_status() {
        let temp = tempdir().expect("tempdir");
        let status = get_cached_memories_status(temp.path(), None, None, None)
            .expect("cached status query")
            .expect("status");
        assert!(!status.db.db_exists);
        assert_eq!(status.db.thread_count, 0);
        assert_eq!(status.db.stage1_output_count, 0);
    }
}
