use std::collections::HashMap;
use std::io;
use std::path::Path;

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use code_memories_state::{
    MemoryPlatformFamily, MemoryShellStyle, MemoryThread, MemoriesState,
    SessionMemoryMode as StateMemoryMode, Stage1Claim, Stage1EpochInput, Stage1EpochProvenance,
    Stage1EpochRecord,
};
use code_protocol::models::{ContentItem, ResponseItem};
use code_protocol::protocol::RolloutItem;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::config_types::MemoriesConfig;
use crate::environment_context::{
    apply_environment_context_update, parse_environment_context_update_from_rollout_item,
    EnvironmentContextSnapshot, ParsedEnvironmentContextUpdate,
};
use crate::rollout::catalog::{SessionCatalog, SessionIndexEntry, SessionMemoryMode};
use crate::rollout::recorder::RecordedRolloutLine;
use crate::rollout::RolloutRecorder;

use super::current_generation_path;
use super::ensure_layout;
use super::generation_snapshot_dir;
use super::manifest::{
    memory_platform_family_from_os_family, memory_shell_style_from_script_style,
    SnapshotEpochManifestEntry, SnapshotManifest,
};
use super::memory_root;
use super::snapshot_manifest_path;
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
    manifest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EpochContextKey {
    platform_family: MemoryPlatformFamily,
    shell_style: MemoryShellStyle,
    shell_program: Option<String>,
    workspace_root: Option<String>,
}

#[derive(Debug, Clone)]
struct EpochAccumulator {
    context: EpochContextKey,
    epoch_start_at: Option<i64>,
    epoch_end_at: Option<i64>,
    epoch_start_line: i64,
    epoch_end_line: i64,
    cwd_display: String,
    git_branch: Option<String>,
    last_user_snippet: Option<String>,
}

impl EpochAccumulator {
    fn new(context: EpochContextKey, cwd_display: String, git_branch: Option<String>) -> Self {
        Self {
            context,
            epoch_start_at: None,
            epoch_end_at: None,
            epoch_start_line: 0,
            epoch_end_line: 0,
            cwd_display,
            git_branch,
            last_user_snippet: None,
        }
    }

    fn touch(&mut self, ordinal: i64, timestamp: Option<i64>) {
        if self.epoch_start_at.is_none() {
            self.epoch_start_at = timestamp;
            self.epoch_start_line = ordinal;
        }
        self.epoch_end_at = timestamp.or(self.epoch_end_at);
        self.epoch_end_line = ordinal;
    }

    fn has_meaningful_content(&self) -> bool {
        self.last_user_snippet.is_some()
    }
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
        match build_stage1_epochs(code_home, &claim).await {
            Ok(epochs) => {
                if let Err(err) = state.replace_stage1_epochs(claim.thread_id, &epochs).await {
                    let _ = state.fail_stage1_job(claim.thread_id, &err.to_string()).await;
                    warn!(
                        "failed to persist stage1 epochs for {}: {err}",
                        claim.thread_id
                    );
                }
            }
            Err(err) => {
                let _ = state.fail_stage1_job(claim.thread_id, &err.to_string()).await;
                warn!("failed to extract stage1 epochs for {}: {err}", claim.thread_id);
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
        git_project_root: entry.git_project_root.clone(),
        git_branch: entry.git_branch.clone(),
        last_user_snippet: entry.last_user_snippet.clone(),
    })
}

async fn build_stage1_epochs(
    code_home: &Path,
    claim: &Stage1Claim,
) -> io::Result<Vec<Stage1EpochInput>> {
    let rollout_path = code_home.join(&claim.rollout_path);
    let lines = RolloutRecorder::get_recorded_rollout_lines(&rollout_path).await;
    match lines {
        Ok(lines) => Ok(derive_stage1_epochs_from_lines(claim, &lines)),
        Err(err) if claim.last_user_snippet.is_some() => {
            warn!(
                "falling back to catalog snippet for {} after rollout read failed: {err}",
                claim.thread_id
            );
            Ok(vec![fallback_epoch_input(
                claim,
                Stage1EpochProvenance::CatalogFallback,
            )])
        }
        Err(err) => Err(err),
    }
}

fn derive_stage1_epochs_from_lines(
    claim: &Stage1Claim,
    lines: &[RecordedRolloutLine],
) -> Vec<Stage1EpochInput> {
    let mut current_context = fallback_context_key(claim);
    let mut current_epoch = EpochAccumulator::new(
        current_context.clone(),
        claim.cwd_display.clone(),
        claim.git_branch.clone(),
    );
    let mut current_snapshot: Option<EnvironmentContextSnapshot> = None;
    let mut epochs = Vec::new();

    for line in lines {
        if let Some(update) = parse_environment_context_update_from_rollout_item(&line.item) {
            let Some(next_snapshot) =
                apply_environment_context_update(current_snapshot.as_ref(), &update)
            else {
                if matches!(update, ParsedEnvironmentContextUpdate::Delta(_)) {
                    debug!(
                        thread_id = %claim.thread_id,
                        line_ordinal = line.ordinal,
                        "ignoring memories environment context delta without baseline"
                    );
                }
                continue;
            };
            let next_context = context_key_from_snapshot(&next_snapshot, claim);
            let next_cwd_display = next_snapshot
                .cwd
                .clone()
                .unwrap_or_else(|| current_epoch.cwd_display.clone());
            let next_git_branch = next_snapshot
                .git_branch
                .clone()
                .or_else(|| current_epoch.git_branch.clone())
                .or_else(|| claim.git_branch.clone());
            if next_context != current_context && current_epoch.has_meaningful_content() {
                epochs.push(finalize_epoch(
                    claim,
                    epochs.len() as i64,
                    &current_epoch,
                    Stage1EpochProvenance::Derived,
                ));
                current_epoch = EpochAccumulator::new(
                    next_context.clone(),
                    next_cwd_display,
                    next_git_branch,
                );
            } else {
                if next_context != current_context && !current_epoch.has_meaningful_content() {
                    debug!(
                        thread_id = %claim.thread_id,
                        line_ordinal = line.ordinal,
                        ?current_context,
                        ?next_context,
                        "merging memories context change into current epoch before first meaningful content"
                    );
                }
                current_epoch.context = next_context.clone();
                current_epoch.cwd_display = next_cwd_display;
                current_epoch.git_branch = next_git_branch;
            }
            current_context = next_context;
            current_snapshot = Some(next_snapshot);
            continue;
        }

        let timestamp = parse_timestamp(&line.timestamp).map(|value| value.timestamp());
        if should_count_toward_epoch(&line.item) {
            current_epoch.touch(line.ordinal, timestamp);
        }

        if let Some(snippet) = snippet_from_rollout_item(&line.item)
            && !is_system_status_snippet(&snippet)
        {
            current_epoch.last_user_snippet = Some(snippet);
        }
    }

    if current_epoch.has_meaningful_content() {
        epochs.push(finalize_epoch(
            claim,
            epochs.len() as i64,
            &current_epoch,
            Stage1EpochProvenance::Derived,
        ));
    }

    if epochs.is_empty() {
        epochs.push(fallback_epoch_input(
            claim,
            Stage1EpochProvenance::EmptyDerivationFallback,
        ));
    }

    epochs
}

fn fallback_context_key(claim: &Stage1Claim) -> EpochContextKey {
    EpochContextKey {
        platform_family: MemoryPlatformFamily::Unknown,
        shell_style: MemoryShellStyle::Unknown,
        shell_program: None,
        workspace_root: claim
            .git_project_root
            .as_ref()
            .map(|path| path.display().to_string()),
    }
}

fn context_key_from_snapshot(
    snapshot: &EnvironmentContextSnapshot,
    claim: &Stage1Claim,
) -> EpochContextKey {
    let platform_family = snapshot
        .operating_system
        .as_ref()
        .and_then(|info| info.family.as_deref())
        .map(memory_platform_family_from_os_family)
        .unwrap_or(MemoryPlatformFamily::Unknown);
    let shell_style = snapshot
        .shell
        .as_ref()
        .and_then(crate::shell::Shell::script_style)
        .map(memory_shell_style_from_script_style)
        .unwrap_or(MemoryShellStyle::Unknown);
    let shell_program = snapshot.shell.as_ref().and_then(crate::shell::Shell::name);
    let workspace_root = snapshot
        .git_project_root
        .clone()
        .or_else(|| claim.git_project_root.as_ref().map(|path| path.display().to_string()));

    EpochContextKey {
        platform_family,
        shell_style,
        shell_program,
        workspace_root,
    }
}

fn should_count_toward_epoch(item: &RolloutItem) -> bool {
    match item {
        RolloutItem::SessionMeta(_) => false,
        _ => parse_environment_context_update_from_rollout_item(item).is_none(),
    }
}

fn snippet_from_rollout_item(item: &RolloutItem) -> Option<String> {
    match item {
        RolloutItem::ResponseItem(ResponseItem::Message { role, content, .. })
            if role.eq_ignore_ascii_case("user") => snippet_from_content(content),
        _ => None,
    }
}

fn finalize_epoch(
    claim: &Stage1Claim,
    epoch_index: i64,
    epoch: &EpochAccumulator,
    provenance: Stage1EpochProvenance,
) -> Stage1EpochInput {
    let snippet = epoch
        .last_user_snippet
        .clone()
        .or_else(|| claim.last_user_snippet.clone())
        .unwrap_or_else(|| "(no user snippet)".to_string());
    let rollout_slug = format!(
        "{}-e{epoch_index}",
        rollout_summary_file_stem(
            claim.thread_id,
            claim.updated_at,
            &epoch.cwd_display,
            epoch.git_branch.as_deref(),
        )
    );

    Stage1EpochInput {
        id: code_memories_state::MemoryEpochId {
            thread_id: claim.thread_id,
            epoch_index,
        },
        provenance,
        source_updated_at: claim.updated_at,
        generated_at: Utc::now().timestamp(),
        epoch_start_at: epoch.epoch_start_at.or(Some(claim.updated_at)),
        epoch_end_at: epoch.epoch_end_at.or(Some(claim.updated_at)),
        epoch_start_line: epoch.epoch_start_line,
        epoch_end_line: epoch.epoch_end_line,
        platform_family: epoch.context.platform_family,
        shell_style: epoch.context.shell_style,
        shell_program: epoch.context.shell_program.clone(),
        workspace_root: epoch.context.workspace_root.clone(),
        cwd_display: epoch.cwd_display.clone(),
        git_branch: epoch.git_branch.clone(),
        raw_memory: render_raw_memory_body(
            claim,
            epoch_index,
            epoch,
            provenance,
            &rollout_slug,
            &snippet,
        ),
        rollout_summary: render_rollout_summary_body(
            claim,
            epoch_index,
            epoch,
            provenance,
            &snippet,
        ),
        rollout_slug,
    }
}

fn fallback_epoch_input(
    claim: &Stage1Claim,
    provenance: Stage1EpochProvenance,
) -> Stage1EpochInput {
    let epoch = EpochAccumulator {
        context: fallback_context_key(claim),
        epoch_start_at: Some(claim.updated_at),
        epoch_end_at: Some(claim.updated_at),
        epoch_start_line: 0,
        epoch_end_line: 0,
        cwd_display: claim.cwd_display.clone(),
        git_branch: claim.git_branch.clone(),
        last_user_snippet: claim.last_user_snippet.clone(),
    };
    finalize_epoch(claim, 0, &epoch, provenance)
}

struct EpochRenderView<'a> {
    updated_at_label: &'a str,
    id: code_memories_state::MemoryEpochId,
    epoch_start_at: Option<i64>,
    epoch_end_at: Option<i64>,
    platform_family: MemoryPlatformFamily,
    shell_style: MemoryShellStyle,
    shell_program: Option<&'a str>,
    workspace_root: Option<&'a str>,
    cwd_display: &'a str,
    git_branch: Option<&'a str>,
    provenance: Stage1EpochProvenance,
    rollout_path: Option<&'a Path>,
    rollout_summary_filename: Option<&'a str>,
    last_user_request: &'a str,
}

fn push_epoch_metadata_lines(body: &mut String, view: &EpochRenderView<'_>) {
    body.push_str(&format!("thread_id: {}\n", view.id.thread_id));
    body.push_str(&format!("epoch_index: {}\n", view.id.epoch_index));
    body.push_str(&format!("updated_at: {}\n", view.updated_at_label));
    if let Some(epoch_start_at) = view.epoch_start_at {
        body.push_str(&format!("epoch_start_at: {}\n", iso_timestamp(epoch_start_at)));
    }
    if let Some(epoch_end_at) = view.epoch_end_at {
        body.push_str(&format!("epoch_end_at: {}\n", iso_timestamp(epoch_end_at)));
    }
    body.push_str(&format!(
        "platform_family: {}\n",
        platform_family_label(view.platform_family)
    ));
    body.push_str(&format!("shell_style: {}\n", shell_style_label(view.shell_style)));
    if let Some(shell_program) = view.shell_program {
        body.push_str(&format!("shell_program: {shell_program}\n"));
    }
    if let Some(workspace_root) = view.workspace_root {
        body.push_str(&format!("workspace_root: {workspace_root}\n"));
    }
    body.push_str(&format!("cwd: {}\n", view.cwd_display));
    if let Some(git_branch) = view.git_branch {
        body.push_str(&format!("git_branch: {git_branch}\n"));
    }
    body.push_str(&format!("provenance: {}\n", provenance_label(view.provenance)));
    if let Some(rollout_path) = view.rollout_path {
        body.push_str(&format!("rollout_path: {}\n", rollout_path.display()));
    }
    if let Some(rollout_summary_filename) = view.rollout_summary_filename {
        body.push_str(&format!(
            "rollout_summary_file: rollout_summaries/{rollout_summary_filename}\n"
        ));
    }
}

fn render_epoch_summary_block(view: &EpochRenderView<'_>) -> String {
    let mut entry = String::new();
    entry.push_str(&format!(
        "## {} | {}#{}\n",
        view.updated_at_label, view.id.thread_id, view.id.epoch_index
    ));
    push_epoch_metadata_lines(&mut entry, view);
    entry.push_str(&format!("last_user_request: {}", view.last_user_request));
    entry
}

fn render_raw_memory_body(
    claim: &Stage1Claim,
    epoch_index: i64,
    epoch: &EpochAccumulator,
    provenance: Stage1EpochProvenance,
    rollout_slug: &str,
    snippet: &str,
) -> String {
    let mut body = String::new();
    let rollout_summary_filename = format!("{rollout_slug}.md");
    let view = EpochRenderView {
        updated_at_label: &claim.updated_at_label,
        id: code_memories_state::MemoryEpochId {
            thread_id: claim.thread_id,
            epoch_index,
        },
        epoch_start_at: epoch.epoch_start_at,
        epoch_end_at: epoch.epoch_end_at,
        platform_family: epoch.context.platform_family,
        shell_style: epoch.context.shell_style,
        shell_program: epoch.context.shell_program.as_deref(),
        workspace_root: epoch.context.workspace_root.as_deref(),
        cwd_display: &epoch.cwd_display,
        git_branch: epoch.git_branch.as_deref(),
        provenance,
        rollout_path: Some(&claim.rollout_path),
        rollout_summary_filename: Some(&rollout_summary_filename),
        last_user_request: snippet,
    };
    push_epoch_metadata_lines(&mut body, &view);
    body.push('\n');
    body.push_str(snippet);
    body.push('\n');
    body
}

fn render_rollout_summary_body(
    claim: &Stage1Claim,
    epoch_index: i64,
    epoch: &EpochAccumulator,
    provenance: Stage1EpochProvenance,
    snippet: &str,
) -> String {
    let mut body = String::new();
    let view = EpochRenderView {
        updated_at_label: &claim.updated_at_label,
        id: code_memories_state::MemoryEpochId {
            thread_id: claim.thread_id,
            epoch_index,
        },
        epoch_start_at: epoch.epoch_start_at,
        epoch_end_at: epoch.epoch_end_at,
        platform_family: epoch.context.platform_family,
        shell_style: epoch.context.shell_style,
        shell_program: epoch.context.shell_program.as_deref(),
        workspace_root: epoch.context.workspace_root.as_deref(),
        cwd_display: &epoch.cwd_display,
        git_branch: epoch.git_branch.as_deref(),
        provenance,
        rollout_path: Some(&claim.rollout_path),
        rollout_summary_filename: None,
        last_user_request: snippet,
    };
    push_epoch_metadata_lines(&mut body, &view);
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
        .select_phase2_epochs(
            settings.max_raw_memories_for_consolidation,
            settings.max_rollout_age_days,
            crate::rollout::INTERACTIVE_SESSION_SOURCES,
        )
        .await
        .map_err(io::Error::other)?;

    let artifacts = render_artifacts_from_state(&selected)?;
    if let Err(err) = write_memory_artifacts(code_home, artifacts).await {
        let _ = state
            .fail_artifact_build_job(&lease.ownership_token, &err.to_string())
            .await;
        return Err(err);
    }

    state
        .succeed_artifact_build_job(&lease.ownership_token)
        .await
        .map_err(io::Error::other)
}

fn render_artifacts_from_state(selected: &[Stage1EpochRecord]) -> io::Result<MemoryArtifacts> {
    let manifest = render_manifest(selected)?;
    Ok(MemoryArtifacts {
        memory_summary: render_memory_summary(selected),
        raw_memories: render_raw_memories(selected),
        rollout_summaries: render_rollout_summaries(selected),
        manifest: serde_json::to_string_pretty(&manifest).map_err(io::Error::other)?,
    })
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
    tokio::fs::write(snapshot_manifest_path(&snapshot_dir), artifacts.manifest).await?;
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

fn render_memory_summary(selected: &[Stage1EpochRecord]) -> String {
    let mut body = String::from("# Memory Summary\n\n");
    if selected.is_empty() {
        body.push_str("No prior interactive memory epochs found.\n");
        return body;
    }

    body.push_str("Recent interactive memory epochs retained for memory prompts:\n\n");
    for record in selected {
        body.push_str(&render_prompt_entry(record));
        body.push_str("\n\n");
    }
    body
}

fn render_raw_memories(selected: &[Stage1EpochRecord]) -> String {
    let mut body = String::from("# Raw Memories\n\n");
    if selected.is_empty() {
        body.push_str("No raw memories yet.\n");
        return body;
    }

    body.push_str("Catalog-derived retained memory epochs (ranked for reuse):\n\n");
    for record in selected {
        body.push_str(&format!(
            "## Epoch `{}#{}`\n",
            record.id.thread_id, record.id.epoch_index
        ));
        body.push_str(&record.raw_memory);
        body.push('\n');
    }
    body
}

fn render_rollout_summaries(selected: &[Stage1EpochRecord]) -> HashMap<String, String> {
    selected
        .iter()
        .map(|record| {
            (
                epoch_rollout_summary_filename(record.id),
                record.rollout_summary.clone(),
            )
        })
        .collect()
}

fn render_manifest(selected: &[Stage1EpochRecord]) -> io::Result<SnapshotManifest> {
    let epochs = selected
        .iter()
        .map(|record| {
            Ok::<_, io::Error>(SnapshotEpochManifestEntry {
                id: record.id,
                provenance: record.provenance,
                platform_family: record.platform_family,
                shell_style: record.shell_style,
                shell_program: record.shell_program.clone(),
                workspace_root: record.workspace_root.clone(),
                cwd_display: record.cwd_display.clone(),
                git_branch: record.git_branch.clone(),
                source_updated_at: record.source_updated_at,
                usage_count: record.usage_count,
                last_usage: record.last_usage,
                rollout_summary_path: format!(
                    "rollout_summaries/{}",
                    epoch_rollout_summary_filename(record.id)
                ),
                prompt_entry: render_prompt_entry(record),
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(SnapshotManifest::new(epochs))
}

fn render_prompt_entry(record: &Stage1EpochRecord) -> String {
    let rollout_summary_filename = epoch_rollout_summary_filename(record.id);
    let view = EpochRenderView {
        updated_at_label: &record.updated_at_label,
        id: record.id,
        epoch_start_at: record.epoch_start_at,
        epoch_end_at: record.epoch_end_at,
        platform_family: record.platform_family,
        shell_style: record.shell_style,
        shell_program: record.shell_program.as_deref(),
        workspace_root: record.workspace_root.as_deref(),
        cwd_display: &record.cwd_display,
        git_branch: record.git_branch.as_deref(),
        provenance: record.provenance,
        rollout_path: None,
        rollout_summary_filename: Some(&rollout_summary_filename),
        last_user_request: summary_snippet_from_rollout_summary(&record.rollout_summary),
    };
    render_epoch_summary_block(&view)
}

async fn sync_rollout_summaries(
    snapshot_dir: &Path,
    summaries: HashMap<String, String>,
) -> io::Result<()> {
    let dir = snapshot_rollout_summaries_dir(snapshot_dir);
    for (filename, body) in summaries {
        tokio::fs::write(dir.join(filename), body).await?;
    }
    Ok(())
}

fn epoch_rollout_summary_filename(id: code_memories_state::MemoryEpochId) -> String {
    format!("{}-{}.md", id.thread_id, id.epoch_index)
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
    // Stage1 rollout summaries end with the retained user snippet as their
    // final non-empty line. Keep that rendering contract stable and test it.
    last_nonempty_line(rollout_summary).unwrap_or("(no user snippet)")
}

fn platform_family_label(platform_family: MemoryPlatformFamily) -> &'static str {
    match platform_family {
        MemoryPlatformFamily::Unix => "unix",
        MemoryPlatformFamily::Windows => "windows",
        MemoryPlatformFamily::Unknown => "unknown",
    }
}

fn shell_style_label(shell_style: MemoryShellStyle) -> &'static str {
    match shell_style {
        MemoryShellStyle::PosixSh => "posix-sh",
        MemoryShellStyle::BashZshCompatible => "bash-zsh-compatible",
        MemoryShellStyle::Zsh => "zsh",
        MemoryShellStyle::PowerShell => "powershell",
        MemoryShellStyle::Cmd => "cmd",
        MemoryShellStyle::Nushell => "nushell",
        MemoryShellStyle::Elvish => "elvish",
        MemoryShellStyle::Unknown => "unknown",
    }
}

fn provenance_label(provenance: Stage1EpochProvenance) -> &'static str {
    match provenance {
        Stage1EpochProvenance::Derived => "derived",
        Stage1EpochProvenance::CatalogFallback => "catalog_fallback",
        Stage1EpochProvenance::EmptyDerivationFallback => "empty_derivation_fallback",
    }
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
    let mut last_was_sep = true;
    for ch in raw.chars() {
        if slug.len() >= max_len {
            break;
        }
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            slug.push('_');
            last_was_sep = true;
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
    use std::sync::OnceLock;

    use code_memories_state::{
        MemoriesState, Stage1EpochProvenance, Stage1EpochRecord,
        STAGE1_TERMINAL_FAILURE_THRESHOLD,
    };
    use code_protocol::protocol::{RolloutLine, SessionSource};
    use tempfile::tempdir;

    use super::*;

    static PUBLISH_TEST_GUARD: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

    async fn lock_publish_tests() -> tokio::sync::MutexGuard<'static, ()> {
        PUBLISH_TEST_GUARD
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await
    }

    fn make_entry(
        session_id: Uuid,
        source: SessionSource,
        last_event_at: &str,
        archived: bool,
        deleted: bool,
        snippet: Option<&str>,
    ) -> SessionIndexEntry {
        make_entry_with_branch(session_id, source, last_event_at, archived, deleted, snippet, "main")
    }

    fn make_entry_with_branch(
        session_id: Uuid,
        source: SessionSource,
        last_event_at: &str,
        archived: bool,
        deleted: bool,
        snippet: Option<&str>,
        git_branch: &str,
    ) -> SessionIndexEntry {
        SessionIndexEntry {
            session_id,
            rollout_path: PathBuf::from(format!("sessions/{session_id}.jsonl")),
            snapshot_path: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_event_at: last_event_at.to_string(),
            cwd_real: PathBuf::from("/tmp/project"),
            cwd_display: "~/project".to_string(),
            git_project_root: Some(PathBuf::from("/tmp/project")),
            git_branch: Some(git_branch.to_string()),
            git_sha: None,
            git_origin_url: None,
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

    async fn write_rollout_lines(
        code_home: &Path,
        session_id: Uuid,
        meta_at: DateTime<Utc>,
        items: Vec<(DateTime<Utc>, RolloutItem)>,
    ) -> io::Result<PathBuf> {
        let rollout_rel_path = rollout_rel_path(session_id, meta_at);
        let rollout_path = code_home.join(&rollout_rel_path);
        if let Some(parent) = rollout_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut lines = Vec::new();
        lines.push(RolloutLine {
            timestamp: meta_at.to_rfc3339(),
            item: RolloutItem::SessionMeta(code_protocol::protocol::SessionMetaLine {
                meta: code_protocol::protocol::SessionMeta {
                    id: code_protocol::ThreadId::from_string(&session_id.to_string())
                        .expect("thread id"),
                    forked_from_id: None,
                    timestamp: meta_at.to_rfc3339(),
                    cwd: PathBuf::from("/tmp/project"),
                    originator: "codex_cli_rs".to_string(),
                    cli_version: "0.0.0".to_string(),
                    source: SessionSource::Cli,
                    model_provider: None,
                    base_instructions: None,
                    dynamic_tools: None,
                },
                git: Some(code_protocol::protocol::GitInfo {
                    commit_hash: None,
                    branch: Some("main".to_string()),
                    repository_url: None,
                }),
            }),
        });
        for (timestamp, item) in items {
            lines.push(RolloutLine {
                timestamp: timestamp.to_rfc3339(),
                item,
            });
        }

        let mut body = String::new();
        for line in lines {
            body.push_str(&serde_json::to_string(&line).map_err(io::Error::other)?);
            body.push('\n');
        }
        tokio::fs::write(&rollout_path, body).await?;
        Ok(rollout_rel_path)
    }

    fn user_message(text: &str) -> RolloutItem {
        RolloutItem::ResponseItem(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
            end_turn: None,
            phase: None,
        })
    }

    fn legacy_system_status_message(cwd: &str, branch: &str) -> RolloutItem {
        RolloutItem::ResponseItem(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: format!("== System Status ==\ncwd: {cwd}\nbranch: {branch}\n"),
            }],
            end_turn: None,
            phase: None,
        })
    }

    fn environment_snapshot(
        os_family: &str,
        cwd: &str,
        git_project_root: &str,
        shell: crate::shell::Shell,
        git_branch: &str,
    ) -> EnvironmentContextSnapshot {
        EnvironmentContextSnapshot {
            version: EnvironmentContextSnapshot::VERSION,
            cwd: Some(cwd.to_string()),
            git_project_root: Some(git_project_root.to_string()),
            approval_policy: None,
            sandbox_mode: None,
            network_access: None,
            writable_roots: Vec::new(),
            operating_system: Some(crate::environment_context::OperatingSystemInfo {
                family: Some(os_family.to_string()),
                version: None,
                architecture: None,
            }),
            common_tools: Vec::new(),
            shell: Some(shell),
            git_branch: Some(git_branch.to_string()),
            reasoning_effort: None,
        }
    }

    fn unix_environment_snapshot(
        cwd: &str,
        git_project_root: &str,
        shell: crate::shell::Shell,
    ) -> EnvironmentContextSnapshot {
        environment_snapshot("linux", cwd, git_project_root, shell, "main")
    }

    fn env_snapshot(cwd: &str, git_project_root: &str, shell: crate::shell::Shell) -> RolloutItem {
        let snapshot = unix_environment_snapshot(cwd, git_project_root, shell);
        RolloutItem::ResponseItem(snapshot.to_response_item().expect("snapshot item"))
    }

    fn env_snapshot_with_branch(
        os_family: &str,
        cwd: &str,
        git_project_root: &str,
        shell: crate::shell::Shell,
        git_branch: &str,
    ) -> RolloutItem {
        let snapshot = environment_snapshot(os_family, cwd, git_project_root, shell, git_branch);
        RolloutItem::ResponseItem(snapshot.to_response_item().expect("snapshot item"))
    }

    fn env_delta(
        previous: &EnvironmentContextSnapshot,
        current: &EnvironmentContextSnapshot,
    ) -> RolloutItem {
        let delta = current.diff_from(previous);
        RolloutItem::ResponseItem(delta.to_response_item().expect("delta item"))
    }

    fn make_claim(session_id: Uuid, updated_at: DateTime<Utc>, snippet: Option<&str>) -> Stage1Claim {
        Stage1Claim {
            thread_id: session_id,
            rollout_path: PathBuf::from(format!("sessions/{session_id}.jsonl")),
            cwd: PathBuf::from("/tmp/project"),
            cwd_display: "~/project".to_string(),
            updated_at: updated_at.timestamp(),
            updated_at_label: updated_at.to_rfc3339(),
            git_project_root: Some(PathBuf::from("/tmp/project")),
            git_branch: Some("main".to_string()),
            last_user_snippet: snippet.map(ToString::to_string),
        }
    }

    #[tokio::test]
    async fn refresh_writes_epoch_manifest_and_artifacts() {
        let _guard = lock_publish_tests().await;
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let meta_at = now - chrono::TimeDelta::hours(6);
        let user_at = now - chrono::TimeDelta::hours(5);
        let rollout_rel_path = write_rollout_lines(
            code_home,
            session_id,
            meta_at,
            vec![(user_at, user_message("remember this"))],
        )
        .await
        .expect("write rollout");

        let mut catalog = SessionCatalog::load(code_home).expect("load catalog");
        let mut entry = make_entry(
            session_id,
            SessionSource::Cli,
            &user_at.to_rfc3339(),
            false,
            false,
            Some("remember this"),
        );
        entry.rollout_path = rollout_rel_path;
        catalog.entries.insert(session_id, entry);
        catalog.save().expect("save catalog");

        let settings = MemoriesConfig {
            max_raw_memories_for_consolidation: 8,
            max_rollout_age_days: 60,
            max_rollouts_per_startup: 8,
            min_rollout_idle_hours: 0,
            ..MemoriesConfig::default()
        };
        refresh_memory_artifacts_from_catalog(code_home, &settings, true)
            .await
            .expect("refresh memories");

        let published = published_artifact_paths(code_home).expect("published artifact paths");
        let manifest_text = tokio::fs::read_to_string(&published.manifest_path)
            .await
            .expect("read manifest");
        let manifest: SnapshotManifest = serde_json::from_str(&manifest_text).expect("manifest json");
        assert_eq!(manifest.epochs.len(), 1);
        assert!(manifest.epochs[0].prompt_entry.contains("remember this"));
        assert!(tokio::fs::try_exists(published.summary_path).await.expect("summary exists"));
        assert!(tokio::fs::try_exists(published.raw_memories_path).await.expect("raw exists"));
    }

    #[tokio::test]
    async fn shell_change_creates_multiple_epochs() {
        let _guard = lock_publish_tests().await;
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let meta_at = now - chrono::TimeDelta::hours(6);
        let t1 = now - chrono::TimeDelta::hours(5);
        let t2 = now - chrono::TimeDelta::hours(4);
        let t3 = now - chrono::TimeDelta::hours(3);
        let t4 = now - chrono::TimeDelta::hours(2);
        let rollout_rel_path = write_rollout_lines(
            code_home,
            session_id,
            meta_at,
            vec![
                (t1, env_snapshot("/tmp/project", "/tmp/project", crate::shell::Shell::Bash(crate::shell::BashShell {
                    shell_path: "/bin/bash".to_string(),
                    bashrc_path: "/tmp/.bashrc".to_string(),
                }))),
                (t2, user_message("remember bash thing")),
                (t3, env_snapshot("/tmp/project", "/tmp/project", crate::shell::Shell::Zsh(crate::shell::ZshShell {
                    shell_path: "/bin/zsh".to_string(),
                    zshrc_path: "/tmp/.zshrc".to_string(),
                }))),
                (t4, user_message("remember zsh thing")),
            ],
        )
        .await
        .expect("write rollout");

        let mut catalog = SessionCatalog::load(code_home).expect("load catalog");
        let mut entry = make_entry(
            session_id,
            SessionSource::Cli,
            &t4.to_rfc3339(),
            false,
            false,
            Some("remember zsh thing"),
        );
        entry.rollout_path = rollout_rel_path;
        catalog.entries.insert(session_id, entry);
        catalog.save().expect("save catalog");

        let settings = MemoriesConfig {
            max_raw_memories_for_consolidation: 8,
            max_rollout_age_days: 60,
            max_rollouts_per_startup: 8,
            min_rollout_idle_hours: 0,
            ..MemoriesConfig::default()
        };
        refresh_memory_artifacts_from_catalog(code_home, &settings, true)
            .await
            .expect("refresh memories");

        let state = MemoriesState::open(code_home).await.expect("open state");
        let status = state
            .status(crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("status after refresh");
        assert_eq!(status.stage1_epoch_count, 2);
    }

    #[tokio::test]
    async fn refresh_and_prompt_selection_follow_shell_context() {
        let _guard = lock_publish_tests().await;
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let zsh_session_id = Uuid::new_v4();
        let pwsh_session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let meta_at = now - chrono::TimeDelta::hours(8);
        let zsh_at = now - chrono::TimeDelta::hours(6);
        let pwsh_at = now - chrono::TimeDelta::hours(4);

        let zsh_rollout_path = write_rollout_lines(
            code_home,
            zsh_session_id,
            meta_at,
            vec![
                (
                    zsh_at - chrono::TimeDelta::minutes(1),
                    env_snapshot_with_branch(
                        "linux",
                        "/tmp/project",
                        "/tmp/project",
                        crate::shell::Shell::Zsh(crate::shell::ZshShell {
                            shell_path: "/bin/zsh".to_string(),
                            zshrc_path: "/tmp/.zshrc".to_string(),
                        }),
                        "main",
                    ),
                ),
                (zsh_at, user_message("remember zsh thing")),
            ],
        )
        .await
        .expect("write zsh rollout");
        let pwsh_rollout_path = write_rollout_lines(
            code_home,
            pwsh_session_id,
            meta_at,
            vec![
                (
                    pwsh_at - chrono::TimeDelta::minutes(1),
                    env_snapshot_with_branch(
                        "windows",
                        "C:/project",
                        "C:/project",
                        crate::shell::Shell::PowerShell(crate::shell::PowerShellConfig {
                            exe: "pwsh".to_string(),
                            bash_exe_fallback: None,
                        }),
                        "main",
                    ),
                ),
                (pwsh_at, user_message("remember powershell thing")),
            ],
        )
        .await
        .expect("write powershell rollout");

        let mut catalog = SessionCatalog::load(code_home).expect("load catalog");
        let mut zsh_entry = make_entry(
            zsh_session_id,
            SessionSource::Cli,
            &zsh_at.to_rfc3339(),
            false,
            false,
            Some("remember zsh thing"),
        );
        zsh_entry.rollout_path = zsh_rollout_path;
        catalog.entries.insert(zsh_session_id, zsh_entry);

        let mut pwsh_entry = make_entry(
            pwsh_session_id,
            SessionSource::Cli,
            &pwsh_at.to_rfc3339(),
            false,
            false,
            Some("remember powershell thing"),
        );
        pwsh_entry.rollout_path = pwsh_rollout_path;
        pwsh_entry.cwd_real = PathBuf::from("C:/project");
        pwsh_entry.cwd_display = "C:/project".to_string();
        pwsh_entry.git_project_root = Some(PathBuf::from("C:/project"));
        catalog.entries.insert(pwsh_session_id, pwsh_entry);
        catalog.save().expect("save catalog");

        let settings = MemoriesConfig {
            max_raw_memories_for_consolidation: 8,
            max_rollout_age_days: 60,
            max_rollouts_per_startup: 8,
            min_rollout_idle_hours: 0,
            ..MemoriesConfig::default()
        };
        refresh_memory_artifacts_from_catalog(code_home, &settings, true)
            .await
            .expect("refresh memories");

        let zsh_prompt = crate::memories::build_memory_tool_developer_instructions(
            code_home,
            &crate::memories::manifest::MemoriesCurrentContext {
                platform_family: MemoryPlatformFamily::Unix,
                shell_style: Some(MemoryShellStyle::Zsh),
                shell_program: Some("zsh".to_string()),
                workspace_root: Some("/tmp/project".to_string()),
                git_branch: Some("main".to_string()),
            },
        )
        .await
        .expect("zsh prompt");
        assert!(zsh_prompt.instructions.contains("remember zsh thing"));
        assert!(!zsh_prompt.instructions.contains("remember powershell thing"));

        let pwsh_prompt = crate::memories::build_memory_tool_developer_instructions(
            code_home,
            &crate::memories::manifest::MemoriesCurrentContext {
                platform_family: MemoryPlatformFamily::Windows,
                shell_style: Some(MemoryShellStyle::PowerShell),
                shell_program: Some("pwsh".to_string()),
                workspace_root: Some("C:/project".to_string()),
                git_branch: Some("main".to_string()),
            },
        )
        .await
        .expect("powershell prompt");
        assert!(pwsh_prompt.instructions.contains("remember powershell thing"));
        assert!(!pwsh_prompt.instructions.contains("remember zsh thing"));
    }

    #[tokio::test]
    async fn refresh_and_prompt_selection_prefers_same_branch_in_same_workspace() {
        let _guard = lock_publish_tests().await;
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let main_session_id = Uuid::new_v4();
        let feature_session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let meta_at = now - chrono::TimeDelta::hours(10);
        let main_at = now - chrono::TimeDelta::hours(6);
        let feature_at = now - chrono::TimeDelta::hours(2);

        let main_rollout_path = write_rollout_lines(
            code_home,
            main_session_id,
            meta_at,
            vec![
                (
                    main_at - chrono::TimeDelta::minutes(1),
                    env_snapshot_with_branch(
                        "linux",
                        "/tmp/project",
                        "/tmp/project",
                        crate::shell::Shell::Zsh(crate::shell::ZshShell {
                            shell_path: "/bin/zsh".to_string(),
                            zshrc_path: "/tmp/.zshrc".to_string(),
                        }),
                        "main",
                    ),
                ),
                (main_at, user_message("remember main branch thing")),
            ],
        )
        .await
        .expect("write main rollout");
        let feature_rollout_path = write_rollout_lines(
            code_home,
            feature_session_id,
            meta_at,
            vec![
                (
                    feature_at - chrono::TimeDelta::minutes(1),
                    env_snapshot_with_branch(
                        "linux",
                        "/tmp/project",
                        "/tmp/project",
                        crate::shell::Shell::Zsh(crate::shell::ZshShell {
                            shell_path: "/bin/zsh".to_string(),
                            zshrc_path: "/tmp/.zshrc".to_string(),
                        }),
                        "feature/demo",
                    ),
                ),
                (feature_at, user_message("remember feature branch thing")),
            ],
        )
        .await
        .expect("write feature rollout");

        let mut catalog = SessionCatalog::load(code_home).expect("load catalog");
        let mut main_entry = make_entry_with_branch(
            main_session_id,
            SessionSource::Cli,
            &main_at.to_rfc3339(),
            false,
            false,
            Some("remember main branch thing"),
            "main",
        );
        main_entry.rollout_path = main_rollout_path;
        catalog.entries.insert(main_session_id, main_entry);

        let mut feature_entry = make_entry_with_branch(
            feature_session_id,
            SessionSource::Cli,
            &feature_at.to_rfc3339(),
            false,
            false,
            Some("remember feature branch thing"),
            "feature/demo",
        );
        feature_entry.rollout_path = feature_rollout_path;
        catalog.entries.insert(feature_session_id, feature_entry);
        catalog.save().expect("save catalog");

        let settings = MemoriesConfig {
            max_raw_memories_for_consolidation: 8,
            max_rollout_age_days: 60,
            max_rollouts_per_startup: 8,
            min_rollout_idle_hours: 0,
            ..MemoriesConfig::default()
        };
        refresh_memory_artifacts_from_catalog(code_home, &settings, true)
            .await
            .expect("refresh memories");

        let prompt = crate::memories::build_memory_tool_developer_instructions(
            code_home,
            &crate::memories::manifest::MemoriesCurrentContext {
                platform_family: MemoryPlatformFamily::Unix,
                shell_style: Some(MemoryShellStyle::Zsh),
                shell_program: Some("zsh".to_string()),
                workspace_root: Some("/tmp/project".to_string()),
                git_branch: Some("main".to_string()),
            },
        )
        .await
        .expect("main-branch prompt");

        assert_eq!(prompt.selected_epoch_ids.len(), 2);
        assert_eq!(prompt.selected_epoch_ids[0].thread_id, main_session_id);
        assert!(prompt.instructions.contains("remember main branch thing"));
        assert!(prompt.instructions.contains("remember feature branch thing"));
    }

    #[tokio::test]
    async fn active_snapshot_with_no_compatible_entries_injects_nothing_end_to_end() {
        let _guard = lock_publish_tests().await;
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let meta_at = now - chrono::TimeDelta::hours(6);
        let user_at = now - chrono::TimeDelta::hours(5);
        let rollout_rel_path = write_rollout_lines(
            code_home,
            session_id,
            meta_at,
            vec![
                (
                    user_at - chrono::TimeDelta::minutes(1),
                    env_snapshot_with_branch(
                        "linux",
                        "/tmp/project",
                        "/tmp/project",
                        crate::shell::Shell::Zsh(crate::shell::ZshShell {
                            shell_path: "/bin/zsh".to_string(),
                            zshrc_path: "/tmp/.zshrc".to_string(),
                        }),
                        "main",
                    ),
                ),
                (user_at, user_message("remember zsh-only thing")),
            ],
        )
        .await
        .expect("write rollout");

        let mut catalog = SessionCatalog::load(code_home).expect("load catalog");
        let mut entry = make_entry(
            session_id,
            SessionSource::Cli,
            &user_at.to_rfc3339(),
            false,
            false,
            Some("remember zsh-only thing"),
        );
        entry.rollout_path = rollout_rel_path;
        catalog.entries.insert(session_id, entry);
        catalog.save().expect("save catalog");

        let settings = MemoriesConfig {
            max_raw_memories_for_consolidation: 8,
            max_rollout_age_days: 60,
            max_rollouts_per_startup: 8,
            min_rollout_idle_hours: 0,
            ..MemoriesConfig::default()
        };
        refresh_memory_artifacts_from_catalog(code_home, &settings, true)
            .await
            .expect("refresh memories");

        let prompt = crate::memories::build_memory_tool_developer_instructions(
            code_home,
            &crate::memories::manifest::MemoriesCurrentContext {
                platform_family: MemoryPlatformFamily::Windows,
                shell_style: Some(MemoryShellStyle::PowerShell),
                shell_program: Some("pwsh".to_string()),
                workspace_root: Some("C:/project".to_string()),
                git_branch: Some("main".to_string()),
            },
        )
        .await;

        assert!(prompt.is_none());
    }

    #[tokio::test]
    async fn rollout_read_failure_uses_catalog_fallback_provenance() {
        let temp = tempdir().expect("tempdir");
        let session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let claim = make_claim(session_id, now, Some("remember fallback snippet"));

        let epochs = build_stage1_epochs(temp.path(), &claim)
            .await
            .expect("fallback epochs");

        assert_eq!(epochs.len(), 1);
        assert_eq!(epochs[0].provenance, Stage1EpochProvenance::CatalogFallback);
        assert!(epochs[0].raw_memory.contains("provenance: catalog_fallback"));
    }

    #[test]
    fn delta_without_baseline_keeps_epoch_on_fallback_context() {
        let session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let claim = make_claim(session_id, now, Some("remember delta thing"));
        let baseline = unix_environment_snapshot(
            "/tmp/project",
            "/tmp/project",
            crate::shell::Shell::Bash(crate::shell::BashShell {
                shell_path: "/bin/bash".to_string(),
                bashrc_path: "/tmp/.bashrc".to_string(),
            }),
        );
        let changed = unix_environment_snapshot(
            "/tmp/project",
            "/tmp/project",
            crate::shell::Shell::Zsh(crate::shell::ZshShell {
                shell_path: "/bin/zsh".to_string(),
                zshrc_path: "/tmp/.zshrc".to_string(),
            }),
        );
        let lines = vec![
            RecordedRolloutLine {
                ordinal: 0,
                timestamp: now.to_rfc3339(),
                item: env_delta(&baseline, &changed),
            },
            RecordedRolloutLine {
                ordinal: 1,
                timestamp: (now + chrono::TimeDelta::minutes(1)).to_rfc3339(),
                item: user_message("remember delta thing"),
            },
        ];

        let epochs = derive_stage1_epochs_from_lines(&claim, &lines);
        assert_eq!(epochs.len(), 1);
        assert_eq!(epochs[0].platform_family, MemoryPlatformFamily::Unknown);
        assert_eq!(epochs[0].shell_style, MemoryShellStyle::Unknown);
        assert_eq!(epochs[0].workspace_root.as_deref(), Some("/tmp/project"));
    }

    #[test]
    fn pre_content_context_changes_do_not_create_empty_epochs() {
        let session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let claim = make_claim(session_id, now, Some("remember zsh thing"));
        let lines = vec![
            RecordedRolloutLine {
                ordinal: 0,
                timestamp: now.to_rfc3339(),
                item: env_snapshot(
                    "/tmp/project",
                    "/tmp/project",
                    crate::shell::Shell::Bash(crate::shell::BashShell {
                        shell_path: "/bin/bash".to_string(),
                        bashrc_path: "/tmp/.bashrc".to_string(),
                    }),
                ),
            },
            RecordedRolloutLine {
                ordinal: 1,
                timestamp: (now + chrono::TimeDelta::minutes(1)).to_rfc3339(),
                item: env_snapshot(
                    "/tmp/project",
                    "/tmp/project",
                    crate::shell::Shell::Zsh(crate::shell::ZshShell {
                        shell_path: "/bin/zsh".to_string(),
                        zshrc_path: "/tmp/.zshrc".to_string(),
                    }),
                ),
            },
            RecordedRolloutLine {
                ordinal: 2,
                timestamp: (now + chrono::TimeDelta::minutes(2)).to_rfc3339(),
                item: user_message("remember zsh thing"),
            },
        ];

        let epochs = derive_stage1_epochs_from_lines(&claim, &lines);
        assert_eq!(epochs.len(), 1);
        assert_eq!(epochs[0].shell_style, MemoryShellStyle::Zsh);
        assert!(epochs[0].raw_memory.contains("remember zsh thing"));
    }

    #[test]
    fn empty_derivation_uses_empty_derivation_fallback_provenance() {
        let session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let claim = make_claim(session_id, now, Some("remember empty fallback"));
        let lines = vec![RecordedRolloutLine {
            ordinal: 0,
            timestamp: now.to_rfc3339(),
            item: env_snapshot(
                "/tmp/project",
                "/tmp/project",
                crate::shell::Shell::Bash(crate::shell::BashShell {
                    shell_path: "/bin/bash".to_string(),
                    bashrc_path: "/tmp/.bashrc".to_string(),
                }),
            ),
        }];

        let epochs = derive_stage1_epochs_from_lines(&claim, &lines);

        assert_eq!(epochs.len(), 1);
        assert_eq!(
            epochs[0].provenance,
            Stage1EpochProvenance::EmptyDerivationFallback
        );
        assert!(epochs[0].raw_memory.contains("provenance: empty_derivation_fallback"));
    }

    #[test]
    fn shared_environment_parser_drives_epoch_counting() {
        let baseline = unix_environment_snapshot(
            "/tmp/project",
            "/tmp/project",
            crate::shell::Shell::Bash(crate::shell::BashShell {
                shell_path: "/bin/bash".to_string(),
                bashrc_path: "/tmp/.bashrc".to_string(),
            }),
        );
        let changed = unix_environment_snapshot(
            "/tmp/project",
            "/tmp/project",
            crate::shell::Shell::Zsh(crate::shell::ZshShell {
                shell_path: "/bin/zsh".to_string(),
                zshrc_path: "/tmp/.zshrc".to_string(),
            }),
        );
        let env_items = vec![
            env_snapshot("/tmp/project", "/tmp/project", crate::shell::Shell::Bash(
                crate::shell::BashShell {
                    shell_path: "/bin/bash".to_string(),
                    bashrc_path: "/tmp/.bashrc".to_string(),
                },
            )),
            env_delta(&baseline, &changed),
            legacy_system_status_message("/tmp/project", "main"),
        ];

        for item in &env_items {
            assert!(
                parse_environment_context_update_from_rollout_item(item).is_some(),
                "expected environment item to parse: {item:?}"
            );
            assert!(
                !should_count_toward_epoch(item),
                "environment items must not count toward epoch spans: {item:?}"
            );
        }

        let user_item = user_message("remember actual content");
        assert!(parse_environment_context_update_from_rollout_item(&user_item).is_none());
        assert!(should_count_toward_epoch(&user_item));
    }

    #[test]
    fn rendered_outputs_include_provenance_metadata() {
        let session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let claim = make_claim(session_id, now, Some("remember fallback"));
        let epoch = EpochAccumulator {
            context: EpochContextKey {
                platform_family: MemoryPlatformFamily::Unix,
                shell_style: MemoryShellStyle::Zsh,
                shell_program: Some("zsh".to_string()),
                workspace_root: Some("/tmp/project".to_string()),
            },
            epoch_start_at: Some(now.timestamp()),
            epoch_end_at: Some(now.timestamp()),
            epoch_start_line: 1,
            epoch_end_line: 2,
            cwd_display: "~/project".to_string(),
            git_branch: Some("main".to_string()),
            last_user_snippet: Some("remember fallback".to_string()),
        };
        let raw_memory = render_raw_memory_body(
            &claim,
            0,
            &epoch,
            Stage1EpochProvenance::CatalogFallback,
            "demo-rollout",
            "remember fallback",
        );
        let rollout_summary = render_rollout_summary_body(
            &claim,
            0,
            &epoch,
            Stage1EpochProvenance::CatalogFallback,
            "remember fallback",
        );
        let record = Stage1EpochRecord {
            id: code_memories_state::MemoryEpochId {
                thread_id: session_id,
                epoch_index: 0,
            },
            provenance: Stage1EpochProvenance::CatalogFallback,
            source_updated_at: now.timestamp(),
            updated_at_label: now.to_rfc3339(),
            generated_at: now.timestamp(),
            epoch_start_at: Some(now.timestamp()),
            epoch_end_at: Some(now.timestamp()),
            epoch_start_line: 1,
            epoch_end_line: 2,
            platform_family: MemoryPlatformFamily::Unix,
            shell_style: MemoryShellStyle::Zsh,
            shell_program: Some("zsh".to_string()),
            workspace_root: Some("/tmp/project".to_string()),
            cwd: PathBuf::from("/tmp/project"),
            cwd_display: "~/project".to_string(),
            git_branch: Some("main".to_string()),
            rollout_path: PathBuf::from(format!("sessions/{session_id}.jsonl")),
            raw_memory: raw_memory.clone(),
            rollout_summary: rollout_summary.clone(),
            rollout_slug: "demo-rollout".to_string(),
            usage_count: 0,
            last_usage: None,
        };

        let summary = render_memory_summary(std::slice::from_ref(&record));
        let prompt_entry = render_prompt_entry(&record);

        assert!(raw_memory.contains("provenance: catalog_fallback"));
        assert!(rollout_summary.contains("provenance: catalog_fallback"));
        assert!(summary.contains("provenance: catalog_fallback"));
        assert!(prompt_entry.contains("provenance: catalog_fallback"));
    }

    #[tokio::test]
    async fn manual_refresh_bypasses_stage1_retry_backoff() {
        let _guard = lock_publish_tests().await;
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let meta_at = now - chrono::TimeDelta::hours(6);
        let user_at = now - chrono::TimeDelta::hours(5);
        let rollout_rel_path = rollout_rel_path(session_id, meta_at);

        let mut catalog = SessionCatalog::load(code_home).expect("load catalog");
        let mut entry = make_entry(session_id, SessionSource::Cli, &user_at.to_rfc3339(), false, false, None);
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

        refresh_memory_artifacts_from_catalog(code_home, &settings, false)
            .await
            .expect("initial refresh completes despite stage1 failure");

        let state = MemoriesState::open(code_home).await.expect("open state");
        let status = state
            .status(crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("status after failed stage1");
        assert_eq!(status.stage1_epoch_count, 0);
        assert_eq!(status.pending_stage1_count, 0);

        let written_rollout_path = write_rollout_lines(
            code_home,
            session_id,
            meta_at,
            vec![(user_at, user_message("remember this"))],
        )
        .await
        .expect("write rollout");
        assert_eq!(written_rollout_path, rollout_rel_path);
        let mut catalog = SessionCatalog::load(code_home).expect("reload catalog");
        let entry = catalog.entries.get_mut(&session_id).expect("entry");
        entry.last_user_snippet = Some("remember this".to_string());
        catalog.save().expect("save catalog snippet");

        refresh_memory_artifacts_from_catalog(code_home, &settings, true)
            .await
            .expect("manual refresh bypasses backoff");

        let status = state
            .status(crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("status after manual refresh");
        assert_eq!(status.stage1_epoch_count, 1);
    }

    #[tokio::test]
    async fn dead_lettered_stage1_jobs_are_skipped_by_normal_refresh_and_forced_refresh_recovers() {
        let _guard = lock_publish_tests().await;
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let session_id = Uuid::new_v4();
        let now = DateTime::<Utc>::from_timestamp_secs(Utc::now().timestamp()).expect("time");
        let meta_at = now - chrono::TimeDelta::hours(6);
        let user_at = now - chrono::TimeDelta::hours(5);
        let rollout_rel_path = rollout_rel_path(session_id, meta_at);

        let mut catalog = SessionCatalog::load(code_home).expect("load catalog");
        let mut entry = make_entry(
            session_id,
            SessionSource::Cli,
            &user_at.to_rfc3339(),
            false,
            false,
            None,
        );
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

        for _ in 0..STAGE1_TERMINAL_FAILURE_THRESHOLD {
            refresh_memory_artifacts_from_catalog(code_home, &settings, true)
                .await
                .expect("forced refresh while rollout is broken");
        }

        let state = MemoriesState::open(code_home).await.expect("open state");
        let status = state
            .status(crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("status after dead letter");
        assert_eq!(status.stage1_epoch_count, 0);
        assert_eq!(status.dead_lettered_stage1_count, 1);
        assert_eq!(status.pending_stage1_count, 0);

        refresh_memory_artifacts_from_catalog(code_home, &settings, false)
            .await
            .expect("normal refresh after dead letter");

        let status = state
            .status(crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("status after normal refresh");
        assert_eq!(status.stage1_epoch_count, 0);
        assert_eq!(status.dead_lettered_stage1_count, 1);
        assert_eq!(status.pending_stage1_count, 0);

        let written_rollout_path = write_rollout_lines(
            code_home,
            session_id,
            meta_at,
            vec![
                (
                    user_at - chrono::TimeDelta::minutes(1),
                    env_snapshot_with_branch(
                        "linux",
                        "/tmp/project",
                        "/tmp/project",
                        crate::shell::Shell::Zsh(crate::shell::ZshShell {
                            shell_path: "/bin/zsh".to_string(),
                            zshrc_path: "/tmp/.zshrc".to_string(),
                        }),
                        "main",
                    ),
                ),
                (user_at, user_message("remember recovered rollout")),
            ],
        )
        .await
        .expect("write repaired rollout");
        assert_eq!(written_rollout_path, rollout_rel_path);

        let mut catalog = SessionCatalog::load(code_home).expect("reload catalog");
        let entry = catalog.entries.get_mut(&session_id).expect("entry");
        entry.last_user_snippet = None;
        catalog.save().expect("save repaired catalog");

        refresh_memory_artifacts_from_catalog(code_home, &settings, true)
            .await
            .expect("forced refresh retries dead-lettered job");

        let status = state
            .status(crate::rollout::INTERACTIVE_SESSION_SOURCES)
            .await
            .expect("status after recovery");
        assert_eq!(status.stage1_epoch_count, 1);
        assert_eq!(status.dead_lettered_stage1_count, 0);
    }

    #[tokio::test]
    async fn publish_switches_active_snapshot_without_clearing_previous_one_first() {
        let _guard = lock_publish_tests().await;
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let memories_root = code_home.join("memories");
        let first = MemoryArtifacts {
            memory_summary: "first summary".to_string(),
            raw_memories: "first raw".to_string(),
            rollout_summaries: HashMap::from([("first.md".to_string(), "first rollout".to_string())]),
            manifest: serde_json::to_string(&SnapshotManifest::new(Vec::new())).expect("manifest"),
        };
        write_memory_artifacts(code_home, first)
            .await
            .expect("write first snapshot");
        let first_paths = published_artifact_paths(code_home).expect("first published paths");
        let first_generation = first_paths.generation.clone().expect("first generation");

        let second = MemoryArtifacts {
            memory_summary: "second summary".to_string(),
            raw_memories: "second raw".to_string(),
            rollout_summaries: HashMap::from([("second.md".to_string(), "second rollout".to_string())]),
            manifest: serde_json::to_string(&SnapshotManifest::new(Vec::new())).expect("manifest"),
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
        assert_ne!(second_paths.generation.as_deref(), Some(first_generation.as_str()));
        assert!(
            !tokio::fs::try_exists(memories_root.join("snapshots").join(first_generation))
                .await
                .expect("stat pruned snapshot")
        );
    }

    #[tokio::test]
    async fn failed_publish_before_pointer_swap_keeps_previous_snapshot_active() {
        let _guard = lock_publish_tests().await;
        let temp = tempdir().expect("tempdir");
        let code_home = temp.path();
        let first = MemoryArtifacts {
            memory_summary: "stable summary".to_string(),
            raw_memories: "stable raw".to_string(),
            rollout_summaries: HashMap::new(),
            manifest: serde_json::to_string(&SnapshotManifest::new(Vec::new())).expect("manifest"),
        };
        write_memory_artifacts(code_home, first)
            .await
            .expect("write first snapshot");
        let before = published_artifact_paths(code_home).expect("published before failure");

        FAIL_BEFORE_POINTER_SWAP.store(true, Ordering::SeqCst);
        let failing = MemoryArtifacts {
            memory_summary: "new summary".to_string(),
            raw_memories: "new raw".to_string(),
            rollout_summaries: HashMap::new(),
            manifest: serde_json::to_string(&SnapshotManifest::new(Vec::new())).expect("manifest"),
        };
        let err = write_memory_artifacts(code_home, failing)
            .await
            .expect_err("publish should fail before pointer swap");
        assert!(err.to_string().contains("injected memories publish failure"));

        let after = published_artifact_paths(code_home).expect("published after failure");
        assert_eq!(before.generation, after.generation);
        assert_eq!(
            tokio::fs::read_to_string(&after.summary_path)
                .await
                .expect("read active summary after failure"),
            "stable summary"
        );
    }
}
