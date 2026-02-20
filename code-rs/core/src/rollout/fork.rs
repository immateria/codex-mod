use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;

use crate::config::Config;
use crate::rollout::recorder::RolloutRecorderParams;
use crate::rollout::RolloutRecorder;
use code_protocol::ConversationId;
use code_protocol::ThreadId;
use code_protocol::protocol::InitialHistory;
use code_protocol::protocol::RolloutItem;

pub async fn fork_rollout(config: &Config, source_rollout: &Path) -> Result<PathBuf> {
    let history = RolloutRecorder::get_rollout_history(source_rollout)
        .await
        .with_context(|| format!("failed to read rollout history from {}", source_rollout.display()))?;

    let source_thread_id = source_thread_id(&history)
        .ok_or_else(|| anyhow!("failed to determine source session id from rollout"))?;
    let source_cwd = history
        .session_cwd()
        .ok_or_else(|| anyhow!("failed to determine source session cwd from rollout"))?;

    let base_instructions = history.get_base_instructions().map(|instr| instr.text);

    let mut fork_config = config.clone();
    fork_config.cwd = source_cwd;

    let convo_id = ConversationId::new();
    let recorder = RolloutRecorder::new(
        &fork_config,
        RolloutRecorderParams::new_with_forked_from(
            convo_id,
            base_instructions,
            source_session_source(&history).unwrap_or_default(),
            source_thread_id,
        ),
    )
    .await
    .context("failed to create rollout recorder for fork")?;

    let mut items = history.get_rollout_items();
    // The new rollout recorder writes its own SessionMeta; avoid duplicating it.
    items.retain(|item| !matches!(item, RolloutItem::SessionMeta(_)));

    if !items.is_empty() {
        recorder
            .record_items(items.as_slice())
            .await
            .context("failed to persist forked rollout items")?;
    }

    // Ensure rollout writer flushes before we return the path to callers.
    recorder
        .shutdown()
        .await
        .context("failed to flush fork rollout file")?;

    // Best-effort: copy any existing snapshot.json so resume is instant.
    let source_snapshot = source_rollout.with_extension("snapshot.json");
    let fork_snapshot = recorder.rollout_path.with_extension("snapshot.json");
    match tokio::fs::copy(&source_snapshot, &fork_snapshot).await {
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            tracing::warn!(
                "failed to copy snapshot {} -> {}: {err}",
                source_snapshot.display(),
                fork_snapshot.display()
            );
        }
    }

    Ok(recorder.rollout_path)
}

fn source_thread_id(history: &InitialHistory) -> Option<ThreadId> {
    match history {
        InitialHistory::New => None,
        InitialHistory::Resumed(resumed) => Some(resumed.conversation_id),
        InitialHistory::Forked(items) => items.iter().find_map(|item| match item {
            RolloutItem::SessionMeta(meta_line) => Some(meta_line.meta.id),
            _ => None,
        }),
    }
}

fn source_session_source(history: &InitialHistory) -> Option<code_protocol::protocol::SessionSource> {
    match history {
        InitialHistory::New => None,
        InitialHistory::Resumed(resumed) => resumed.history.iter().find_map(|item| match item {
            RolloutItem::SessionMeta(meta_line) => Some(meta_line.meta.source.clone()),
            _ => None,
        }),
        InitialHistory::Forked(items) => items.iter().find_map(|item| match item {
            RolloutItem::SessionMeta(meta_line) => Some(meta_line.meta.source.clone()),
            _ => None,
        }),
    }
}

