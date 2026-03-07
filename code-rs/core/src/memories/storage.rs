use std::collections::HashMap;
use std::io;
use std::path::Path;

#[cfg(test)]
use std::fmt::Write;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use code_memories_state::{MemoryThread, MemoriesState, SessionMemoryMode as StateMemoryMode, Stage1Claim, Stage1OutputInput, Stage1OutputRecord};
use code_protocol::models::{ContentItem, ResponseItem};
use code_protocol::protocol::RolloutItem;
use tracing::warn;
use uuid::Uuid;

use crate::config_types::MemoriesConfig;
use crate::rollout::catalog::SessionCatalog;
use crate::rollout::catalog::SessionIndexEntry;
use crate::rollout::catalog::SessionMemoryMode;
use crate::rollout::RolloutRecorder;

use super::ensure_layout;
use super::current_generation_path;
use super::generation_snapshot_dir;
use super::memory_root;
use super::snapshot_memory_summary_path;
use super::snapshot_raw_memories_path;
use super::snapshot_rollout_summaries_dir;
#[cfg(test)]
use super::published_artifact_paths;

#[derive(Debug, Clone)]
struct MemoryArtifacts {
    memory_summary: String,
    raw_memories: String,
    rollout_summaries: HashMap<String, String>,
}

#[cfg(test)]
static FAIL_BEFORE_POINTER_SWAP: AtomicBool = AtomicBool::new(false);

pub(crate) fn to_state_memory_mode(mode: SessionMemoryMode) -> StateMemoryMode {
    match mode {
        SessionMemoryMode::Enabled => StateMemoryMode::Enabled,
        SessionMemoryMode::Disabled => StateMemoryMode::Disabled,
        SessionMemoryMode::Polluted => StateMemoryMode::Polluted,
    }
}

pub(crate) async fn refresh_memory_artifacts_from_catalog(
    code_home: &Path,
    settings: &MemoriesConfig,
    force_refresh: bool,
) -> io::Result<()> {
    // This is still an in-process sequential orchestrator over DB-backed
    // leases, not a background worker. A forced refresh also bypasses any
    // stage1 retry backoff so explicit user actions retry immediately.
    let state = super::open_memories_state(code_home).await?;
    let threads = load_memory_threads(code_home).await?;
    state
        .reconcile_threads(&threads)
        .await
        .map_err(io::Error::other)?;

    let claims = state
        .claim_stage1_candidates(
            settings.max_rollouts_per_startup,
            settings.max_rollout_age_days,
            settings.min_rollout_idle_hours,
            crate::rollout::INTERACTIVE_SESSION_SOURCES,
            force_refresh,
        )
        .await
        .map_err(io::Error::other)?;

    for claim in claims {
        match build_stage1_output(code_home, &claim).await {
            Ok(output) => {
                if let Err(err) = state.upsert_stage1_output(&output).await {
                    let _ = state.fail_stage1_job(claim.thread_id, &err.to_string()).await;
                    warn!(
                        "failed to persist stage1 output for {}: {err}",
                        claim.thread_id
                    );
                }
            }
            Err(err) => {
                let _ = state.fail_stage1_job(claim.thread_id, &err.to_string()).await;
                warn!("failed to extract stage1 output for {}: {err}", claim.thread_id);
            }
        }
    }

    maybe_build_artifacts_from_state(code_home, settings, &state, force_refresh).await
}

async fn load_memory_threads(code_home: &Path) -> io::Result<Vec<MemoryThread>> {
    let code_home = code_home.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let catalog = SessionCatalog::load(&code_home)?;
        Ok::<_, io::Error>(
            catalog
                .all_ordered()
                .into_iter()
                .filter_map(memory_thread_from_entry)
                .collect(),
        )
    })
    .await
    .map_err(|err| io::Error::other(format!("memory thread load join failed: {err}")))?
}

fn memory_thread_from_entry(entry: &SessionIndexEntry) -> Option<MemoryThread> {
    let (updated_at, updated_at_label) = parse_timestamp_with_label(entry)?;
    Some(MemoryThread {
        thread_id: entry.session_id,
        rollout_path: entry.rollout_path.clone(),
        source: entry.session_source.clone(),
        cwd: entry.cwd_real.clone(),
        cwd_display: entry.cwd_display.clone(),
        updated_at: updated_at.timestamp(),
        updated_at_label,
        archived: entry.archived,
        deleted: entry.deleted,
        memory_mode: to_state_memory_mode(entry.memory_mode),
        catalog_seen_at: Utc::now().timestamp(),
        git_branch: entry.git_branch.clone(),
        last_user_snippet: entry.last_user_snippet.clone(),
    })
}

async fn build_stage1_output(
    code_home: &Path,
    claim: &Stage1Claim,
) -> io::Result<Stage1OutputInput> {
    let rollout_path = code_home.join(&claim.rollout_path);
    let last_user_snippet = match RolloutRecorder::get_rollout_history(&rollout_path).await {
        Ok(history) => extract_last_user_snippet(&history.get_rollout_items())
            .or_else(|| claim.last_user_snippet.clone()),
        Err(err) if claim.last_user_snippet.is_some() => {
            warn!(
                "falling back to catalog snippet for {} after rollout read failed: {err}",
                claim.thread_id
            );
            claim.last_user_snippet.clone()
        }
        Err(err) => return Err(err),
    }
    .unwrap_or_else(|| "(no user snippet)".to_string());
    let rollout_slug = rollout_summary_file_stem(
        claim.thread_id,
        claim.updated_at,
        &claim.cwd_display,
        claim.git_branch.as_deref(),
    );

    Ok(Stage1OutputInput {
        thread_id: claim.thread_id,
        source_updated_at: claim.updated_at,
        generated_at: Utc::now().timestamp(),
        raw_memory: render_raw_memory_body(claim, &rollout_slug, &last_user_snippet),
        rollout_summary: render_rollout_summary_body(claim, &last_user_snippet),
        rollout_slug,
    })
}

fn render_raw_memory_body(claim: &Stage1Claim, rollout_slug: &str, snippet: &str) -> String {
    let mut body = String::new();
    body.push_str(&format!("updated_at: {}\n", iso_timestamp(claim.updated_at)));
    body.push_str(&format!("cwd: {}\n", claim.cwd_display));
    body.push_str(&format!("rollout_path: {}\n", claim.rollout_path.display()));
    body.push_str(&format!("rollout_summary_file: {rollout_slug}.md\n"));
    if let Some(git_branch) = claim.git_branch.as_deref() {
        body.push_str(&format!("git_branch: {git_branch}\n"));
    }
    body.push('\n');
    body.push_str(snippet);
    body.push('\n');
    body
}

fn render_rollout_summary_body(claim: &Stage1Claim, snippet: &str) -> String {
    let mut body = String::new();
    body.push_str(&format!("session_id: {}\n", claim.thread_id));
    body.push_str(&format!("updated_at: {}\n", iso_timestamp(claim.updated_at)));
    body.push_str(&format!("rollout_path: {}\n", claim.rollout_path.display()));
    body.push_str(&format!("cwd: {}\n", claim.cwd_display));
    if let Some(git_branch) = claim.git_branch.as_deref() {
        body.push_str(&format!("git_branch: {git_branch}\n"));
    }
    body.push('\n');
    body.push_str(snippet);
    body.push('\n');
    body
}

async fn maybe_build_artifacts_from_state(
    code_home: &Path,
    settings: &MemoriesConfig,
    state: &MemoriesState,
    force_artifact_build: bool,
) -> io::Result<()> {
    let Some(lease) = state
        .claim_artifact_build_job(force_artifact_build)
        .await
        .map_err(io::Error::other)?
    else {
        return Ok(());
    };

    let selected = state
        .select_phase2_inputs(
            settings.max_raw_memories_for_consolidation,
            settings.max_rollout_age_days,
            crate::rollout::INTERACTIVE_SESSION_SOURCES,
        )
        .await
        .map_err(io::Error::other)?;

    let artifacts = render_artifacts_from_state(&selected);
    if let Err(err) = write_memory_artifacts(code_home, artifacts).await {
        let _ = state
            .fail_artifact_build_job(&lease.ownership_token, &err.to_string())
            .await;
        return Err(err);
    }

    state
        .succeed_artifact_build_job(&lease.ownership_token, &selected)
        .await
        .map_err(io::Error::other)
}

fn render_artifacts_from_state(selected: &[Stage1OutputRecord]) -> MemoryArtifacts {
    MemoryArtifacts {
        memory_summary: render_memory_summary(selected),
        raw_memories: render_raw_memories(selected),
        rollout_summaries: render_rollout_summaries(selected),
    }
}

async fn write_memory_artifacts(code_home: &Path, artifacts: MemoryArtifacts) -> io::Result<()> {
    let memory_root = memory_root(code_home);
    let generation = format!(
        "{}-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        Uuid::new_v4()
    );
    let snapshot_dir = generation_snapshot_dir(code_home, &generation);
    let pointer_tmp_path = memory_root.join(format!("current.{generation}.tmp"));
    let current_path = current_generation_path(code_home);

    ensure_layout(code_home).await?;
    tokio::fs::create_dir_all(snapshot_rollout_summaries_dir(&snapshot_dir)).await?;
    tokio::fs::write(snapshot_memory_summary_path(&snapshot_dir), artifacts.memory_summary).await?;
    tokio::fs::write(snapshot_raw_memories_path(&snapshot_dir), artifacts.raw_memories).await?;
    sync_rollout_summaries(&snapshot_dir, artifacts.rollout_summaries).await?;
    #[cfg(test)]
    maybe_fail_before_pointer_swap()?;
    tokio::fs::write(&pointer_tmp_path, format!("{generation}\n")).await?;
    tokio::fs::rename(&pointer_tmp_path, &current_path).await?;
    if let Err(err) = super::control::remove_legacy_artifacts(&memory_root).await {
        warn!("failed to remove legacy memories artifacts after publish: {err}");
    }
    if let Err(err) = super::control::prune_noncurrent_snapshots(&memory_root, &generation).await {
        warn!("failed to prune stale memories snapshots after publish: {err}");
    }
    Ok(())
}

#[cfg(test)]
fn maybe_fail_before_pointer_swap() -> io::Result<()> {
    if FAIL_BEFORE_POINTER_SWAP.swap(false, Ordering::SeqCst) {
        return Err(io::Error::other("injected memories publish failure"));
    }
    Ok(())
}

fn render_memory_summary(selected: &[Stage1OutputRecord]) -> String {
    let mut body = String::from("# Memory Summary\n\n");
    if selected.is_empty() {
        body.push_str("No prior interactive sessions found.\n");
        return body;
    }

    body.push_str("Recent interactive sessions retained for memory prompts:\n\n");
    for record in selected {
        body.push_str(&format!("## {} | {}\n", record.updated_at_label, record.thread_id));
        body.push_str(&format!("cwd: {}\n", record.cwd_display));
        if let Some(git_branch) = record.git_branch.as_deref() {
            body.push_str(&format!("git_branch: {git_branch}\n"));
        }
        body.push_str(&format!(
            "rollout_summary_file: rollout_summaries/{}.md\n",
            record.rollout_slug
        ));
        body.push_str(&format!(
            "last_user_request: {}\n\n",
            summary_snippet_from_rollout_summary(&record.rollout_summary)
        ));
    }
    body
}

fn render_raw_memories(selected: &[Stage1OutputRecord]) -> String {
    let mut body = String::from("# Raw Memories\n\n");
    if selected.is_empty() {
        body.push_str("No raw memories yet.\n");
        return body;
    }

    body.push_str("Catalog-derived retained memories (latest first):\n\n");
    for record in selected {
        body.push_str(&format!("## Session `{}`\n", record.thread_id));
        body.push_str(&record.raw_memory);
        body.push('\n');
    }
    body
}

fn render_rollout_summaries(selected: &[Stage1OutputRecord]) -> HashMap<String, String> {
    selected
        .iter()
        .map(|record| (record.rollout_slug.clone(), record.rollout_summary.clone()))
        .collect()
}

async fn sync_rollout_summaries(
    snapshot_dir: &Path,
    summaries: HashMap<String, String>,
) -> io::Result<()> {
    let dir = snapshot_rollout_summaries_dir(snapshot_dir);
    for (stem, body) in summaries {
        tokio::fs::write(dir.join(format!("{stem}.md")), body).await?;
    }
    Ok(())
}

fn extract_last_user_snippet(items: &[RolloutItem]) -> Option<String> {
    for item in items.iter().rev() {
        if let RolloutItem::ResponseItem(ResponseItem::Message { role, content, .. }) = item
            && role.eq_ignore_ascii_case("user")
            && let Some(snippet) = snippet_from_content(content)
        {
            if is_system_status_snippet(&snippet) {
                continue;
            }
            return Some(snippet);
        }
    }
    None
}

fn snippet_from_content(content: &[ContentItem]) -> Option<String> {
    content.iter().find_map(|item| match item {
        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
            Some(text.chars().take(100).collect())
        }
        _ => None,
    })
}

fn is_system_status_snippet(text: &str) -> bool {
    text.starts_with("== System Status ==")
}

fn parse_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn iso_timestamp(timestamp: i64) -> String {
    DateTime::<Utc>::from_timestamp_secs(timestamp)
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| timestamp.to_string())
}

fn parse_timestamp_with_label(entry: &SessionIndexEntry) -> Option<(DateTime<Utc>, String)> {
    if let Some(parsed) = parse_timestamp(&entry.last_event_at) {
        return Some((parsed, entry.last_event_at.clone()));
    }
    if let Some(parsed) = parse_timestamp(&entry.created_at) {
        return Some((parsed, entry.created_at.clone()));
    }
    None
}

fn last_nonempty_line(text: &str) -> Option<&str> {
    text.lines().rev().find_map(|line| {
        let trimmed = line.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn summary_snippet_from_rollout_summary(rollout_summary: &str) -> &str {
    // Stage1 rollout summaries currently end with the retained user snippet as
    // their final non-empty line. Keep the markdown format stable and lock this
    // extraction rule with tests instead of adding duplicate snippet storage.
    last_nonempty_line(rollout_summary).unwrap_or("(no user snippet)")
}

fn rollout_summary_file_stem(
    session_id: Uuid,
    updated_at: i64,
    cwd_display: &str,
    git_branch: Option<&str>,
) -> String {
    const SLUG_MAX_LEN: usize = 48;
    const SHORT_HASH_ALPHABET: &[u8; 62] =
        b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const SHORT_HASH_SPACE: u32 = 14_776_336;

    let timestamp_fragment = DateTime::<Utc>::from_timestamp_secs(updated_at)
        .unwrap_or_else(Utc::now)
        .format("%Y-%m-%dT%H-%M-%S")
        .to_string();
    let mut short_hash_value = (session_id.as_u128() & 0xFFFF_FFFF) as u32 % SHORT_HASH_SPACE;
    let mut short_hash_chars = ['0'; 4];
    for idx in (0..short_hash_chars.len()).rev() {
        let alphabet_idx = (short_hash_value % SHORT_HASH_ALPHABET.len() as u32) as usize;
        short_hash_chars[idx] = SHORT_HASH_ALPHABET[alphabet_idx] as char;
        short_hash_value /= SHORT_HASH_ALPHABET.len() as u32;
    }
    let short_hash: String = short_hash_chars.iter().collect();
    let prefix = format!("{timestamp_fragment}-{short_hash}");
    let slug = rollout_summary_slug(cwd_display, git_branch, SLUG_MAX_LEN);
    if slug.is_empty() {
        prefix
    } else {
        format!("{prefix}-{slug}")
    }
}

fn rollout_summary_slug(cwd_display: &str, git_branch: Option<&str>, max_len: usize) -> String {
    let raw = git_branch
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            Path::new(cwd_display)
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or("");

    let mut slug = String::with_capacity(max_len);
    for ch in raw.chars() {
        if slug.len() >= max_len {
            break;
        }
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else {
            slug.push('_');
        }
    }
    while slug.ends_with('_') {
        slug.pop();
    }
    slug
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use code_protocol::protocol::SessionSource;
    use tempfile::tempdir;

    use super::*;

    #[derive(Debug, Clone)]
    struct LegacySelectedMemory {
        session_id: Uuid,
        rollout_path: PathBuf,
        last_event_at: DateTime<Utc>,
        last_event_at_label: String,
        cwd_display: String,
        git_branch: Option<String>,
        last_user_snippet: Option<String>,
        summary_file_stem: String,
    }

    fn make_entry(
        session_id: Uuid,
        source: SessionSource,
        last_event_at: &str,
        archived: bool,
        deleted: bool,
        snippet: Option<&str>,
    ) -> SessionIndexEntry {
        SessionIndexEntry {
            session_id,
            rollout_path: PathBuf::from(format!("sessions/{session_id}.jsonl")),
            snapshot_path: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_event_at: last_event_at.to_string(),
            cwd_real: PathBuf::from("/tmp/project"),
            cwd_display: "~/project".to_string(),
            git_project_root: None,
            git_branch: Some("main".to_string()),
            model_provider: None,
            session_source: source,
            message_count: 1,
            user_message_count: 1,
            last_user_snippet: snippet.map(ToString::to_string),
            nickname: None,
            sync_origin_device: None,
            sync_version: 0,
            archived,
            deleted,
            memory_mode: SessionMemoryMode::Enabled,
        }
    }

    fn rollout_rel_path(session_id: Uuid, meta_at: DateTime<Utc>) -> PathBuf {
        PathBuf::from(format!(
            "sessions/{}/{}/{}/rollout-{}-{session_id}.jsonl",
            meta_at.format("%Y"),
            meta_at.format("%m"),
            meta_at.format("%d"),
            meta_at.format("%Y-%m-%dT%H-%M-%S"),
        ))
    }

    async fn write_rollout_with_user_messages(
        code_home: &Path,
        session_id: Uuid,
        meta_at: DateTime<Utc>,
        messages: &[(DateTime<Utc>, &str)],
    ) -> io::Result<PathBuf> {
        let rollout_rel_path = rollout_rel_path(session_id, meta_at);
        let rollout_path = code_home.join(&rollout_rel_path);
        if let Some(parent) = rollout_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let rollout_line = serde_json::json!({
            "timestamp": meta_at.to_rfc3339(),
            "item": {
                "SessionMeta": {
                    "meta": {
                        "id": session_id,
                        "forked_from_id": null,
                        "timestamp": meta_at.to_rfc3339(),
                        "cwd": "/tmp/project",
                        "originator": "codex_cli_rs",
                        "cli_version": "0.0.0",
                        "source": "cli",
                        "model_provider": null,
                        "base_instructions": null,
                        "dynamic_tools": null
                    },
                    "git": {
                        "commit_hash": null,
                        "branch": "main",
                        "repository_url": null
                    }
                }
            }
        });

        let mut body = format!("{rollout_line}\n");
        for (timestamp, text) in messages {
            let user_line = serde_json::json!({
                "timestamp": timestamp.to_rfc3339(),
                "item": {
                    "ResponseItem": {
                        "Message": {
                            "id": null,
                            "role": "user",
                            "content": [{ "InputText": { "text": text } }],
                            "end_turn": null,
                            "phase": null
                        }
                    }
                }
            });
            body.push_str(&format!("{user_line}\n"));
        }

        tokio::fs::write(&rollout_path, body).await?;
        Ok(rollout_rel_path)
    }

    fn legacy_select_memories_from_catalog(
        catalog: &SessionCatalog,
        settings: &MemoriesConfig,
        now: DateTime<Utc>,
    ) -> Vec<LegacySelectedMemory> {
        let limit = settings
            .max_raw_memories_for_consolidation
            .min(settings.max_rollouts_per_startup);
        if limit == 0 {
            return Vec::new();
        }

        let max_age = chrono::TimeDelta::days(settings.max_rollout_age_days);
        let min_idle = chrono::TimeDelta::hours(settings.min_rollout_idle_hours);

        let mut selected = Vec::with_capacity(limit);
        for entry in catalog.all_ordered() {
            if selected.len() >= limit {
                break;
            }
            if entry.deleted
                || entry.archived
                || entry.memory_mode != SessionMemoryMode::Enabled
                || !matches!(entry.session_source, SessionSource::Cli | SessionSource::VSCode)
            {
                continue;
            }
            let Some((last_event_at, last_event_at_label)) = parse_timestamp_with_label(entry) else {
                continue;
            };
            if settings.max_rollout_age_days > 0 && now.signed_duration_since(last_event_at) > max_age {
                continue;
            }
            if settings.min_rollout_idle_hours > 0 && now.signed_duration_since(last_event_at) < min_idle {
                continue;
            }
            selected.push(LegacySelectedMemory {
                session_id: entry.session_id,
                rollout_path: entry.rollout_path.clone(),
                last_event_at,
                last_event_at_label,
                cwd_display: entry.cwd_display.clone(),
                git_branch: entry.git_branch.clone(),
                last_user_snippet: entry.last_user_snippet.clone(),
                summary_file_stem: rollout_summary_file_stem(
                    entry.session_id,
                    last_event_at.timestamp(),
                    &entry.cwd_display,
                    entry.git_branch.as_deref(),
                ),
            });
        }
        selected
    }

    fn legacy_render_artifacts(selected: &[LegacySelectedMemory]) -> io::Result<MemoryArtifacts> {
        let mut memory_summary = String::from("# Memory Summary\n\n");
        if selected.is_empty() {
            memory_summary.push_str("No prior interactive sessions found.\n");
        } else {
            memory_summary.push_str("Recent interactive sessions retained for memory prompts:\n\n");
            for memory in selected {
                writeln!(memory_summary, "## {} | {}", memory.last_event_at_label, memory.session_id)
                    .map_err(io::Error::other)?;
                writeln!(memory_summary, "cwd: {}", memory.cwd_display).map_err(io::Error::other)?;
                if let Some(git_branch) = memory.git_branch.as_deref() {
                    writeln!(memory_summary, "git_branch: {git_branch}").map_err(io::Error::other)?;
                }
                writeln!(
                    memory_summary,
                    "rollout_summary_file: rollout_summaries/{}.md",
                    memory.summary_file_stem
                )
                .map_err(io::Error::other)?;
                writeln!(
                    memory_summary,
                    "last_user_request: {}",
                    memory
                        .last_user_snippet
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("(no user snippet)")
                )
                .map_err(io::Error::other)?;
                writeln!(memory_summary).map_err(io::Error::other)?;
            }
        }

        let mut raw_memories = String::from("# Raw Memories\n\n");
        if selected.is_empty() {
            raw_memories.push_str("No raw memories yet.\n");
        } else {
            raw_memories.push_str("Catalog-derived retained memories (latest first):\n\n");
            for memory in selected {
                writeln!(raw_memories, "## Session `{}`", memory.session_id).map_err(io::Error::other)?;
                writeln!(raw_memories, "updated_at: {}", memory.last_event_at.to_rfc3339())
                    .map_err(io::Error::other)?;
                writeln!(raw_memories, "cwd: {}", memory.cwd_display).map_err(io::Error::other)?;
                writeln!(raw_memories, "rollout_path: {}", memory.rollout_path.display())
                    .map_err(io::Error::other)?;
                writeln!(raw_memories, "rollout_summary_file: {}.md", memory.summary_file_stem)
                    .map_err(io::Error::other)?;
                if let Some(git_branch) = memory.git_branch.as_deref() {
                    writeln!(raw_memories, "git_branch: {git_branch}").map_err(io::Error::other)?;
                }
                writeln!(raw_memories).map_err(io::Error::other)?;
                raw_memories.push_str(
                    memory
                        .last_user_snippet
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("(no user snippet)"),
                );
                raw_memories.push_str("\n\n");
            }
        }

        let mut rollout_summaries = HashMap::with_capacity(selected.len());
        for memory in selected {
            let mut body = String::new();
            writeln!(body, "session_id: {}", memory.session_id).map_err(io::Error::other)?;
            writeln!(body, "updated_at: {}", memory.last_event_at.to_rfc3339())
                .map_err(io::Error::other)?;
            writeln!(body, "rollout_path: {}", memory.rollout_path.display()).map_err(io::Error::other)?;
            writeln!(body, "cwd: {}", memory.cwd_display).map_err(io::Error::other)?;
            if let Some(git_branch) = memory.git_branch.as_deref() {
                writeln!(body, "git_branch: {git_branch}").map_err(io::Error::other)?;
            }
            writeln!(body).map_err(io::Error::other)?;
            body.push_str(
                memory
                    .last_user_snippet
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("(no user snippet)"),
            );
            body.push('\n');
            rollout_summaries.insert(memory.summary_file_stem.clone(), body);
        }

        Ok(MemoryArtifacts {
            memory_summary,
            raw_memories,
            rollout_summaries,
        })
    }

    async fn build_state_artifacts(
        code_home: &Path,
        settings: &MemoriesConfig,
    ) -> io::Result<MemoryArtifacts> {
        let state = MemoriesState::open(code_home.to_path_buf())
            .await
            .map_err(io::Error::other)?;
        let threads = load_memory_threads(code_home).await?;
        state
            .reconcile_threads(&threads)
            .await
            .map_err(io::Error::other)?;
        let claims = state
            .claim_stage1_candidates(
                settings.max_rollouts_per_startup,
                settings.max_rollout_age_days,
                settings.min_rollout_idle_hours,
                crate::rollout::INTERACTIVE_SESSION_SOURCES,
                false,
            )
            .await
            .map_err(io::Error::other)?;
        for claim in claims {
            let output = build_stage1_output(code_home, &claim).await?;
            state.upsert_stage1_output(&output).await.map_err(io::Error::other)?;
        }
        let selected = state
            .select_phase2_inputs(
                settings.max_raw_memories_for_consolidation,
                settings.max_rollout_age_days,
                crate::rollout::INTERACTIVE_SESSION_SOURCES,
            )
            .await
            .map_err(io::Error::other)?;
        Ok(render_artifacts_from_state(&selected))
    }

    #[tokio::test]
    async fn refresh_prunes_stale_rollout_summary_files_and_writes_all_artifacts() {
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let memories_root = code_home.join("memories");
        tokio::fs::create_dir_all(memories_root.join("rollout_summaries"))
            .await
            .expect("create rollout summaries dir");
        tokio::fs::write(
            memories_root.join("rollout_summaries").join("stale.md"),
            "stale",
        )
        .await
        .expect("write stale file");

        let session_id = Uuid::parse_str("0194f5a6-89ab-7cde-8123-456789abcdef").expect("uuid");
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let meta_at = now - chrono::TimeDelta::hours(6);
        let user_at = now - chrono::TimeDelta::hours(5);
        let last_event_at = user_at.to_rfc3339();
        write_rollout_with_user_messages(
            code_home,
            session_id,
            meta_at,
            &[(user_at, "remember this")],
        )
        .await
        .expect("write rollout");

        let mut catalog = SessionCatalog::load(code_home).expect("load catalog");
        catalog.entries.insert(
            session_id,
            make_entry(
                session_id,
                SessionSource::Cli,
                &last_event_at,
                false,
                false,
                Some("remember this"),
            ),
        );
        catalog.save().expect("save catalog");

        let settings = MemoriesConfig {
            max_raw_memories_for_consolidation: 8,
            max_rollout_age_days: 60,
            max_rollouts_per_startup: 8,
            min_rollout_idle_hours: 0,
            ..MemoriesConfig::default()
        };
        super::refresh_memory_artifacts_from_catalog(code_home, &settings, true)
            .await
            .expect("refresh memories");

        let published = published_artifact_paths(code_home).expect("published artifact paths");
        assert!(tokio::fs::try_exists(memories_root.join("current")).await.expect("stat current pointer"));
        assert!(published.generation.is_some());
        let summary = tokio::fs::read_to_string(&published.summary_path)
            .await
            .expect("read summary");
        assert!(summary.contains("remember this"));

        let raw = tokio::fs::read_to_string(&published.raw_memories_path)
            .await
            .expect("read raw memories");
        assert!(raw.contains("remember this"));

        let mut rollout_dir = tokio::fs::read_dir(&published.rollout_summaries_dir)
            .await
            .expect("read rollout summaries");
        let mut file_names = Vec::new();
        while let Some(entry) = rollout_dir.next_entry().await.expect("next entry") {
            file_names.push(entry.file_name().to_string_lossy().to_string());
        }

        assert_eq!(file_names.len(), 1);
        assert!(file_names[0].ends_with(".md"));
        assert_ne!(file_names[0], "stale.md");
        assert!(
            !tokio::fs::try_exists(memories_root.join("rollout_summaries"))
                .await
                .expect("stat legacy rollout summaries")
        );
    }

    #[tokio::test]
    async fn manual_refresh_bypasses_stage1_retry_backoff() {
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let session_id = Uuid::parse_str("0194f5a6-89ab-7cde-8123-456789abcdef").expect("uuid");
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let meta_at = now - chrono::TimeDelta::hours(6);
        let user_at = now - chrono::TimeDelta::hours(5);
        let last_event_at = user_at.to_rfc3339();
        let rollout_rel_path = rollout_rel_path(session_id, meta_at);

        let mut catalog = SessionCatalog::load(code_home).expect("load catalog");
        let mut entry = make_entry(session_id, SessionSource::Cli, &last_event_at, false, false, None);
        entry.rollout_path = rollout_rel_path.clone();
        catalog.entries.insert(session_id, entry);
        catalog.save().expect("save catalog");

        let settings = MemoriesConfig {
            max_raw_memories_for_consolidation: 8,
            max_rollout_age_days: 60,
            max_rollouts_per_startup: 8,
            min_rollout_idle_hours: 0,
            ..MemoriesConfig::default()
        };

        super::refresh_memory_artifacts_from_catalog(code_home, &settings, false)
            .await
            .expect("initial refresh completes despite stage1 failure");

        let state = MemoriesState::open(code_home).await.expect("open state");
        let status = state
            .status(crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("status after failed stage1");
        assert_eq!(status.stage1_output_count, 0);
        assert_eq!(status.pending_stage1_count, 0);

        let written_rollout_path = write_rollout_with_user_messages(
            code_home,
            session_id,
            meta_at,
            &[(user_at, "remember this")],
        )
            .await
            .expect("write rollout");
        assert_eq!(written_rollout_path, rollout_rel_path);
        let mut catalog = SessionCatalog::load(code_home).expect("reload catalog");
        let entry = catalog
            .entries
            .get_mut(&session_id)
            .expect("catalog entry after failure");
        entry.last_user_snippet = Some("remember this".to_string());
        catalog.save().expect("save catalog snippet");

        super::refresh_memory_artifacts_from_catalog(code_home, &settings, true)
            .await
            .expect("manual refresh bypasses backoff");

        let status = state
            .status(crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("status after manual refresh");
        assert_eq!(status.stage1_output_count, 1);

        let published = published_artifact_paths(code_home).expect("published artifact paths");
        let summary = tokio::fs::read_to_string(&published.summary_path)
            .await
            .expect("read memory summary");
        assert!(summary.contains("remember this"));
    }

    #[tokio::test]
    async fn publish_switches_active_snapshot_without_clearing_previous_one_first() {
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let memories_root = code_home.join("memories");
        let first = MemoryArtifacts {
            memory_summary: "first summary".to_string(),
            raw_memories: "first raw".to_string(),
            rollout_summaries: HashMap::from([("first".to_string(), "first rollout".to_string())]),
        };
        write_memory_artifacts(code_home, first)
            .await
            .expect("write first snapshot");
        let first_paths = published_artifact_paths(code_home).expect("first published paths");
        let first_generation = first_paths.generation.clone().expect("first generation");
        assert_eq!(
            tokio::fs::read_to_string(&first_paths.summary_path)
                .await
                .expect("read first summary"),
            "first summary"
        );

        let second = MemoryArtifacts {
            memory_summary: "second summary".to_string(),
            raw_memories: "second raw".to_string(),
            rollout_summaries: HashMap::from([("second".to_string(), "second rollout".to_string())]),
        };
        write_memory_artifacts(code_home, second)
            .await
            .expect("write second snapshot");

        let second_paths = published_artifact_paths(code_home).expect("second published paths");
        assert_eq!(
            tokio::fs::read_to_string(&second_paths.summary_path)
                .await
                .expect("read second summary"),
            "second summary"
        );
        assert_ne!(
            second_paths.generation.as_deref(),
            Some(first_generation.as_str())
        );
        assert!(
            !tokio::fs::try_exists(
                memories_root.join("snapshots").join(first_generation)
            )
            .await
            .expect("stat pruned snapshot")
        );
    }

    #[tokio::test]
    async fn failed_publish_before_pointer_swap_keeps_previous_snapshot_active() {
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        write_memory_artifacts(
            code_home,
            MemoryArtifacts {
                memory_summary: "stable summary".to_string(),
                raw_memories: "stable raw".to_string(),
                rollout_summaries: HashMap::new(),
            },
        )
        .await
        .expect("write stable snapshot");
        let stable_paths = published_artifact_paths(code_home).expect("stable published paths");
        let stable_generation = stable_paths.generation.clone().expect("stable generation");
        FAIL_BEFORE_POINTER_SWAP.store(true, Ordering::SeqCst);

        let err = write_memory_artifacts(
            code_home,
            MemoryArtifacts {
                memory_summary: "new summary".to_string(),
                raw_memories: "new raw".to_string(),
                rollout_summaries: HashMap::new(),
            },
        )
        .await
        .expect_err("publish should fail before pointer swap");
        assert_eq!(err.kind(), io::ErrorKind::Other);

        let published = published_artifact_paths(code_home).expect("published paths after failure");
        assert_eq!(published.generation.as_deref(), Some(stable_generation.as_str()));
        assert_eq!(
            tokio::fs::read_to_string(&published.summary_path)
                .await
                .expect("read stable summary"),
            "stable summary"
        );
    }

    #[tokio::test]
    async fn db_backed_artifacts_match_legacy_renderer() {
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let session_id = Uuid::parse_str("0194f5a6-89ab-7cde-8123-456789abcdef").expect("uuid");
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let meta_at = now - chrono::TimeDelta::hours(6);
        let user_at = now - chrono::TimeDelta::hours(5);
        let last_event_at = user_at.to_rfc3339();
        write_rollout_with_user_messages(
            code_home,
            session_id,
            meta_at,
            &[(user_at, "remember this")],
        )
        .await
        .expect("write rollout");

        let mut catalog = SessionCatalog::load(code_home).expect("load catalog");
        catalog.entries.insert(
            session_id,
            make_entry(
                session_id,
                SessionSource::Cli,
                &last_event_at,
                false,
                false,
                Some("remember this"),
            ),
        );
        catalog.save().expect("save catalog");

        let settings = MemoriesConfig {
            max_raw_memories_for_consolidation: 8,
            max_rollout_age_days: 60,
            max_rollouts_per_startup: 8,
            min_rollout_idle_hours: 0,
            ..MemoriesConfig::default()
        };
        let legacy_catalog = SessionCatalog::load(code_home).expect("reload catalog");
        let legacy_selected = legacy_select_memories_from_catalog(
            &legacy_catalog,
            &settings,
            now,
        );
        let legacy = legacy_render_artifacts(&legacy_selected).expect("legacy artifacts");
        let db_backed = build_state_artifacts(code_home, &settings)
            .await
            .expect("db-backed artifacts");

        assert_eq!(legacy.memory_summary, db_backed.memory_summary);
        assert_eq!(legacy.raw_memories, db_backed.raw_memories);
        assert_eq!(legacy.rollout_summaries, db_backed.rollout_summaries);
    }

    #[test]
    fn extract_last_user_snippet_skips_system_status_messages() {
        let items = vec![
            RolloutItem::ResponseItem(ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "remember this".to_string(),
                }],
                end_turn: None,
                phase: None,
            }),
            RolloutItem::ResponseItem(ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "== System Status ==\nconnected".to_string(),
                }],
                end_turn: None,
                phase: None,
            }),
        ];

        let snippet = extract_last_user_snippet(&items).expect("user snippet");
        assert_eq!(snippet, "remember this");
    }
}
