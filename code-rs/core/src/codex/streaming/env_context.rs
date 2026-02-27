use super::*;

pub(in crate::codex) fn debug_history(label: &str, items: &[ResponseItem]) {
    let preview: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| match item {
            ResponseItem::Message { role, content, .. } => {
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                            Some(text.as_str())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let snippet: String = text.chars().take(80).collect();
                format!("{idx}:{role}:{snippet}")
            }
            _ => format!("{idx}:{item:?}"),
        })
        .collect();
    let rendered = preview.join(" | ");
    if std::env::var_os("CODEX_COMPACT_TRACE").is_some() {
        tracing::debug!("[compact_history] {label} => [{rendered}]");
    }
    info!(target = "code_core::compact_history", "{} => [{}]", label, rendered);
}

#[derive(Debug)]
pub(in crate::codex) struct TimelineReplayContext {
    pub(in crate::codex) timeline: ContextTimeline,
    pub(in crate::codex) next_sequence: u64,
    pub(in crate::codex) last_snapshot: Option<EnvironmentContextSnapshot>,
    pub(in crate::codex) legacy_baseline: Option<EnvironmentContextSnapshot>,
}

impl Default for TimelineReplayContext {
    fn default() -> Self {
        Self {
            timeline: ContextTimeline::new(),
            next_sequence: 1,
            last_snapshot: None,
            legacy_baseline: None,
        }
    }
}

pub(in crate::codex) fn process_rollout_env_item(ctx: &mut TimelineReplayContext, item: &ResponseItem) {
    if let Some(snapshot) = parse_env_snapshot_from_response(item) {
        if ctx.timeline.baseline().is_none()
            && let Err(err) = ctx.timeline.add_baseline_once(snapshot.clone())
        {
            tracing::warn!("env_ctx_v2: failed to seed baseline during replay: {err}");
        }

        match ctx.timeline.record_snapshot(snapshot.clone()) {
            Ok(true) => crate::telemetry::global_telemetry().record_snapshot_commit(),
            Ok(false) => crate::telemetry::global_telemetry().record_dedup_drop(),
            Err(err) => tracing::warn!("env_ctx_v2: failed to record snapshot during replay: {err}"),
        }

        ctx.last_snapshot = Some(snapshot);
        return;
    }

    if let Some(delta) = parse_env_delta_from_response(item) {
        if let Some(base_snapshot) = ctx.last_snapshot.clone() {
            if delta.base_fingerprint != base_snapshot.fingerprint() {
                tracing::warn!(
                    "env_ctx_v2: delta base fingerprint mismatch during replay; requesting baseline resend"
                );
                crate::telemetry::global_telemetry().record_baseline_resend();
                crate::telemetry::global_telemetry().record_delta_gap();
                ctx.timeline = ContextTimeline::new();
                ctx.last_snapshot = None;
                ctx.legacy_baseline = None;
                ctx.next_sequence = 1;
                return;
            }

            let sequence = ctx.next_sequence;
            match ctx.timeline.apply_delta(sequence, delta.clone()) {
                Ok(_) => {
                    ctx.next_sequence = ctx.next_sequence.saturating_add(1);
                }
                Err(err) => {
                    tracing::warn!("env_ctx_v2: failed to apply delta during replay: {err}");
                    crate::telemetry::global_telemetry().record_delta_gap();
                    return;
                }
            }

            let next_snapshot = base_snapshot.apply_delta(&delta);
            match ctx.timeline.record_snapshot(next_snapshot.clone()) {
                Ok(true) => crate::telemetry::global_telemetry().record_snapshot_commit(),
                Ok(false) => crate::telemetry::global_telemetry().record_dedup_drop(),
                Err(err) => tracing::warn!("env_ctx_v2: failed to record snapshot during replay: {err}"),
            }

            ctx.last_snapshot = Some(next_snapshot);
        } else {
            tracing::warn!("env_ctx_v2: encountered delta before baseline while replaying rollout");
            crate::telemetry::global_telemetry().record_delta_gap();
        }
        return;
    }

    if ctx.legacy_baseline.is_none()
        && is_legacy_system_status(item)
        && let Some(snapshot) = parse_legacy_status_snapshot(item)
    {
        ctx.legacy_baseline = Some(snapshot);
    }
}

fn extract_tagged_json<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = text.rfind(close)?;
    if end <= start {
        return None;
    }
    Some(text[start..end].trim())
}

pub(in crate::codex) fn parse_env_snapshot_from_response(
    item: &ResponseItem,
) -> Option<EnvironmentContextSnapshot> {
    if let ResponseItem::Message { role, content, .. } = item {
        if role != "user" {
            return None;
        }
        for piece in content {
            if let ContentItem::InputText { text } = piece
                && let Some(json) = extract_tagged_json(
                    text,
                    ENVIRONMENT_CONTEXT_OPEN_TAG,
                    ENVIRONMENT_CONTEXT_CLOSE_TAG,
                )
                && let Ok(snapshot) = serde_json::from_str::<EnvironmentContextSnapshot>(json)
            {
                return Some(snapshot);
            }
        }
    }
    None
}

pub(in crate::codex) fn parse_env_delta_from_response(
    item: &ResponseItem,
) -> Option<EnvironmentContextDelta> {
    if let ResponseItem::Message { role, content, .. } = item {
        if role != "user" {
            return None;
        }
        for piece in content {
            if let ContentItem::InputText { text } = piece
                && let Some(json) = extract_tagged_json(
                    text,
                    ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG,
                    ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG,
                )
                && let Ok(delta) = serde_json::from_str::<EnvironmentContextDelta>(json)
            {
                return Some(delta);
            }
        }
    }
    None
}

fn is_legacy_system_status(item: &ResponseItem) -> bool {
    if let ResponseItem::Message { role, content, .. } = item {
        if role != "user" {
            return false;
        }
        return content.iter().any(|c| {
            if let ContentItem::InputText { text } = c {
                text.contains("== System Status ==")
            } else {
                false
            }
        });
    }
    false
}

fn parse_legacy_status_snapshot(item: &ResponseItem) -> Option<EnvironmentContextSnapshot> {
    if let ResponseItem::Message { role, content, .. } = item {
        if role != "user" {
            return None;
        }
        for piece in content {
            if let ContentItem::InputText { text } = piece {
                if !text.contains("== System Status ==") {
                    continue;
                }

                let mut cwd: Option<String> = None;
                let mut branch: Option<String> = None;
                for line in text.lines() {
                    let trimmed = line.trim();
                    if let Some(rest) = trimmed.strip_prefix("cwd:") {
                        let value = rest.trim();
                        if !value.is_empty() {
                            cwd = Some(value.to_string());
                        }
                    } else if let Some(rest) = trimmed.strip_prefix("branch:") {
                        let value = rest.trim();
                        if !value.is_empty() && value != "unknown" {
                            branch = Some(value.to_string());
                        }
                    }
                }

                return Some(EnvironmentContextSnapshot {
                    version: EnvironmentContextSnapshot::VERSION,
                    cwd,
                    approval_policy: None,
                    sandbox_mode: None,
                    network_access: None,
                    writable_roots: Vec::new(),
                    operating_system: None,
                    common_tools: Vec::new(),
                    shell: None,
                    git_branch: branch,
                    reasoning_effort: None,
                });
            }
        }
    }
    None
}
