use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use code_protocol::protocol::SessionSource;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Executor, QueryBuilder, Row, Sqlite, SqlitePool};
use tracing::warn;
use uuid::Uuid;

const APP_ID: i64 = 1_129_136_980;
const STATE_SCHEMA_VERSION: i64 = 5;
const ARTIFACT_STATE_KEY: &str = "global";
const JOB_KIND_STAGE1: &str = "stage1";
const JOB_KIND_ARTIFACTS: &str = "artifacts";
const JOB_LEASE_SECONDS: i64 = 300;
const STAGE1_RETRY_BASE_SECONDS: i64 = 300;
const STAGE1_RETRY_MAX_SECONDS: i64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMemoryMode {
    Enabled,
    Disabled,
    Polluted,
}

impl SessionMemoryMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
            Self::Polluted => "polluted",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryThread {
    pub thread_id: Uuid,
    pub rollout_path: PathBuf,
    pub source: SessionSource,
    pub cwd: PathBuf,
    pub cwd_display: String,
    pub updated_at: i64,
    pub updated_at_label: String,
    pub archived: bool,
    pub deleted: bool,
    pub memory_mode: SessionMemoryMode,
    /// Reconciliation metadata reserved for future incremental sync rules.
    pub catalog_seen_at: i64,
    pub git_project_root: Option<PathBuf>,
    pub git_branch: Option<String>,
    pub last_user_snippet: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Stage1Claim {
    pub thread_id: Uuid,
    pub rollout_path: PathBuf,
    pub cwd: PathBuf,
    pub cwd_display: String,
    pub updated_at: i64,
    pub updated_at_label: String,
    pub git_project_root: Option<PathBuf>,
    pub git_branch: Option<String>,
    pub last_user_snippet: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryEpochId {
    pub thread_id: Uuid,
    pub epoch_index: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPlatformFamily {
    Unix,
    Windows,
    Unknown,
}

impl MemoryPlatformFamily {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unix => "unix",
            Self::Windows => "windows",
            Self::Unknown => "unknown",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "unix" => Self::Unix,
            "windows" => Self::Windows,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryShellStyle {
    PosixSh,
    BashZshCompatible,
    Zsh,
    PowerShell,
    Cmd,
    Nushell,
    Elvish,
    Unknown,
}

impl MemoryShellStyle {
    fn as_str(self) -> &'static str {
        match self {
            Self::PosixSh => "posix-sh",
            Self::BashZshCompatible => "bash-zsh-compatible",
            Self::Zsh => "zsh",
            Self::PowerShell => "powershell",
            Self::Cmd => "cmd",
            Self::Nushell => "nushell",
            Self::Elvish => "elvish",
            Self::Unknown => "unknown",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "posix-sh" => Self::PosixSh,
            "bash-zsh-compatible" => Self::BashZshCompatible,
            "zsh" => Self::Zsh,
            "powershell" => Self::PowerShell,
            "cmd" => Self::Cmd,
            "nushell" => Self::Nushell,
            "elvish" => Self::Elvish,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage1EpochProvenance {
    Derived,
    CatalogFallback,
    EmptyDerivationFallback,
}

impl Stage1EpochProvenance {
    fn as_str(self) -> &'static str {
        match self {
            Self::Derived => "derived",
            Self::CatalogFallback => "catalog_fallback",
            Self::EmptyDerivationFallback => "empty_derivation_fallback",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "catalog_fallback" => Self::CatalogFallback,
            "empty_derivation_fallback" => Self::EmptyDerivationFallback,
            _ => Self::Derived,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage1EpochInput {
    pub id: MemoryEpochId,
    pub provenance: Stage1EpochProvenance,
    pub source_updated_at: i64,
    pub generated_at: i64,
    pub epoch_start_at: Option<i64>,
    pub epoch_end_at: Option<i64>,
    pub epoch_start_line: i64,
    pub epoch_end_line: i64,
    pub platform_family: MemoryPlatformFamily,
    pub shell_style: MemoryShellStyle,
    pub shell_program: Option<String>,
    pub workspace_root: Option<String>,
    pub cwd_display: String,
    pub git_branch: Option<String>,
    pub raw_memory: String,
    pub rollout_summary: String,
    pub rollout_slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage1EpochRecord {
    pub id: MemoryEpochId,
    pub provenance: Stage1EpochProvenance,
    pub rollout_path: PathBuf,
    pub cwd: PathBuf,
    pub source_updated_at: i64,
    pub generated_at: i64,
    pub epoch_start_at: Option<i64>,
    pub epoch_end_at: Option<i64>,
    pub epoch_start_line: i64,
    pub epoch_end_line: i64,
    pub platform_family: MemoryPlatformFamily,
    pub shell_style: MemoryShellStyle,
    pub shell_program: Option<String>,
    pub workspace_root: Option<String>,
    pub cwd_display: String,
    pub git_branch: Option<String>,
    pub updated_at_label: String,
    pub raw_memory: String,
    pub rollout_summary: String,
    pub rollout_slug: String,
    pub usage_count: i64,
    pub last_usage: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileResult {
    pub upserted_threads: usize,
    pub pruned_threads: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoriesStateStatus {
    pub db_exists: bool,
    pub thread_count: usize,
    pub stage1_epoch_count: usize,
    pub pending_stage1_count: usize,
    pub running_stage1_count: usize,
    pub artifact_job_running: bool,
    pub artifact_dirty: bool,
    pub last_artifact_build_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ArtifactBuildLease {
    pub ownership_token: String,
    pub dirty: bool,
}

#[async_trait]
trait MemoriesStore: Send + Sync {
    async fn reconcile_threads(&self, threads: &[MemoryThread]) -> Result<ReconcileResult>;
    async fn claim_stage1_candidates(
        &self,
        max_claimed: usize,
        max_age_days: i64,
        min_rollout_idle_hours: i64,
        allowed_sources: &[SessionSource],
        bypass_retry_backoff: bool,
    ) -> Result<Vec<Stage1Claim>>;
    async fn replace_stage1_epochs(&self, thread_id: Uuid, epochs: &[Stage1EpochInput]) -> Result<()>;
    async fn mark_thread_memory_mode(
        &self,
        thread_id: Uuid,
        mode: SessionMemoryMode,
    ) -> Result<bool>;
    async fn mark_artifact_dirty(&self) -> Result<()>;
    async fn select_phase2_epochs(
        &self,
        limit: usize,
        max_retained_age_days: i64,
        allowed_sources: &[SessionSource],
    ) -> Result<Vec<Stage1EpochRecord>>;
    async fn claim_artifact_build_job(&self, force: bool) -> Result<Option<ArtifactBuildLease>>;
    async fn heartbeat_artifact_build_job(&self, token: &str) -> Result<bool>;
    async fn fail_stage1_job(&self, thread_id: Uuid, reason: &str) -> Result<()>;
    async fn succeed_artifact_build_job(&self, token: &str) -> Result<()>;
    async fn fail_artifact_build_job(&self, token: &str, reason: &str) -> Result<()>;
    async fn record_epoch_usage(&self, epoch_ids: &[MemoryEpochId]) -> Result<()>;
    async fn status(&self, allowed_sources: &[SessionSource]) -> Result<MemoriesStateStatus>;
}

struct SqliteMemoriesStore {
    pool: SqlitePool,
}

#[derive(Clone)]
pub struct MemoriesState {
    code_home: PathBuf,
    backend: Arc<dyn MemoriesStore>,
}

impl MemoriesState {
    pub async fn open(code_home: impl Into<PathBuf>) -> Result<Arc<Self>> {
        let code_home = code_home.into();
        tokio::fs::create_dir_all(&code_home).await?;
        let db_path = db_path(&code_home);
        let existed = tokio::fs::try_exists(&db_path).await.unwrap_or(false);
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .with_context(|| format!("open memories sqlite at {}", db_path.display()))?;
        apply_migrations(&pool).await?;
        if !existed {
            warn!("created memories sqlite at {}", db_path.display());
        }
        let backend = Arc::new(SqliteMemoriesStore { pool });
        Ok(Arc::new(Self { code_home, backend }))
    }

    pub fn db_path(&self) -> PathBuf {
        db_path(&self.code_home)
    }

    pub async fn reconcile_threads(&self, threads: &[MemoryThread]) -> Result<ReconcileResult> {
        self.backend.reconcile_threads(threads).await
    }

    pub async fn claim_stage1_candidates(
        &self,
        max_claimed: usize,
        max_age_days: i64,
        min_rollout_idle_hours: i64,
        allowed_sources: &[SessionSource],
        bypass_retry_backoff: bool,
    ) -> Result<Vec<Stage1Claim>> {
        self.backend
            .claim_stage1_candidates(
                max_claimed,
                max_age_days,
                min_rollout_idle_hours,
                allowed_sources,
                bypass_retry_backoff,
            )
            .await
    }

    pub async fn replace_stage1_epochs(&self, thread_id: Uuid, epochs: &[Stage1EpochInput]) -> Result<()> {
        self.backend.replace_stage1_epochs(thread_id, epochs).await
    }

    pub async fn mark_thread_memory_mode(&self, thread_id: Uuid, mode: SessionMemoryMode) -> Result<bool> {
        self.backend.mark_thread_memory_mode(thread_id, mode).await
    }

    pub async fn mark_artifact_dirty(&self) -> Result<()> {
        self.backend.mark_artifact_dirty().await
    }

    pub async fn select_phase2_epochs(
        &self,
        limit: usize,
        max_retained_age_days: i64,
        allowed_sources: &[SessionSource],
    ) -> Result<Vec<Stage1EpochRecord>> {
        self.backend
            .select_phase2_epochs(limit, max_retained_age_days, allowed_sources)
            .await
    }

    pub async fn claim_artifact_build_job(&self, force: bool) -> Result<Option<ArtifactBuildLease>> {
        self.backend.claim_artifact_build_job(force).await
    }

    pub async fn heartbeat_artifact_build_job(&self, token: &str) -> Result<bool> {
        self.backend.heartbeat_artifact_build_job(token).await
    }

    pub async fn fail_stage1_job(&self, thread_id: Uuid, reason: &str) -> Result<()> {
        self.backend.fail_stage1_job(thread_id, reason).await
    }

    pub async fn succeed_artifact_build_job(&self, token: &str) -> Result<()> {
        self.backend.succeed_artifact_build_job(token).await
    }

    pub async fn fail_artifact_build_job(&self, token: &str, reason: &str) -> Result<()> {
        self.backend.fail_artifact_build_job(token, reason).await
    }

    pub async fn record_epoch_usage(&self, epoch_ids: &[MemoryEpochId]) -> Result<()> {
        if epoch_ids.is_empty() {
            return Ok(());
        }
        self.backend.record_epoch_usage(epoch_ids).await
    }

    pub async fn status(&self, allowed_sources: &[SessionSource]) -> Result<MemoriesStateStatus> {
        self.backend.status(allowed_sources).await
    }
}

fn db_path(code_home: &Path) -> PathBuf {
    code_home.join("memories_state.sqlite")
}

async fn apply_migrations(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await?;
    let current_app_id: i64 = sqlx::query_scalar("PRAGMA application_id")
        .fetch_one(&mut *tx)
        .await?;
    let current_version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *tx)
        .await?;

    match current_version {
        0 => create_schema_v5(&mut tx).await?,
        1 => {
            migrate_v1_to_v2(&mut tx).await?;
            migrate_v2_to_v3(&mut tx).await?;
            migrate_v3_to_v4(&mut tx).await?;
            migrate_v4_to_v5(&mut tx).await?;
        }
        2 => {
            migrate_v2_to_v3(&mut tx).await?;
            migrate_v3_to_v4(&mut tx).await?;
            migrate_v4_to_v5(&mut tx).await?;
        }
        3 => {
            migrate_v3_to_v4(&mut tx).await?;
            migrate_v4_to_v5(&mut tx).await?;
        }
        4 => migrate_v4_to_v5(&mut tx).await?,
        5 => {}
        version => {
            return Err(anyhow::anyhow!(
                "unsupported memories sqlite schema version {version}"
            ));
        }
    }

    if current_app_id != APP_ID {
        tx.execute(sqlx::query(format!("PRAGMA application_id = {APP_ID}").as_str()))
            .await?;
    }
    if current_version < STATE_SCHEMA_VERSION {
        tx.execute(sqlx::query(format!("PRAGMA user_version = {STATE_SCHEMA_VERSION}").as_str()))
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn create_schema_v5(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<()> {
    tx.execute(sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS memory_threads (
    thread_id TEXT PRIMARY KEY,
    rollout_path TEXT NOT NULL,
    source TEXT NOT NULL,
    cwd TEXT NOT NULL,
    cwd_display TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    updated_at_label TEXT NOT NULL,
    archived INTEGER NOT NULL,
    deleted INTEGER NOT NULL,
    memory_mode TEXT NOT NULL,
    catalog_seen_at INTEGER NOT NULL,
    git_project_root TEXT,
    git_branch TEXT,
    last_user_snippet TEXT
)
        "#,
    ))
    .await?;
    tx.execute(sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS stage1_epochs (
    thread_id TEXT NOT NULL,
    epoch_index INTEGER NOT NULL,
    provenance TEXT NOT NULL DEFAULT 'derived',
    source_updated_at INTEGER NOT NULL,
    generated_at INTEGER NOT NULL,
    epoch_start_at INTEGER,
    epoch_end_at INTEGER,
    epoch_start_line INTEGER NOT NULL,
    epoch_end_line INTEGER NOT NULL,
    platform_family TEXT NOT NULL,
    shell_style TEXT NOT NULL,
    shell_program TEXT,
    workspace_root TEXT,
    cwd_display TEXT NOT NULL,
    git_branch TEXT,
    raw_memory TEXT NOT NULL,
    rollout_summary TEXT NOT NULL,
    rollout_slug TEXT NOT NULL,
    usage_count INTEGER NOT NULL DEFAULT 0,
    last_usage INTEGER,
    PRIMARY KEY(thread_id, epoch_index),
    FOREIGN KEY(thread_id) REFERENCES memory_threads(thread_id) ON DELETE CASCADE
)
        "#,
    ))
    .await?;
    tx.execute(sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS memory_jobs (
    kind TEXT NOT NULL,
    job_key TEXT NOT NULL,
    ownership_token TEXT,
    lease_until INTEGER,
    last_error TEXT,
    retry_after INTEGER,
    failure_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(kind, job_key)
)
        "#,
    ))
    .await?;
    tx.execute(sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS artifact_state (
    state_key TEXT PRIMARY KEY,
    dirty INTEGER NOT NULL DEFAULT 1,
    last_build_at INTEGER
)
        "#,
    ))
    .await?;
    sqlx::query("INSERT OR IGNORE INTO artifact_state (state_key, dirty, last_build_at) VALUES (?, 1, NULL)")
        .bind(ARTIFACT_STATE_KEY)
        .execute(&mut **tx)
        .await?;
    create_indexes_v4(tx).await?;
    Ok(())
}

async fn create_indexes_v4(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<()> {
    tx.execute(sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_memory_threads_updated_at ON memory_threads(updated_at DESC)",
    ))
    .await?;
    tx.execute(sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_memory_threads_mode ON memory_threads(memory_mode, archived, deleted, source)",
    ))
    .await?;
    tx.execute(sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_stage1_epochs_thread_source_updated ON stage1_epochs(thread_id, source_updated_at DESC)",
    ))
    .await?;
    tx.execute(sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_stage1_epochs_usage ON stage1_epochs(usage_count DESC, last_usage DESC, source_updated_at DESC)",
    ))
    .await?;
    Ok(())
}

async fn migrate_v4_to_v5(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<()> {
    tx.execute(sqlx::query(
        "ALTER TABLE stage1_epochs ADD COLUMN provenance TEXT NOT NULL DEFAULT 'derived'",
    ))
    .await?;
    Ok(())
}

async fn migrate_v1_to_v2(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<()> {
    tx.execute(sqlx::query("ALTER TABLE memory_jobs RENAME TO memory_jobs_v1"))
        .await?;
    tx.execute(sqlx::query("ALTER TABLE artifact_state RENAME TO artifact_state_v1"))
        .await?;

    tx.execute(sqlx::query(
        r#"
CREATE TABLE memory_jobs (
    kind TEXT NOT NULL,
    job_key TEXT NOT NULL,
    ownership_token TEXT,
    lease_until INTEGER,
    last_error TEXT,
    PRIMARY KEY(kind, job_key)
)
        "#,
    ))
    .await?;
    tx.execute(sqlx::query(
        r#"
INSERT INTO memory_jobs (kind, job_key, ownership_token, lease_until, last_error)
SELECT kind, job_key, ownership_token, lease_until, last_error
FROM memory_jobs_v1
        "#,
    ))
    .await?;
    tx.execute(sqlx::query("DROP TABLE memory_jobs_v1")).await?;

    tx.execute(sqlx::query(
        r#"
CREATE TABLE artifact_state (
    state_key TEXT PRIMARY KEY,
    dirty INTEGER NOT NULL DEFAULT 1,
    last_build_at INTEGER
)
        "#,
    ))
    .await?;
    tx.execute(sqlx::query(
        r#"
INSERT INTO artifact_state (state_key, dirty, last_build_at)
SELECT state_key, dirty, last_build_at
FROM artifact_state_v1
        "#,
    ))
    .await?;
    tx.execute(sqlx::query("DROP TABLE artifact_state_v1")).await?;
    sqlx::query("INSERT OR IGNORE INTO artifact_state (state_key, dirty, last_build_at) VALUES (?, 1, NULL)")
        .bind(ARTIFACT_STATE_KEY)
        .execute(&mut **tx)
        .await?;

    tx.execute(sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_memory_threads_updated_at ON memory_threads(updated_at DESC)",
    ))
    .await?;
    tx.execute(sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_memory_threads_mode ON memory_threads(memory_mode, archived, deleted, source)",
    ))
    .await?;
    tx.execute(sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_stage1_outputs_selection ON stage1_outputs(selected_for_phase2, selected_for_phase2_source_updated_at)",
    ))
    .await?;
    tx.execute(sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_stage1_outputs_usage ON stage1_outputs(usage_count DESC, last_usage DESC, source_updated_at DESC)",
    ))
    .await?;
    Ok(())
}

async fn migrate_v2_to_v3(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<()> {
    tx.execute(sqlx::query("ALTER TABLE memory_jobs RENAME TO memory_jobs_v2"))
        .await?;
    tx.execute(sqlx::query(
        r#"
CREATE TABLE memory_jobs (
    kind TEXT NOT NULL,
    job_key TEXT NOT NULL,
    ownership_token TEXT,
    lease_until INTEGER,
    last_error TEXT,
    retry_after INTEGER,
    failure_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(kind, job_key)
)
        "#,
    ))
    .await?;
    tx.execute(sqlx::query(
        r#"
INSERT INTO memory_jobs (
    kind, job_key, ownership_token, lease_until, last_error, retry_after, failure_count
)
SELECT kind, job_key, ownership_token, lease_until, last_error, NULL, 0
FROM memory_jobs_v2
        "#,
    ))
    .await?;
    tx.execute(sqlx::query("DROP TABLE memory_jobs_v2")).await?;
    Ok(())
}

async fn migrate_v3_to_v4(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<()> {
    tx.execute(sqlx::query("ALTER TABLE stage1_outputs RENAME TO stage1_outputs_v3"))
        .await?;
    tx.execute(sqlx::query("ALTER TABLE memory_threads RENAME TO memory_threads_v3"))
        .await?;

    tx.execute(sqlx::query(
        r#"
CREATE TABLE memory_threads (
    thread_id TEXT PRIMARY KEY,
    rollout_path TEXT NOT NULL,
    source TEXT NOT NULL,
    cwd TEXT NOT NULL,
    cwd_display TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    updated_at_label TEXT NOT NULL,
    archived INTEGER NOT NULL,
    deleted INTEGER NOT NULL,
    memory_mode TEXT NOT NULL,
    catalog_seen_at INTEGER NOT NULL,
    git_project_root TEXT,
    git_branch TEXT,
    last_user_snippet TEXT
)
        "#,
    ))
    .await?;
    tx.execute(sqlx::query(
        r#"
CREATE TABLE stage1_epochs (
    thread_id TEXT NOT NULL,
    epoch_index INTEGER NOT NULL,
    source_updated_at INTEGER NOT NULL,
    generated_at INTEGER NOT NULL,
    epoch_start_at INTEGER,
    epoch_end_at INTEGER,
    epoch_start_line INTEGER NOT NULL,
    epoch_end_line INTEGER NOT NULL,
    platform_family TEXT NOT NULL,
    shell_style TEXT NOT NULL,
    shell_program TEXT,
    workspace_root TEXT,
    cwd_display TEXT NOT NULL,
    git_branch TEXT,
    raw_memory TEXT NOT NULL,
    rollout_summary TEXT NOT NULL,
    rollout_slug TEXT NOT NULL,
    usage_count INTEGER NOT NULL DEFAULT 0,
    last_usage INTEGER,
    PRIMARY KEY(thread_id, epoch_index),
    FOREIGN KEY(thread_id) REFERENCES memory_threads(thread_id) ON DELETE CASCADE
)
        "#,
    ))
    .await?;
    create_indexes_v4(tx).await?;

    tx.execute(sqlx::query(
        r#"
INSERT INTO memory_threads (
    thread_id, rollout_path, source, cwd, cwd_display, updated_at, updated_at_label,
    archived, deleted, memory_mode, catalog_seen_at, git_project_root, git_branch, last_user_snippet
)
SELECT
    thread_id, rollout_path, source, cwd, cwd_display, updated_at, updated_at_label,
    archived, deleted, memory_mode, catalog_seen_at, NULL, git_branch, last_user_snippet
FROM memory_threads_v3
        "#,
    ))
    .await?;

    tx.execute(sqlx::query(
        r#"
INSERT INTO stage1_epochs (
    thread_id, epoch_index, source_updated_at, generated_at, epoch_start_at, epoch_end_at,
    epoch_start_line, epoch_end_line, platform_family, shell_style, shell_program,
    workspace_root, cwd_display, git_branch, raw_memory, rollout_summary, rollout_slug,
    usage_count, last_usage
)
SELECT
    so.thread_id,
    0,
    so.source_updated_at,
    so.generated_at,
    so.source_updated_at,
    so.source_updated_at,
    0,
    0,
    'unknown',
    'unknown',
    NULL,
    NULL,
    mt.cwd_display,
    mt.git_branch,
    so.raw_memory,
    so.rollout_summary,
    so.rollout_slug,
    so.usage_count,
    so.last_usage
FROM stage1_outputs_v3 so
JOIN memory_threads_v3 mt ON mt.thread_id = so.thread_id
        "#,
    ))
    .await?;

    tx.execute(sqlx::query("DROP TABLE stage1_outputs_v3")).await?;
    tx.execute(sqlx::query("DROP TABLE memory_threads_v3")).await?;
    Ok(())
}

fn stage1_retry_delay_seconds(failure_count: i64) -> i64 {
    let exponent = failure_count.saturating_sub(1).clamp(0, 20) as u32;
    let multiplier = 1_i64.checked_shl(exponent).unwrap_or(i64::MAX);
    STAGE1_RETRY_BASE_SECONDS
        .saturating_mul(multiplier)
        .min(STAGE1_RETRY_MAX_SECONDS)
}

fn session_source_label(source: &SessionSource) -> String {
    source.to_string()
}

fn now_epoch() -> i64 {
    Utc::now().timestamp()
}

fn as_iso(ts: Option<i64>) -> Option<String> {
    ts.and_then(DateTime::<Utc>::from_timestamp_secs)
        .map(|value| value.to_rfc3339())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PersistedEpochRow {
    id: MemoryEpochId,
    provenance: Stage1EpochProvenance,
    source_updated_at: i64,
    generated_at: i64,
    epoch_start_at: Option<i64>,
    epoch_end_at: Option<i64>,
    epoch_start_line: i64,
    epoch_end_line: i64,
    platform_family: MemoryPlatformFamily,
    shell_style: MemoryShellStyle,
    shell_program: Option<String>,
    workspace_root: Option<String>,
    cwd_display: String,
    git_branch: Option<String>,
    raw_memory: String,
    rollout_summary: String,
    rollout_slug: String,
    usage_count: i64,
    last_usage: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct StableEpochUsageKey {
    epoch_start_line: i64,
    epoch_end_line: i64,
    platform_family: MemoryPlatformFamily,
    shell_style: MemoryShellStyle,
    shell_program: Option<String>,
    workspace_root: Option<String>,
}

impl PersistedEpochRow {
    fn usage_key(&self) -> StableEpochUsageKey {
        StableEpochUsageKey {
            epoch_start_line: self.epoch_start_line,
            epoch_end_line: self.epoch_end_line,
            platform_family: self.platform_family,
            shell_style: self.shell_style,
            shell_program: self.shell_program.clone(),
            workspace_root: self.workspace_root.clone(),
        }
    }

    fn equivalent_input(&self, input: &Stage1EpochInput) -> bool {
        // `generated_at` reflects when stage1 ran. It is intentionally not
        // part of semantic equality so identical regenerated epochs do not
        // trigger a full rewrite or dirty the published artifacts.
        self.id == input.id
            && self.provenance == input.provenance
            && self.source_updated_at == input.source_updated_at
            && self.epoch_start_at == input.epoch_start_at
            && self.epoch_end_at == input.epoch_end_at
            && self.epoch_start_line == input.epoch_start_line
            && self.epoch_end_line == input.epoch_end_line
            && self.platform_family == input.platform_family
            && self.shell_style == input.shell_style
            && self.shell_program == input.shell_program
            && self.workspace_root == input.workspace_root
            && self.cwd_display == input.cwd_display
            && self.git_branch == input.git_branch
            && self.raw_memory == input.raw_memory
            && self.rollout_summary == input.rollout_summary
            && self.rollout_slug == input.rollout_slug
    }
}

impl Stage1EpochInput {
    fn usage_key(&self) -> StableEpochUsageKey {
        StableEpochUsageKey {
            epoch_start_line: self.epoch_start_line,
            epoch_end_line: self.epoch_end_line,
            platform_family: self.platform_family,
            shell_style: self.shell_style,
            shell_program: self.shell_program.clone(),
            workspace_root: self.workspace_root.clone(),
        }
    }
}

async fn load_existing_epochs(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    thread_id: Uuid,
) -> Result<Vec<PersistedEpochRow>> {
    let rows = sqlx::query(
        r#"
SELECT
    thread_id,
    epoch_index,
    provenance,
    source_updated_at,
    generated_at,
    epoch_start_at,
    epoch_end_at,
    epoch_start_line,
    epoch_end_line,
    platform_family,
    shell_style,
    shell_program,
    workspace_root,
    cwd_display,
    git_branch,
    raw_memory,
    rollout_summary,
    rollout_slug,
    usage_count,
    last_usage
FROM stage1_epochs
WHERE thread_id = ?
ORDER BY epoch_index ASC
        "#,
    )
    .bind(thread_id.to_string())
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter().map(persisted_epoch_from_row).collect()
}

fn persisted_epoch_from_row(row: sqlx::sqlite::SqliteRow) -> Result<PersistedEpochRow> {
    Ok(PersistedEpochRow {
        id: MemoryEpochId {
            thread_id: Uuid::parse_str(&row.try_get::<String, _>("thread_id")?)?,
            epoch_index: row.try_get("epoch_index")?,
        },
        provenance: Stage1EpochProvenance::from_str(&row.try_get::<String, _>("provenance")?),
        source_updated_at: row.try_get("source_updated_at")?,
        generated_at: row.try_get("generated_at")?,
        epoch_start_at: row.try_get("epoch_start_at")?,
        epoch_end_at: row.try_get("epoch_end_at")?,
        epoch_start_line: row.try_get("epoch_start_line")?,
        epoch_end_line: row.try_get("epoch_end_line")?,
        platform_family: MemoryPlatformFamily::from_str(&row.try_get::<String, _>("platform_family")?),
        shell_style: MemoryShellStyle::from_str(&row.try_get::<String, _>("shell_style")?),
        shell_program: row.try_get("shell_program")?,
        workspace_root: row.try_get("workspace_root")?,
        cwd_display: row.try_get("cwd_display")?,
        git_branch: row.try_get("git_branch")?,
        raw_memory: row.try_get("raw_memory")?,
        rollout_summary: row.try_get("rollout_summary")?,
        rollout_slug: row.try_get("rollout_slug")?,
        usage_count: row.try_get("usage_count")?,
        last_usage: row.try_get("last_usage")?,
    })
}

#[async_trait]
impl MemoriesStore for SqliteMemoriesStore {
    async fn reconcile_threads(&self, threads: &[MemoryThread]) -> Result<ReconcileResult> {
        let mut tx = self.pool.begin().await?;
        let seen: HashSet<String> = threads.iter().map(|thread| thread.thread_id.to_string()).collect();
        let mut upserted = 0usize;
        for thread in threads {
            let updated = sqlx::query(
                r#"
INSERT INTO memory_threads (
    thread_id, rollout_path, source, cwd, cwd_display, updated_at, updated_at_label,
    archived, deleted, memory_mode, catalog_seen_at, git_project_root, git_branch, last_user_snippet
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(thread_id) DO UPDATE SET
    rollout_path = excluded.rollout_path,
    source = excluded.source,
    cwd = excluded.cwd,
    cwd_display = excluded.cwd_display,
    updated_at = excluded.updated_at,
    updated_at_label = excluded.updated_at_label,
    archived = excluded.archived,
    deleted = excluded.deleted,
    memory_mode = excluded.memory_mode,
    catalog_seen_at = excluded.catalog_seen_at,
    git_project_root = excluded.git_project_root,
    git_branch = excluded.git_branch,
    last_user_snippet = excluded.last_user_snippet
WHERE memory_threads.rollout_path != excluded.rollout_path
   OR memory_threads.source != excluded.source
   OR memory_threads.cwd != excluded.cwd
   OR memory_threads.cwd_display != excluded.cwd_display
   OR memory_threads.updated_at != excluded.updated_at
   OR memory_threads.updated_at_label != excluded.updated_at_label
   OR memory_threads.archived != excluded.archived
   OR memory_threads.deleted != excluded.deleted
   OR memory_threads.memory_mode != excluded.memory_mode
   OR memory_threads.git_project_root IS NOT excluded.git_project_root
   OR memory_threads.git_branch IS NOT excluded.git_branch
   OR memory_threads.last_user_snippet IS NOT excluded.last_user_snippet
                "#,
            )
            .bind(thread.thread_id.to_string())
            .bind(thread.rollout_path.to_string_lossy().to_string())
            .bind(session_source_label(&thread.source))
            .bind(thread.cwd.to_string_lossy().to_string())
            .bind(&thread.cwd_display)
            .bind(thread.updated_at)
            .bind(&thread.updated_at_label)
            .bind(thread.archived)
            .bind(thread.deleted)
            .bind(thread.memory_mode.as_str())
            .bind(thread.catalog_seen_at)
            .bind(
                thread
                    .git_project_root
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
            )
            .bind(thread.git_branch.as_deref())
            .bind(thread.last_user_snippet.as_deref())
            .execute(&mut *tx)
            .await?;
            upserted += updated.rows_affected() as usize;
        }

        let rows = sqlx::query("SELECT thread_id FROM memory_threads")
            .fetch_all(&mut *tx)
            .await?;
        let mut stale = Vec::new();
        for row in rows {
            let thread_id: String = row.try_get("thread_id")?;
            if !seen.contains(&thread_id) {
                stale.push(thread_id);
            }
        }

        if !stale.is_empty() {
            let mut delete_jobs = QueryBuilder::<Sqlite>::new(
                "DELETE FROM memory_jobs WHERE kind = ",
            );
            delete_jobs.push_bind(JOB_KIND_STAGE1);
            delete_jobs.push(" AND job_key IN (");
            let mut jobs = delete_jobs.separated(", ");
            for thread_id in &stale {
                jobs.push_bind(thread_id);
            }
            jobs.push_unseparated(")");
            delete_jobs.build().execute(&mut *tx).await?;

            let mut delete_threads =
                QueryBuilder::<Sqlite>::new("DELETE FROM memory_threads WHERE thread_id IN (");
            let mut thread_ids = delete_threads.separated(", ");
            for thread_id in &stale {
                thread_ids.push_bind(thread_id);
            }
            thread_ids.push_unseparated(")");
            delete_threads.build().execute(&mut *tx).await?;
        }

        if upserted > 0 || !stale.is_empty() {
            sqlx::query("UPDATE artifact_state SET dirty = 1 WHERE state_key = ?")
                .bind(ARTIFACT_STATE_KEY)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(ReconcileResult {
            upserted_threads: upserted,
            pruned_threads: stale.len(),
        })
    }

    async fn claim_stage1_candidates(
        &self,
        max_claimed: usize,
        max_age_days: i64,
        min_rollout_idle_hours: i64,
        allowed_sources: &[SessionSource],
        bypass_retry_backoff: bool,
    ) -> Result<Vec<Stage1Claim>> {
        if max_claimed == 0 {
            return Ok(Vec::new());
        }
        let now = now_epoch();
        let max_age_cutoff = if max_age_days > 0 {
            now - max_age_days * 86_400
        } else {
            i64::MIN
        };
        let idle_cutoff = if min_rollout_idle_hours > 0 {
            now - min_rollout_idle_hours * 3_600
        } else {
            i64::MAX
        };
        let allowed: Vec<String> = allowed_sources.iter().map(session_source_label).collect();

        let mut query = String::from(
            r#"
SELECT
    mt.thread_id,
    mt.rollout_path,
    mt.cwd,
    mt.cwd_display,
    mt.updated_at,
    mt.updated_at_label,
    mt.git_project_root,
    mt.git_branch,
    mt.last_user_snippet
FROM memory_threads mt
LEFT JOIN (
    SELECT thread_id, MAX(source_updated_at) AS source_updated_at
    FROM stage1_epochs
    GROUP BY thread_id
) se ON se.thread_id = mt.thread_id
LEFT JOIN memory_jobs mj ON mj.kind = ? AND mj.job_key = mt.thread_id
WHERE mt.archived = 0
  AND mt.deleted = 0
  AND mt.memory_mode = 'enabled'
  AND mt.updated_at >= ?
  AND mt.updated_at <= ?
  AND COALESCE(se.source_updated_at, -1) < mt.updated_at
"#,
        );
        if !bypass_retry_backoff {
            query.push_str("  AND (mj.retry_after IS NULL OR mj.retry_after < ?)\n");
        }
        if !allowed.is_empty() {
            query.push_str("  AND mt.source IN (");
            for idx in 0..allowed.len() {
                if idx > 0 {
                    query.push_str(", ");
                }
                query.push('?');
            }
            query.push_str(")\n");
        }
        query.push_str("ORDER BY mt.updated_at DESC, mt.thread_id DESC");

        let mut q = sqlx::query(query.as_str())
            .bind(JOB_KIND_STAGE1)
            .bind(max_age_cutoff)
            .bind(idle_cutoff);
        if !bypass_retry_backoff {
            q = q.bind(now);
        }
        for source in &allowed {
            q = q.bind(source);
        }
        let rows = q.fetch_all(&self.pool).await?;

        let lease_until = now + JOB_LEASE_SECONDS;
        let mut tx = self.pool.begin().await?;
        let mut claims = Vec::new();
        for row in rows {
            if claims.len() >= max_claimed {
                break;
            }
            let thread_id_text: String = row.try_get("thread_id")?;
            let thread_id = Uuid::parse_str(&thread_id_text)?;
            let updated = sqlx::query(
                r#"
INSERT INTO memory_jobs (kind, job_key, ownership_token, lease_until, last_error, retry_after, failure_count)
VALUES (?, ?, ?, ?, NULL, NULL, 0)
ON CONFLICT(kind, job_key) DO UPDATE SET
    ownership_token = excluded.ownership_token,
    lease_until = excluded.lease_until,
    last_error = NULL,
    retry_after = NULL
WHERE memory_jobs.lease_until IS NULL OR memory_jobs.lease_until < ? OR memory_jobs.ownership_token IS NULL
                "#,
            )
            .bind(JOB_KIND_STAGE1)
            .bind(&thread_id_text)
            .bind(Uuid::new_v4().to_string())
            .bind(lease_until)
            .bind(now)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated == 0 {
                continue;
            }
            claims.push(Stage1Claim {
                thread_id,
                rollout_path: PathBuf::from(row.try_get::<String, _>("rollout_path")?),
                cwd: PathBuf::from(row.try_get::<String, _>("cwd")?),
                cwd_display: row.try_get("cwd_display")?,
                updated_at: row.try_get("updated_at")?,
                updated_at_label: row.try_get("updated_at_label")?,
                git_project_root: row
                    .try_get::<Option<String>, _>("git_project_root")?
                    .map(PathBuf::from),
                git_branch: row.try_get("git_branch")?,
                last_user_snippet: row.try_get("last_user_snippet")?,
            });
        }
        tx.commit().await?;
        Ok(claims)
    }

    async fn replace_stage1_epochs(&self, thread_id: Uuid, epochs: &[Stage1EpochInput]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let existing = load_existing_epochs(&mut tx, thread_id).await?;
        let unchanged = existing.len() == epochs.len()
            && existing
                .iter()
                .zip(epochs.iter())
                .all(|(persisted, input)| persisted.equivalent_input(input));

        if !unchanged {
            let usage_by_key: std::collections::HashMap<StableEpochUsageKey, (i64, Option<i64>)> = existing
                .iter()
                .map(|row| (row.usage_key(), (row.usage_count, row.last_usage)))
                .collect();

            sqlx::query("DELETE FROM stage1_epochs WHERE thread_id = ?")
                .bind(thread_id.to_string())
                .execute(&mut *tx)
                .await?;

            for epoch in epochs {
                let (usage_count, last_usage) = usage_by_key
                    .get(&epoch.usage_key())
                    .copied()
                    .unwrap_or((0, None));
                sqlx::query(
                    r#"
INSERT INTO stage1_epochs (
    thread_id, epoch_index, provenance, source_updated_at, generated_at, epoch_start_at, epoch_end_at,
    epoch_start_line, epoch_end_line, platform_family, shell_style, shell_program,
    workspace_root, cwd_display, git_branch, raw_memory, rollout_summary, rollout_slug,
    usage_count, last_usage
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(epoch.id.thread_id.to_string())
                .bind(epoch.id.epoch_index)
                .bind(epoch.provenance.as_str())
                .bind(epoch.source_updated_at)
                .bind(epoch.generated_at)
                .bind(epoch.epoch_start_at)
                .bind(epoch.epoch_end_at)
                .bind(epoch.epoch_start_line)
                .bind(epoch.epoch_end_line)
                .bind(epoch.platform_family.as_str())
                .bind(epoch.shell_style.as_str())
                .bind(epoch.shell_program.as_deref())
                .bind(epoch.workspace_root.as_deref())
                .bind(&epoch.cwd_display)
                .bind(epoch.git_branch.as_deref())
                .bind(&epoch.raw_memory)
                .bind(&epoch.rollout_summary)
                .bind(&epoch.rollout_slug)
                .bind(usage_count)
                .bind(last_usage)
                .execute(&mut *tx)
                .await?;
            }

            sqlx::query("UPDATE artifact_state SET dirty = 1 WHERE state_key = ?")
                .bind(ARTIFACT_STATE_KEY)
                .execute(&mut *tx)
                .await?;
        }

        sqlx::query(
            "UPDATE memory_jobs SET ownership_token = NULL, lease_until = NULL, last_error = NULL, retry_after = NULL, failure_count = 0 WHERE kind = ? AND job_key = ?",
        )
        .bind(JOB_KIND_STAGE1)
        .bind(thread_id.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn mark_thread_memory_mode(&self, thread_id: Uuid, mode: SessionMemoryMode) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query(
            "UPDATE memory_threads SET memory_mode = ? WHERE thread_id = ? AND memory_mode != ?",
        )
        .bind(mode.as_str())
        .bind(thread_id.to_string())
        .bind(mode.as_str())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated > 0 {
            sqlx::query("UPDATE artifact_state SET dirty = 1 WHERE state_key = ?")
                .bind(ARTIFACT_STATE_KEY)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(updated > 0)
    }

    async fn mark_artifact_dirty(&self) -> Result<()> {
        sqlx::query("UPDATE artifact_state SET dirty = 1 WHERE state_key = ?")
            .bind(ARTIFACT_STATE_KEY)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn select_phase2_epochs(
        &self,
        limit: usize,
        max_retained_age_days: i64,
        allowed_sources: &[SessionSource],
    ) -> Result<Vec<Stage1EpochRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let cutoff = if max_retained_age_days > 0 {
            now_epoch() - max_retained_age_days * 86_400
        } else {
            i64::MIN
        };
        let allowed: Vec<String> = allowed_sources.iter().map(session_source_label).collect();
        let mut query = QueryBuilder::<Sqlite>::new(
            r#"
SELECT
    se.thread_id,
    se.epoch_index,
    se.provenance,
    mt.rollout_path,
    mt.cwd,
    mt.updated_at_label,
    se.source_updated_at,
    se.generated_at,
    se.epoch_start_at,
    se.epoch_end_at,
    se.epoch_start_line,
    se.epoch_end_line,
    se.platform_family,
    se.shell_style,
    se.shell_program,
    se.workspace_root,
    se.cwd_display,
    se.git_branch,
    se.raw_memory,
    se.rollout_summary,
    se.rollout_slug,
    se.usage_count,
    se.last_usage
FROM stage1_epochs se
JOIN memory_threads mt ON mt.thread_id = se.thread_id
WHERE mt.archived = 0
  AND mt.deleted = 0
  AND mt.memory_mode = 'enabled'
  AND (length(trim(se.raw_memory)) > 0 OR length(trim(se.rollout_summary)) > 0)
  AND (se.source_updated_at >= "#,
        );
        query.push_bind(cutoff);
        query.push(" OR (se.last_usage IS NOT NULL AND se.last_usage >= ");
        query.push_bind(cutoff);
        query.push("))");
        if !allowed.is_empty() {
            query.push(" AND mt.source IN (");
            let mut sources = query.separated(", ");
            for source in &allowed {
                sources.push_bind(source);
            }
            sources.push_unseparated(")");
        }
        query.push(
            r#"
ORDER BY CASE se.provenance
             WHEN 'derived' THEN 0
             WHEN 'catalog_fallback' THEN 1
             ELSE 2
         END ASC,
         se.usage_count DESC,
         CASE
             WHEN se.last_usage IS NULL OR se.last_usage < se.source_updated_at
                 THEN se.source_updated_at
             ELSE se.last_usage
         END DESC,
         se.source_updated_at DESC,
         se.thread_id DESC,
         se.epoch_index ASC
LIMIT "#,
        );
        query.push_bind(limit as i64);
        let rows = query.build().fetch_all(&self.pool).await?;
        rows.into_iter().map(stage1_epoch_record_from_row).collect()
    }

    async fn claim_artifact_build_job(&self, force: bool) -> Result<Option<ArtifactBuildLease>> {
        let mut tx = self.pool.begin().await?;
        let dirty: i64 = sqlx::query_scalar("SELECT dirty FROM artifact_state WHERE state_key = ?")
            .bind(ARTIFACT_STATE_KEY)
            .fetch_one(&mut *tx)
            .await?;
        if dirty == 0 && !force {
            tx.commit().await?;
            return Ok(None);
        }
        let now = now_epoch();
        let token = Uuid::new_v4().to_string();
        let updated = sqlx::query(
            r#"
INSERT INTO memory_jobs (kind, job_key, ownership_token, lease_until, last_error, retry_after, failure_count)
VALUES (?, ?, ?, ?, NULL, NULL, 0)
ON CONFLICT(kind, job_key) DO UPDATE SET
    ownership_token = excluded.ownership_token,
    lease_until = excluded.lease_until,
    last_error = NULL
WHERE memory_jobs.lease_until IS NULL OR memory_jobs.lease_until < ? OR memory_jobs.ownership_token IS NULL
            "#,
        )
        .bind(JOB_KIND_ARTIFACTS)
        .bind(ARTIFACT_STATE_KEY)
        .bind(&token)
        .bind(now + JOB_LEASE_SECONDS)
        .bind(now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        if updated == 0 {
            return Ok(None);
        }
        Ok(Some(ArtifactBuildLease {
            ownership_token: token,
            dirty: dirty != 0,
        }))
    }

    async fn heartbeat_artifact_build_job(&self, token: &str) -> Result<bool> {
        let updated = sqlx::query(
            "UPDATE memory_jobs SET lease_until = ? WHERE kind = ? AND job_key = ? AND ownership_token = ?",
        )
        .bind(now_epoch() + JOB_LEASE_SECONDS)
        .bind(JOB_KIND_ARTIFACTS)
        .bind(ARTIFACT_STATE_KEY)
        .bind(token)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(updated > 0)
    }

    async fn fail_stage1_job(&self, thread_id: Uuid, reason: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let failure_count: i64 = sqlx::query_scalar(
            "SELECT COALESCE(failure_count, 0) FROM memory_jobs WHERE kind = ? AND job_key = ?",
        )
        .bind(JOB_KIND_STAGE1)
        .bind(thread_id.to_string())
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(0);
        let next_failure_count = failure_count.saturating_add(1);
        let retry_after = now_epoch() + stage1_retry_delay_seconds(next_failure_count);
        sqlx::query(
            "UPDATE memory_jobs SET ownership_token = NULL, lease_until = NULL, last_error = ?, retry_after = ?, failure_count = ? WHERE kind = ? AND job_key = ?",
        )
        .bind(reason)
        .bind(retry_after)
        .bind(next_failure_count)
        .bind(JOB_KIND_STAGE1)
        .bind(thread_id.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn succeed_artifact_build_job(&self, token: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE artifact_state SET dirty = 0, last_build_at = ? WHERE state_key = ?")
            .bind(now_epoch())
            .bind(ARTIFACT_STATE_KEY)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE memory_jobs SET ownership_token = NULL, lease_until = NULL, last_error = NULL WHERE kind = ? AND job_key = ? AND ownership_token = ?",
        )
        .bind(JOB_KIND_ARTIFACTS)
        .bind(ARTIFACT_STATE_KEY)
        .bind(token)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn fail_artifact_build_job(&self, token: &str, reason: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE memory_jobs SET ownership_token = NULL, lease_until = NULL, last_error = ? WHERE kind = ? AND job_key = ? AND ownership_token = ?",
        )
        .bind(reason)
        .bind(JOB_KIND_ARTIFACTS)
        .bind(ARTIFACT_STATE_KEY)
        .bind(token)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn record_epoch_usage(&self, epoch_ids: &[MemoryEpochId]) -> Result<()> {
        let now = now_epoch();
        let mut tx = self.pool.begin().await?;
        for epoch_id in epoch_ids {
            sqlx::query(
                "UPDATE stage1_epochs SET usage_count = usage_count + 1, last_usage = ? WHERE thread_id = ? AND epoch_index = ?",
            )
            .bind(now)
            .bind(epoch_id.thread_id.to_string())
            .bind(epoch_id.epoch_index)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn status(&self, allowed_sources: &[SessionSource]) -> Result<MemoriesStateStatus> {
        let now = now_epoch();
        let allowed: Vec<String> = allowed_sources.iter().map(session_source_label).collect();
        let thread_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_threads")
            .fetch_one(&self.pool)
            .await?;
        let stage1_epoch_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stage1_epochs")
            .fetch_one(&self.pool)
            .await?;

        let mut running_query = QueryBuilder::<Sqlite>::new(
            r#"
SELECT COUNT(*)
FROM memory_jobs mj
JOIN memory_threads mt ON mt.thread_id = mj.job_key
WHERE mj.kind = "#,
        );
        running_query.push_bind(JOB_KIND_STAGE1);
        running_query.push(
            r#"
  AND mj.lease_until IS NOT NULL
  AND mj.lease_until >= "#,
        );
        running_query.push_bind(now);
        running_query.push(
            r#"
  AND mt.archived = 0
  AND mt.deleted = 0
  AND mt.memory_mode = 'enabled'
"#,
        );
        if !allowed.is_empty() {
            running_query.push("  AND mt.source IN (");
            let mut sources = running_query.separated(", ");
            for source in &allowed {
                sources.push_bind(source);
            }
            sources.push_unseparated(")\n");
        }
        let running_stage1_count: i64 = running_query
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await?;

        let mut pending_query = QueryBuilder::<Sqlite>::new(
            r#"
SELECT COUNT(*)
FROM memory_threads mt
LEFT JOIN (
    SELECT thread_id, MAX(source_updated_at) AS source_updated_at
    FROM stage1_epochs
    GROUP BY thread_id
) se ON se.thread_id = mt.thread_id
LEFT JOIN memory_jobs mj ON mj.kind = "#,
        );
        pending_query.push_bind(JOB_KIND_STAGE1);
        pending_query.push(
            r#"
 AND mj.job_key = mt.thread_id
WHERE mt.archived = 0
  AND mt.deleted = 0
  AND mt.memory_mode = 'enabled'
  AND COALESCE(se.source_updated_at, -1) < mt.updated_at
  AND (mj.lease_until IS NULL OR mj.lease_until < "#,
        );
        pending_query.push_bind(now);
        pending_query.push(
            r#" OR mj.ownership_token IS NULL)
  AND (mj.retry_after IS NULL OR mj.retry_after < "#,
        );
        pending_query.push_bind(now);
        pending_query.push(
            r#")
"#,
        );
        if !allowed.is_empty() {
            pending_query.push("  AND mt.source IN (");
            let mut sources = pending_query.separated(", ");
            for source in &allowed {
                sources.push_bind(source);
            }
            sources.push_unseparated(")\n");
        }
        let pending_stage1_count: i64 = pending_query
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await?;

        let artifact_row = sqlx::query(
            "SELECT dirty, last_build_at FROM artifact_state WHERE state_key = ?",
        )
        .bind(ARTIFACT_STATE_KEY)
        .fetch_one(&self.pool)
        .await?;
        let artifact_dirty: i64 = artifact_row.try_get("dirty")?;
        let last_artifact_build_at: Option<i64> = artifact_row.try_get("last_build_at")?;
        let artifact_running: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory_jobs WHERE kind = ? AND job_key = ? AND lease_until IS NOT NULL AND lease_until >= ?",
        )
        .bind(JOB_KIND_ARTIFACTS)
        .bind(ARTIFACT_STATE_KEY)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        Ok(MemoriesStateStatus {
            db_exists: true,
            thread_count: thread_count as usize,
            stage1_epoch_count: stage1_epoch_count as usize,
            pending_stage1_count: pending_stage1_count as usize,
            running_stage1_count: running_stage1_count as usize,
            artifact_job_running: artifact_running > 0,
            artifact_dirty: artifact_dirty != 0,
            last_artifact_build_at: as_iso(last_artifact_build_at),
        })
    }
}

fn stage1_epoch_record_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Stage1EpochRecord> {
    Ok(Stage1EpochRecord {
        id: MemoryEpochId {
            thread_id: Uuid::parse_str(&row.try_get::<String, _>("thread_id")?)?,
            epoch_index: row.try_get("epoch_index")?,
        },
        provenance: Stage1EpochProvenance::from_str(&row.try_get::<String, _>("provenance")?),
        rollout_path: PathBuf::from(row.try_get::<String, _>("rollout_path")?),
        cwd: PathBuf::from(row.try_get::<String, _>("cwd")?),
        source_updated_at: row.try_get("source_updated_at")?,
        generated_at: row.try_get("generated_at")?,
        epoch_start_at: row.try_get("epoch_start_at")?,
        epoch_end_at: row.try_get("epoch_end_at")?,
        epoch_start_line: row.try_get("epoch_start_line")?,
        epoch_end_line: row.try_get("epoch_end_line")?,
        platform_family: MemoryPlatformFamily::from_str(&row.try_get::<String, _>("platform_family")?),
        shell_style: MemoryShellStyle::from_str(&row.try_get::<String, _>("shell_style")?),
        shell_program: row.try_get("shell_program")?,
        workspace_root: row.try_get("workspace_root")?,
        cwd_display: row.try_get("cwd_display")?,
        git_branch: row.try_get("git_branch")?,
        updated_at_label: row.try_get("updated_at_label")?,
        raw_memory: row.try_get("raw_memory")?,
        rollout_summary: row.try_get("rollout_summary")?,
        rollout_slug: row.try_get("rollout_slug")?,
        usage_count: row.try_get("usage_count")?,
        last_usage: row.try_get("last_usage")?,
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::tempdir;

    use super::*;

    const INTERACTIVE_SOURCES: &[SessionSource] = &[SessionSource::Cli, SessionSource::VSCode];

    fn sample_thread(id: Uuid, updated_at: i64) -> MemoryThread {
        MemoryThread {
            thread_id: id,
            rollout_path: PathBuf::from(format!("sessions/{id}.jsonl")),
            source: SessionSource::Cli,
            cwd: PathBuf::from("/tmp/workspace"),
            cwd_display: "~/workspace".to_string(),
            updated_at,
            updated_at_label: DateTime::<Utc>::from_timestamp_secs(updated_at)
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| updated_at.to_string()),
            archived: false,
            deleted: false,
            memory_mode: SessionMemoryMode::Enabled,
            catalog_seen_at: updated_at,
            git_project_root: Some(PathBuf::from("/tmp/workspace")),
            git_branch: Some("main".to_string()),
            last_user_snippet: Some("Investigate regression".to_string()),
        }
    }

    fn sample_epoch(thread_id: Uuid, epoch_index: i64, source_updated_at: i64, raw_memory: &str) -> Stage1EpochInput {
        Stage1EpochInput {
            id: MemoryEpochId {
                thread_id,
                epoch_index,
            },
            provenance: Stage1EpochProvenance::Derived,
            source_updated_at,
            generated_at: source_updated_at + 60,
            epoch_start_at: Some(source_updated_at),
            epoch_end_at: Some(source_updated_at + 10),
            epoch_start_line: epoch_index * 10,
            epoch_end_line: epoch_index * 10 + 9,
            platform_family: MemoryPlatformFamily::Unix,
            shell_style: MemoryShellStyle::BashZshCompatible,
            shell_program: Some("bash".to_string()),
            workspace_root: Some("/tmp/workspace".to_string()),
            cwd_display: "~/workspace".to_string(),
            git_branch: Some("main".to_string()),
            raw_memory: raw_memory.to_string(),
            rollout_summary: format!("{raw_memory} summary"),
            rollout_slug: format!("{thread_id}-{epoch_index}"),
        }
    }

    async fn open_test_pool(db_path: &Path) -> SqlitePool {
        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("open test pool")
    }

    async fn create_v4_schema_for_test(pool: &SqlitePool) {
        sqlx::query(
            r#"
CREATE TABLE memory_threads (
    thread_id TEXT PRIMARY KEY,
    rollout_path TEXT NOT NULL,
    source TEXT NOT NULL,
    cwd TEXT NOT NULL,
    cwd_display TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    updated_at_label TEXT NOT NULL,
    archived INTEGER NOT NULL,
    deleted INTEGER NOT NULL,
    memory_mode TEXT NOT NULL,
    catalog_seen_at INTEGER NOT NULL,
    git_project_root TEXT,
    git_branch TEXT,
    last_user_snippet TEXT
)
            "#,
        )
        .execute(pool)
        .await
        .expect("create v4 memory_threads");
        sqlx::query(
            r#"
CREATE TABLE stage1_epochs (
    thread_id TEXT NOT NULL,
    epoch_index INTEGER NOT NULL,
    source_updated_at INTEGER NOT NULL,
    generated_at INTEGER NOT NULL,
    epoch_start_at INTEGER,
    epoch_end_at INTEGER,
    epoch_start_line INTEGER NOT NULL,
    epoch_end_line INTEGER NOT NULL,
    platform_family TEXT NOT NULL,
    shell_style TEXT NOT NULL,
    shell_program TEXT,
    workspace_root TEXT,
    cwd_display TEXT NOT NULL,
    git_branch TEXT,
    raw_memory TEXT NOT NULL,
    rollout_summary TEXT NOT NULL,
    rollout_slug TEXT NOT NULL,
    usage_count INTEGER NOT NULL DEFAULT 0,
    last_usage INTEGER,
    PRIMARY KEY(thread_id, epoch_index),
    FOREIGN KEY(thread_id) REFERENCES memory_threads(thread_id) ON DELETE CASCADE
)
            "#,
        )
        .execute(pool)
        .await
        .expect("create v4 stage1_epochs");
        sqlx::query(
            r#"
CREATE TABLE memory_jobs (
    kind TEXT NOT NULL,
    job_key TEXT NOT NULL,
    ownership_token TEXT,
    lease_until INTEGER,
    last_error TEXT,
    retry_after INTEGER,
    failure_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(kind, job_key)
)
            "#,
        )
        .execute(pool)
        .await
        .expect("create v4 memory_jobs");
        sqlx::query(
            r#"
CREATE TABLE artifact_state (
    state_key TEXT PRIMARY KEY,
    dirty INTEGER NOT NULL DEFAULT 1,
    last_build_at INTEGER
)
            "#,
        )
        .execute(pool)
        .await
        .expect("create v4 artifact_state");
        sqlx::query(
            "INSERT INTO artifact_state (state_key, dirty, last_build_at) VALUES (?, 1, NULL)",
        )
        .bind(ARTIFACT_STATE_KEY)
        .execute(pool)
        .await
        .expect("seed v4 artifact_state");
    }

    async fn succeed_artifacts(state: &MemoriesState) {
        let lease = state
            .claim_artifact_build_job(true)
            .await
            .expect("claim artifact build")
            .expect("artifact lease");
        state
            .succeed_artifact_build_job(&lease.ownership_token)
            .await
            .expect("succeed artifact build");
    }

    async fn stage1_status(state: &MemoriesState) -> MemoriesStateStatus {
        state
            .status(INTERACTIVE_SOURCES)
            .await
            .expect("memories status")
    }

    #[tokio::test]
    async fn reconcile_and_prune_threads() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread_a = sample_thread(Uuid::new_v4(), now_epoch() - 86_400);
        let thread_b = sample_thread(Uuid::new_v4(), now_epoch() - 172_800);
        state
            .reconcile_threads(&[thread_a.clone(), thread_b.clone()])
            .await
            .expect("reconcile threads");

        let status = stage1_status(&state).await;
        assert_eq!(status.thread_count, 2);

        let result = state
            .reconcile_threads(&[thread_a.clone(), thread_b.clone()])
            .await
            .expect("reconcile threads again");
        assert_eq!(result.upserted_threads, 0);

        let mut thread_a_seen_again = thread_a.clone();
        thread_a_seen_again.catalog_seen_at = thread_a_seen_again.catalog_seen_at.saturating_add(60);
        let result = state
            .reconcile_threads(&[thread_a_seen_again, thread_b.clone()])
            .await
            .expect("reconcile threads with only catalog_seen_at changed");
        assert_eq!(result.upserted_threads, 0);

        let result = state
            .reconcile_threads(&[thread_a])
            .await
            .expect("prune stale thread");
        assert_eq!(result.pruned_threads, 1);
        let status = stage1_status(&state).await;
        assert_eq!(status.thread_count, 1);
    }

    #[tokio::test]
    async fn migrate_v3_db_preserves_usage_into_unknown_epoch() {
        let temp = tempdir().expect("tempdir");
        let db_path = db_path(temp.path());
        let thread_id = Uuid::new_v4();
        let updated_at = now_epoch() - 172_800;
        let pool = open_test_pool(&db_path).await;

        sqlx::query(format!("PRAGMA application_id = {APP_ID}").as_str())
            .execute(&pool)
            .await
            .expect("set app id");
        sqlx::query("PRAGMA user_version = 3")
            .execute(&pool)
            .await
            .expect("set user version");
        sqlx::query(
            r#"
CREATE TABLE memory_threads (
    thread_id TEXT PRIMARY KEY,
    rollout_path TEXT NOT NULL,
    source TEXT NOT NULL,
    cwd TEXT NOT NULL,
    cwd_display TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    updated_at_label TEXT NOT NULL,
    archived INTEGER NOT NULL,
    deleted INTEGER NOT NULL,
    memory_mode TEXT NOT NULL,
    catalog_seen_at INTEGER NOT NULL,
    git_branch TEXT,
    last_user_snippet TEXT
)
            "#,
        )
        .execute(&pool)
        .await
        .expect("create memory_threads");
        sqlx::query(
            r#"
CREATE TABLE stage1_outputs (
    thread_id TEXT PRIMARY KEY,
    source_updated_at INTEGER NOT NULL,
    generated_at INTEGER NOT NULL,
    raw_memory TEXT NOT NULL,
    rollout_summary TEXT NOT NULL,
    rollout_slug TEXT NOT NULL,
    usage_count INTEGER NOT NULL DEFAULT 0,
    last_usage INTEGER,
    selected_for_phase2 INTEGER NOT NULL DEFAULT 0,
    selected_for_phase2_source_updated_at INTEGER,
    FOREIGN KEY(thread_id) REFERENCES memory_threads(thread_id) ON DELETE CASCADE
)
            "#,
        )
        .execute(&pool)
        .await
        .expect("create stage1_outputs");
        sqlx::query(
            r#"
CREATE TABLE memory_jobs (
    kind TEXT NOT NULL,
    job_key TEXT NOT NULL,
    ownership_token TEXT,
    lease_until INTEGER,
    last_error TEXT,
    retry_after INTEGER,
    failure_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(kind, job_key)
)
            "#,
        )
        .execute(&pool)
        .await
        .expect("create memory_jobs");
        sqlx::query(
            r#"
CREATE TABLE artifact_state (
    state_key TEXT PRIMARY KEY,
    dirty INTEGER NOT NULL DEFAULT 1,
    last_build_at INTEGER
)
            "#,
        )
        .execute(&pool)
        .await
        .expect("create artifact_state");
        sqlx::query(
            r#"
INSERT INTO memory_threads (
    thread_id, rollout_path, source, cwd, cwd_display, updated_at, updated_at_label,
    archived, deleted, memory_mode, catalog_seen_at, git_branch, last_user_snippet
) VALUES (?, ?, ?, ?, ?, ?, ?, 0, 0, 'enabled', ?, ?, ?)
            "#,
        )
        .bind(thread_id.to_string())
        .bind(format!("sessions/{thread_id}.jsonl"))
        .bind(session_source_label(&SessionSource::Cli))
        .bind("/tmp/workspace")
        .bind("~/workspace")
        .bind(updated_at)
        .bind(as_iso(Some(updated_at)).expect("iso"))
        .bind(updated_at)
        .bind("main")
        .bind("Investigate regression")
        .execute(&pool)
        .await
        .expect("insert thread");
        sqlx::query(
            r#"
INSERT INTO stage1_outputs (
    thread_id, source_updated_at, generated_at, raw_memory, rollout_summary, rollout_slug,
    usage_count, last_usage, selected_for_phase2, selected_for_phase2_source_updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?)
            "#,
        )
        .bind(thread_id.to_string())
        .bind(updated_at)
        .bind(updated_at + 60)
        .bind("raw memory")
        .bind("rollout summary")
        .bind("memory-slug")
        .bind(7_i64)
        .bind(updated_at + 120)
        .bind(updated_at)
        .execute(&pool)
        .await
        .expect("insert output");
        sqlx::query(
            "INSERT INTO artifact_state (state_key, dirty, last_build_at) VALUES (?, 0, ?)",
        )
        .bind(ARTIFACT_STATE_KEY)
        .bind(updated_at + 240)
        .execute(&pool)
        .await
        .expect("insert artifact state");
        drop(pool);

        let state = MemoriesState::open(temp.path()).await.expect("open migrated state");
        let status = stage1_status(&state).await;
        assert_eq!(status.stage1_epoch_count, 1);
        assert!(!status.artifact_dirty);

        let selected = state
            .select_phase2_epochs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select epochs");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].usage_count, 7);
        assert_eq!(selected[0].id.epoch_index, 0);
        assert_eq!(selected[0].provenance, Stage1EpochProvenance::Derived);
        assert_eq!(selected[0].platform_family, MemoryPlatformFamily::Unknown);
        assert_eq!(selected[0].shell_style, MemoryShellStyle::Unknown);
    }

    #[tokio::test]
    async fn migrate_v4_db_defaults_epoch_provenance_to_derived() {
        let temp = tempdir().expect("tempdir");
        let db_path = db_path(temp.path());
        let thread_id = Uuid::new_v4();
        let updated_at = now_epoch() - 172_800;
        let pool = open_test_pool(&db_path).await;

        sqlx::query(format!("PRAGMA application_id = {APP_ID}").as_str())
            .execute(&pool)
            .await
            .expect("set app id");
        sqlx::query("PRAGMA user_version = 4")
            .execute(&pool)
            .await
            .expect("set user version");
        create_v4_schema_for_test(&pool).await;
        sqlx::query(
            r#"
INSERT INTO memory_threads (
    thread_id, rollout_path, source, cwd, cwd_display, updated_at, updated_at_label,
    archived, deleted, memory_mode, catalog_seen_at, git_project_root, git_branch, last_user_snippet
) VALUES (?, ?, ?, ?, ?, ?, ?, 0, 0, 'enabled', ?, ?, ?, ?)
            "#,
        )
        .bind(thread_id.to_string())
        .bind(format!("sessions/{thread_id}.jsonl"))
        .bind(session_source_label(&SessionSource::Cli))
        .bind("/tmp/workspace")
        .bind("~/workspace")
        .bind(updated_at)
        .bind(as_iso(Some(updated_at)).expect("iso"))
        .bind(updated_at)
        .bind("/tmp/workspace")
        .bind("main")
        .bind("Investigate regression")
        .execute(&pool)
        .await
        .expect("insert thread");
        sqlx::query(
            r#"
INSERT INTO stage1_epochs (
    thread_id, epoch_index, source_updated_at, generated_at, epoch_start_at, epoch_end_at,
    epoch_start_line, epoch_end_line, platform_family, shell_style, shell_program,
    workspace_root, cwd_display, git_branch, raw_memory, rollout_summary, rollout_slug,
    usage_count, last_usage
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(thread_id.to_string())
        .bind(0_i64)
        .bind(updated_at)
        .bind(updated_at + 60)
        .bind(updated_at)
        .bind(updated_at + 10)
        .bind(0_i64)
        .bind(9_i64)
        .bind("unix")
        .bind("zsh")
        .bind("zsh")
        .bind("/tmp/workspace")
        .bind("~/workspace")
        .bind("main")
        .bind("raw memory")
        .bind("rollout summary")
        .bind("memory-slug")
        .bind(3_i64)
        .bind(updated_at + 120)
        .execute(&pool)
        .await
        .expect("insert stage1 epoch");
        drop(pool);

        let state = MemoriesState::open(temp.path()).await.expect("open migrated v5 state");
        let selected = state
            .select_phase2_epochs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select epochs");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].provenance, Stage1EpochProvenance::Derived);
    }

    #[tokio::test]
    async fn stage1_failures_back_off_until_retry_window_expires() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread_id = Uuid::new_v4();
        state
            .reconcile_threads(&[sample_thread(thread_id, now_epoch() - 172_800)])
            .await
            .expect("reconcile thread");

        let claims = state
            .claim_stage1_candidates(1, 365, 0, INTERACTIVE_SOURCES, false)
            .await
            .expect("claim stage1");
        assert_eq!(claims.len(), 1);

        state
            .fail_stage1_job(thread_id, "boom")
            .await
            .expect("fail stage1");

        let claims = state
            .claim_stage1_candidates(1, 365, 0, INTERACTIVE_SOURCES, false)
            .await
            .expect("claim during backoff");
        assert!(claims.is_empty());

        let pool = open_test_pool(&state.db_path()).await;
        sqlx::query(
            "UPDATE memory_jobs SET retry_after = ? WHERE kind = ? AND job_key = ?",
        )
        .bind(now_epoch() - 1)
        .bind(JOB_KIND_STAGE1)
        .bind(thread_id.to_string())
        .execute(&pool)
        .await
        .expect("expire retry");

        let claims = state
            .claim_stage1_candidates(1, 365, 0, INTERACTIVE_SOURCES, false)
            .await
            .expect("claim after retry");
        assert_eq!(claims.len(), 1);
    }

    #[tokio::test]
    async fn replace_epochs_preserves_usage_and_only_dirties_on_change() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread_id = Uuid::new_v4();
        let updated_at = now_epoch() - 200_000;
        state
            .reconcile_threads(&[sample_thread(thread_id, updated_at)])
            .await
            .expect("reconcile thread");

        let epoch = sample_epoch(thread_id, 0, updated_at, "raw");
        state
            .replace_stage1_epochs(thread_id, &[epoch.clone()])
            .await
            .expect("replace epochs");
        succeed_artifacts(&state).await;
        assert!(!stage1_status(&state).await.artifact_dirty);

        state
            .record_epoch_usage(&[epoch.id])
            .await
            .expect("record epoch usage");
        let selected = state
            .select_phase2_epochs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select epochs");
        assert_eq!(selected[0].usage_count, 1);

        state
            .replace_stage1_epochs(thread_id, &[epoch.clone()])
            .await
            .expect("replace unchanged epochs");
        assert!(!stage1_status(&state).await.artifact_dirty);
        let selected = state
            .select_phase2_epochs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select after unchanged replace");
        assert_eq!(selected[0].usage_count, 1);

        let mut regenerated = epoch.clone();
        regenerated.generated_at = regenerated.generated_at.saturating_add(300);
        state
            .replace_stage1_epochs(thread_id, &[regenerated])
            .await
            .expect("replace epochs with only generated_at changed");
        assert!(!stage1_status(&state).await.artifact_dirty);
        let selected = state
            .select_phase2_epochs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select after generated_at-only replace");
        assert_eq!(selected[0].usage_count, 1);

        let mut changed = epoch.clone();
        changed.raw_memory = "changed".to_string();
        state
            .replace_stage1_epochs(thread_id, &[changed])
            .await
            .expect("replace changed epochs");
        assert!(stage1_status(&state).await.artifact_dirty);
        let selected = state
            .select_phase2_epochs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select after change");
        assert_eq!(selected[0].usage_count, 1);
    }

    #[tokio::test]
    async fn phase2_selection_downranks_fallback_epochs() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let updated_at = now_epoch() - 200_000;
        let derived_id = Uuid::new_v4();
        let catalog_fallback_id = Uuid::new_v4();
        let empty_fallback_id = Uuid::new_v4();
        state
            .reconcile_threads(&[
                sample_thread(derived_id, updated_at),
                sample_thread(catalog_fallback_id, updated_at),
                sample_thread(empty_fallback_id, updated_at),
            ])
            .await
            .expect("reconcile threads");

        let mut derived = sample_epoch(derived_id, 0, updated_at, "derived");
        derived.provenance = Stage1EpochProvenance::Derived;
        let mut catalog_fallback = sample_epoch(catalog_fallback_id, 0, updated_at, "catalog");
        catalog_fallback.provenance = Stage1EpochProvenance::CatalogFallback;
        let mut empty_fallback = sample_epoch(empty_fallback_id, 0, updated_at, "empty");
        empty_fallback.provenance = Stage1EpochProvenance::EmptyDerivationFallback;
        state
            .replace_stage1_epochs(derived_id, &[derived])
            .await
            .expect("replace derived");
        state
            .replace_stage1_epochs(catalog_fallback_id, &[catalog_fallback])
            .await
            .expect("replace catalog fallback");
        state
            .replace_stage1_epochs(empty_fallback_id, &[empty_fallback])
            .await
            .expect("replace empty fallback");

        let selected = state
            .select_phase2_epochs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select epochs");
        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].provenance, Stage1EpochProvenance::Derived);
        assert_eq!(selected[1].provenance, Stage1EpochProvenance::CatalogFallback);
        assert_eq!(selected[2].provenance, Stage1EpochProvenance::EmptyDerivationFallback);
    }

    #[tokio::test]
    async fn replace_epochs_preserves_usage_when_epoch_indexes_shift() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread_id = Uuid::new_v4();
        let updated_at = now_epoch() - 200_000;
        state
            .reconcile_threads(&[sample_thread(thread_id, updated_at)])
            .await
            .expect("reconcile thread");

        let first_epoch = sample_epoch(thread_id, 0, updated_at, "first");
        let mut second_epoch = sample_epoch(thread_id, 1, updated_at, "second");
        second_epoch.epoch_start_line = 10;
        second_epoch.epoch_end_line = 19;
        state
            .replace_stage1_epochs(thread_id, &[first_epoch, second_epoch.clone()])
            .await
            .expect("replace initial epochs");
        state
            .record_epoch_usage(&[second_epoch.id])
            .await
            .expect("record usage on second epoch");

        let mut renumbered_second_epoch = second_epoch.clone();
        renumbered_second_epoch.id = MemoryEpochId {
            thread_id,
            epoch_index: 0,
        };
        state
            .replace_stage1_epochs(thread_id, &[renumbered_second_epoch])
            .await
            .expect("replace with renumbered epoch");

        let selected = state
            .select_phase2_epochs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select after renumber");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id.epoch_index, 0);
        assert_eq!(selected[0].usage_count, 1);
    }

    #[tokio::test]
    async fn status_respects_allowed_sources_and_excludes_running_jobs() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread = sample_thread(Uuid::new_v4(), now_epoch() - 86_400);
        state
            .reconcile_threads(&[thread])
            .await
            .expect("reconcile thread");

        let status = state
            .status(&[SessionSource::VSCode])
            .await
            .expect("status for vscode");
        assert_eq!(status.pending_stage1_count, 0);

        let status = stage1_status(&state).await;
        assert_eq!(status.pending_stage1_count, 1);

        let claims = state
            .claim_stage1_candidates(1, 365, 0, INTERACTIVE_SOURCES, false)
            .await
            .expect("claim stage1");
        assert_eq!(claims.len(), 1);

        let status = stage1_status(&state).await;
        assert_eq!(status.pending_stage1_count, 0);
        assert_eq!(status.running_stage1_count, 1);
    }

    #[tokio::test]
    async fn artifact_leases_do_not_get_stolen_and_usage_changes_selection_order() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let older_id = Uuid::new_v4();
        let newer_id = Uuid::new_v4();
        let older_updated_at = now_epoch() - 200_000;
        let newer_updated_at = now_epoch() - 100_000;
        state
            .reconcile_threads(&[
                sample_thread(older_id, older_updated_at),
                sample_thread(newer_id, newer_updated_at),
            ])
            .await
            .expect("reconcile threads");
        let older_epoch = sample_epoch(older_id, 0, older_updated_at, "older");
        let newer_epoch = sample_epoch(newer_id, 0, newer_updated_at, "newer");
        state
            .replace_stage1_epochs(older_id, &[older_epoch.clone()])
            .await
            .expect("replace older");
        state
            .replace_stage1_epochs(newer_id, &[newer_epoch.clone()])
            .await
            .expect("replace newer");

        let lease = state
            .claim_artifact_build_job(true)
            .await
            .expect("claim artifact build")
            .expect("artifact lease");
        let stolen = state
            .claim_artifact_build_job(true)
            .await
            .expect("second artifact claim");
        assert!(stolen.is_none());
        state
            .succeed_artifact_build_job(&lease.ownership_token)
            .await
            .expect("succeed artifact build");

        state
            .record_epoch_usage(&[older_epoch.id])
            .await
            .expect("record usage");
        let selected = state
            .select_phase2_epochs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select after usage");
        assert_eq!(selected[0].id, older_epoch.id);
    }

    #[tokio::test]
    async fn phase2_selection_respects_allowed_sources() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let cli_id = Uuid::new_v4();
        let exec_id = Uuid::new_v4();
        let cli_updated_at = now_epoch() - 120_000;
        let exec_updated_at = now_epoch() - 100_000;
        let mut exec_thread = sample_thread(exec_id, exec_updated_at);
        exec_thread.source = SessionSource::Exec;
        state
            .reconcile_threads(&[sample_thread(cli_id, cli_updated_at), exec_thread])
            .await
            .expect("reconcile threads");
        state
            .replace_stage1_epochs(cli_id, &[sample_epoch(cli_id, 0, cli_updated_at, "cli")])
            .await
            .expect("replace cli epochs");
        state
            .replace_stage1_epochs(exec_id, &[sample_epoch(exec_id, 0, exec_updated_at, "exec")])
            .await
            .expect("replace exec epochs");

        let selected = state
            .select_phase2_epochs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select phase2");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id.thread_id, cli_id);
    }

    #[tokio::test]
    async fn phase2_selection_keeps_recently_updated_epochs_with_stale_usage() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread_id = Uuid::new_v4();
        let stale_source_updated_at = now_epoch() - 400 * 86_400;
        state
            .reconcile_threads(&[sample_thread(thread_id, stale_source_updated_at)])
            .await
            .expect("reconcile thread");

        let epoch = sample_epoch(thread_id, 0, stale_source_updated_at, "old epoch");
        state
            .replace_stage1_epochs(thread_id, &[epoch.clone()])
            .await
            .expect("replace initial epoch");
        state
            .record_epoch_usage(&[epoch.id])
            .await
            .expect("record usage");

        let pool = open_test_pool(&state.db_path()).await;
        let recent_source_updated_at = now_epoch() - 60;
        sqlx::query(
            "UPDATE memory_threads SET updated_at = ?, updated_at_label = ? WHERE thread_id = ?",
        )
        .bind(recent_source_updated_at)
        .bind(
            DateTime::<Utc>::from_timestamp_secs(recent_source_updated_at)
                .expect("recent timestamp")
                .to_rfc3339(),
        )
        .bind(thread_id.to_string())
        .execute(&pool)
        .await
        .expect("bump thread freshness");

        let mut refreshed_epoch = epoch.clone();
        refreshed_epoch.source_updated_at = recent_source_updated_at;
        refreshed_epoch.generated_at = recent_source_updated_at + 5;
        state
            .replace_stage1_epochs(thread_id, &[refreshed_epoch])
            .await
            .expect("replace refreshed epoch");

        sqlx::query(
            "UPDATE stage1_epochs SET last_usage = ? WHERE thread_id = ? AND epoch_index = ?",
        )
        .bind(stale_source_updated_at)
        .bind(thread_id.to_string())
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("force stale last_usage");

        let selected = state
            .select_phase2_epochs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select epochs");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id.thread_id, thread_id);
        assert_eq!(selected[0].source_updated_at, recent_source_updated_at);
    }
}
