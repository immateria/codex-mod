impl ChatWidget<'_> {
    pub(crate) fn show_agents_overview_ui(&mut self) {
        let (rows, commands) = self.collect_agents_overview_rows();
        let total_rows = rows
            .len()
            .saturating_add(commands.len())
            .saturating_add(AGENTS_OVERVIEW_STATIC_ROWS);
        let selected = if total_rows == 0 {
            0
        } else {
            self
                .agents_overview_selected_index
                .min(total_rows.saturating_sub(1))
        };
        self.agents_overview_selected_index = selected;

        self.ensure_settings_overlay_section(SettingsSection::Agents);

        let updated = self.try_update_agents_settings_overview(
            rows.clone(),
            commands.clone(),
            selected,
        );

        if !updated
            && let Some(overlay) = self.settings.overlay.as_mut() {
                let content = AgentsSettingsContent::new_overview(
                    rows,
                    commands,
                    selected,
                    self.app_event_tx.clone(),
                );
                overlay.set_agents_content(content);
            }

        self.request_redraw();
    }

    fn try_update_agents_settings_overview(
        &mut self,
        rows: Vec<AgentOverviewRow>,
        commands: Vec<String>,
        selected: usize,
    ) -> bool {
        if let Some(overlay) = self.settings.overlay.as_mut()
            && overlay.active_section() == SettingsSection::Agents {
                if let Some(content) = overlay.agents_content_mut() {
                    content.set_overview(rows, commands, selected);
                } else {
                    overlay.set_agents_content(AgentsSettingsContent::new_overview(
                        rows,
                        commands,
                        selected,
                        self.app_event_tx.clone(),
                    ));
                }
                return true;
            }
        false
    }

    fn try_set_agents_settings_editor(&mut self, editor: SubagentEditorView) -> bool {
        let mut editor = Some(editor);
        let mut needs_content = false;

        if let Some(overlay) = self.settings.overlay.as_mut()
            && overlay.active_section() == SettingsSection::Agents {
                if let Some(content) = overlay.agents_content_mut() {
                    let Some(editor_view) = editor.take() else {
                        return false;
                    };
                    content.set_editor(editor_view);
                    self.request_redraw();
                    return true;
                } else {
                    needs_content = true;
                }
            }

        if needs_content {
            let (rows, commands) = self.collect_agents_overview_rows();
            let total = rows
                .len()
                .saturating_add(commands.len())
                .saturating_add(AGENTS_OVERVIEW_STATIC_ROWS);
            let selected = if total == 0 {
                0
            } else {
                self.agents_overview_selected_index.min(total.saturating_sub(1))
            };
            self.agents_overview_selected_index = selected;

            if let Some(overlay) = self.settings.overlay.as_mut()
                && overlay.active_section() == SettingsSection::Agents {
                    let mut content = AgentsSettingsContent::new_overview(
                        rows,
                        commands,
                        selected,
                        self.app_event_tx.clone(),
                    );
                    let Some(editor_view) = editor.take() else {
                        return false;
                    };
                    content.set_editor(editor_view);
                    overlay.set_agents_content(content);
                    self.request_redraw();
                    return true;
                }
        }

        false
    }

    fn try_set_agents_settings_agent_editor(&mut self, editor: AgentEditorView) -> bool {
        let mut editor = Some(editor);
        let mut needs_content = false;

        if let Some(overlay) = self.settings.overlay.as_mut()
            && overlay.active_section() == SettingsSection::Agents {
                if let Some(content) = overlay.agents_content_mut() {
                    let Some(editor_view) = editor.take() else {
                        return false;
                    };
                    content.set_agent_editor(editor_view);
                    self.request_redraw();
                    return true;
                } else {
                    needs_content = true;
                }
            }

        if needs_content {
            let (rows, commands) = self.collect_agents_overview_rows();
            let total = rows
                .len()
                .saturating_add(commands.len())
                .saturating_add(AGENTS_OVERVIEW_STATIC_ROWS);
            let selected = if total == 0 {
                0
            } else {
                self.agents_overview_selected_index.min(total.saturating_sub(1))
            };
            self.agents_overview_selected_index = selected;

            if let Some(overlay) = self.settings.overlay.as_mut()
                && overlay.active_section() == SettingsSection::Agents {
                    let mut content = AgentsSettingsContent::new_overview(
                        rows,
                        commands,
                        selected,
                        self.app_event_tx.clone(),
                    );
                    let Some(editor_view) = editor.take() else {
                        return false;
                    };
                    content.set_agent_editor(editor_view);
                    overlay.set_agents_content(content);
                    self.request_redraw();
                    return true;
                }
        }

        false
    }

    pub(crate) fn set_agents_overview_selection(&mut self, index: usize) {
        self.agents_overview_selected_index = index;
        if let Some(overlay) = self.settings.overlay.as_mut()
            && overlay.active_section() == SettingsSection::Agents
                && let Some(content) = overlay.agents_content_mut() {
                    content.set_overview_selection(index);
                }
    }

    fn agent_batch_metadata(&self, batch_id: &str) -> AgentBatchMetadata {
        if let Some(key) = self.tools_state.agent_run_by_batch.get(batch_id)
            && let Some(tracker) = self.tools_state.agent_runs.get(key) {
                return AgentBatchMetadata {
                    label: tracker.overlay_display_label(),
                    prompt: tracker.overlay_task(),
                    context: tracker.overlay_context(),
                };
            }
        AgentBatchMetadata::default()
    }

    fn append_agents_overlay_section(
        &self,
        lines: &mut Vec<ratatui::text::Line<'static>>,
        title: &str,
        text: &str,
    ) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        let header_style = ratatui::style::Style::default()
            .fg(crate::colors::text())
            .add_modifier(ratatui::style::Modifier::BOLD);
        lines.push(ratatui::text::Line::from(vec![
            ratatui::text::Span::raw(" "),
            ratatui::text::Span::styled(title.to_string(), header_style),
        ]));
        for raw_line in trimmed.lines() {
            let content = raw_line.trim_end();
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::raw("   "),
                ratatui::text::Span::styled(
                    content.to_string(),
                    ratatui::style::Style::default().fg(crate::colors::text()),
                ),
            ]));
        }
    }

    fn truncate_overlay_text(&self, text: &str, limit: usize) -> String {
        let collapsed = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let normalized = if collapsed.trim().is_empty() {
            text.trim().to_string()
        } else {
            collapsed.trim().to_string()
        };

        if normalized.chars().count() <= limit {
            return normalized;
        }

        let mut out: String = normalized.chars().take(limit.saturating_sub(1)).collect();
        out.push('…');
        out
    }

    fn append_agent_highlights(
        &self,
        lines: &mut Vec<ratatui::text::Line<'static>>,
        entry: &AgentTerminalEntry,
        available_width: u16,
        collapsed: bool,
    ) {
        let mut bullets: Vec<(String, ratatui::style::Style)> = Vec::new();

        if matches!(entry.source_kind, Some(AgentSourceKind::AutoReview)) {
            let is_terminal = matches!(
                entry.status,
                AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Cancelled
            );

            if is_terminal {
                let (mut has_findings, findings_count, summary) =
                    Self::parse_agent_review_result(entry.result.as_deref());

                // Avoid showing a warning when we didn't get an explicit findings list.
                // Some heuristic parses can claim "issues" but provide a zero count; treat those as clean
                // to keep the UI consistent with successful, issue-free reviews.
                if has_findings && findings_count == 0 {
                    has_findings = false;
                }

                let mut label = if has_findings {
                    let plural = if findings_count == 1 { "issue" } else { "issues" };
                    format!("Auto Review: {findings_count} {plural} found")
                } else if matches!(entry.status, AgentStatus::Completed) {
                    "Auto Review: no issues found".to_string()
                } else {
                    String::new()
                };
                if label.is_empty() {
                    label = "Auto Review".to_string();
                }

                if has_findings || matches!(entry.status, AgentStatus::Completed) {
                    let color = if has_findings {
                        ratatui::style::Style::default().fg(crate::colors::warning())
                    } else {
                        ratatui::style::Style::default().fg(crate::colors::success())
                    };
                    bullets.push((label, color));
                }

                if let Some(summary_text) = summary {
                    for line in summary_text.lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        bullets.push((
                            self.truncate_overlay_text(trimmed, 280),
                            ratatui::style::Style::default().fg(crate::colors::text_dim()),
                        ));
                    }
                }
            }
        }

        if let Some(result) = entry.result.as_ref() {
            let text = self.truncate_overlay_text(result, 320);
            if !text.is_empty() {
                bullets.push((
                    format!("Final: {text}"),
                    ratatui::style::Style::default().fg(crate::colors::text_dim()),
                ));
            }
        }

        match entry.status {
            AgentStatus::Failed => {
                if entry.error.is_none() {
                    bullets.push((
                        "Failed".to_string(),
                        ratatui::style::Style::default().fg(crate::colors::error()),
                    ));
                }
            }
            AgentStatus::Cancelled => {
                if entry.error.is_none() {
                    bullets.push((
                        "Cancelled".to_string(),
                        ratatui::style::Style::default().fg(crate::colors::warning()),
                    ));
                }
            }
            AgentStatus::Pending | AgentStatus::Running => {
                if bullets.is_empty()
                    && let Some(progress) = entry.last_progress.as_ref() {
                        let text = self.truncate_overlay_text(progress, 200);
                        if !text.is_empty() {
                            bullets.push((
                                format!("Latest progress: {text}"),
                                ratatui::style::Style::default()
                                    .fg(crate::colors::text_dim()),
                            ));
                        }
                    }
            }
            _ => {}
        }

        let header_style = ratatui::style::Style::default()
            .fg(crate::colors::text())
            .add_modifier(ratatui::style::Modifier::BOLD);
        let chevron = if collapsed { "▶" } else { "▼" };
        let title = format!("╭ Highlights (h) {chevron} ");
        let title_width = unicode_width::UnicodeWidthStr::width(title.as_str()) as u16;
        let pad = available_width
            .saturating_sub(title_width)
            .saturating_sub(1);
        let mut heading = title;
        heading.push_str(&"─".repeat(pad as usize));
        heading.push('╮');
        lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
            heading,
            header_style,
        )));

        if collapsed || bullets.is_empty() {
            let footer_width = available_width.saturating_sub(1);
            let mut footer = String::from("╰");
            footer.push_str(&"─".repeat(footer_width as usize));
            lines.push(ratatui::text::Line::from(footer));
            self.ensure_trailing_blank_line(lines);
            return;
        }

        let wrap_width = available_width.saturating_sub(6).max(12) as usize;
        for (text, style) in bullets.into_iter() {
            let opts = textwrap::Options::new(wrap_width)
                .break_words(false)
                .word_splitter(textwrap::word_splitters::WordSplitter::NoHyphenation)
                .initial_indent("• ")
                .subsequent_indent("  ");
            for (idx, wrapped) in textwrap::wrap(text.as_str(), opts).into_iter().enumerate() {
                let prefix = if idx == 0 { "│   " } else { "│     " };
                lines.push(ratatui::text::Line::from(vec![
                    ratatui::text::Span::raw(prefix),
                    ratatui::text::Span::styled(wrapped.to_string(), style),
                ]));
            }
        }

        if let Some(error_text) = entry
            .error
            .as_ref()
            .map(|e| self.truncate_overlay_text(e, 320))
            && !error_text.is_empty() {
                let msg = format!("Last error: {error_text}");
                for (idx, wrapped) in textwrap::wrap(msg.as_str(), wrap_width).into_iter().enumerate() {
                    let prefix = if idx == 0 { "│   " } else { "│     " };
                    lines.push(ratatui::text::Line::from(vec![
                        ratatui::text::Span::raw(prefix),
                        ratatui::text::Span::styled(
                            wrapped.to_string(),
                            ratatui::style::Style::default().fg(crate::colors::error()),
                        ),
                    ]));
                }
            }

        let footer_width = available_width.saturating_sub(1);
        let mut footer = String::from("╰");
        footer.push_str(&"─".repeat(footer_width as usize));
        lines.push(ratatui::text::Line::from(footer));
        self.ensure_trailing_blank_line(lines);
    }

    fn append_agent_log_lines(
        &self,
        lines: &mut Vec<ratatui::text::Line<'static>>,
        _idx: usize,
        log: &AgentLogEntry,
        available_width: u16,
        is_new_kind: bool,
    ) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};

        let time_text = log.timestamp.format("%H:%M").to_string();
        let time_style = Style::default().fg(crate::colors::text_dim());
        let kind_style = Style::default()
            .fg(agent_log_color(log.kind))
            .add_modifier(Modifier::BOLD);
        let message_base_style = if matches!(log.kind, AgentLogKind::Error) {
            Style::default().fg(crate::colors::error())
        } else {
            Style::default().fg(crate::colors::text())
        };

        // Insert a section header when the log kind changes (TYPE column removed).
        if is_new_kind {
            let header = agent_log_label(log.kind).to_uppercase();
            lines.push(Line::from(vec![Span::styled(header, kind_style)]));
        }

        // Compact prefix: time only, kept short per request.
        let prefix_plain = format!("{time_text}  ");
        let prefix_width = unicode_width::UnicodeWidthStr::width(prefix_plain.as_str()) as u16;
        let wrap_width = available_width.saturating_sub(prefix_width).max(4);

        // Break message into lines, sanitizing and keeping ANSI colors.
        let mut message_lines: Vec<&str> = log.message.split('\n').collect();
        if log.message.ends_with('\n') {
            message_lines.push("");
        }

        for (line_idx, raw_line) in message_lines.into_iter().enumerate() {
            let sanitized = self.sanitize_agent_log_line(raw_line);
            let parsed = ansi_escape_line(&sanitized);
            let wrapped = crate::insert_history::word_wrap_lines(&[self.apply_log_fallback_style(parsed, message_base_style)], wrap_width);

            for (wrap_idx, wrapped_line) in wrapped.into_iter().enumerate() {
                let mut spans: Vec<Span> = Vec::new();
                if wrap_idx == 0 && line_idx == 0 {
                    // First visible line: show time prefix.
                    spans.push(Span::styled(time_text.clone(), time_style));
                    spans.push(Span::raw("  "));
                } else {
                    // Continuation lines align under the message body.
                    spans.push(Span::raw(" ".repeat(prefix_width as usize)));
                }

                if wrapped_line.spans.is_empty() {
                    spans.push(Span::raw(""));
                } else {
                    spans.extend(wrapped_line.spans.into_iter());
                }

                lines.push(Line::from(spans));
            }

        }
    }

    fn sanitize_agent_log_line(&self, raw: &str) -> String {
        let without_ts = Self::strip_leading_timestamp(raw.trim_end_matches('\r'));
        sanitize_for_tui(
            without_ts,
            SanitizeMode::AnsiPreserving,
            SanitizeOptions {
                expand_tabs: true,
                tabstop: 4,
                ..Default::default()
            },
        )
    }

    fn apply_log_fallback_style(
        &self,
        mut line: ratatui::text::Line<'static>,
        base: ratatui::style::Style,
    ) -> ratatui::text::Line<'static> {
        for span in line.spans.iter_mut() {
            span.style = base.patch(span.style);
        }
        line
    }

    fn strip_leading_timestamp(text: &str) -> &str {
        fn is_digit(b: u8) -> bool { b.is_ascii_digit() }

        fn consume_hms(bytes: &[u8]) -> usize {
            if bytes.len() < 5 {
                return 0;
            }
            if !(is_digit(bytes[0]) && is_digit(bytes[1]) && bytes[2] == b':' && is_digit(bytes[3]) && is_digit(bytes[4])) {
                return 0;
            }
            let mut idx = 5;
            if idx + 2 < bytes.len() && bytes[idx] == b':' && is_digit(bytes[idx + 1]) && is_digit(bytes[idx + 2]) {
                idx += 3;
                while idx < bytes.len() && (bytes[idx].is_ascii_digit() || bytes[idx] == b'.') {
                    idx += 1;
                }
            }
            idx
        }

        fn consume_ymd(bytes: &[u8]) -> usize {
            if bytes.len() < 10 {
                return 0;
            }
            if !(is_digit(bytes[0])
                && is_digit(bytes[1])
                && is_digit(bytes[2])
                && is_digit(bytes[3])
                && bytes[4] == b'-'
                && is_digit(bytes[5])
                && is_digit(bytes[6])
                && bytes[7] == b'-'
                && is_digit(bytes[8])
                && is_digit(bytes[9]))
            {
                return 0;
            }
            let mut idx = 10;
            if idx < bytes.len() && (bytes[idx] == b'T' || bytes[idx] == b' ') {
                idx += 1;
                idx += consume_hms(&bytes[idx..]);
            }
            idx
        }

        let trimmed = text.trim_start();
        let mut candidate = trimmed.strip_prefix('[').unwrap_or(trimmed);
        let bytes = candidate.as_bytes();

        let mut consumed = consume_ymd(bytes);
        if consumed == 0 {
            consumed = consume_hms(bytes);
        }

        if consumed == 0 {
            return text;
        }

        candidate = &candidate[consumed..];
        if let Some(rest) = candidate.strip_prefix(']') {
            candidate = rest;
        }
        candidate.trim_start()
    }

    fn ensure_trailing_blank_line(
        &self,
        lines: &mut Vec<ratatui::text::Line<'static>>,
    ) {
        if lines
            .last()
            .map(|line| {
                line.spans.is_empty()
                    || (line.spans.len() == 1 && line.spans[0].content.is_empty())
            })
            .unwrap_or(false)
        {
            return;
        }
        lines.push(ratatui::text::Line::from(""));
    }

    fn update_agents_terminal_state(
        &mut self,
        agents: &[code_core::protocol::AgentInfo],
        context: Option<String>,
        task: Option<String>,
    ) {
        self.agents_terminal.shared_context = context;
        self.agents_terminal.shared_task = task;

        let mut saw_new_agent = false;
        for info in agents {
            let status = agent_status_from_str(info.status.as_str());
            let batch_metadata = info
                .batch_id
                .as_deref()
                .map(|id| self.agent_batch_metadata(id))
                .unwrap_or_default();
            let is_new = !self.agents_terminal.entries.contains_key(&info.id);
            if is_new
                && !self
                    .agents_terminal
                    .order
                    .iter()
                    .any(|id| id == &info.id)
            {
                self.agents_terminal.order.push(info.id.clone());
                saw_new_agent = true;
            }

            let entry = self.agents_terminal.entries.entry(info.id.clone());
            let entry = entry.or_insert_with(|| {
                saw_new_agent = true;
                let mut new_entry = AgentTerminalEntry::new(
                    info.name.clone(),
                    info.model.clone(),
                    status.clone(),
                    info.batch_id.clone(),
                );
                new_entry.source_kind = info.source_kind.clone();
                new_entry.push_log(
                    AgentLogKind::Status,
                    format!("Status → {}", agent_status_label(status.clone())),
                );
                new_entry
            });

            entry.name = info.name.clone();
            entry.batch_id = info.batch_id.clone();
            entry.model = info.model.clone();
            entry.source_kind = info.source_kind.clone();

            let AgentBatchMetadata { label, prompt: meta_prompt, context: meta_context } = batch_metadata;
            let auto_review_label = matches!(entry.source_kind, Some(AgentSourceKind::AutoReview))
                .then(|| "Auto Review".to_string());
            let previous_label = entry.batch_label.clone();
            entry.batch_label = label
                .or(auto_review_label)
                .or_else(|| info.batch_id.clone())
                .or(previous_label);

            let fallback_prompt = self
                .agents_terminal
                .shared_task
                .clone()
                .or_else(|| self.agent_task.clone());
            let previous_prompt = entry.batch_prompt.clone();
            entry.batch_prompt = meta_prompt
                .or(fallback_prompt)
                .or(previous_prompt);

            let fallback_context = self
                .agents_terminal
                .shared_context
                .clone()
                .or_else(|| self.agent_context.clone());
            let previous_context = entry.batch_context.clone();
            entry.batch_context = meta_context
                .or(fallback_context)
                .or(previous_context);

            if entry.status != status {
                entry.status = status.clone();
                entry.push_log(
                    AgentLogKind::Status,
                    format!("Status → {}", agent_status_label(status.clone())),
                );
            }

            if let Some(progress) = info.last_progress.as_ref()
                && entry.last_progress.as_ref() != Some(progress) {
                    entry.last_progress = Some(progress.clone());
                    entry.push_log(AgentLogKind::Progress, progress.clone());
                }

            if let Some(result) = info.result.as_ref()
                && entry.result.as_ref() != Some(result) {
                    entry.result = Some(result.clone());
                    entry.push_log(AgentLogKind::Result, result.clone());
                }

            if let Some(error) = info.error.as_ref()
                && entry.error.as_ref() != Some(error) {
                    entry.error = Some(error.clone());
                    entry.push_log(AgentLogKind::Error, error.clone());
                }
        }

        if let Some(pending) = self.agents_terminal.pending_stop.clone() {
            let still_running = self
                .agents_terminal
                .entries
                .get(&pending.agent_id)
                .map(|entry| matches!(entry.status, AgentStatus::Pending | AgentStatus::Running))
                .unwrap_or(false);
            if !still_running {
                self.agents_terminal.clear_stop_prompt();
            }
        }

        self.agents_terminal.clamp_selected_index();

        if saw_new_agent && self.agents_terminal.active {
            self.layout.scroll_offset.set(0);
        }
    }

    fn enter_agents_terminal_mode(&mut self) {
        if self.agents_terminal.active {
            return;
        }
        self.browser_overlay_visible = false;
        self.agents_terminal.active = true;
        self.agents_terminal.focus_sidebar();
        self.agents_terminal.clear_stop_prompt();
        self.bottom_pane.set_input_focus(false);
        self.agents_terminal.saved_scroll_offset = self.layout.scroll_offset.get();
        if self.agents_terminal.order.is_empty() {
            for agent in &self.active_agents {
                if !self
                    .agents_terminal
                    .entries
                    .contains_key(&agent.id)
                {
                    self.agents_terminal.order.push(agent.id.clone());
                    let mut entry = AgentTerminalEntry::new(
                        agent.name.clone(),
                        agent.model.clone(),
                        agent.status.clone(),
                        agent.batch_id.clone(),
                    );
                    let batch_metadata = agent
                        .batch_id
                        .as_deref()
                        .map(|id| self.agent_batch_metadata(id))
                        .unwrap_or_default();
                    let AgentBatchMetadata { label, prompt: meta_prompt, context: meta_context } = batch_metadata;
                    entry.batch_label = label
                        .or_else(|| agent.batch_id.clone())
                        .or(entry.batch_label.clone());
                    let fallback_prompt = self
                        .agents_terminal
                        .shared_task
                        .clone()
                        .or_else(|| self.agent_task.clone());
                    entry.batch_prompt = meta_prompt
                        .or(fallback_prompt)
                        .or(entry.batch_prompt.clone());
                    let fallback_context = self
                        .agents_terminal
                        .shared_context
                        .clone()
                        .or_else(|| self.agent_context.clone());
                    entry.batch_context = meta_context
                        .or(fallback_context)
                        .or(entry.batch_context.clone());
                    if let Some(progress) = agent.last_progress.as_ref() {
                        entry.last_progress = Some(progress.clone());
                        entry.push_log(AgentLogKind::Progress, progress.clone());
                    }
                    if let Some(result) = agent.result.as_ref() {
                        entry.result = Some(result.clone());
                        entry.push_log(AgentLogKind::Result, result.clone());
                    }
                    if let Some(error) = agent.error.as_ref() {
                        entry.error = Some(error.clone());
                        entry.push_log(AgentLogKind::Error, error.clone());
                    }
                    self.agents_terminal
                        .entries
                        .insert(agent.id.clone(), entry);
                }
            }
        }
        self.agents_terminal.clamp_selected_index();
        self.restore_selected_agent_scroll();
        self.request_redraw();
    }

    fn exit_agents_terminal_mode(&mut self) {
        if !self.agents_terminal.active {
            return;
        }
        self.record_current_agent_scroll();
        self.agents_terminal.active = false;
        self.agents_terminal.clear_stop_prompt();
        self.agents_terminal.focus_sidebar();
        self.layout.scroll_offset
            .set(self.agents_terminal.saved_scroll_offset);
        self.bottom_pane.set_input_focus(true);
        self.request_redraw();
    }

    fn record_current_agent_scroll(&mut self) {
        if let Some(entry) = self.agents_terminal.current_sidebar_entry() {
            let capped = self
                .layout
                .scroll_offset
                .get()
                .min(self.layout.last_max_scroll.get());
            self
                .agents_terminal
                .scroll_offsets
                .insert(entry.scroll_key(), capped);
        }
    }

    fn restore_selected_agent_scroll(&mut self) {
        if let Some(entry) = self.agents_terminal.current_sidebar_entry() {
            // Always reset to the top when switching agents; use a sentinel so the
            // next render clamps to the new agent's maximum scroll.
            let key = entry.scroll_key();
            self
                .agents_terminal
                .scroll_offsets
                .insert(key, u16::MAX);
            self.layout.scroll_offset.set(u16::MAX);
        } else {
            self.layout.scroll_offset.set(0);
        }
    }

    fn sync_agents_terminal_scroll(&mut self) {
        if !self.agents_terminal.active {
            return;
        }
        let applied = self
            .agents_terminal
            .last_render_scroll
            .get()
            .min(self.layout.last_max_scroll.get());
        self.layout.scroll_offset.set(applied);
        if let Some(entry) = self.agents_terminal.current_sidebar_entry() {
            self
                .agents_terminal
                .scroll_offsets
                .insert(entry.scroll_key(), applied);
        }
    }

    fn prompt_stop_selected_agent(&mut self) {
        let Some(AgentsSidebarEntry::Agent(agent_id)) = self.agents_terminal.current_sidebar_entry() else {
            return;
        };

        let is_active = self
            .active_agents
            .iter()
            .any(|agent| agent.id == agent_id && matches!(agent.status, AgentStatus::Pending | AgentStatus::Running));
        let is_entry_active = self
            .agents_terminal
            .entries
            .get(agent_id.as_str())
            .map(|entry| matches!(entry.status, AgentStatus::Pending | AgentStatus::Running))
            .unwrap_or(false);

        if !(is_active || is_entry_active) {
            return;
        }

        let agent_name = self
            .agents_terminal
            .entries
            .get(agent_id.as_str())
            .map(|entry| entry.name.clone())
            .or_else(|| {
                self.active_agents
                    .iter()
                    .find(|a| a.id == agent_id)
                    .map(|a| a.name.clone())
            })
            .unwrap_or_else(|| agent_id.clone());

        self.agents_terminal
            .set_stop_prompt(agent_id, agent_name);
        self.request_redraw();
    }

    fn cancel_agent_by_id(&mut self, agent_id: &str) -> bool {
        let mut can_cancel = false;
        for agent in &self.active_agents {
            if agent.id == agent_id
                && matches!(agent.status, AgentStatus::Pending | AgentStatus::Running)
            {
                can_cancel = true;
                break;
            }
        }

        if !can_cancel {
            can_cancel = self
                .agents_terminal
                .entries
                .get(agent_id)
                .map(|entry| matches!(entry.status, AgentStatus::Pending | AgentStatus::Running))
                .unwrap_or(false);
        }

        if !can_cancel {
            return false;
        }

        let agent_name = self
            .agents_terminal
            .entries
            .get(agent_id)
            .map(|entry| entry.name.clone())
            .or_else(|| {
                self.active_agents
                    .iter()
                    .find(|a| a.id == agent_id)
                    .map(|a| a.name.clone())
            })
            .unwrap_or_else(|| agent_id.to_string());

        self.push_background_tail(format!("Cancelling agent {agent_name}…"));
        self.bottom_pane
            .update_status_text(format!("Cancelling {agent_name}…"));
        self.bottom_pane.set_task_running(true);
        self.agents_ready_to_start = false;

        self.submit_op(Op::CancelAgents {
            batch_ids: Vec::new(),
            agent_ids: vec![agent_id.to_string()],
        });

        for agent in &mut self.active_agents {
            if agent.id == agent_id
                && matches!(agent.status, AgentStatus::Pending | AgentStatus::Running)
            {
                agent.status = AgentStatus::Cancelled;
                agent.error.get_or_insert_with(|| "Cancelled by user".to_string());
            }
        }

        if let Some(entry) = self.agents_terminal.entries.get_mut(agent_id)
            && matches!(entry.status, AgentStatus::Pending | AgentStatus::Running) {
                entry.status = AgentStatus::Cancelled;
                entry.push_log(
                    AgentLogKind::Status,
                    format!("Status → {}", agent_status_label(AgentStatus::Cancelled)),
                );
            }

        self.request_redraw();
        true
    }

    fn navigate_agents_terminal_selection(&mut self, delta: isize) {
        let entries = self.agents_terminal.sidebar_entries();
        if entries.is_empty() {
            return;
        }
        self.agents_terminal.focus_sidebar();
        let len = entries.len() as isize;
        self.record_current_agent_scroll();
        let mut new_index = self.agents_terminal.selected_index as isize + delta;
        if new_index >= len {
            new_index %= len;
        }
        while new_index < 0 {
            new_index += len;
        }
        self.agents_terminal.selected_index = new_index as usize;
        self.agents_terminal.clamp_selected_index();
        self.agents_terminal.clear_stop_prompt();
        self.restore_selected_agent_scroll();
        self.request_redraw();
    }

    fn navigate_agents_terminal_page(&mut self, delta_pages: isize) {
        let entries = self.agents_terminal.sidebar_entries();
        if entries.is_empty() {
            return;
        }
        let page = self.layout.last_history_viewport_height.get() as isize;
        let step = if page > 0 { page.saturating_sub(1) } else { 1 };
        let delta = step.max(1) * delta_pages;
        self.navigate_agents_terminal_selection(delta);
    }

    fn navigate_agents_terminal_home(&mut self) {
        let entries = self.agents_terminal.sidebar_entries();
        if entries.is_empty() {
            return;
        }
        self.agents_terminal.selected_index = 0;
        self.agents_terminal.clamp_selected_index();
        self.agents_terminal.clear_stop_prompt();
        self.restore_selected_agent_scroll();
        self.request_redraw();
    }

    fn navigate_agents_terminal_end(&mut self) {
        let entries = self.agents_terminal.sidebar_entries();
        if entries.is_empty() {
            return;
        }
        self.agents_terminal.selected_index = entries.len().saturating_sub(1);
        self.agents_terminal.clamp_selected_index();
        self.agents_terminal.clear_stop_prompt();
        self.restore_selected_agent_scroll();
        self.request_redraw();
    }
    fn resolve_agent_install_command(&self, agent_name: &str) -> Option<(Vec<String>, String)> {
        let cmd = self
            .config
            .agents
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(agent_name))
            .map(|cfg| cfg.command.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| agent_name.to_string());
        if cmd.trim().is_empty() {
            return None;
        }

        #[cfg(target_os = "windows")]
        {
            let script = format!(
                "if (Get-Command {cmd} -ErrorAction SilentlyContinue) {{ Write-Output \"{cmd} already installed\"; exit 0 }} else {{ Write-Warning \"{cmd} is not installed.\"; Write-Output \"Please install {cmd} via winget, Chocolatey, or the vendor installer.\"; exit 1 }}",
                cmd = cmd
            );
            let command = vec![
                "powershell.exe".to_string(),
                "-NoProfile".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
                "-Command".to_string(),
                script.clone(),
            ];
            return Some((command, format!("PowerShell install check for {cmd}")));
        }

        #[cfg(target_os = "macos")]
        {
            let brew_formula = macos_brew_formula_for_command(&cmd);
            let script = format!("brew install {brew_formula}");
            let command = vec!["/bin/bash".to_string(), "-lc".to_string(), script.clone()];
            return Some((command, script));
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            fn linux_agent_install_script(agent_cmd: &str, npm_package: &str) -> String {
                format!(
                    "set -euo pipefail\n\
if ! command -v npm >/dev/null 2>&1; then\n\
    echo \"npm is required to install {agent_cmd}. Install Node.js 20+ and rerun.\" >&2\n\
    exit 1\n\
fi\n\
prefix=\"$(npm config get prefix 2>/dev/null || true)\"\n\
if [ -z \"$prefix\" ] || [ ! -w \"$prefix\" ]; then\n\
    prefix=\"$HOME/.npm-global\"\n\
fi\n\
mkdir -p \"$prefix/bin\"\n\
export PATH=\"$prefix/bin:$PATH\"\n\
export npm_config_prefix=\"$prefix\"\n\
node_major=0\n\
if command -v node >/dev/null 2>&1; then\n\
    node_major=\"$(node -v | sed 's/^v\\([0-9][0-9]*\\).*/\\1/')\"\n\
fi\n\
if [ \"$node_major\" -lt 20 ]; then\n\
    npm install -g n\n\
    export N_PREFIX=\"${{N_PREFIX:-$HOME/.n}}\"\n\
    mkdir -p \"$N_PREFIX/bin\"\n\
    export PATH=\"$N_PREFIX/bin:$PATH\"\n\
    n 20.18.1\n\
    hash -r\n\
    node_major=\"$(node -v | sed 's/^v\\([0-9][0-9]*\\).*/\\1/')\"\n\
    if [ \"$node_major\" -lt 20 ]; then\n\
        echo \"Failed to activate Node.js 20+. Check that $N_PREFIX/bin is on PATH.\" >&2\n\
        exit 1\n\
    fi\n\
else\n\
    export N_PREFIX=\"${{N_PREFIX:-$HOME/.n}}\"\n\
    if [ -d \"$N_PREFIX/bin\" ]; then\n\
        export PATH=\"$N_PREFIX/bin:$PATH\"\n\
    fi\n\
fi\n\
npm install -g {npm_package}\n\
hash -r\n\
if ! command -v {agent_cmd} >/dev/null 2>&1; then\n\
    echo \"{agent_cmd} installed but not found on PATH. Add 'export PATH=\\\"$prefix/bin:$PATH\\\"' to your shell profile.\" >&2\n\
    exit 1\n\
fi\n\
{agent_cmd} --version\n",
                    agent_cmd = agent_cmd,
                    npm_package = npm_package,
                )
            }

            let lowercase = agent_name.trim().to_ascii_lowercase();
            let script = match lowercase.as_str() {
                "claude" => linux_agent_install_script(&cmd, "@anthropic-ai/claude-code"),
                "gemini" => linux_agent_install_script(&cmd, "@google/gemini-cli"),
                "qwen" => linux_agent_install_script(&cmd, "@qwen-code/qwen-code"),
                _ => format!(
                    "{cmd} --version || (echo \"Please install {cmd} via your package manager\" && false)",
                    cmd = cmd
                ),
            };
            let command = vec!["/bin/bash".to_string(), "-lc".to_string(), script.clone()];
            return Some((command, script));
        }

        #[allow(unreachable_code)]
        {
            None
        }
    }

    pub(crate) fn launch_agent_install(
        &mut self,
        name: String,
        selected_index: usize,
    ) -> Option<TerminalLaunch> {
        self.agents_overview_selected_index = selected_index;
        let Some((_, default_command)) = self.resolve_agent_install_command(&name) else {
            self.history_push_plain_state(history_cell::new_error_event(format!(
                "No install command available for agent '{name}' on this platform."
            )));
            self.show_agents_overview_ui();
            return None;
        };
        let id = self.terminal.alloc_id();
        self.terminal.after = Some(TerminalAfter::RefreshAgentsAndClose { selected_index });
        let (controller_tx, controller_rx) = mpsc::channel();
        let controller = TerminalRunController { tx: controller_tx };
        let cwd = self.config.cwd.to_string_lossy().to_string();
        self.push_background_before_next_output(format!(
            "Starting guided install for agent '{name}'"
        ));
        start_agent_install_session(AgentInstallSessionArgs {
            app_event_tx: self.app_event_tx.clone(),
            terminal_id: id,
            agent_name: name.clone(),
            default_command,
            cwd: Some(cwd),
            control: GuidedTerminalControl {
                controller: controller.clone(),
                controller_rx,
            },
            selected_index,
            debug_enabled: self.config.debug,
        });
        Some(TerminalLaunch {
            id,
            title: format!("Install {name}"),
            command: Vec::new(),
            command_display: "Preparing install assistant…".to_string(),
            controller: Some(controller),
            auto_close_on_success: false,
            start_running: true,
        })
    }

    pub(crate) fn launch_validation_tool_install(
        &mut self,
        tool_name: &str,
        install_hint: &str,
    ) -> Option<TerminalLaunch> {
        let trimmed = install_hint.trim();
        if trimmed.is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(format!(
                "No install command available for validation tool '{tool_name}'."
            )));
            self.request_redraw();
            return None;
        }

        let wrapped = wrap_command(trimmed);
        if wrapped.is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(format!(
                "Unable to build install command for validation tool '{tool_name}'."
            )));
            self.request_redraw();
            return None;
        }

        let id = self.terminal.alloc_id();
        let display = Self::truncate_with_ellipsis(trimmed, 128);
        let launch = TerminalLaunch {
            id,
            title: format!("Install {tool_name}"),
            command: wrapped,
            command_display: display,
            controller: None,
            auto_close_on_success: false,
            start_running: true,
        };

        self.push_background_before_next_output(format!(
            "Installing validation tool '{tool_name}' with `{trimmed}`"
        ));
        Some(launch)
    }

    fn try_handle_terminal_shortcut(&mut self, raw_text: &str) -> bool {
        let trimmed = raw_text.trim_start();
        if let Some(rest) = trimmed.strip_prefix("$$") {
            let prompt = rest.trim();
            if prompt.is_empty() {
                self.history_push_plain_state(history_cell::new_error_event(
                    "No prompt provided after '$$'.".to_string(),
                ));
                self.app_event_tx.send(AppEvent::RequestRedraw);
            } else {
                self.launch_guided_terminal_prompt(prompt);
            }
            return true;
        }
        if let Some(rest) = trimmed.strip_prefix('$') {
            let command = rest.trim();
            if command.is_empty() {
                self.launch_manual_terminal();
            } else {
                self.run_terminal_command(command);
            }
            return true;
        }
        false
    }

    fn launch_manual_terminal(&mut self) {
        let id = self.terminal.alloc_id();
        let launch = TerminalLaunch {
            id,
            title: "Shell".to_string(),
            command: Vec::new(),
            command_display: String::new(),
            controller: None,
            auto_close_on_success: false,
            start_running: false,
        };
        self.app_event_tx.send(AppEvent::OpenTerminal(launch));
    }

    fn run_terminal_command(&mut self, command: &str) {
        if wrap_command(command).is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(
                "Unable to build shell command for execution.".to_string(),
            ));
            self.app_event_tx.send(AppEvent::RequestRedraw);
            return;
        }

        let id = self.terminal.alloc_id();
        let title = Self::truncate_with_ellipsis(&format!("Shell: {command}"), 64);
        let display = Self::truncate_with_ellipsis(command, 128);
        let (controller_tx, controller_rx) = mpsc::channel();
        let controller = TerminalRunController { tx: controller_tx };
        let launch = TerminalLaunch {
            id,
            title,
            command: Vec::new(),
            command_display: display,
            controller: Some(controller.clone()),
            auto_close_on_success: false,
            start_running: true,
        };
        self.push_background_before_next_output(format!(
            "Terminal command: {command}"
        ));
        self.app_event_tx.send(AppEvent::OpenTerminal(launch));
        let cwd = self.config.cwd.to_string_lossy().to_string();
        start_direct_terminal_session(
            self.app_event_tx.clone(),
            id,
            command.to_string(),
            Some(cwd),
            controller,
            controller_rx,
            self.config.debug,
        );
    }

    fn launch_guided_terminal_prompt(&mut self, prompt: &str) {
        let id = self.terminal.alloc_id();
        let (controller_tx, controller_rx) = mpsc::channel();
        let controller = TerminalRunController { tx: controller_tx };
        let cwd = self.config.cwd.to_string_lossy().to_string();
        let title = Self::truncate_with_ellipsis(&format!("Guided: {prompt}"), 64);
        let display = Self::truncate_with_ellipsis(prompt, 128);

        let launch = TerminalLaunch {
            id,
            title,
            command: Vec::new(),
            command_display: display,
            controller: Some(controller.clone()),
            auto_close_on_success: false,
            start_running: true,
        };

        self.push_background_before_next_output(format!(
            "Guided terminal request: {prompt}"
        ));
        self.app_event_tx.send(AppEvent::OpenTerminal(launch));
        start_prompt_terminal_session(
            self.app_event_tx.clone(),
            id,
            prompt.to_string(),
            Some(cwd),
            controller,
            controller_rx,
            self.config.debug,
        );
    }

}
