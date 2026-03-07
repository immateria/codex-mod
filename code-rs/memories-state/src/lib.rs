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
use sqlx::{Executor, Row, SqlitePool};
use tracing::warn;
use uuid::Uuid;

const APP_ID: i64 = 1_129_136_980;
const STATE_SCHEMA_VERSION: i64 = 1;
const ARTIFACT_STATE_KEY: &str = "global";
const JOB_KIND_STAGE1: &str = "stage1";
const JOB_KIND_ARTIFACTS: &str = "artifacts";
const JOB_LEASE_SECONDS: i64 = 300;

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
    pub source_updated_at: i64,
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
    ) -> Result<Vec<Stage1Claim>>;
    async fn upsert_stage1_output(&self, output: &Stage1OutputInput) -> Result<()>;
    async fn mark_thread_memory_mode(
        &self,
        thread_id: Uuid,
        mode: SessionMemoryMode,
    ) -> Result<bool>;
    async fn select_phase2_inputs(&self, limit: usize, max_unused_days: i64) -> Result<Vec<Stage1OutputRecord>>;
    async fn claim_artifact_build_job(&self, owner: Uuid, force: bool) -> Result<Option<ArtifactBuildLease>>;
    async fn heartbeat_artifact_build_job(&self, token: &str) -> Result<bool>;
    async fn succeed_artifact_build_job(&self, token: &str, selected: &[Stage1OutputRecord]) -> Result<()>;
    async fn fail_artifact_build_job(&self, token: &str, reason: &str) -> Result<()>;
    async fn record_usage(&self, thread_ids: &[Uuid]) -> Result<()>;
    async fn current_selected_outputs(&self) -> Result<Vec<Stage1OutputRecord>>;
    async fn status(&self) -> Result<MemoriesStateStatus>;
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
    ) -> Result<Vec<Stage1Claim>> {
        self.backend
            .claim_stage1_candidates(max_claimed, max_age_days, min_rollout_idle_hours, allowed_sources)
            .await
    }

    pub async fn upsert_stage1_output(&self, output: &Stage1OutputInput) -> Result<()> {
        self.backend.upsert_stage1_output(output).await
    }

    pub async fn mark_thread_memory_mode(&self, thread_id: Uuid, mode: SessionMemoryMode) -> Result<bool> {
        self.backend.mark_thread_memory_mode(thread_id, mode).await
    }

    pub async fn select_phase2_inputs(&self, limit: usize, max_unused_days: i64) -> Result<Vec<Stage1OutputRecord>> {
        self.backend.select_phase2_inputs(limit, max_unused_days).await
    }

    pub async fn claim_artifact_build_job(&self, owner: Uuid, force: bool) -> Result<Option<ArtifactBuildLease>> {
        self.backend.claim_artifact_build_job(owner, force).await
    }

    pub async fn heartbeat_artifact_build_job(&self, token: &str) -> Result<bool> {
        self.backend.heartbeat_artifact_build_job(token).await
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

    pub async fn current_selected_outputs(&self) -> Result<Vec<Stage1OutputRecord>> {
        self.backend.current_selected_outputs().await
    }

    pub async fn status(&self) -> Result<MemoriesStateStatus> {
        self.backend.status().await
    }
}

fn db_path(code_home: &Path) -> PathBuf {
    code_home.join("memories_state.sqlite")
}

async fn apply_migrations(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await?;
    tx.execute(sqlx::query(format!("PRAGMA application_id = {APP_ID}").as_str()))
        .await?;
    let current_version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *tx)
        .await?;
    if current_version >= STATE_SCHEMA_VERSION {
        tx.commit().await?;
        return Ok(());
    }

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
    retry_after INTEGER,
    last_success_watermark INTEGER,
    last_error TEXT,
    dirty INTEGER NOT NULL DEFAULT 0,
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
    last_build_at INTEGER,
    last_selected_count INTEGER NOT NULL DEFAULT 0,
    last_success_watermark INTEGER
)
        "#,
    ))
    .await?;
    tx.execute(sqlx::query(
        "INSERT OR IGNORE INTO artifact_state (state_key, dirty, last_selected_count) VALUES (?, 1, 0)"
    )
    .bind(ARTIFACT_STATE_KEY))
    .await?;
    tx.execute(sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_threads_updated_at ON memory_threads(updated_at DESC)"))
        .await?;
    tx.execute(sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_threads_mode ON memory_threads(memory_mode, archived, deleted, source)"))
        .await?;
    tx.execute(sqlx::query("CREATE INDEX IF NOT EXISTS idx_stage1_outputs_selection ON stage1_outputs(selected_for_phase2, selected_for_phase2_source_updated_at)"))
        .await?;
    tx.execute(sqlx::query("CREATE INDEX IF NOT EXISTS idx_stage1_outputs_usage ON stage1_outputs(usage_count DESC, last_usage DESC, source_updated_at DESC)"))
        .await?;
    tx.execute(sqlx::query(format!("PRAGMA user_version = {STATE_SCHEMA_VERSION}").as_str()))
        .await?;
    tx.commit().await?;
    Ok(())
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
        for thread_id in &stale {
            sqlx::query("DELETE FROM stage1_outputs WHERE thread_id = ?")
                .bind(thread_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM memory_threads WHERE thread_id = ?")
                .bind(thread_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM memory_jobs WHERE kind = ? AND job_key = ?")
                .bind(JOB_KIND_STAGE1)
                .bind(thread_id)
                .execute(&mut *tx)
                .await?;
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
SELECT mt.thread_id, mt.rollout_path, mt.cwd, mt.cwd_display, mt.updated_at, mt.updated_at_label, mt.git_branch, mt.last_user_snippet,
       COALESCE(so.source_updated_at, -1) AS source_updated_at
FROM memory_threads mt
LEFT JOIN stage1_outputs so ON so.thread_id = mt.thread_id
WHERE mt.archived = 0
  AND mt.deleted = 0
  AND mt.memory_mode = 'enabled'
  AND mt.updated_at >= ?
  AND mt.updated_at <= ?
  AND COALESCE(so.source_updated_at, -1) < mt.updated_at
            "#,
        );
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
        let mut q = sqlx::query(query.as_str()).bind(max_age_cutoff).bind(idle_cutoff);
        for source in &allowed {
            q = q.bind(source);
        }
        let rows = q.fetch_all(&self.pool).await?;
        let lease_until = now_epoch() + JOB_LEASE_SECONDS;
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
INSERT INTO memory_jobs (kind, job_key, ownership_token, lease_until, retry_after, dirty)
VALUES (?, ?, ?, ?, 0, 0)
ON CONFLICT(kind, job_key) DO UPDATE SET
    ownership_token = excluded.ownership_token,
    lease_until = excluded.lease_until,
    retry_after = 0
WHERE memory_jobs.lease_until IS NULL OR memory_jobs.lease_until < ? OR memory_jobs.ownership_token IS NULL
                "#,
            )
            .bind(JOB_KIND_STAGE1)
            .bind(&thread_id_text)
            .bind(Uuid::new_v4().to_string())
            .bind(lease_until)
            .bind(now_epoch())
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
                source_updated_at: row.try_get("source_updated_at")?,
                git_branch: row.try_get("git_branch")?,
                last_user_snippet: row.try_get("last_user_snippet")?,
            });
        }
        tx.commit().await?;
        Ok(claims)
    }

    async fn upsert_stage1_output(&self, output: &Stage1OutputInput) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
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
            "#,
        )
        .bind(output.thread_id.to_string())
        .bind(output.source_updated_at)
        .bind(output.generated_at)
        .bind(&output.raw_memory)
        .bind(&output.rollout_summary)
        .bind(&output.rollout_slug)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE memory_jobs SET ownership_token = NULL, lease_until = NULL, last_success_watermark = ? WHERE kind = ? AND job_key = ?")
            .bind(output.source_updated_at)
            .bind(JOB_KIND_STAGE1)
            .bind(output.thread_id.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE artifact_state SET dirty = 1 WHERE state_key = ?")
            .bind(ARTIFACT_STATE_KEY)
            .execute(&mut *tx)
            .await?;
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

    async fn select_phase2_inputs(&self, limit: usize, max_unused_days: i64) -> Result<Vec<Stage1OutputRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let cutoff = if max_unused_days > 0 {
            now_epoch() - max_unused_days * 86_400
        } else {
            i64::MIN
        };
        let rows = sqlx::query(
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
  AND ((so.last_usage IS NOT NULL AND so.last_usage >= ?) OR (so.last_usage IS NULL AND so.source_updated_at >= ?))
ORDER BY so.usage_count DESC,
         COALESCE(so.last_usage, so.source_updated_at) DESC,
         so.source_updated_at DESC,
         so.thread_id DESC
LIMIT ?
            "#,
        )
        .bind(cutoff)
        .bind(cutoff)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(stage1_record_from_row).collect()
    }

    async fn claim_artifact_build_job(&self, owner: Uuid, force: bool) -> Result<Option<ArtifactBuildLease>> {
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
INSERT INTO memory_jobs (kind, job_key, ownership_token, lease_until, retry_after, dirty)
VALUES (?, ?, ?, ?, 0, 0)
ON CONFLICT(kind, job_key) DO UPDATE SET
    ownership_token = excluded.ownership_token,
    lease_until = excluded.lease_until,
    retry_after = 0
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
        let _ = owner;
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

    async fn succeed_artifact_build_job(&self, token: &str, selected: &[Stage1OutputRecord]) -> Result<()> {
        let selected_ids: HashSet<String> = selected.iter().map(|row| row.thread_id.to_string()).collect();
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE stage1_outputs SET selected_for_phase2 = 0, selected_for_phase2_source_updated_at = NULL")
            .execute(&mut *tx)
            .await?;
        for row in selected {
            sqlx::query(
                "UPDATE stage1_outputs SET selected_for_phase2 = 1, selected_for_phase2_source_updated_at = ? WHERE thread_id = ?"
            )
            .bind(row.source_updated_at)
            .bind(row.thread_id.to_string())
            .execute(&mut *tx)
            .await?;
        }
        let watermark = selected.iter().map(|row| row.source_updated_at).max();
        sqlx::query(
            "UPDATE artifact_state SET dirty = 0, last_build_at = ?, last_selected_count = ?, last_success_watermark = ? WHERE state_key = ?"
        )
        .bind(now_epoch())
        .bind(selected.len() as i64)
        .bind(watermark)
        .bind(ARTIFACT_STATE_KEY)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE memory_jobs SET ownership_token = NULL, lease_until = NULL, last_success_watermark = ?, dirty = 0, last_error = NULL WHERE kind = ? AND job_key = ? AND ownership_token = ?"
        )
        .bind(watermark)
        .bind(JOB_KIND_ARTIFACTS)
        .bind(ARTIFACT_STATE_KEY)
        .bind(token)
        .execute(&mut *tx)
        .await?;
        let _ = selected_ids;
        tx.commit().await?;
        Ok(())
    }

    async fn fail_artifact_build_job(&self, token: &str, reason: &str) -> Result<()> {
        sqlx::query(
            "UPDATE memory_jobs SET ownership_token = NULL, lease_until = NULL, dirty = 1, last_error = ? WHERE kind = ? AND job_key = ? AND ownership_token = ?"
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

    async fn current_selected_outputs(&self) -> Result<Vec<Stage1OutputRecord>> {
        let rows = sqlx::query(
            r#"
SELECT
    so.thread_id,
    mt.rollout_path,
    mt.cwd,
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
ORDER BY COALESCE(so.last_usage, so.source_updated_at) DESC, so.thread_id DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(stage1_record_from_row).collect()
    }

    async fn status(&self) -> Result<MemoriesStateStatus> {
        let thread_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_threads")
            .fetch_one(&self.pool)
            .await?;
        let stage1_output_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stage1_outputs")
            .fetch_one(&self.pool)
            .await?;
        let running_stage1_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory_jobs WHERE kind = ? AND lease_until IS NOT NULL AND lease_until >= ?"
        )
        .bind(JOB_KIND_STAGE1)
        .bind(now_epoch())
        .fetch_one(&self.pool)
        .await?;
        let pending_stage1_count: i64 = sqlx::query_scalar(
            r#"
SELECT COUNT(*)
FROM memory_threads mt
LEFT JOIN stage1_outputs so ON so.thread_id = mt.thread_id
WHERE mt.archived = 0
  AND mt.deleted = 0
  AND mt.memory_mode = 'enabled'
  AND COALESCE(so.source_updated_at, -1) < mt.updated_at
            "#,
        )
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
        .bind(now_epoch())
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
    use tempfile::tempdir;

    use super::*;

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

        let status = state.status().await.expect("status");
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
        let status = state.status().await.expect("status after prune");
        assert_eq!(status.thread_count, 1);
    }

    #[tokio::test]
    async fn usage_and_selection_round_trip() {
        let temp = tempdir().expect("tempdir");
        let state = MemoriesState::open(temp.path()).await.expect("open state");
        let thread_id = Uuid::new_v4();
        let thread = sample_thread(thread_id, now_epoch() - 172_800);
        state
            .reconcile_threads(&[thread])
            .await
            .expect("reconcile thread");
        state
            .upsert_stage1_output(&Stage1OutputInput {
                thread_id,
                source_updated_at: now_epoch() - 172_800,
                generated_at: now_epoch(),
                raw_memory: "raw memory".to_string(),
                rollout_summary: "rollout summary".to_string(),
                rollout_slug: "memory-slug".to_string(),
            })
            .await
            .expect("upsert stage1 output");

        let selected = state
            .select_phase2_inputs(8, 365)
            .await
            .expect("select phase2 inputs");
        assert_eq!(selected.len(), 1);

        let lease = state
            .claim_artifact_build_job(Uuid::new_v4(), true)
            .await
            .expect("claim artifacts")
            .expect("lease");
        state
            .succeed_artifact_build_job(&lease.ownership_token, &selected)
            .await
            .expect("succeed artifact build");

        state.record_usage(&[thread_id]).await.expect("record usage");
        let selected = state.current_selected_outputs().await.expect("selected outputs");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].usage_count, 1);
    }
}
