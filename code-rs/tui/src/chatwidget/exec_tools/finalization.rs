use super::*;
use super::helpers::*;

pub(in super::super) fn finalize_exec_cell_at(
    chat: &mut ChatWidget<'_>,
    idx: usize,
    exit_code: i32,
    stdout: String,
    stderr: String,
) {
    if idx >= chat.history_cells.len() {
        return;
    }
    if let Some(exec) = chat.history_cells[idx]
        .as_any()
        .downcast_ref::<history_cell::ExecCell>()
        && exec.output.is_none() {
            let completed = history_cell::new_completed_exec_command(
                exec.command.clone(),
                exec.parsed.clone(),
                CommandOutput {
                    exit_code,
                    stdout,
                    stderr,
                },
            );
            chat.history_replace_at(idx, Box::new(completed));
        }
}

pub(in super::super) fn finalize_all_running_as_interrupted(chat: &mut ChatWidget<'_>) {
    let interrupted_msg = "Cancelled by user.".to_string();
    let stdout_empty = String::new();
    let running: Vec<RunningExecEntry> = chat
        .exec
        .running_commands
        .iter()
        .map(|(k, v)| (k.clone(), v.history_index, v.explore_entry))
        .collect();
    let mut agg_was_updated = false;
    for (call_id, maybe_idx, explore_entry) in &running {
        if let Some(idx) = maybe_idx {
            finalize_exec_cell_at(
                chat,
                *idx,
                130,
                stdout_empty.clone(),
                interrupted_msg.clone(),
            );
        }
        if let Some((agg_idx, entry_idx)) = explore_entry
            && *agg_idx < chat.history_cells.len()
                && let Some(existing) = chat.history_cells[*agg_idx]
                    .as_any()
                    .downcast_ref::<history_cell::ExploreAggregationCell>()
                {
                    let mut record = existing.record().clone();
                    history_cell::explore_record_update_status(
                        &mut record,
                        *entry_idx,
                        history_cell::ExploreEntryStatus::Error { exit_code: None },
                    );
                    let cell = history_cell::ExploreAggregationCell::from_record(record.clone());
                    chat.history_replace_with_record(
                        *agg_idx,
                        Box::new(cell),
                        HistoryDomainRecord::Explore(record),
                    );
                    chat.autoscroll_if_near_bottom();
                    agg_was_updated = true;
                }
        chat.canceled_exec_call_ids.insert(call_id.clone());
    }
    chat.exec.running_commands.clear();
    if agg_was_updated {
        chat.exec.running_explore_agg_index = None;
        chat.invalidate_height_cache();
        chat.request_redraw();
    }

    if !chat.tools_state.running_custom_tools.is_empty() {
        let entries: Vec<(super::ToolCallId, super::RunningToolEntry)> = chat
            .tools_state
            .running_custom_tools
            .iter()
            .map(|(k, entry)| (k.clone(), *entry))
            .collect();
        for (tool_id, entry) in entries {
                if let Some(idx) = running_tools::resolve_entry_index(chat, &entry, &tool_id.0)
                    && idx < chat.history_cells.len() {
                    let emphasis = TextEmphasis {
                        bold: true,
                        ..TextEmphasis::default()
                    };
                    let wait_state = PlainMessageState {
                        id: HistoryId::ZERO,
                        role: PlainMessageRole::Error,
                        kind: PlainMessageKind::Error,
                        header: None,
                        lines: vec![MessageLine {
                            kind: MessageLineKind::Paragraph,
                            spans: vec![InlineSpan {
                                text: "Wait cancelled".into(),
                                tone: TextTone::Error,
                                emphasis,
                                entity: None,
                            }],
                        }],
                        metadata: None,
                    };

                    let replaced = chat.history_cells[idx]
                        .as_any()
                        .downcast_ref::<history_cell::RunningToolCallCell>()
                        .map(|cell| cell.has_title("Waiting"))
                        .unwrap_or(false);

                    if replaced {
                        chat.history_replace_with_record(
                            idx,
                            Box::new(history_cell::PlainHistoryCell::from_state(wait_state.clone())),
                            HistoryDomainRecord::Plain(wait_state.clone()),
                        );
                    } else {
                        let completed = history_cell::new_completed_custom_tool_call(
                            "custom".to_string(),
                            None,
                            std::time::Duration::from_millis(0),
                            false,
                            "Cancelled by user.".to_string(),
                        );
                        chat.history_replace_at(idx, Box::new(completed));
                    }
                }
        }
        chat.tools_state.running_custom_tools.clear();
        chat.invalidate_height_cache();
        chat.request_redraw();
    }

    web_search_sessions::finalize_all_failed(chat, "Search cancelled by user.");

    if !chat.tools_state.running_wait_tools.is_empty() {
        chat.tools_state.running_wait_tools.clear();
    }

    if !chat.tools_state.running_kill_tools.is_empty() {
        chat.tools_state.running_kill_tools.clear();
    }

    chat.bottom_pane.update_status_text("cancelled".to_string());
    let any_tasks_active = !chat.active_task_ids.is_empty();
    if !any_tasks_active {
        chat.bottom_pane.set_task_running(false);
    }
    chat.maybe_hide_spinner();
    chat.refresh_auto_drive_visuals();
}

pub(in super::super) fn finalize_all_running_due_to_answer(chat: &mut ChatWidget<'_>) {
    const STALE_MSG: &str = "Running in background after turn end.";
    const STREAM_TAIL_MAX_BYTES: usize = 64 * 1024;

    // Drain running execs so we can mark them stale and stop the spinner.
    let mut agg_was_updated = false;
    let running_keys: Vec<super::ExecCallId> = chat
        .exec
        .running_commands
        .keys()
        .cloned()
        .collect();

    for call_id in running_keys {
        if let Some(running) = chat.exec.running_commands.remove(&call_id) {
            // Update any explore aggregation entry tied to this exec.
            if let Some((agg_idx, entry_idx)) = running.explore_entry {
                let updated = update_explore_entry_status(
                    chat,
                    Some(agg_idx),
                    entry_idx,
                    history_cell::ExploreEntryStatus::Success,
                )
                .is_some();
                agg_was_updated |= updated;
            }

            let exit_code = 0;
            let now = SystemTime::now();
            let wait_notes_pairs = running.wait_notes.clone();
            let wait_notes_record = exec_wait_notes_from_pairs(&wait_notes_pairs);

            let history_id = running
                .history_id
                .or_else(|| chat.history_state.history_id_for_exec_call(call_id.as_ref()))
                .or_else(|| {
                    running
                        .history_index
                        .and_then(|idx| chat.history_cell_ids.get(idx).and_then(|h| *h))
                });

            let stderr_prefix = if running.stderr_offset > 0 { "\n" } else { "" };
            let stderr_tail_event = Some(format!("{stderr_prefix}{STALE_MSG}"));
            let stdout_tail_event = None;

            let finish_mutation = chat.history_state.apply_domain_event(
                HistoryDomainEvent::FinishExec {
                    id: history_id,
                    call_id: Some(call_id.as_ref().to_string()),
                    status: ExecStatus::Success,
                    exit_code: Some(exit_code),
                    completed_at: Some(now),
                    wait_total: running.wait_total,
                    wait_active: false,
                    wait_notes: wait_notes_record,
                    stdout_tail: stdout_tail_event,
                    stderr_tail: stderr_tail_event,
                },
            );

            let mut handled_via_state = false;
            if let HistoryMutation::Replaced {
                id,
                record: HistoryRecord::Exec(exec_record),
                ..
            } = finish_mutation
            {
                chat.update_cell_from_record(id, HistoryRecord::Exec(exec_record.clone()));
                if let Some(idx) = chat.cell_index_for_history_id(id) {
                    crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
                }
                handled_via_state = true;
            }

            if !handled_via_state {
                let mut stdout_so_far = String::new();
                let mut stderr_so_far = String::new();
                if let Some(history_id) = history_id
                    && let Some(record) = chat.history_state.record(history_id) {
                        match record {
                            HistoryRecord::Exec(exec_record) => {
                                stdout_so_far = stream_chunks_tail_to_text(
                                    &exec_record.stdout_chunks,
                                    STREAM_TAIL_MAX_BYTES,
                                );
                                stderr_so_far = stream_chunks_tail_to_text(
                                    &exec_record.stderr_chunks,
                                    STREAM_TAIL_MAX_BYTES,
                                );
                            }
                            HistoryRecord::MergedExec(merged) => {
                                if let Some(last) = merged.segments.last() {
                                    stdout_so_far = stream_chunks_tail_to_text(
                                        &last.stdout_chunks,
                                        STREAM_TAIL_MAX_BYTES,
                                    );
                                    stderr_so_far = stream_chunks_tail_to_text(
                                        &last.stderr_chunks,
                                        STREAM_TAIL_MAX_BYTES,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }

                if !stderr_so_far.is_empty() {
                    stderr_so_far.push('\n');
                }
                stderr_so_far.push_str(STALE_MSG);

                let mut completed_cell = history_cell::new_completed_exec_command(
                    running.command.clone(),
                    running.parsed.clone(),
                    CommandOutput {
                        exit_code,
                        stdout: stdout_so_far,
                        stderr: stderr_so_far,
                    },
                );
                // Preserve linkage to the original call id when possible.
                completed_cell.record.call_id = Some(call_id.as_ref().to_string());

                if let Some(idx) = running.history_index {
                    chat.history_replace_and_maybe_merge(idx, Box::new(completed_cell));
                } else {
                    let key = chat.next_internal_key();
                    let idx = chat.history_insert_with_key_global(Box::new(completed_cell), key);
                    crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
                }
            }
        }
    }

    if agg_was_updated {
        chat.exec.running_explore_agg_index = None;
        chat.invalidate_height_cache();
        chat.request_redraw();
    }

    crate::chatwidget::running_tools::finalize_all_due_to_answer(chat);

    web_search_sessions::finalize_all_completed(chat, "Search finished");

    chat.maybe_hide_spinner();
    chat.refresh_auto_drive_visuals();
}

pub(in super::super) fn finalize_wait_missing_exec(
    chat: &mut ChatWidget<'_>,
    call_id: super::ExecCallId,
    message: &str,
) -> bool {
    const STREAM_TAIL_MAX_BYTES: usize = 64 * 1024;
    let trimmed = message.trim();
    let fallback_message = if trimmed.is_empty() {
        "Background job finished but output was unavailable."
    } else {
        trimmed
    };

    let history_id: Option<HistoryId>;
    let mut history_index: Option<usize> = None;
    let mut command: Vec<String> = Vec::new();
    let mut parsed: Vec<ParsedCommand> = Vec::new();
    let mut wait_total: Option<std::time::Duration> = None;
    let mut wait_notes_pairs: Vec<(String, bool)> = Vec::new();
    let mut explore_entry: Option<(usize, usize)> = None;

    if let Some(running) = chat.exec.running_commands.remove(&call_id) {
        history_id = running
            .history_id
            .or_else(|| chat.history_state.history_id_for_exec_call(call_id.as_ref()))
            .or_else(|| {
                running
                    .history_index
                    .and_then(|idx| chat.history_cell_ids.get(idx).and_then(|slot| *slot))
            });
        history_index = running.history_index;
        command = running.command;
        parsed = running.parsed;
        wait_total = running.wait_total;
        wait_notes_pairs = running.wait_notes;
        explore_entry = running.explore_entry;
    } else {
        history_id = chat.history_state.history_id_for_exec_call(call_id.as_ref());
        if let Some(id) = history_id {
            if let Some(HistoryRecord::Exec(record)) = chat.history_state.record(id).cloned() {
                command = record.command.clone();
                parsed = record.parsed.clone();
                wait_total = record.wait_total;
                wait_notes_pairs = ChatWidget::wait_pairs_from_exec_notes(&record.wait_notes);
            }
        } else {
            return false;
        }
    }

    if let Some(id) = history_id
        && let Some(record) = chat.history_state.record(id) {
            match record {
                HistoryRecord::Exec(exec_record) => {
                    if exec_record.status != ExecStatus::Running {
                        chat.maybe_hide_spinner();
                        chat.refresh_auto_drive_visuals();
                        return true;
                    }
                }
                HistoryRecord::MergedExec(_) => {
                    chat.maybe_hide_spinner();
                    chat.refresh_auto_drive_visuals();
                    return true;
                }
                _ => {}
            }
        }

    if let Some((agg_idx, entry_idx)) = explore_entry {
        let status = history_cell::ExploreEntryStatus::Error { exit_code: None };
        let updated =
            update_explore_entry_status(chat, Some(agg_idx), entry_idx, status).is_some();
        if updated && !chat
            .exec
            .running_commands
            .values()
            .any(|rc| rc.explore_entry.is_some())
        {
            chat.exec.running_explore_agg_index = None;
        }
    }

    let wait_notes_record = exec_wait_notes_from_pairs(&wait_notes_pairs);
    let finish_mutation = chat
        .history_state
        .apply_domain_event(HistoryDomainEvent::FinishExec {
            id: history_id,
            call_id: Some(call_id.as_ref().to_string()),
            status: ExecStatus::Error,
            exit_code: Some(-1),
            completed_at: Some(SystemTime::now()),
            wait_total,
            wait_active: false,
            wait_notes: wait_notes_record,
            stdout_tail: None,
            stderr_tail: Some(fallback_message.to_string()),
        });

    let mut handled_via_state = false;
    if let HistoryMutation::Replaced {
        id,
        record: HistoryRecord::Exec(exec_record),
        ..
    } = finish_mutation
    {
        chat.update_cell_from_record(id, HistoryRecord::Exec(exec_record));
        if let Some(idx) = chat.cell_index_for_history_id(id) {
            crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
        }
        handled_via_state = true;
    }

    if !handled_via_state {
        let mut stdout_so_far = String::new();
        let mut stderr_so_far = String::new();
        if let Some(history_id) = history_id
            && let Some(record) = chat.history_state.record(history_id) {
                match record {
                    HistoryRecord::Exec(exec_record) => {
                        stdout_so_far = stream_chunks_tail_to_text(
                            &exec_record.stdout_chunks,
                            STREAM_TAIL_MAX_BYTES,
                        );
                        stderr_so_far = stream_chunks_tail_to_text(
                            &exec_record.stderr_chunks,
                            STREAM_TAIL_MAX_BYTES,
                        );
                    }
                    HistoryRecord::MergedExec(merged) => {
                        if let Some(last) = merged.segments.last() {
                            stdout_so_far = stream_chunks_tail_to_text(
                                &last.stdout_chunks,
                                STREAM_TAIL_MAX_BYTES,
                            );
                            stderr_so_far = stream_chunks_tail_to_text(
                                &last.stderr_chunks,
                                STREAM_TAIL_MAX_BYTES,
                            );
                        }
                    }
                    _ => {}
                }
            }

        if !stderr_so_far.is_empty() {
            stderr_so_far.push('\n');
        }
        stderr_so_far.push_str(fallback_message);

        let mut completed_cell = history_cell::new_completed_exec_command(
            command,
            parsed,
            CommandOutput {
                exit_code: -1,
                stdout: stdout_so_far,
                stderr: stderr_so_far,
            },
        );
        completed_cell.record.call_id = Some(call_id.as_ref().to_string());

        if let Some(idx) = history_index {
            chat.history_replace_and_maybe_merge(idx, Box::new(completed_cell));
        } else {
            let key = chat.next_internal_key();
            let idx = chat.history_insert_with_key_global(Box::new(completed_cell), key);
            crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
        }
    }

    chat.maybe_hide_spinner();
    chat.refresh_auto_drive_visuals();
    true
}

pub(in super::super) fn try_merge_completed_exec_at(chat: &mut ChatWidget<'_>, idx: usize) {
    if idx == 0 || idx >= chat.history_cells.len() {
        return;
    }

    let Some(HistoryRecord::Exec(current_exec)) = history_record_for_cell(chat, idx) else {
        return;
    };

    if !exec_record_has_output(&current_exec) {
        return;
    }

    if matches!(current_exec.action, ExecAction::Run) {
        return;
    }

    let Some(prev_record) = history_record_for_cell(chat, idx - 1) else {
        return;
    };

    match prev_record {
        HistoryRecord::Exec(prev_exec) => {
            if prev_exec.action != current_exec.action {
                return;
            }
            if !exec_record_has_output(&prev_exec) {
                return;
            }

            let merged = history_cell::MergedExecCell::from_records(
                prev_exec.id,
                prev_exec.action,
                vec![prev_exec, current_exec],
            );
            chat.history_replace_at(idx - 1, Box::new(merged));
            chat.history_remove_at(idx);
            chat.autoscroll_if_near_bottom();
            chat.bottom_pane.set_has_chat_history(true);
            chat.process_animation_cleanup();
            chat.app_event_tx.send(AppEvent::RequestRedraw);
        }
        HistoryRecord::MergedExec(mut merged_exec) => {
            if merged_exec.action != current_exec.action {
                return;
            }
            merged_exec.segments.push(current_exec);
            let merged_cell = history_cell::MergedExecCell::from_state(merged_exec.clone());
            chat.history_replace_at(idx - 1, Box::new(merged_cell));
            chat.history_remove_at(idx);
            chat.autoscroll_if_near_bottom();
            chat.bottom_pane.set_has_chat_history(true);
            chat.process_animation_cleanup();
            chat.app_event_tx.send(AppEvent::RequestRedraw);
        }
        _ => {}
    }
}
