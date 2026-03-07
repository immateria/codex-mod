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
const STATE_SCHEMA_VERSION: i64 = 3;
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
    pub git_branch: Option<String>,
    pub last_user_snippet: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Stage1OutputInput {
    pub thread_id: Uuid,
    pub source_updated_at: i64,
    pub generated_at: i64,
    pub raw_memory: String,
    pub rollout_summary: String,
    pub rollout_slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage1OutputRecord {
    pub thread_id: Uuid,
    pub rollout_path: PathBuf,
    pub cwd: PathBuf,
    pub cwd_display: String,
    pub updated_at_label: String,
    pub git_branch: Option<String>,
    pub source_updated_at: i64,
    pub generated_at: i64,
    pub raw_memory: String,
    pub rollout_summary: String,
    pub rollout_slug: String,
    pub usage_count: i64,
    pub last_usage: Option<i64>,
    pub selected_for_phase2: bool,
    pub selected_for_phase2_source_updated_at: Option<i64>,
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
    pub stage1_output_count: usize,
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
    async fn upsert_stage1_output(&self, output: &Stage1OutputInput) -> Result<()>;
    async fn mark_thread_memory_mode(
        &self,
        thread_id: Uuid,
        mode: SessionMemoryMode,
    ) -> Result<bool>;
    async fn mark_artifact_dirty(&self) -> Result<()>;
    async fn select_phase2_inputs(
        &self,
        limit: usize,
        max_retained_age_days: i64,
        allowed_sources: &[SessionSource],
    ) -> Result<Vec<Stage1OutputRecord>>;
    async fn claim_artifact_build_job(&self, force: bool) -> Result<Option<ArtifactBuildLease>>;
    async fn heartbeat_artifact_build_job(&self, token: &str) -> Result<bool>;
    async fn fail_stage1_job(&self, thread_id: Uuid, reason: &str) -> Result<()>;
    async fn succeed_artifact_build_job(&self, token: &str, selected: &[Stage1OutputRecord]) -> Result<()>;
    async fn fail_artifact_build_job(&self, token: &str, reason: &str) -> Result<()>;
    async fn record_usage(&self, thread_ids: &[Uuid]) -> Result<()>;
    async fn current_selected_outputs(
        &self,
        allowed_sources: &[SessionSource],
    ) -> Result<Vec<Stage1OutputRecord>>;
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

    pub async fn upsert_stage1_output(&self, output: &Stage1OutputInput) -> Result<()> {
        self.backend.upsert_stage1_output(output).await
    }

    pub async fn mark_thread_memory_mode(&self, thread_id: Uuid, mode: SessionMemoryMode) -> Result<bool> {
        self.backend.mark_thread_memory_mode(thread_id, mode).await
    }

    pub async fn mark_artifact_dirty(&self) -> Result<()> {
        self.backend.mark_artifact_dirty().await
    }

    pub async fn select_phase2_inputs(
        &self,
        limit: usize,
        max_retained_age_days: i64,
        allowed_sources: &[SessionSource],
    ) -> Result<Vec<Stage1OutputRecord>> {
        self.backend
            .select_phase2_inputs(limit, max_retained_age_days, allowed_sources)
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

    pub async fn succeed_artifact_build_job(&self, token: &str, selected: &[Stage1OutputRecord]) -> Result<()> {
        self.backend.succeed_artifact_build_job(token, selected).await
    }

    pub async fn fail_artifact_build_job(&self, token: &str, reason: &str) -> Result<()> {
        self.backend.fail_artifact_build_job(token, reason).await
    }

    pub async fn record_usage(&self, thread_ids: &[Uuid]) -> Result<()> {
        if thread_ids.is_empty() {
            return Ok(());
        }
        self.backend.record_usage(thread_ids).await
    }

    pub async fn current_selected_outputs(
        &self,
        allowed_sources: &[SessionSource],
    ) -> Result<Vec<Stage1OutputRecord>> {
        self.backend.current_selected_outputs(allowed_sources).await
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

    if current_version == 0 {
        create_schema_v3(&mut tx).await?;
    } else if current_version == 1 {
        migrate_v1_to_v2(&mut tx).await?;
        migrate_v2_to_v3(&mut tx).await?;
    } else if current_version == 2 {
        migrate_v2_to_v3(&mut tx).await?;
    }

    if current_app_id != APP_ID {
        tx.execute(sqlx::query(format!("PRAGMA application_id = {APP_ID}").as_str()))
            .await?;
    }
    if current_version < STATE_SCHEMA_VERSION {
        tx.execute(sqlx::query(format!("PRAGMA user_version = {STATE_SCHEMA_VERSION}").as_str()))
            .await?;
    }
    if current_version >= STATE_SCHEMA_VERSION {
        tx.commit().await?;
        return Ok(());
    }
    tx.commit().await?;
    Ok(())
}

async fn create_schema_v3(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<()> {
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
    git_branch TEXT,
    last_user_snippet TEXT
)
        "#,
    ))
    .await?;
    tx.execute(sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS stage1_outputs (
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
    tx.execute(sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_threads_updated_at ON memory_threads(updated_at DESC)"))
        .await?;
    tx.execute(sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_threads_mode ON memory_threads(memory_mode, archived, deleted, source)"))
        .await?;
    tx.execute(sqlx::query("CREATE INDEX IF NOT EXISTS idx_stage1_outputs_selection ON stage1_outputs(selected_for_phase2, selected_for_phase2_source_updated_at)"))
        .await?;
    tx.execute(sqlx::query("CREATE INDEX IF NOT EXISTS idx_stage1_outputs_usage ON stage1_outputs(usage_count DESC, last_usage DESC, source_updated_at DESC)"))
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

fn stage1_retry_delay_seconds(failure_count: i64) -> i64 {
    // Repeated rollout parse/read failures should cool down quickly without
    // permanently starving the thread from future extraction attempts.
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
    thread_id, rollout_path, source, cwd, cwd_display, updated_at, updated_at_label, archived, deleted, memory_mode, catalog_seen_at, git_branch, last_user_snippet
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        let allowed: Vec<String> = allowed_sources
            .iter()
            .map(session_source_label)
            .collect();
        let mut query = String::from(
            r#"
SELECT mt.thread_id, mt.rollout_path, mt.cwd, mt.cwd_display, mt.updated_at, mt.updated_at_label, mt.git_branch, mt.last_user_snippet
FROM memory_threads mt
LEFT JOIN stage1_outputs so ON so.thread_id = mt.thread_id
LEFT JOIN memory_jobs mj ON mj.kind = ? AND mj.job_key = mt.thread_id
WHERE mt.archived = 0
  AND mt.deleted = 0
  AND mt.memory_mode = 'enabled'
  AND mt.updated_at >= ?
  AND mt.updated_at <= ?
  AND COALESCE(so.source_updated_at, -1) < mt.updated_at
            "#,
        );
        if !bypass_retry_backoff {
            query.push_str(" AND (mj.retry_after IS NULL OR mj.retry_after < ?)");
        }
        if !allowed.is_empty() {
            query.push_str(" AND mt.source IN (");
            for idx in 0..allowed.len() {
                if idx > 0 {
                    query.push_str(", ");
                }
                query.push('?');
            }
            query.push(')');
        }
        query.push_str(" ORDER BY mt.updated_at DESC, mt.thread_id DESC");
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
INSERT INTO memory_jobs (kind, job_key, ownership_token, lease_until, last_error)
VALUES (?, ?, ?, ?, NULL)
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
                git_branch: row.try_get("git_branch")?,
                last_user_snippet: row.try_get("last_user_snippet")?,
            });
        }
        tx.commit().await?;
        Ok(claims)
    }

    async fn upsert_stage1_output(&self, output: &Stage1OutputInput) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query(
            r#"
INSERT INTO stage1_outputs (
    thread_id, source_updated_at, generated_at, raw_memory, rollout_summary, rollout_slug,
    usage_count, last_usage, selected_for_phase2, selected_for_phase2_source_updated_at
) VALUES (?, ?, ?, ?, ?, ?, 0, NULL, 0, NULL)
ON CONFLICT(thread_id) DO UPDATE SET
    source_updated_at = excluded.source_updated_at,
    generated_at = excluded.generated_at,
    raw_memory = excluded.raw_memory,
    rollout_summary = excluded.rollout_summary,
    rollout_slug = excluded.rollout_slug,
    selected_for_phase2 = CASE
        WHEN stage1_outputs.source_updated_at = excluded.source_updated_at THEN stage1_outputs.selected_for_phase2
        ELSE 0
    END,
    selected_for_phase2_source_updated_at = CASE
        WHEN stage1_outputs.source_updated_at = excluded.source_updated_at THEN stage1_outputs.selected_for_phase2_source_updated_at
        ELSE NULL
    END
WHERE stage1_outputs.source_updated_at != excluded.source_updated_at
   OR stage1_outputs.raw_memory != excluded.raw_memory
   OR stage1_outputs.rollout_summary != excluded.rollout_summary
   OR stage1_outputs.rollout_slug != excluded.rollout_slug
            "#,
        )
        .bind(output.thread_id.to_string())
        .bind(output.source_updated_at)
        .bind(output.generated_at)
        .bind(&output.raw_memory)
        .bind(&output.rollout_summary)
        .bind(&output.rollout_slug)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        sqlx::query("UPDATE memory_jobs SET ownership_token = NULL, lease_until = NULL, last_error = NULL, retry_after = NULL, failure_count = 0 WHERE kind = ? AND job_key = ?")
            .bind(JOB_KIND_STAGE1)
            .bind(output.thread_id.to_string())
            .execute(&mut *tx)
            .await?;
        if updated > 0 {
            sqlx::query("UPDATE artifact_state SET dirty = 1 WHERE state_key = ?")
                .bind(ARTIFACT_STATE_KEY)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn mark_thread_memory_mode(&self, thread_id: Uuid, mode: SessionMemoryMode) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query("UPDATE memory_threads SET memory_mode = ? WHERE thread_id = ? AND memory_mode != ?")
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

    async fn select_phase2_inputs(
        &self,
        limit: usize,
        max_retained_age_days: i64,
        allowed_sources: &[SessionSource],
    ) -> Result<Vec<Stage1OutputRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let cutoff = if max_retained_age_days > 0 {
            now_epoch() - max_retained_age_days * 86_400
        } else {
            i64::MIN
        };
        let allowed: Vec<String> = allowed_sources.iter().map(session_source_label).collect();
        // Selection favors broadly useful memories first, then breaks ties by
        // recent usage or source freshness.
        let mut query = QueryBuilder::<Sqlite>::new(
            r#"
SELECT
    so.thread_id,
    mt.rollout_path,
    mt.cwd,
    mt.cwd_display,
    mt.updated_at_label,
    mt.git_branch,
    so.source_updated_at,
    so.generated_at,
    so.raw_memory,
    so.rollout_summary,
    so.rollout_slug,
    so.usage_count,
    so.last_usage,
    so.selected_for_phase2,
    so.selected_for_phase2_source_updated_at
FROM stage1_outputs so
JOIN memory_threads mt ON mt.thread_id = so.thread_id
WHERE mt.archived = 0
  AND mt.deleted = 0
  AND mt.memory_mode = 'enabled'
  AND (length(trim(so.raw_memory)) > 0 OR length(trim(so.rollout_summary)) > 0)
  AND COALESCE(so.last_usage, so.source_updated_at) >= "#,
        );
        query.push_bind(cutoff);
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
ORDER BY so.usage_count DESC,
         COALESCE(so.last_usage, so.source_updated_at) DESC,
         so.source_updated_at DESC,
         so.thread_id DESC
LIMIT "#,
        );
        query.push_bind(limit as i64);
        let rows = query.build().fetch_all(&self.pool).await?;
        rows.into_iter().map(stage1_record_from_row).collect()
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
        let token = Uuid::new_v4().to_string();
        let updated = sqlx::query(
            r#"
INSERT INTO memory_jobs (kind, job_key, ownership_token, lease_until, last_error)
VALUES (?, ?, ?, ?, NULL)
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
        .bind(now_epoch() + JOB_LEASE_SECONDS)
        .bind(now_epoch())
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
            "UPDATE memory_jobs SET lease_until = ? WHERE kind = ? AND job_key = ? AND ownership_token = ?"
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
            "UPDATE memory_jobs SET ownership_token = NULL, lease_until = NULL, last_error = ?, retry_after = ?, failure_count = ? WHERE kind = ? AND job_key = ?"
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

    async fn succeed_artifact_build_job(&self, token: &str, selected: &[Stage1OutputRecord]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE stage1_outputs SET selected_for_phase2 = 0, selected_for_phase2_source_updated_at = NULL")
            .execute(&mut *tx)
            .await?;
        if !selected.is_empty() {
            let mut select_rows = QueryBuilder::<Sqlite>::new(
                "UPDATE stage1_outputs SET selected_for_phase2 = 1, selected_for_phase2_source_updated_at = source_updated_at WHERE thread_id IN (",
            );
            let mut selected_ids = select_rows.separated(", ");
            for row in selected {
                selected_ids.push_bind(row.thread_id.to_string());
            }
            selected_ids.push_unseparated(")");
            select_rows.build().execute(&mut *tx).await?;
        }
        let last_build_at = now_epoch();
        sqlx::query(
            "UPDATE artifact_state SET dirty = 0, last_build_at = ? WHERE state_key = ?"
        )
        .bind(last_build_at)
        .bind(ARTIFACT_STATE_KEY)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE memory_jobs SET ownership_token = NULL, lease_until = NULL, last_error = NULL WHERE kind = ? AND job_key = ? AND ownership_token = ?"
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
        sqlx::query(
            "UPDATE memory_jobs SET ownership_token = NULL, lease_until = NULL, last_error = ? WHERE kind = ? AND job_key = ? AND ownership_token = ?"
        )
        .bind(reason)
        .bind(JOB_KIND_ARTIFACTS)
        .bind(ARTIFACT_STATE_KEY)
        .bind(token)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn record_usage(&self, thread_ids: &[Uuid]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for thread_id in thread_ids {
            sqlx::query(
                "UPDATE stage1_outputs SET usage_count = usage_count + 1, last_usage = ? WHERE thread_id = ?"
            )
            .bind(now_epoch())
            .bind(thread_id.to_string())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn current_selected_outputs(
        &self,
        allowed_sources: &[SessionSource],
    ) -> Result<Vec<Stage1OutputRecord>> {
        let allowed: Vec<String> = allowed_sources.iter().map(session_source_label).collect();
        // Selected memories are returned in prompt/render order by recency,
        // which is intentionally different from the selection ranking query.
        let mut query = QueryBuilder::<Sqlite>::new(
            r#"
SELECT
    so.thread_id,
    mt.rollout_path,
    mt.cwd,
    mt.cwd_display,
    mt.updated_at_label,
    mt.git_branch,
    so.source_updated_at,
    so.generated_at,
    so.raw_memory,
    so.rollout_summary,
    so.rollout_slug,
    so.usage_count,
    so.last_usage,
    so.selected_for_phase2,
    so.selected_for_phase2_source_updated_at
FROM stage1_outputs so
JOIN memory_threads mt ON mt.thread_id = so.thread_id
WHERE so.selected_for_phase2 = 1
  AND mt.archived = 0
  AND mt.deleted = 0
  AND mt.memory_mode = 'enabled'
"#,
        );
        if !allowed.is_empty() {
            query.push("  AND mt.source IN (");
            let mut sources = query.separated(", ");
            for source in &allowed {
                sources.push_bind(source);
            }
            sources.push_unseparated(")\n");
        }
        query.push(
            "ORDER BY COALESCE(so.last_usage, so.source_updated_at) DESC, so.thread_id DESC",
        );
        let rows = query.build().fetch_all(&self.pool).await?;
        rows.into_iter().map(stage1_record_from_row).collect()
    }

    async fn status(&self, allowed_sources: &[SessionSource]) -> Result<MemoriesStateStatus> {
        let now = now_epoch();
        let allowed: Vec<String> = allowed_sources
            .iter()
            .map(session_source_label)
            .collect();
        let thread_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_threads")
            .fetch_one(&self.pool)
            .await?;
        let stage1_output_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stage1_outputs")
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
LEFT JOIN stage1_outputs so ON so.thread_id = mt.thread_id
LEFT JOIN memory_jobs mj ON mj.kind = "#,
        );
        pending_query.push_bind(JOB_KIND_STAGE1);
        pending_query.push(
            r#"
 AND mj.job_key = mt.thread_id
WHERE mt.archived = 0
  AND mt.deleted = 0
  AND mt.memory_mode = 'enabled'
  AND COALESCE(so.source_updated_at, -1) < mt.updated_at
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
            "SELECT dirty, last_build_at FROM artifact_state WHERE state_key = ?"
        )
        .bind(ARTIFACT_STATE_KEY)
        .fetch_one(&self.pool)
        .await?;
        let artifact_dirty: i64 = artifact_row.try_get("dirty")?;
        let last_artifact_build_at: Option<i64> = artifact_row.try_get("last_build_at")?;
        let artifact_running: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory_jobs WHERE kind = ? AND job_key = ? AND lease_until IS NOT NULL AND lease_until >= ?"
        )
        .bind(JOB_KIND_ARTIFACTS)
        .bind(ARTIFACT_STATE_KEY)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(MemoriesStateStatus {
            db_exists: true,
            thread_count: thread_count as usize,
            stage1_output_count: stage1_output_count as usize,
            pending_stage1_count: pending_stage1_count as usize,
            running_stage1_count: running_stage1_count as usize,
            artifact_job_running: artifact_running > 0,
            artifact_dirty: artifact_dirty != 0,
            last_artifact_build_at: as_iso(last_artifact_build_at),
        })
    }
}

fn stage1_record_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Stage1OutputRecord> {
    Ok(Stage1OutputRecord {
        thread_id: Uuid::parse_str(&row.try_get::<String, _>("thread_id")?)?,
        rollout_path: PathBuf::from(row.try_get::<String, _>("rollout_path")?),
        cwd: PathBuf::from(row.try_get::<String, _>("cwd")?),
        cwd_display: row.try_get("cwd_display")?,
        updated_at_label: row.try_get("updated_at_label")?,
        git_branch: row.try_get("git_branch")?,
        source_updated_at: row.try_get("source_updated_at")?,
        generated_at: row.try_get("generated_at")?,
        raw_memory: row.try_get("raw_memory")?,
        rollout_summary: row.try_get("rollout_summary")?,
        rollout_slug: row.try_get("rollout_slug")?,
        usage_count: row.try_get("usage_count")?,
        last_usage: row.try_get("last_usage")?,
        selected_for_phase2: row.try_get::<i64, _>("selected_for_phase2")? != 0,
        selected_for_phase2_source_updated_at: row.try_get("selected_for_phase2_source_updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Row as _;
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
            git_branch: Some("main".to_string()),
            last_user_snippet: Some("Investigate regression".to_string()),
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

    async fn claim_and_succeed_artifacts(
        state: &MemoriesState,
        selected: &[Stage1OutputRecord],
    ) {
        let lease = state
            .claim_artifact_build_job(true)
            .await
            .expect("claim artifact build job")
            .expect("artifact lease");
        state
            .succeed_artifact_build_job(&lease.ownership_token, selected)
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

        let result = state
            .reconcile_threads(&[thread_a])
            .await
            .expect("prune stale thread");
        assert_eq!(result.pruned_threads, 1);
        let status = stage1_status(&state).await;
        assert_eq!(status.thread_count, 1);
    }

    #[tokio::test]
    async fn migrate_v1_db_preserves_outputs_and_usage() {
        let temp = tempdir().expect("tempdir");
        let thread_id = Uuid::new_v4();
        let updated_at = now_epoch() - 172_800;
        let db_path = db_path(temp.path());
        let pool = open_test_pool(&db_path).await;

        sqlx::query(format!("PRAGMA application_id = {APP_ID}").as_str())
            .execute(&pool)
            .await
            .expect("set app id");
        sqlx::query("PRAGMA user_version = 1")
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
        .expect("create v1 memory_threads");
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
        .expect("create v1 stage1_outputs");
        sqlx::query(
            r#"
CREATE TABLE memory_jobs (
    kind TEXT NOT NULL,
    job_key TEXT NOT NULL,
    ownership_token TEXT,
    lease_until INTEGER,
    retry_after INTEGER,
    last_success_watermark INTEGER,
    last_error TEXT,
    dirty INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(kind, job_key)
)
            "#,
        )
        .execute(&pool)
        .await
        .expect("create v1 memory_jobs");
        sqlx::query(
            r#"
CREATE TABLE artifact_state (
    state_key TEXT PRIMARY KEY,
    dirty INTEGER NOT NULL DEFAULT 1,
    last_build_at INTEGER,
    last_selected_count INTEGER NOT NULL DEFAULT 0,
    last_success_watermark INTEGER
)
            "#,
        )
        .execute(&pool)
        .await
        .expect("create v1 artifact_state");

        sqlx::query(
            r#"
INSERT INTO memory_threads (
    thread_id, rollout_path, source, cwd, cwd_display, updated_at, updated_at_label, archived, deleted, memory_mode, catalog_seen_at, git_branch, last_user_snippet
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
        .expect("insert memory thread");
        sqlx::query(
            r#"
INSERT INTO stage1_outputs (
    thread_id, source_updated_at, generated_at, raw_memory, rollout_summary, rollout_slug, usage_count, last_usage, selected_for_phase2, selected_for_phase2_source_updated_at
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
        .expect("insert stage1 output");
        sqlx::query(
            "INSERT INTO artifact_state (state_key, dirty, last_build_at, last_selected_count, last_success_watermark) VALUES (?, 0, ?, 1, ?)",
        )
        .bind(ARTIFACT_STATE_KEY)
        .bind(updated_at + 240)
        .bind(updated_at)
        .execute(&pool)
        .await
        .expect("insert artifact state");
        drop(pool);

        let state = MemoriesState::open(temp.path()).await.expect("open migrated state");
        let status = stage1_status(&state).await;
        assert_eq!(status.stage1_output_count, 1);
        assert!(!status.artifact_dirty);

        let selected = state
            .current_selected_outputs(INTERACTIVE_SOURCES)
            .await
            .expect("selected outputs after migration");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].usage_count, 7);
        assert!(selected[0].selected_for_phase2);

        let pool = open_test_pool(&db_path).await;
        let memory_job_columns: Vec<String> = sqlx::query("PRAGMA table_info(memory_jobs)")
            .fetch_all(&pool)
            .await
            .expect("memory_jobs columns")
            .into_iter()
            .map(|row| row.get("name"))
            .collect();
        assert!(memory_job_columns.iter().any(|name| name == "retry_after"));
        assert!(memory_job_columns.iter().any(|name| name == "failure_count"));
        assert!(!memory_job_columns.iter().any(|name| name == "dirty"));
        assert!(!memory_job_columns.iter().any(|name| name == "last_success_watermark"));

        let artifact_state_columns: Vec<String> = sqlx::query("PRAGMA table_info(artifact_state)")
            .fetch_all(&pool)
            .await
            .expect("artifact_state columns")
            .into_iter()
            .map(|row| row.get("name"))
            .collect();
        assert!(!artifact_state_columns.iter().any(|name| name == "last_selected_count"));
        assert!(!artifact_state_columns.iter().any(|name| name == "last_success_watermark"));
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
        let status = stage1_status(&state).await;
        assert_eq!(status.pending_stage1_count, 0);
        assert_eq!(status.running_stage1_count, 1);

        state
            .fail_stage1_job(thread_id, "boom")
            .await
            .expect("fail stage1 job");
        let status = stage1_status(&state).await;
        assert_eq!(status.pending_stage1_count, 0);
        assert_eq!(status.running_stage1_count, 0);

        let pool = open_test_pool(&state.db_path()).await;
        let failed_row = sqlx::query(
            "SELECT retry_after, failure_count FROM memory_jobs WHERE kind = ? AND job_key = ?"
        )
        .bind(JOB_KIND_STAGE1)
        .bind(thread_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("fetch failed stage1 job");
        let retry_after: i64 = failed_row.get("retry_after");
        let failure_count: i64 = failed_row.get("failure_count");
        assert!(retry_after > now_epoch());
        assert_eq!(failure_count, 1);

        let claims = state
            .claim_stage1_candidates(1, 365, 0, INTERACTIVE_SOURCES, false)
            .await
            .expect("claim should respect retry window");
        assert!(claims.is_empty());

        sqlx::query("UPDATE memory_jobs SET retry_after = ?, ownership_token = NULL WHERE kind = ? AND job_key = ?")
            .bind(now_epoch() - 1)
            .bind(JOB_KIND_STAGE1)
            .bind(thread_id.to_string())
            .execute(&pool)
            .await
            .expect("expire retry window");

        let claims = state
            .claim_stage1_candidates(1, 365, 0, INTERACTIVE_SOURCES, false)
            .await
            .expect("reclaim stage1 after retry window");
        assert_eq!(claims.len(), 1);

        sqlx::query("UPDATE memory_jobs SET lease_until = ?, ownership_token = ? WHERE kind = ? AND job_key = ?")
            .bind(now_epoch() - 1)
            .bind("stale-token")
            .bind(JOB_KIND_STAGE1)
            .bind(thread_id.to_string())
            .execute(&pool)
            .await
            .expect("expire stage1 lease");

        let claims = state
            .claim_stage1_candidates(1, 365, 0, INTERACTIVE_SOURCES, false)
            .await
            .expect("reclaim expired lease");
        assert_eq!(claims.len(), 1);

        state
            .fail_stage1_job(thread_id, "still broken")
            .await
            .expect("fail stage1 job again");
        let failed_row = sqlx::query(
            "SELECT retry_after, failure_count FROM memory_jobs WHERE kind = ? AND job_key = ?"
        )
        .bind(JOB_KIND_STAGE1)
        .bind(thread_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("fetch failed stage1 job after second failure");
        let next_retry_after: i64 = failed_row.get("retry_after");
        let next_failure_count: i64 = failed_row.get("failure_count");
        assert!(next_retry_after > retry_after);
        assert_eq!(next_failure_count, 2);
    }

    #[tokio::test]
    async fn unchanged_upsert_does_not_dirty_artifacts_and_mode_noop_stays_clean() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread_id = Uuid::new_v4();
        let source_updated_at = now_epoch() - 172_800;
        let output = Stage1OutputInput {
            thread_id,
            source_updated_at,
            generated_at: now_epoch(),
            raw_memory: "raw memory".to_string(),
            rollout_summary: "rollout summary".to_string(),
            rollout_slug: "memory-slug".to_string(),
        };
        state
            .reconcile_threads(&[sample_thread(thread_id, source_updated_at)])
            .await
            .expect("reconcile thread");
        state
            .upsert_stage1_output(&output)
            .await
            .expect("insert stage1 output");

        let lease = state
            .claim_artifact_build_job(true)
            .await
            .expect("claim artifacts")
            .expect("lease");
        let selected = state
            .select_phase2_inputs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select phase2 inputs");
        state
            .succeed_artifact_build_job(&lease.ownership_token, &selected)
            .await
            .expect("succeed artifact build");
        assert!(!stage1_status(&state).await.artifact_dirty);

        let no_op = Stage1OutputInput {
            generated_at: now_epoch(),
            ..output.clone()
        };
        state
            .upsert_stage1_output(&no_op)
            .await
            .expect("noop stage1 upsert");
        assert!(!stage1_status(&state).await.artifact_dirty);

        assert!(
            !state
                .mark_thread_memory_mode(thread_id, SessionMemoryMode::Enabled)
                .await
                .expect("noop mode update")
        );
        assert!(!stage1_status(&state).await.artifact_dirty);

        let changed = Stage1OutputInput {
            raw_memory: "updated raw memory".to_string(),
            ..output
        };
        state
            .upsert_stage1_output(&changed)
            .await
            .expect("changed stage1 upsert");
        assert!(stage1_status(&state).await.artifact_dirty);

        state
            .mark_artifact_dirty()
            .await
            .expect("mark dirty before clean rebuild");
        let selected = state
            .select_phase2_inputs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select phase2 after change");
        claim_and_succeed_artifacts(&state, &selected).await;
        assert!(
            state
                .mark_thread_memory_mode(thread_id, SessionMemoryMode::Polluted)
                .await
                .expect("mark polluted")
        );
        assert!(stage1_status(&state).await.artifact_dirty);
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
            .expect("status for vscode sources");
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
    async fn artifact_leases_do_not_get_stolen_and_selected_outputs_follow_recency() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let older_id = Uuid::new_v4();
        let newer_id = Uuid::new_v4();
        state
            .reconcile_threads(&[
                sample_thread(older_id, now_epoch() - 200_000),
                sample_thread(newer_id, now_epoch() - 100_000),
            ])
            .await
            .expect("reconcile threads");
        state
            .upsert_stage1_output(&Stage1OutputInput {
                thread_id: older_id,
                source_updated_at: now_epoch() - 200_000,
                generated_at: now_epoch(),
                raw_memory: "older".to_string(),
                rollout_summary: "older summary".to_string(),
                rollout_slug: "older".to_string(),
            })
            .await
            .expect("upsert older output");
        state
            .upsert_stage1_output(&Stage1OutputInput {
                thread_id: newer_id,
                source_updated_at: now_epoch() - 100_000,
                generated_at: now_epoch(),
                raw_memory: "newer".to_string(),
                rollout_summary: "newer summary".to_string(),
                rollout_slug: "newer".to_string(),
            })
            .await
            .expect("upsert newer output");

        let selected = state
            .select_phase2_inputs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select phase2");
        let lease = state
            .claim_artifact_build_job(true)
            .await
            .expect("claim artifact build job")
            .expect("artifact lease");
        let stolen = state
            .claim_artifact_build_job(true)
            .await
            .expect("second artifact claim");
        assert!(stolen.is_none());
        state
            .succeed_artifact_build_job(&lease.ownership_token, &selected)
            .await
            .expect("succeed artifact build");

        state.record_usage(&[older_id]).await.expect("record usage");
        let selected_outputs = state
            .current_selected_outputs(INTERACTIVE_SOURCES)
            .await
            .expect("selected outputs");
        assert_eq!(selected_outputs[0].thread_id, older_id);
        assert_eq!(selected_outputs.len(), 2);

        state
            .mark_artifact_dirty()
            .await
            .expect("mark artifact dirty");
        let lease = state
            .claim_artifact_build_job(true)
            .await
            .expect("claim artifact rebuild")
            .expect("artifact rebuild lease");
        state
            .succeed_artifact_build_job(&lease.ownership_token, &[selected_outputs[1].clone()])
            .await
            .expect("succeed narrowed artifact build");
        let selected_outputs = state
            .current_selected_outputs(INTERACTIVE_SOURCES)
            .await
            .expect("selected outputs after narrowing");
        assert_eq!(selected_outputs.len(), 1);
        assert_eq!(selected_outputs[0].thread_id, newer_id);
    }

    #[tokio::test]
    async fn phase2_selection_and_selected_outputs_respect_allowed_sources() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let cli_id = Uuid::new_v4();
        let exec_id = Uuid::new_v4();
        let mut exec_thread = sample_thread(exec_id, now_epoch() - 100_000);
        exec_thread.source = SessionSource::Exec;
        state
            .reconcile_threads(&[
                sample_thread(cli_id, now_epoch() - 120_000),
                exec_thread,
            ])
            .await
            .expect("reconcile threads");
        state
            .upsert_stage1_output(&Stage1OutputInput {
                thread_id: cli_id,
                source_updated_at: now_epoch() - 120_000,
                generated_at: now_epoch(),
                raw_memory: "cli".to_string(),
                rollout_summary: "cli summary".to_string(),
                rollout_slug: "cli".to_string(),
            })
            .await
            .expect("upsert cli output");
        state
            .upsert_stage1_output(&Stage1OutputInput {
                thread_id: exec_id,
                source_updated_at: now_epoch() - 100_000,
                generated_at: now_epoch(),
                raw_memory: "exec".to_string(),
                rollout_summary: "exec summary".to_string(),
                rollout_slug: "exec".to_string(),
            })
            .await
            .expect("upsert exec output");

        let selected = state
            .select_phase2_inputs(8, 365, INTERACTIVE_SOURCES)
            .await
            .expect("select phase2");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].thread_id, cli_id);

        let pool = open_test_pool(&state.db_path()).await;
        sqlx::query(
            "UPDATE stage1_outputs SET selected_for_phase2 = 1, selected_for_phase2_source_updated_at = source_updated_at WHERE thread_id IN (?, ?)",
        )
        .bind(cli_id.to_string())
        .bind(exec_id.to_string())
        .execute(&pool)
        .await
        .expect("mark selected outputs");

        let selected_outputs = state
            .current_selected_outputs(INTERACTIVE_SOURCES)
            .await
            .expect("selected outputs");
        assert_eq!(selected_outputs.len(), 1);
        assert_eq!(selected_outputs[0].thread_id, cli_id);
    }
}
