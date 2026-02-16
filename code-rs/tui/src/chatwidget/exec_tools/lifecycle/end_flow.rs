use super::*;

pub(in super::super::super) fn handle_exec_end_now(
    chat: &mut ChatWidget<'_>,
    ev: ExecCommandEndEvent,
    order: &OrderMeta,
) {
    let call_id = super::ExecCallId(ev.call_id.clone());
    let suppressing = chat.exec.should_suppress_exec_end(&call_id);
    if suppressing {
        chat.exec.unsuppress_exec_end(&call_id);
        if !chat.exec.running_commands.contains_key(&call_id) {
            chat.ended_call_ids.insert(super::ExecCallId(ev.call_id));
            chat.maybe_hide_spinner();
            chat.refresh_auto_drive_visuals();
            return;
        }
    }
    chat
        .ended_call_ids
        .insert(super::ExecCallId(ev.call_id.clone()));
    // If this call was already marked as cancelled, drop the End to avoid
    // inserting a duplicate completed cell after the user interrupt.
    if chat
        .canceled_exec_call_ids
        .remove(&super::ExecCallId(ev.call_id.clone()))
    {
        chat.maybe_hide_spinner();
        chat.refresh_auto_drive_visuals();
        return;
    }
    let ExecCommandEndEvent {
        call_id,
        exit_code,
        duration,
        stdout,
        stderr,
    } = ev;
    let cmd = chat
        .exec
        .running_commands
        .remove(&super::ExecCallId(call_id.clone()));
    chat.height_manager
        .borrow_mut()
        .record_event(HeightEvent::RunEnd);
    let (
        command,
        parsed,
        history_id,
        history_index,
        explore_entry,
        wait_total,
        wait_notes,
        streamed_stdout,
        streamed_stderr,
    ) = match cmd {
        Some(super::RunningCommand {
            command,
            parsed,
            history_index,
            history_id,
            explore_entry,
            wait_total,
            wait_notes,
            ..
        }) => {
            let mut streamed_stdout = String::new();
            let mut streamed_stderr = String::new();
            if let Some(id) = history_id {
                if let Some(HistoryRecord::Exec(record)) = chat.history_state.record(id).cloned() {
                    streamed_stdout = stream_chunks_to_text(&record.stdout_chunks);
                    streamed_stderr = stream_chunks_to_text(&record.stderr_chunks);
                }
            } else if let Some(idx) = history_index
                && let Some(exec_cell) = chat
                    .history_cells
                    .get(idx)
                    .and_then(|cell| cell.as_any().downcast_ref::<history_cell::ExecCell>())
                {
                    streamed_stdout = stream_chunks_to_text(&exec_cell.record.stdout_chunks);
                    streamed_stderr = stream_chunks_to_text(&exec_cell.record.stderr_chunks);
                }

            (
                command,
                parsed,
                history_id,
                history_index,
                explore_entry,
                wait_total,
                wait_notes,
                streamed_stdout,
                streamed_stderr,
            )
        }
        None => {
            let mut history_id = chat
                .history_state
                .history_id_for_exec_call(call_id.as_ref());
            let mut history_index = history_id.and_then(|id| chat.cell_index_for_history_id(id));
            let mut command = vec![format!("Command running ({call_id})")];
            let mut parsed: Vec<ParsedCommand> = Vec::new();
            let mut wait_total: Option<std::time::Duration> = None;
            let mut wait_notes_pairs: Vec<(String, bool)> = Vec::new();
            let mut streamed_stdout = String::new();
            let mut streamed_stderr = String::new();

            if let Some(id) = history_id
                && let Some(HistoryRecord::Exec(record)) = chat.history_state.record(id).cloned() {
                    command = record.command.clone();
                    parsed = record.parsed.clone();
                    wait_total = record.wait_total;
                    wait_notes_pairs = ChatWidget::wait_pairs_from_exec_notes(&record.wait_notes);
                    streamed_stdout = stream_chunks_to_text(&record.stdout_chunks);
                    streamed_stderr = stream_chunks_to_text(&record.stderr_chunks);
                    if history_index.is_none() {
                        history_index = chat.cell_index_for_history_id(id);
                    }
                }

            if history_index.is_none()
                && let Some((idx, exec_cell)) = chat
                    .history_cells
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(idx, cell)| {
                        cell.as_any()
                            .downcast_ref::<history_cell::ExecCell>()
                            .and_then(|exec_cell| {
                                if exec_cell.record.call_id.as_deref() == Some(call_id.as_ref()) {
                                    Some((idx, exec_cell))
                                } else {
                                    None
                                }
                            })
                    })
                {
                    history_index = Some(idx);
                    history_id = chat.history_cell_ids.get(idx).and_then(|slot| *slot);
                    command = exec_cell.command.clone();
                    parsed = exec_cell.parsed.clone();
                    wait_total = exec_cell.record.wait_total;
                    wait_notes_pairs = ChatWidget::wait_pairs_from_exec_notes(&exec_cell.record.wait_notes);
                    streamed_stdout = stream_chunks_to_text(&exec_cell.record.stdout_chunks);
                    streamed_stderr = stream_chunks_to_text(&exec_cell.record.stderr_chunks);
                }

            (
                command,
                parsed,
                history_id,
                history_index,
                None,
                wait_total,
                wait_notes_pairs,
                streamed_stdout,
                streamed_stderr,
            )
        }
    };

    if let Some((agg_idx, entry_idx)) = explore_entry {
        let action = history_cell::action_enum_from_parsed(&parsed);
        let status = match (exit_code, action) {
            (0, _) => history_cell::ExploreEntryStatus::Success,
            (1, ExecAction::Search) => history_cell::ExploreEntryStatus::NotFound,
            (1, ExecAction::List) => history_cell::ExploreEntryStatus::NotFound,
            _ => history_cell::ExploreEntryStatus::Error {
                exit_code: Some(exit_code),
            },
        };
        let updated_index = update_explore_entry_status(chat, Some(agg_idx), entry_idx, status.clone());
        if !chat
            .exec
            .running_commands
            .values()
            .any(|rc| rc.explore_entry.is_some())
        {
            chat.exec.running_explore_agg_index = None;
        } else if let Some(actual_idx) = updated_index {
            chat.exec.running_explore_agg_index = Some(actual_idx);
        }
        let status_text = match status {
            history_cell::ExploreEntryStatus::Success => match action {
                ExecAction::Read => "files read".to_string(),
                _ => "exploration updated".to_string(),
            },
            history_cell::ExploreEntryStatus::NotFound => match action {
                ExecAction::List => "path not found".to_string(),
                _ => "no matches found".to_string(),
            },
            history_cell::ExploreEntryStatus::Error { .. } => match action {
                ExecAction::Read => format!("read failed (exit {exit_code})"),
                ExecAction::Search => {
                    if exit_code == 2 { "invalid pattern".to_string() } else { format!("search failed (exit {exit_code})") }
                }
                ExecAction::List => format!("list failed (exit {exit_code})"),
                _ => format!("exploration failed (exit {exit_code})"),
            },
            history_cell::ExploreEntryStatus::Running => "exploring…".to_string(),
        };
        chat.bottom_pane.update_status_text(status_text);
        chat.maybe_hide_spinner();
        chat.refresh_auto_drive_visuals();
        return;
    }

    let command_for_watch = command.clone();
    let wait_notes_pairs = wait_notes;
    let status = if exit_code == 0 {
        ExecStatus::Success
    } else {
        ExecStatus::Error
    };
    let now = SystemTime::now();
    let wait_notes_record = exec_wait_notes_from_pairs(&wait_notes_pairs);
    let stdout_tail_event = stream_tail(&stdout, &streamed_stdout);
    let stderr_tail_event = stream_tail(&stderr, &streamed_stderr);

    let finish_mutation = chat
        .history_state
        .apply_domain_event(HistoryDomainEvent::FinishExec {
            id: history_id,
            call_id: Some(call_id.clone()),
            status,
            exit_code: Some(exit_code),
            completed_at: Some(now),
            wait_total,
            wait_active: false,
            wait_notes: wait_notes_record,
            stdout_tail: stdout_tail_event,
            stderr_tail: stderr_tail_event,
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
        let mut completed_opt = Some(history_cell::new_completed_exec_command(
            command,
            parsed,
            CommandOutput {
                exit_code,
                stdout,
                stderr,
            },
        ));
        if let Some(cell) = completed_opt.as_mut() {
            cell.set_wait_total(wait_total);
            cell.set_wait_notes(&wait_notes_pairs);
            cell.set_waiting(false);
            cell.set_run_duration(Some(duration));
            if cell.record.call_id.as_deref().is_none() {
                cell.record.call_id = Some(call_id.clone());
            }
        }

        let mut replaced = false;
        if let Some(idx) = history_index {
            if idx < chat.history_cells.len() {
                let is_match = chat.history_cells[idx]
                    .as_any()
                    .downcast_ref::<history_cell::ExecCell>()
                    .map(|e| {
                        if let Some(ref c) = completed_opt {
                            // Match by command OR call_id to reliably update the correct cell
                            let command_matches = e.command == c.command;
                            let call_id_matches = e.record.call_id.as_deref() == Some(call_id.as_ref());
                            (command_matches || call_id_matches) && e.output.is_none()
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if is_match {
                    if let Some(c) = completed_opt.take() {
                        chat.history_replace_and_maybe_merge(idx, Box::new(c));
                    }
                    replaced = true;
                }
            }
            if !replaced {
                let mut found: Option<usize> = None;
                for i in (0..chat.history_cells.len()).rev() {
                    if let Some(exec) = chat.history_cells[i]
                        .as_any()
                        .downcast_ref::<history_cell::ExecCell>()
                    {
                        let is_same = if let Some(ref c) = completed_opt {
                            // Match by command OR call_id to avoid leaving exec cells stuck running
                            let command_matches = exec.command == c.command;
                            let call_id_matches = exec.record.call_id.as_deref() == Some(call_id.as_ref());
                            command_matches || call_id_matches
                        } else {
                            false
                        };
                        if exec.output.is_none() && is_same {
                            found = Some(i);
                            break;
                        }
                    }
                }
                if let Some(i) = found {
                    if let Some(c) = completed_opt.take() {
                        chat.history_replace_and_maybe_merge(i, Box::new(c));
                    }
                    replaced = true;
                }
            }
        }

        if !replaced
            && let Some(c) = completed_opt.take() {
                let key = chat.provider_order_key_from_order_meta(order);
                let idx = chat.history_insert_with_key_global(Box::new(c), key);
                crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
            }
    }

    if exit_code == 0 {
        chat
            .bottom_pane
            .update_status_text("command completed".to_string());
        let gh_ticket = chat.make_background_tail_ticket();
        let tx = chat.app_event_tx.clone();
        let cfg = chat.config.clone();
        crate::chatwidget::gh_actions::maybe_watch_after_push(
            tx,
            cfg,
            &command_for_watch,
            gh_ticket,
        );
    } else {
        chat
            .bottom_pane
            .update_status_text(format!("command failed (exit {exit_code})"));
    }
    chat.maybe_hide_spinner();
    chat.refresh_auto_drive_visuals();
}

// Stable ordering now inserts at the correct position; these helpers are removed.

// `handle_exec_approval_now` remains on ChatWidget in chatwidget.rs because
// it is referenced directly from interrupt handling and is trivial.
