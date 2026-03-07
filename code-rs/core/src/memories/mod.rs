use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::Weak;
use std::time::Duration;
use std::time::Instant;

use crate::config_types::MemoriesConfig;
use crate::config_types::MemoriesToml;
use chrono::{DateTime, Utc};
use code_memories_state::MemoriesState;
use uuid::Uuid;

mod control;
mod prompts;
mod storage;

const MEMORIES_DIR: &str = "memories";
const MEMORY_SUMMARY_FILENAME: &str = "memory_summary.md";
const RAW_MEMORIES_FILENAME: &str = "raw_memories.md";
const ROLLOUT_SUMMARIES_SUBDIR: &str = "rollout_summaries";
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);
const STATUS_CACHE_INTERVAL: Duration = Duration::from_secs(1);

static LAST_REFRESH_AT: OnceLock<Mutex<HashMap<PathBuf, Instant>>> = OnceLock::new();
static MEMORIES_STATES: OnceLock<Mutex<HashMap<PathBuf, Weak<MemoriesState>>>> = OnceLock::new();
static STATUS_CACHE: OnceLock<Mutex<HashMap<PathBuf, (Instant, MemoriesDbStatus)>>> =
    OnceLock::new();

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

fn should_refresh_now(code_home: &Path) -> bool {
    let now = Instant::now();
    let mutex = LAST_REFRESH_AT.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = match mutex.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };

    let should_refresh = guard
        .get(code_home)
        .is_none_or(|last| now.duration_since(*last) >= REFRESH_INTERVAL);
    if should_refresh {
        guard.insert(code_home.to_path_buf(), now);
    }
    should_refresh
}

fn clear_refresh_backoff(code_home: &Path) {
    if let Ok(mut guard) = LAST_REFRESH_AT
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        guard.remove(code_home);
    }
}

fn clear_status_cache(code_home: &Path) {
    if let Ok(mut guard) = STATUS_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        guard.remove(code_home);
    }
}

pub(crate) async fn open_memories_state(code_home: &Path) -> io::Result<std::sync::Arc<MemoriesState>> {
    let code_home = code_home.to_path_buf();
    if let Some(existing) = MEMORIES_STATES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|guard| guard.get(&code_home).and_then(Weak::upgrade))
    {
        return Ok(existing);
    }

    let state = MemoriesState::open(code_home.clone())
        .await
        .map_err(io::Error::other)?;
    if let Ok(mut guard) = MEMORIES_STATES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        guard.insert(code_home, std::sync::Arc::downgrade(&state));
    }
    Ok(state)
}

async fn maybe_existing_memories_state(
    code_home: &Path,
) -> io::Result<Option<std::sync::Arc<MemoriesState>>> {
    if !std::fs::exists(memories_state_path(code_home))? {
        return Ok(None);
    }
    open_memories_state(code_home).await.map(Some)
}

pub(crate) fn maybe_spawn_memory_refresh(code_home: PathBuf, settings: MemoriesConfig) {
    if !should_refresh_now(&code_home) {
        return;
    }

    tokio::spawn(async move {
        if let Err(err) =
            refresh_memory_artifacts_from_catalog(&code_home, &settings, false).await
        {
            clear_refresh_backoff(&code_home);
            tracing::warn!("memory refresh skipped: {err}");
        }
        clear_status_cache(&code_home);
    });
}

pub(crate) async fn ensure_layout(code_home: &Path) -> io::Result<()> {
    tokio::fs::create_dir_all(rollout_summaries_dir(code_home)).await
}

pub async fn clear_generated_memory_artifacts(code_home: &Path) -> io::Result<()> {
    clear_refresh_backoff(code_home);
    clear_status_cache(code_home);
    control::clear_memory_root_contents(&memory_root(code_home)).await
}

pub async fn refresh_memory_artifacts_now(
    code_home: &Path,
    settings: &MemoriesConfig,
) -> io::Result<()> {
    clear_refresh_backoff(code_home);
    clear_status_cache(code_home);
    storage::refresh_memory_artifacts_from_catalog(code_home, settings, true).await
}

pub async fn note_selected_memories_used(code_home: &Path) -> io::Result<()> {
    let Some(state) = maybe_existing_memories_state(code_home).await? else {
        return Ok(());
    };
    let ids: Vec<Uuid> = state
        .current_selected_outputs()
        .await
        .map_err(io::Error::other)?
        .into_iter()
        .map(|row| row.thread_id)
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    clear_status_cache(code_home);
    state.record_usage(&ids).await.map_err(io::Error::other)
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
    clear_status_cache(code_home);
    Ok(())
}

pub fn get_memories_artifacts_status(code_home: &Path) -> io::Result<MemoriesArtifactsStatus> {
    let root = memory_root(code_home);
    let summary = artifact_status(memory_summary_path(code_home))?;
    let raw_memories = artifact_status(raw_memories_path(code_home))?;
    let rollout_dir = rollout_summaries_dir(code_home);
    let rollout_summaries = artifact_status(rollout_dir.clone())?;
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

pub fn get_memories_status(
    code_home: &Path,
    global: Option<&MemoriesToml>,
    profile: Option<&MemoriesToml>,
    project: Option<&MemoriesToml>,
) -> io::Result<MemoriesStatus> {
    let artifacts = get_memories_artifacts_status(code_home)?;
    let db = get_memories_db_status(code_home)?;
    let effective = crate::config_types::resolve_memories_config(global, profile, project);
    let sources = MemoriesResolvedSources {
        no_memories_if_mcp_or_web_search: bool_source(
            global.and_then(|t| t.no_memories_if_mcp_or_web_search),
            profile.and_then(|t| t.no_memories_if_mcp_or_web_search),
            project.and_then(|t| t.no_memories_if_mcp_or_web_search),
        ),
        generate_memories: bool_source(
            global.and_then(|t| t.generate_memories),
            profile.and_then(|t| t.generate_memories),
            project.and_then(|t| t.generate_memories),
        ),
        use_memories: bool_source(
            global.and_then(|t| t.use_memories),
            profile.and_then(|t| t.use_memories),
            project.and_then(|t| t.use_memories),
        ),
        max_raw_memories_for_consolidation: usize_source(
            global
                .and_then(|t| {
                    t.max_raw_memories_for_consolidation
                        .or(t.max_raw_memories_for_global)
                }),
            profile
                .and_then(|t| {
                    t.max_raw_memories_for_consolidation
                        .or(t.max_raw_memories_for_global)
                }),
            project
                .and_then(|t| {
                    t.max_raw_memories_for_consolidation
                        .or(t.max_raw_memories_for_global)
                }),
        ),
        max_rollout_age_days: i64_source(
            global.and_then(|t| t.max_rollout_age_days),
            profile.and_then(|t| t.max_rollout_age_days),
            project.and_then(|t| t.max_rollout_age_days),
        ),
        max_rollouts_per_startup: usize_source(
            global.and_then(|t| t.max_rollouts_per_startup),
            profile.and_then(|t| t.max_rollouts_per_startup),
            project.and_then(|t| t.max_rollouts_per_startup),
        ),
        min_rollout_idle_hours: i64_source(
            global.and_then(|t| t.min_rollout_idle_hours),
            profile.and_then(|t| t.min_rollout_idle_hours),
            project.and_then(|t| t.min_rollout_idle_hours),
        ),
    };

    Ok(MemoriesStatus {
        artifacts,
        db,
        effective,
        sources,
    })
}

pub fn get_memories_db_status(code_home: &Path) -> io::Result<MemoriesDbStatus> {
    if let Some(status) = STATUS_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|guard| guard.get(code_home).cloned())
        .filter(|(cached_at, _)| cached_at.elapsed() < STATUS_CACHE_INTERVAL)
        .map(|(_, status)| status)
    {
        return Ok(status);
    }

    let db_path = memories_state_path(code_home);
    if !std::fs::exists(db_path)? {
        let status = MemoriesDbStatus {
            db_exists: false,
            thread_count: 0,
            stage1_output_count: 0,
            pending_stage1_count: 0,
            running_stage1_count: 0,
            artifact_job_running: false,
            artifact_dirty: false,
            last_artifact_build_at: None,
        };
        if let Ok(mut guard) = STATUS_CACHE
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
        {
            guard.insert(code_home.to_path_buf(), (Instant::now(), status.clone()));
        }
        return Ok(status);
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(io::Error::other)?;
    let db = rt.block_on(async {
        let state = open_memories_state(code_home).await?;
        state.status().await.map_err(io::Error::other)
    })?;
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
    if let Ok(mut guard) = STATUS_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        guard.insert(code_home.to_path_buf(), (Instant::now(), status.clone()));
    }
    Ok(status)
}

fn artifact_status(path: PathBuf) -> io::Result<MemoryArtifactStatus> {
    match std::fs::metadata(&path) {
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

fn bool_source(
    global: Option<bool>,
    profile: Option<bool>,
    project: Option<bool>,
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

fn usize_source(
    global: Option<usize>,
    profile: Option<usize>,
    project: Option<usize>,
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

fn i64_source(
    global: Option<i64>,
    profile: Option<i64>,
    project: Option<i64>,
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
    use tempfile::tempdir;

    use super::get_memories_status;
    use super::MemoriesSettingSource;
    use crate::config_types::MemoriesToml;

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

        let status = get_memories_status(temp.path(), Some(&global), Some(&profile), Some(&project))
            .expect("memories status");

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
}
