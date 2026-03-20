impl ChatWidget<'_> {
    pub(crate) fn handle_undo_command(&mut self) {
        if self.ghost_snapshots_disabled {
            let reason = self
                .ghost_snapshots_disabled_reason
                .as_ref()
                .map(|reason| reason.message.clone())
                .unwrap_or_else(|| "Snapshots are currently disabled.".to_string());
            self.push_background_tail(format!("/undo unavailable: {reason}"));
            self.show_undo_snapshots_disabled();
            return;
        }

        if self.ghost_snapshots.is_empty() {
            self.push_background_tail(
                "/undo unavailable: no snapshots captured yet. Run a file-modifying command to create one.".to_string(),
            );
            self.show_undo_empty_state();
            return;
        }

        self.show_undo_snapshot_picker();
    }

    pub(in super::super) fn show_undo_snapshots_disabled(&mut self) {
        let mut lines: Vec<String> = Vec::new();
        if let Some(reason) = &self.ghost_snapshots_disabled_reason {
            lines.push(reason.message.clone());
            if let Some(hint) = &reason.hint {
                lines.push(hint.clone());
            }
        } else {
            lines.push(
                "Snapshots are currently disabled. Resolve the Git issue and restart Code to re-enable them.".to_string(),
            );
        }

        self.show_undo_status_popup(
            "Snapshots unavailable",
            Some(
                "Restores workspace files only. Conversation history remains unchanged.".to_string(),
            ),
            Some("Automatic snapshotting failed, so /undo cannot restore the workspace.".to_string()),
            lines,
        );
    }

    pub(in super::super) fn show_undo_empty_state(&mut self) {
        self.show_undo_status_popup(
            "No snapshots yet",
            Some(
                "Restores workspace files only. Conversation history remains unchanged.".to_string(),
            ),
            Some("Snapshots appear once Code captures a Git checkpoint.".to_string()),
            vec![
                "No snapshot is available to restore.".to_string(),
                "Run a command that modifies files to create the first snapshot.".to_string(),
            ],
        );
    }

    pub(in super::super) fn show_undo_status_popup(
        &mut self,
        title: &str,
        scope_hint: Option<String>,
        subtitle: Option<String>,
        mut lines: Vec<String>,
    ) {
        if lines.is_empty() {
            lines.push("No snapshot information available.".to_string());
        }

        let headline = lines.remove(0);
        let description = if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        };

        let mut composed_subtitle = Vec::new();
        if let Some(hint) = scope_hint {
            composed_subtitle.push(hint);
        }
        if let Some(extra) = subtitle {
            composed_subtitle.push(extra);
        }
        let subtitle_for_view = if composed_subtitle.is_empty() {
            None
        } else {
            Some(composed_subtitle.join("\n"))
        };

        let items = vec![SelectionItem {
            name: headline,
            description,
            is_current: true,
            actions: Vec::new(),
        }];

        let view = ListSelectionView::new(
            format!(" {title} "),
            subtitle_for_view,
            Some("Esc close".to_string()),
            items,
            self.app_event_tx.clone(),
            1,
        );

        self.bottom_pane.show_list_selection(view);
    }

    pub(in super::super) fn show_undo_snapshot_picker(&mut self) {
        let entries = self.build_undo_timeline_entries();
        if entries.len() <= 1 {
            self.push_background_tail(
                "/undo unavailable: no snapshots captured yet. Run a file-modifying command to create one.".to_string(),
            );
            self.show_undo_empty_state();
            return;
        }

        let current_index = entries.len().saturating_sub(1);
        let view = UndoTimelineView::new(entries, current_index, self.app_event_tx.clone());
        self.bottom_pane.show_undo_timeline_view(view);
    }

    pub(in super::super) fn build_undo_timeline_entries(&self) -> Vec<UndoTimelineEntry> {
        let mut entries: Vec<UndoTimelineEntry> = Vec::with_capacity(self.ghost_snapshots.len().saturating_add(1));
        for snapshot in self.ghost_snapshots.iter() {
            entries.push(self.timeline_entry_for_snapshot(snapshot));
        }
        entries.push(self.timeline_entry_for_current());
        entries
    }

    pub(in super::super) fn timeline_entry_for_snapshot(&self, snapshot: &GhostSnapshot) -> UndoTimelineEntry {
        let short_id = snapshot.short_id();
        let label = format!("Snapshot {short_id}");
        let summary = snapshot.summary.clone();
        let timestamp_line = Some(snapshot.captured_at.format("%Y-%m-%d %H:%M:%S").to_string());
        let relative_time = snapshot
            .age_from(Local::now())
            .map(|age| format!("captured {} ago", format_duration(age)));
        let (user_delta, assistant_delta) = self.conversation_delta_since(&snapshot.conversation);
        let stats_line = if user_delta == 0 && assistant_delta == 0 {
            Some("conversation already matches current state".to_string())
        } else if assistant_delta == 0 {
            Some(format!(
                "rewind {} user turn{}",
                user_delta,
                if user_delta == 1 { "" } else { "s" }
            ))
        } else {
            Some(format!(
                "rewind {} user turn{} and {} assistant repl{}",
                user_delta,
                if user_delta == 1 { "" } else { "s" },
                assistant_delta,
                if assistant_delta == 1 { "y" } else { "ies" }
            ))
        };

        let conversation_lines = Self::conversation_preview_lines_from_snapshot(&snapshot.history);
        let file_lines = self.timeline_file_lines_for_commit(snapshot.commit().id());

        UndoTimelineEntry {
            label,
            summary,
            timestamp_line,
            relative_time,
            stats_line,
            commit_line: Some(format!("commit {short_id}")),
            conversation_lines,
            file_lines,
            conversation_available: user_delta > 0,
            files_available: true,
            kind: UndoTimelineEntryKind::Snapshot {
                commit: snapshot.commit().id().to_string(),
            },
        }
    }

    pub(in super::super) fn timeline_entry_for_current(&self) -> UndoTimelineEntry {
        let history_snapshot = self.history_snapshot_for_persistence();
        let conversation_lines = Self::conversation_preview_lines_from_snapshot(&history_snapshot);
        let file_lines = self.timeline_file_lines_for_current();
        UndoTimelineEntry {
            label: "Current workspace".to_string(),
            summary: None,
            timestamp_line: Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            relative_time: Some("current point".to_string()),
            stats_line: Some("Already at this point in time".to_string()),
            commit_line: None,
            conversation_lines,
            file_lines,
            conversation_available: false,
            files_available: false,
            kind: UndoTimelineEntryKind::Current,
        }
    }

    pub(in super::super) fn conversation_preview_lines_from_snapshot(snapshot: &HistorySnapshot) -> Vec<Line<'static>> {
        let mut state = HistoryState::new();
        state.restore(snapshot);
        let mut messages: Vec<(UndoPreviewRole, String)> = Vec::new();
        for record in &state.records {
            match record {
                HistoryRecord::PlainMessage(msg) => match msg.kind {
                    PlainMessageKind::User => {
                        let text = Self::message_lines_to_plain_preview(&msg.lines);
                        if !text.is_empty() {
                            messages.push((UndoPreviewRole::User, text));
                        }
                    }
                    PlainMessageKind::Assistant => {
                        let text = Self::message_lines_to_plain_preview(&msg.lines);
                        if !text.is_empty() {
                            messages.push((UndoPreviewRole::Assistant, text));
                        }
                    }
                    _ => {}
                },
                HistoryRecord::AssistantMessage(msg) => {
                    let text = Self::markdown_to_plain_preview(&msg.markdown);
                    if !text.is_empty() {
                        messages.push((UndoPreviewRole::Assistant, text));
                    }
                }
                _ => {}
            }
        }

        if messages.is_empty() {
            return vec![Line::from(Span::styled(
                "No conversation captured in this snapshot.",
                Style::default().fg(crate::colors::text_dim()),
            ))];
        }

        let len = messages.len();
        let start = len.saturating_sub(Self::MAX_UNDO_CONVERSATION_MESSAGES);
        messages[start..]
            .iter()
            .map(|(role, text)| Self::conversation_line(*role, text.as_str()))
            .collect()
    }

    pub(in super::super) fn conversation_line(role: UndoPreviewRole, text: &str) -> Line<'static> {
        let (label, color) = match role {
            UndoPreviewRole::User => ("You", crate::colors::text_bright()),
            UndoPreviewRole::Assistant => ("Code", crate::colors::primary()),
        };
        let label_span = Span::styled(
            format!("{label}: "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        );
        let content_span = Span::styled(text.to_string(), Style::default().fg(crate::colors::text()));
        Line::from(vec![label_span, content_span])
    }

    pub(in super::super) fn message_lines_to_plain_preview(lines: &[MessageLine]) -> String {
        let mut segments: Vec<String> = Vec::new();
        for line in lines {
            match line.kind {
                MessageLineKind::Blank => continue,
                MessageLineKind::Metadata => continue,
                _ => {
                    let mut text = String::new();
                    for span in &line.spans {
                        text.push_str(&span.text);
                    }
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        segments.push(trimmed.to_string());
                    }
                }
            }
            if segments.len() >= Self::MAX_UNDO_CONVERSATION_MESSAGES {
                break;
            }
        }
        let joined = segments.join(" ");
        Self::truncate_preview_text(joined, Self::MAX_UNDO_PREVIEW_CHARS)
    }

    pub(in super::super) fn markdown_to_plain_preview(markdown: &str) -> String {
        let mut segments: Vec<String> = Vec::new();
        for line in markdown.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('#') {
                segments.push(trimmed.trim_start_matches('#').trim().to_string());
            } else {
                segments.push(trimmed.to_string());
            }
            if segments.len() >= Self::MAX_UNDO_CONVERSATION_MESSAGES {
                break;
            }
        }
        if segments.is_empty() {
            return String::new();
        }
        let joined = segments.join(" ");
        Self::truncate_preview_text(joined, Self::MAX_UNDO_PREVIEW_CHARS)
    }

    pub(in super::super) fn truncate_preview_text(text: String, limit: usize) -> String {
        crate::text_formatting::truncate_chars_with_ellipsis(&text, limit)
    }

    pub(in super::super) fn timeline_file_lines_for_commit(&self, commit_id: &str) -> Vec<Line<'static>> {
        match self.git_numstat(["show", "--numstat", "--format=", commit_id]) {
            Ok(entries) => Self::file_change_lines(entries),
            Err(err) => vec![Line::from(Span::styled(
                err,
                Style::default().fg(crate::colors::error()),
            ))],
        }
    }

    pub(in super::super) fn timeline_file_lines_for_current(&self) -> Vec<Line<'static>> {
        match self.git_numstat(["diff", "--numstat", "HEAD"]) {
            Ok(entries) => {
                if entries.is_empty() {
                    vec![Line::from(Span::styled(
                        "Working tree clean",
                        Style::default().fg(crate::colors::text_dim()),
                    ))]
                } else {
                    Self::file_change_lines(entries)
                }
            }
            Err(err) => vec![Line::from(Span::styled(
                err,
                Style::default().fg(crate::colors::error()),
            ))],
        }
    }

    pub(in super::super) fn git_numstat<I, S>(
        &self,
        args: I,
    ) -> Result<Vec<NumstatRow>, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.run_git_command(args, |stdout| {
            let mut out = Vec::new();
            for line in stdout.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let mut parts = trimmed.splitn(3, '\t');
                let added = parts.next();
                let removed = parts.next();
                let path = parts.next();
                if let (Some(added), Some(removed), Some(path)) = (added, removed, path) {
                    out.push((
                        Self::parse_numstat_count(added),
                        Self::parse_numstat_count(removed),
                        path.to_string(),
                    ));
                }
            }
            Ok(out)
        })
    }

    pub(in super::super) fn run_git_command<I, S, F, T>(&self, args: I, parser: F) -> Result<T, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
        F: FnOnce(String) -> Result<T, String>,
    {
        let args_vec: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
        let output = Command::new("git")
            .current_dir(&self.config.cwd)
            .args(&args_vec)
            .output()
            .map_err(|err| format!("git {} failed: {err}", args_vec.join(" ")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.trim();
            if msg.is_empty() {
                Err(format!(
                    "git {} exited with status {}",
                    args_vec.join(" "),
                    output.status
                ))
            } else {
                Err(msg.to_string())
            }
        } else {
            if args_vec
                .iter()
                .any(|arg| matches!(arg.as_str(), "pull" | "checkout" | "merge" | "apply"))
            {
                bump_snapshot_epoch_for(&self.config.cwd);
            }
            parser(String::from_utf8_lossy(&output.stdout).to_string())
        }
    }

    pub(in super::super) fn parse_numstat_count(raw: &str) -> Option<u32> {
        if raw == "-" {
            None
        } else {
            raw.parse::<u32>().ok()
        }
    }

    pub(in super::super) fn file_change_lines(entries: Vec<(Option<u32>, Option<u32>, String)>) -> Vec<Line<'static>> {
        if entries.is_empty() {
            return vec![Line::from(Span::styled(
                "No file changes recorded for this snapshot.",
                Style::default().fg(crate::colors::text_dim()),
            ))];
        }

        let max_entries = (Self::MAX_UNDO_FILE_LINES / 2).max(1);
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (idx, (added, removed, path)) in entries.iter().enumerate() {
            if idx >= max_entries {
                break;
            }
            lines.push(Line::from(Span::styled(
                path.clone(),
                Style::default().fg(crate::colors::text()),
            )));

            let added_text = added.map_or("-".to_string(), |v| v.to_string());
            let removed_text = removed.map_or("-".to_string(), |v| v.to_string());
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("+{added_text}"),
                    Style::default().fg(crate::colors::success()),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("-{removed_text}"),
                    Style::default().fg(crate::colors::error()),
                ),
            ]));
        }

        if entries.len() > max_entries {
            let remaining = entries.len() - max_entries;
            lines.push(Line::from(Span::styled(
                format!("… and {remaining} more file{}", if remaining == 1 { "" } else { "s" }),
                Style::default().fg(crate::colors::text_dim()),
            )));
        }

        lines
    }

}
