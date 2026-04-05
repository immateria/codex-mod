use super::super::*;

impl ChatWidget<'_> {
    pub(in crate::chatwidget) fn auto_review_git_root(&self) -> Option<PathBuf> {
        self.run_git_command(["rev-parse", "--show-toplevel"], |stdout| {
            let root = stdout.lines().next().unwrap_or("").trim();
            if root.is_empty() {
                Err("auto review git root unavailable".to_string())
            } else {
                Ok(root.to_string())
            }
        })
        .ok()
        .map(PathBuf::from)
    }

    pub(in crate::chatwidget) fn auto_review_baseline_path(&self) -> Option<PathBuf> {
        let git_root = self.auto_review_git_root()?;
        match auto_review_baseline_path_for_repo(&git_root) {
            Ok(path) => Some(path),
            Err(err) => {
                tracing::warn!("failed to resolve auto review baseline path: {err}");
                None
            }
        }
    }

    pub(in crate::chatwidget) fn load_auto_review_baseline_marker(&mut self) {
        if self.auto_review_reviewed_marker.is_some() {
            return;
        }
        let Some(path) = self.auto_review_baseline_path() else {
            return;
        };
        let Ok(contents) = std::fs::read_to_string(&path) else {
            return;
        };
        let commit_id = contents.lines().next().unwrap_or("").trim();
        if commit_id.is_empty() {
            return;
        }
        self.auto_review_reviewed_marker = Some(GhostCommit::new(commit_id.to_string(), None));
    }

    pub(in crate::chatwidget) fn persist_auto_review_baseline_marker(&self, commit_id: &str) {
        let commit_id = commit_id.trim();
        if commit_id.is_empty() {
            return;
        }
        let Some(path) = self.auto_review_baseline_path() else {
            return;
        };
        if let Some(parent) = path.parent()
            && let Err(err) = std::fs::create_dir_all(parent) {
                tracing::warn!("failed to create auto review baseline dir: {err}");
                return;
            }
        if let Err(err) = std::fs::write(&path, format!("{commit_id}\n")) {
            tracing::warn!("failed to persist auto review baseline: {err}");
        }
    }

    pub(in crate::chatwidget) fn maybe_trigger_auto_review(&mut self) {
        if !self.config.tui.auto_review_enabled {
            return;
        }
        self.recover_stuck_background_review();

        if !self.turn_had_code_edits && self.pending_auto_review_range.is_none() {
            return;
        }
        if matches!(self.current_turn_origin, Some(TurnOrigin::Developer)) {
            return;
        }

        if self.pending_auto_review_deferred_for_current_turn() {
            return;
        }

        if let Some(reviewed) = self.auto_review_reviewed_marker.as_ref()
            && !self.auto_review_has_changes_since(reviewed) {
                return;
            }

        if self.background_review.is_some() || self.is_review_flow_active() {
            if let Some(base) = self.take_or_capture_auto_review_baseline() {
                self.queue_skipped_auto_review(base);
            }
            return;
        }

        let base_snapshot = if let Some(base) = self.take_ready_pending_range_base() {
            Some(base)
        } else {
            self.take_or_capture_auto_review_baseline()
        };

        if let Some(base) = base_snapshot {
            self.launch_background_review(Some(base));
        }
    }

    pub(in crate::chatwidget) fn auto_review_has_changes_since(&self, reviewed: &GhostCommit) -> bool {
        let reviewed_id = reviewed.id();
        let tracked_changes = match self.run_git_command(
            ["diff", "--name-only", reviewed_id],
            |stdout| {
                Ok(stdout.lines().any(|line| !line.trim().is_empty()))
            },
        ) {
            Ok(changed) => changed,
            Err(err) => {
                tracing::warn!("auto review diff failed for {reviewed_id}: {err}");
                return true;
            }
        };

        if tracked_changes {
            return true;
        }

        let snapshot_paths = match self.run_git_command(
            ["ls-tree", "-r", "--name-only", reviewed_id],
            |stdout| {
                let mut paths = HashSet::new();
                for line in stdout.lines().map(str::trim) {
                    if !line.is_empty() {
                        paths.insert(line.to_string());
                    }
                }
                Ok(paths)
            },
        ) {
            Ok(paths) => paths,
            Err(err) => {
                tracing::warn!("auto review snapshot listing failed for {reviewed_id}: {err}");
                return true;
            }
        };

        

        match self.run_git_command(
            ["ls-files", "--others", "--exclude-standard"],
            |stdout| {
                Ok(stdout
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .any(|path| !snapshot_paths.contains(path)))
            },
        ) {
            Ok(changed) => changed,
            Err(err) => {
                tracing::warn!("auto review untracked check failed: {err}");
                true
            }
        }
    }

    pub(in crate::chatwidget) fn pending_auto_review_deferred_for_current_turn(&self) -> bool {
        matches!(
            self.pending_auto_review_range.as_ref(),
            Some(range)
                if matches!(range.defer_until_turn, Some(turn) if turn == self.turn_sequence)
        )
    }

    pub(in crate::chatwidget) fn take_ready_pending_range_base(&mut self) -> Option<GhostCommit> {
        if let Some(range) = self.pending_auto_review_range.as_ref()
            && let Some(turn) = range.defer_until_turn
                && turn == self.turn_sequence {
                    return None;
                }
        self.pending_auto_review_range.take().map(|range| range.base)
    }

    pub(in crate::chatwidget) fn take_or_capture_auto_review_baseline(&mut self) -> Option<GhostCommit> {
        if let Some(existing) = self.auto_review_baseline.take() {
            return Some(existing);
        }
        match self.capture_auto_turn_commit("auto review baseline snapshot", None) {
            Ok(commit) => Some(commit),
            Err(err) => {
                tracing::warn!("failed to capture auto review baseline: {err}");
                self.auto_review_reviewed_marker.clone()
            }
        }
    }

    pub(in crate::chatwidget) fn queue_skipped_auto_review(&mut self, base: GhostCommit) {
        if self.pending_auto_review_range.is_some() {
            return;
        }
        self.pending_auto_review_range = Some(PendingAutoReviewRange {
            base,
            defer_until_turn: None,
        });
    }

    pub(in crate::chatwidget) fn recover_stuck_background_review(&mut self) {
        let Some(state) = self.background_review.as_ref() else {
            return;
        };

        let elapsed = state.last_seen.elapsed();
        if elapsed.as_secs() < AUTO_REVIEW_STALE_SECS {
            return;
        }

        let stale = self.background_review.take();
        self.background_review_guard = None;

        let Some(stale) = stale else {
            return;
        };

        if self.pending_auto_review_range.is_none()
            && let Some(base) = stale.base {
                self.pending_auto_review_range = Some(PendingAutoReviewRange {
                    base,
                    defer_until_turn: None,
                });
            }
    }

    pub(in crate::chatwidget) fn launch_background_review(&mut self, base_snapshot: Option<GhostCommit>) {
        // Record state immediately to avoid duplicate launches when multiple
        // TaskComplete events fire in quick succession.
        self.turn_had_code_edits = false;
        let had_notice = self.auto_review_notice.is_some();
        let had_fixed_indicator = matches!(
            self.auto_review_status,
            Some(AutoReviewStatus { status: AutoReviewIndicatorStatus::Fixed, .. })
        );
        self.background_review = Some(BackgroundReviewState {
            worktree_path: std::path::PathBuf::new(),
            branch: String::new(),
            agent_id: None,
            snapshot: None,
            base: base_snapshot.clone(),
            last_seen: std::time::Instant::now(),
        });
        self.auto_review_status = None;
        self.bottom_pane.set_auto_review_status(None);
        self.auto_review_notice = None;
        self.set_auto_review_indicator(
            AutoReviewIndicatorStatus::Running,
            None,
            AutoReviewPhase::Reviewing,
        );

        #[cfg(test)]
        if let Some(stub) = AUTO_REVIEW_STUB.lock().unwrap().as_mut() {
            (stub)();
            return;
        }

        let config = self.config.clone();
        let app_event_tx = self.app_event_tx.clone();
        let base_snapshot_for_task = base_snapshot;
        let turn_context = self.turn_context_block();
        let prefer_fallback = had_notice || had_fixed_indicator;
        tokio::spawn(async move {
            run_background_review(
                config,
                app_event_tx,
                base_snapshot_for_task,
                turn_context,
                prefer_fallback,
            )
            .await;
        });
    }

    pub(in crate::chatwidget) fn observe_auto_review_status(&mut self, agents: &[code_core::protocol::AgentInfo]) {
        let now = Instant::now();
        for agent in agents {
            if !Self::is_auto_review_agent(agent) {
                continue;
            }

            if let Some(progress) = agent.last_progress.as_deref()
                && progress.contains(SKIP_REVIEW_PROGRESS_SENTINEL) {
                    // Treat skipped review as benign: clear indicator and state, do not surface.
                    self.clear_auto_review_indicator();
                    self.background_review = None;
                    self.background_review_guard = None;
                    self.processed_auto_review_agents.insert(agent.id.clone());
                    continue;
                }

            if let Some(state) = self.background_review.as_mut() {
                state.last_seen = now;
                if state.agent_id.is_none() {
                    state.agent_id = Some(agent.id.clone());
                }
                if state.branch.is_empty()
                    && let Some(batch) = agent.batch_id.as_ref() {
                        state.branch = batch.clone();
                    }
            }

            let status = agent_status_from_str(agent.status.as_str());
            let is_terminal = matches!(
                status,
                AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Cancelled
            );
            let phase = detect_auto_review_phase(agent.last_progress.as_deref());

            if matches!(status, AgentStatus::Running | AgentStatus::Pending) {
                let findings = self.auto_review_status.and_then(|s| s.findings);
                self.set_auto_review_indicator(
                    AutoReviewIndicatorStatus::Running,
                    findings,
                    phase,
                );
                continue;
            }

            if let Some(mut state) = self.auto_review_status {
                state.phase = phase;
                self.auto_review_status = Some(state);
                self.bottom_pane.set_auto_review_status(Some(AutoReviewFooterStatus {
                    status: state.status,
                    findings: state.findings,
                    phase,
                }));
            }

            if is_terminal && self.processed_auto_review_agents.contains(&agent.id) {
                continue;
            }
            if !is_terminal {
                continue;
            }

            let (worktree_path, branch, snapshot) = if let Some(state) = self.background_review.as_ref() {
                (
                    state.worktree_path.clone(),
                    state.branch.clone(),
                    state.snapshot.clone(),
                )
            } else {
                let Some(branch) = agent
                    .batch_id
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                else {
                    // We sometimes observe the same terminal auto-review agent multiple
                    // times (especially after cancellation/resend). If the background
                    // review state is already cleared and we cannot resolve the worktree,
                    // do not surface a misleading blank-path error.
                    self.processed_auto_review_agents.insert(agent.id.clone());
                    continue;
                };
                let Some(worktree_path) =
                    resolve_auto_review_worktree_path(&self.config.cwd, &branch)
                else {
                    self.processed_auto_review_agents.insert(agent.id.clone());
                    continue;
                };
                (worktree_path, branch, None)
            };

            let (has_findings, findings, summary) = Self::parse_agent_review_result(agent.result.as_deref());

            self.processed_auto_review_agents.insert(agent.id.clone());
            self.on_background_review_finished(BackgroundReviewFinishedEvent {
                worktree_path,
                branch,
                has_findings,
                findings,
                summary,
                error: agent.error.clone(),
                agent_id: Some(agent.id.clone()),
                snapshot,
            });
        }
    }

    /// Parse the auto-review agent result to derive findings count and a concise summary.
    /// Tries to deserialize `ReviewOutputEvent` JSON (direct or fenced). Falls back to heuristics.
    pub(in crate::chatwidget) fn parse_agent_review_result(raw: Option<&str>) -> (bool, usize, Option<String>) {
        const MAX_AUTO_REVIEW_FALLBACK_SUMMARY_CHARS: usize = 280;

        let Some(text) = raw else { return (false, 0, None); };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return (false, 0, None);
        }

        #[derive(serde::Deserialize)]
        struct MultiRunReview {
            #[serde(flatten)]
            latest: ReviewOutputEvent,
            #[serde(default)]
            runs: Vec<ReviewOutputEvent>,
        }

        // Try multi-run JSON first (our /review output that preserves all passes).
        if let Ok(wrapper) = serde_json::from_str::<MultiRunReview>(trimmed) {
            let mut runs = wrapper.runs;
            if runs.is_empty() {
                runs.push(wrapper.latest);
            }
            return Self::review_result_from_runs(&runs);
        }

        // Try direct JSON first.
        if let Ok(output) = serde_json::from_str::<ReviewOutputEvent>(trimmed) {
            return Self::review_result_from_output(&output);
        }

        // Some runners prepend logs before printing JSON. Scan for embedded JSON
        // objects and prefer the latest parseable review payload.
        if let Some((has_findings, findings, summary)) =
            Self::extract_review_from_mixed_text(trimmed)
        {
            return (has_findings, findings, summary);
        }

        // Try to extract JSON from fenced code blocks.
        if let Some(start) = trimmed.find("```")
            && let Some((body, _)) = trimmed[start + 3..].split_once("```") {
                let candidate = body.trim_start_matches("json").trim();
                if let Ok(output) = serde_json::from_str::<ReviewOutputEvent>(candidate) {
                    return Self::review_result_from_output(&output);
                }
            }

        // Heuristic: treat plain text as summary; infer findings only when the text
        // explicitly mentions issues. Avoid false positives for skip/lock messages.
        let lowered = trimmed.to_ascii_lowercase();
        let clean_phrases = ["no issues", "no findings", "clean", "looks good", "nothing to fix"];
        let skip_phrases = ["already running", "another review", "skipping this", "skip this"];
        let issue_markers = ["issue", "issues", "finding", "findings", "bug", "bugs", "problem", "problems", "error", "errors"]; // keep broad but guarded

        if skip_phrases.iter().any(|p| lowered.contains(p)) {
            return (
                false,
                0,
                Some(Self::summarize_plain_review_text(
                    trimmed,
                    MAX_AUTO_REVIEW_FALLBACK_SUMMARY_CHARS,
                )),
            );
        }

        if clean_phrases.iter().any(|p| lowered.contains(p)) {
            return (
                false,
                0,
                Some(Self::summarize_plain_review_text(
                    trimmed,
                    MAX_AUTO_REVIEW_FALLBACK_SUMMARY_CHARS,
                )),
            );
        }

        let has_findings = issue_markers.iter().any(|p| lowered.contains(p));
        (
            has_findings,
            0,
            Some(Self::summarize_plain_review_text(
                trimmed,
                MAX_AUTO_REVIEW_FALLBACK_SUMMARY_CHARS,
            )),
        )
    }

    fn extract_review_from_mixed_text(text: &str) -> Option<(bool, usize, Option<String>)> {
        #[derive(serde::Deserialize)]
        struct MultiRunReview {
            #[serde(flatten)]
            latest: ReviewOutputEvent,
            #[serde(default)]
            runs: Vec<ReviewOutputEvent>,
        }

        let mut brace_depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let mut object_start = None;
        let mut best_match: Option<(bool, usize, Option<String>)> = None;

        for (idx, ch) in text.char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == '"' {
                    in_string = false;
                }
                continue;
            }

            match ch {
                '"' => in_string = true,
                '{' => {
                    if brace_depth == 0 {
                        object_start = Some(idx);
                    }
                    brace_depth = brace_depth.saturating_add(1);
                }
                '}' => {
                    if brace_depth == 0 {
                        continue;
                    }
                    brace_depth -= 1;
                    if brace_depth == 0
                        && let Some(start) = object_start.take()
                    {
                        let candidate = &text[start..=idx];
                        if let Ok(wrapper) = serde_json::from_str::<MultiRunReview>(candidate) {
                            let mut runs = wrapper.runs;
                            if runs.is_empty() {
                                runs.push(wrapper.latest);
                            }
                            best_match = Some(Self::review_result_from_runs(&runs));
                            continue;
                        }
                        if let Ok(output) = serde_json::from_str::<ReviewOutputEvent>(candidate) {
                            best_match = Some(Self::review_result_from_output(&output));
                        }
                    }
                }
                _ => {}
            }
        }

        best_match
    }

    fn summarize_plain_review_text(text: &str, max_chars: usize) -> String {
        let mut line = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or(text)
            .replace(['\n', '\r'], " ");

        line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.chars().count() <= max_chars {
            return line;
        }

        let truncated: String = line.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}…")
    }

    pub(in crate::chatwidget) fn review_result_from_runs(outputs: &[ReviewOutputEvent]) -> (bool, usize, Option<String>) {
        if outputs.is_empty() {
            return (false, 0, None);
        }

        let Some(last) = outputs.last() else {
            return (false, 0, None);
        };
        let last_with_findings_idx = outputs
            .iter()
            .rposition(|o| !o.findings.is_empty());

        let (mut has_findings, mut findings_len, mut summary) = Self::review_result_from_output(last);

        if let Some(idx) = last_with_findings_idx {
            let with_findings = &outputs[idx];
            let (has, len, summary_with_findings) = Self::review_result_from_output(with_findings);
            has_findings |= has;
            findings_len = len;
            summary = summary_with_findings.or(summary);

            // If fixes cleared the final pass, note that in the summary.
            if last.findings.is_empty() {
                let tail = "Final pass reported no issues after auto-resolve.";
                summary = Some(match summary {
                    Some(existing) if existing.contains(tail) => existing,
                    Some(existing) => format!("{existing} \n{tail}"),
                    None => tail.to_string(),
                });
            }
        }

        (has_findings, findings_len, summary)
    }

    pub(in crate::chatwidget) fn review_result_from_output(output: &ReviewOutputEvent) -> (bool, usize, Option<String>) {
        let findings_len = output.findings.len();
        let has_findings = findings_len > 0;

        let mut summary_parts: Vec<String> = Vec::new();
        if !output.overall_explanation.trim().is_empty() {
            summary_parts.push(output.overall_explanation.trim().to_string());
        }
        if findings_len > 0 {
            let titles: Vec<String> = output
                .findings
                .iter()
                .filter_map(|f| {
                    let title = f.title.trim();
                    (!title.is_empty()).then_some(title.to_string())
                })
                .collect();
            if !titles.is_empty() {
                summary_parts.push(format!("Findings: {}", titles.join("; ")));
            }
        }

        let summary = if summary_parts.is_empty() {
            None
        } else {
            Some(summary_parts.join(" \n"))
        };

        (has_findings, findings_len, summary)
    }

    pub(in crate::chatwidget) fn set_auto_review_indicator(
        &mut self,
        status: AutoReviewIndicatorStatus,
        findings: Option<usize>,
        phase: AutoReviewPhase,
    ) {
        let state = AutoReviewStatus {
            status,
            findings,
            phase,
        };
        self.auto_review_status = Some(state);
        self.bottom_pane
            .set_auto_review_status(Some(AutoReviewFooterStatus {
                status,
                findings,
                phase,
            }));
        self.request_redraw();
    }

    pub(in crate::chatwidget) fn clear_auto_review_indicator(&mut self) {
        self.auto_review_status = None;
        self.bottom_pane.set_auto_review_status(None);
    }

    pub(in crate::chatwidget) fn last_assistant_cell_index(&self) -> Option<usize> {
        self.history_cells.iter().enumerate().rev().find_map(|(idx, cell)| {
            cell.as_any()
                .downcast_ref::<history_cell::AssistantMarkdownCell>()
                .map(|_| idx)
        })
    }

    pub(in crate::chatwidget) fn insert_auto_review_notice(
        &mut self,
        branch: &str,
        worktree_path: &std::path::Path,
        summary: Option<&str>,
        findings: usize,
    ) {
        let path_text = worktree_path.display().to_string();
        let has_path = !path_text.is_empty();

        let summary_text = summary.and_then(|text| {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then_some(trimmed.to_string())
        });

        let mut line = format!("Auto Review: {findings} issue(s) found");
        if let Some(summary_text) = summary_text {
            line.push_str(". ");
            line.push_str(&summary_text);
        } else {
            line.push_str(&format!(" in '{branch}'"));
        }
        if has_path {
            line.push(' ');
            line.push_str(&format!("Merge {path_text} to apply fixes."));
        }
        line.push_str(" [Ctrl+A] Show");

        let message_lines = vec![MessageLine {
            kind: MessageLineKind::Paragraph,
            spans: vec![InlineSpan {
                text: line,
                tone: TextTone::Default,
                emphasis: TextEmphasis::default(),
                entity: None,
            }],
        }];

        let state = PlainMessageState {
            id: HistoryId::ZERO,
            role: PlainMessageRole::System,
            kind: PlainMessageKind::Notice,
            header: Some(MessageHeader { label: "Auto Review".to_string(), badge: None }),
            lines: message_lines,
            metadata: None,
        };

        // Replace existing notice if present
        if let Some(notice) = self.auto_review_notice.clone()
            && let Some(idx) = self
                .history_cell_ids
                .iter()
                .position(|maybe| maybe.map(|id| id == notice.history_id).unwrap_or(false))
            {
                let cell = crate::history_cell::PlainHistoryCell::from_state(state);
                self.history_replace_at(idx, Box::new(cell));
                return;
            }

        // Insert after the indicator if present; otherwise after the assistant cell
        let base_key = self
            .last_assistant_cell_index()
            .and_then(|idx| self.cell_order_seq.get(idx).copied())
            .unwrap_or(OrderKey {
                req: 0,
                out: -1,
                seq: 0,
            });

        let insert_key = Self::order_key_successor(base_key);
        let pos = self.history_insert_plain_state_with_key(state, insert_key, "auto-review-notice");
        if let Some(Some(id)) = self.history_cell_ids.get(pos) {
            self.auto_review_notice = Some(AutoReviewNotice { history_id: *id });
        }
    }

    pub(crate) fn on_background_review_started(
        &mut self,
        worktree_path: std::path::PathBuf,
        branch: String,
        agent_id: Option<String>,
        snapshot: Option<String>,
    ) {
        if let Some(state) = self.background_review.as_mut() {
            state.worktree_path = worktree_path;
            state.branch = branch;
            state.agent_id = agent_id;
            state.snapshot = snapshot;
            state.last_seen = Instant::now();
        }
        if self.auto_review_status.is_none() {
            self.set_auto_review_indicator(
                AutoReviewIndicatorStatus::Running,
                None,
                AutoReviewPhase::Reviewing,
            );
        }
        // Ensure the main status spinner is cleared once the foreground turn ends;
        // background auto review should not keep the composer in a "running" state.
        self.bottom_pane.set_task_running(false);
        self.bottom_pane.update_status_text(String::new());
        self.auto_review_notice = None;
        self.request_redraw();
    }

    pub(crate) fn on_background_review_finished(
        &mut self,
        event: BackgroundReviewFinishedEvent,
    ) {
        let BackgroundReviewFinishedEvent {
            worktree_path,
            branch,
            has_findings,
            findings,
            summary,
            error,
            agent_id,
            snapshot,
        } = event;
        // Normalize zero-count "issues" so the indicator and developer notes stay
        // aligned with the overlay: if the parser could not produce a findings list,
        // treat the run as clean instead of "fixed".
        let mut has_findings = has_findings;
        if has_findings && findings == 0 {
            has_findings = false;
        }

        let inflight_base = self
            .background_review
            .as_ref()
            .and_then(|state| state.base.clone());
        let inflight_snapshot = snapshot.or_else(|| {
            self.background_review
                .as_ref()
                .and_then(|state| state.snapshot.clone())
        });
        // Clear flags up front so subsequent auto reviews can start even if this finishes with an error
        self.background_review = None;
        self.background_review_guard = None;
        release_background_lock(&agent_id);
        let mut developer_note: Option<String> = None;
        let snapshot_note = inflight_snapshot
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!("Snapshot: {s} (review target)"))
            .unwrap_or_else(|| "Snapshot: (unknown)".to_string());
        let agent_note = agent_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|id| {
                let short = id.chars().take(8).collect::<String>();
                format!("Agent: #{short} (auto-review)")
            })
            .unwrap_or_else(|| "Agent: (unknown)".to_string());
        let summary_note = summary
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.replace('\n', " "))
            .map(|s| format!("Summary: {s}"));
        let errored = error.is_some();
        let indicator_status = if let Some(err) = error {
            developer_note = Some(format!(
                "[developer] Background auto-review failed.\n\nThis auto-review ran in an isolated git worktree and did not modify your current workspace.\n\nWorktree: '{branch}'\nWorktree path: {}\n{snapshot_note}\n{agent_note}\nError: {err}",
                worktree_path.display(),
            ));
            AutoReviewIndicatorStatus::Failed
        } else if has_findings {
            let mut note = format!(
                "[developer] Background auto-review completed and reported {findings} issue(s).\n\nA separate LLM ran /review (and may have run auto-resolve) in an isolated git worktree. Any proposed fixes live only in that worktree until you merge them.\n\nNext: Decide if the findings are genuine. If yes, Merge the worktree '{branch}' to apply the changes (or cherry-pick selectively). If not, do not merge.\n\nWorktree path: {}\n{snapshot_note}\n{agent_note}",
                worktree_path.display(),
            );
            if let Some(summary_note) = summary_note {
                note.push('\n');
                note.push_str(&summary_note);
            }
            developer_note = Some(note);
            AutoReviewIndicatorStatus::Fixed
        } else {
            AutoReviewIndicatorStatus::Clean
        };

        let findings_for_indicator =
            matches!(indicator_status, AutoReviewIndicatorStatus::Fixed).then_some(findings.max(1));
        let phase = self
            .auto_review_status
            .map(|s| s.phase)
            .unwrap_or(AutoReviewPhase::Reviewing);
        self.set_auto_review_indicator(indicator_status, findings_for_indicator, phase);
        if matches!(indicator_status, AutoReviewIndicatorStatus::Fixed) {
            self.insert_auto_review_notice(
                &branch,
                &worktree_path,
                summary.as_deref(),
                findings.max(1),
            );
        }

        if let Some(note) = developer_note {
            // Immediately inject as a developer message so the user sees it in the
            // transcript, even if tasks/streams are still running. Do not defer to
            // pending_agent_notes; that path can be lost if the session ends.
            self.submit_hidden_text_message_with_preface(String::new(), note);
        }

        // Auto review completion should never leave the composer spinner active.
        self.bottom_pane.set_task_running(false);
        self.bottom_pane.update_status_text(String::new());

        self.handle_auto_review_completion_state(
            has_findings,
            errored,
            inflight_base,
            inflight_snapshot,
        );

        // Auto review findings are inserted as history notices, but Auto Drive resumes
        // from cached conversation state. Rebuild before resuming so the coordinator
        // receives the latest background review context.
        if self.auto_state.is_active() {
            self.rebuild_auto_history();
        }

        self.maybe_resume_auto_after_review();
        self.request_redraw();
    }

    pub(in crate::chatwidget) fn handle_auto_review_completion_state(
        &mut self,
        has_findings: bool,
        errored: bool,
        inflight_base: Option<GhostCommit>,
        snapshot: Option<String>,
    ) {
        let was_skipped = !has_findings && !errored && snapshot.is_none();

        if !errored
            && let Some(id) = snapshot.as_ref() {
                self.auto_review_reviewed_marker = Some(GhostCommit::new(id.clone(), None));
                self.persist_auto_review_baseline_marker(id);
            }

        if was_skipped || errored {
            if let Some(base) = inflight_base {
                self.queue_skipped_auto_review(base);
            }
            return;
        }

        if has_findings {
            if let Some(range) = self.pending_auto_review_range.as_mut() {
                if range.defer_until_turn.is_none() {
                    range.defer_until_turn = Some(self.turn_sequence);
                }
            } else if let Some(base) = inflight_base {
                self.pending_auto_review_range = Some(PendingAutoReviewRange {
                    base,
                    defer_until_turn: Some(self.turn_sequence),
                });
            }
            return;
        }

        if let Some(pending) = self.pending_auto_review_range.take() {
            self.launch_background_review(Some(pending.base));
        }
    }

    pub(in crate::chatwidget) fn restore_auto_resolve_attempts_if_lost(&mut self) {
        if self.auto_resolve_attempts_baseline == 0 {
            return;
        }
        let current = self
            .config
            .auto_drive
            .auto_resolve_review_attempts
            .get();
        if current == 0
            && let Ok(limit) = code_core::config_types::AutoResolveAttemptLimit::try_new(
                self.auto_resolve_attempts_baseline,
            ) {
                self.config.auto_drive.auto_resolve_review_attempts = limit;
            }

        self.background_review = None;
    }

    pub(in crate::chatwidget) fn update_review_settings_model_row(&mut self) {
        if let Some(overlay) = self.settings.overlay.as_mut()
            && let Some(content) = overlay.review_content_mut() {
                content.update_review_model(
                    self.config.review_model.clone(),
                    self.config.review_model_reasoning_effort,
                );
                content.set_review_use_chat_model(self.config.review_use_chat_model);
                content.update_review_resolve_model(
                    self.config.review_resolve_model.clone(),
                    self.config.review_resolve_model_reasoning_effort,
                );
                content.set_review_resolve_use_chat_model(self.config.review_resolve_use_chat_model);
                content.update_auto_review_model(
                    self.config.auto_review_model.clone(),
                    self.config.auto_review_model_reasoning_effort,
                );
                content.set_auto_review_use_chat_model(self.config.auto_review_use_chat_model);
                content.update_auto_review_resolve_model(
                    self.config.auto_review_resolve_model.clone(),
                    self.config.auto_review_resolve_model_reasoning_effort,
                );
                content.set_auto_review_resolve_use_chat_model(
                    self.config.auto_review_resolve_use_chat_model,
                );
                content.set_review_followups(self.config.auto_drive.auto_resolve_review_attempts.get());
                content.set_auto_review_followups(
                    self.config.auto_drive.auto_review_followup_attempts.get(),
                );
            }
    }

    pub(in crate::chatwidget) fn update_planning_settings_model_row(&mut self) {
        if let Some(overlay) = self.settings.overlay.as_mut()
            && let Some(content) = overlay.planning_content_mut() {
                content.update_planning_model(
                    self.config.planning_model.clone(),
                    self.config.planning_model_reasoning_effort,
                );
                content.set_use_chat_model(self.config.planning_use_chat_model);
            }
    }

    pub(in crate::chatwidget) fn update_auto_drive_settings_model_row(&mut self) {
        if let Some(overlay) = self.settings.overlay.as_mut()
            && let Some(content) = overlay.auto_drive_content_mut() {
                content.update_model(
                    self.config.auto_drive.model.clone(),
                    self.config.auto_drive.model_reasoning_effort,
                );
                content.set_use_chat_model(
                    self.config.auto_drive_use_chat_model,
                    self.config.model.clone(),
                    self.config.model_reasoning_effort,
                );
            }
    }

}
