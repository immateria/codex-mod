use super::*;

fn try_upgrade_fallback_exec_cell(
    chat: &mut ChatWidget<'_>,
    ev: &ExecCommandBeginEvent,
) -> bool {
    for i in (0..chat.history_cells.len()).rev() {
        if let Some(exec) = chat.history_cells[i]
            .as_any()
            .downcast_ref::<history_cell::ExecCell>()
        {
            let command_matches_call = exec.command.len() == 1
                && exec.command
                    .first()
                    .map(|cmd| cmd == &ev.call_id)
                    .unwrap_or(false);
            let record_matches_call = exec
                .record
                .call_id
                .as_deref()
                .map(|cid| cid == ev.call_id)
                .unwrap_or(false);
            let looks_like_fallback = exec.output.is_some()
                && exec.parsed.is_empty()
                && (command_matches_call || record_matches_call);
            if looks_like_fallback {
                let mut upgraded = false;
                if let Some(HistoryRecord::Exec(mut exec_record)) =
                    history_record_for_cell(chat, i)
                {
                    exec_record.command = ev.command.clone();
                    exec_record.parsed = ev.parsed_cmd.clone();
                    exec_record.action = history_cell::action_enum_from_parsed(&exec_record.parsed);
                    exec_record.call_id = Some(ev.call_id.clone());
                    if exec_record.working_dir.is_none() {
                        exec_record.working_dir = Some(ev.cwd.clone());
                    }

                    let record_index = chat
                        .record_index_for_cell(i)
                        .unwrap_or_else(|| chat.record_index_for_position(i));
                    let mutation = chat.history_state.apply_domain_event(
                        HistoryDomainEvent::Replace {
                            index: record_index,
                            record: HistoryDomainRecord::Exec(exec_record),
                        },
                    );

                    if let HistoryMutation::Replaced {
                        id,
                        record: HistoryRecord::Exec(updated_record),
                        ..
                    } = mutation
                    {
                        chat.update_cell_from_record(
                            id,
                            HistoryRecord::Exec(updated_record),
                        );
                        if let Some(idx) = chat.cell_index_for_history_id(id) {
                            if promote_exec_cell_to_explore(chat, idx) {
                                return true;
                            }
                            crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
                        }
                        upgraded = true;
                    }
                }

                if !upgraded {
                    if let Some(exec_mut) = chat.history_cells[i]
                        .as_any_mut()
                        .downcast_mut::<history_cell::ExecCell>()
                    {
                        exec_mut.replace_command_metadata(ev.command.clone(), ev.parsed_cmd.clone());
                    }
                    if promote_exec_cell_to_explore(chat, i) {
                        return true;
                    }
                    try_merge_completed_exec_at(chat, i);
                }

                chat.invalidate_height_cache();
                chat.request_redraw();
                return true;
            }
        }
    }
    false
}

fn hydrate_exec_record_from_begin(
    record: &mut ExecRecord,
    ev: &ExecCommandBeginEvent,
) -> bool {
    let mut changed = false;
    if record.command != ev.command {
        record.command = ev.command.clone();
        changed = true;
    }
    if record.parsed != ev.parsed_cmd {
        record.parsed = ev.parsed_cmd.clone();
        changed = true;
    }
    let new_action = history_cell::action_enum_from_parsed(&record.parsed);
    if record.action != new_action {
        record.action = new_action;
        changed = true;
    }
    if record.call_id.as_deref() != Some(ev.call_id.as_str()) {
        record.call_id = Some(ev.call_id.clone());
        changed = true;
    }
    if record.working_dir.is_none() {
        record.working_dir = Some(ev.cwd.clone());
        changed = true;
    }
    changed
}

fn apply_exec_begin_metadata_to_finished_call(
    chat: &mut ChatWidget<'_>,
    ev: &ExecCommandBeginEvent,
) -> bool {
    let history_id = match chat
        .history_state
        .history_id_for_exec_call(&ev.call_id)
    {
        Some(id) => id,
        None => return false,
    };
    let index = match chat.history_state.index_of(history_id) {
        Some(idx) => idx,
        None => return false,
    };
    let record = match chat.history_state.record(history_id).cloned() {
        Some(record) => record,
        None => return false,
    };
    match record {
        HistoryRecord::Exec(mut exec_record) => {
            if !hydrate_exec_record_from_begin(&mut exec_record, ev) {
                return false;
            }

            let mutation = chat
                .history_state
                .apply_domain_event(HistoryDomainEvent::Replace {
                    index,
                    record: HistoryDomainRecord::Exec(exec_record.clone()),
                });

            if let HistoryMutation::Replaced {
                id,
                record: HistoryRecord::Exec(updated_record),
                ..
            } = mutation
            {
                chat.update_cell_from_record(id, HistoryRecord::Exec(updated_record));
                if let Some(idx) = chat.cell_index_for_history_id(id) {
                    if promote_exec_cell_to_explore(chat, idx) {
                        return true;
                    }
                    crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
                }
                chat.invalidate_height_cache();
                chat.request_redraw();
                return true;
            }

            if let Some(idx) = chat.cell_index_for_history_id(history_id) {
                let cell = history_cell::ExecCell::from_record(exec_record.clone());
                chat.history_replace_with_record(
                    idx,
                    Box::new(cell),
                    HistoryDomainRecord::Exec(exec_record),
                );
                if promote_exec_cell_to_explore(chat, idx) {
                    return true;
                }
                return true;
            }

            false
        }
        HistoryRecord::MergedExec(mut merged_record) => {
            let mut segment_found = false;
            for segment in merged_record.segments.iter_mut() {
                let matches_call = segment
                    .call_id
                    .as_deref()
                    .map(|cid| cid == ev.call_id)
                    .unwrap_or(false);
                let fallback_matches = segment.call_id.is_none()
                    && segment.command.len() == 1
                    && segment
                        .command
                        .first()
                        .map(|cmd| cmd.contains(&ev.call_id))
                        .unwrap_or(false);
                if matches_call || fallback_matches {
                    if hydrate_exec_record_from_begin(segment, ev) {
                        segment_found = true;
                    }
                    break;
                }
            }

            if !segment_found {
                return false;
            }

            let new_action = history_cell::action_enum_from_parsed(&ev.parsed_cmd);
            if merged_record.action != new_action {
                merged_record.action = new_action;
            }

            let mutation = chat
                .history_state
                .apply_domain_event(HistoryDomainEvent::Replace {
                    index,
                    record: HistoryDomainRecord::MergedExec(merged_record.clone()),
                });

            if let HistoryMutation::Replaced {
                id,
                record: HistoryRecord::MergedExec(updated_record),
                ..
            } = mutation
            {
                chat.update_cell_from_record(id, HistoryRecord::MergedExec(updated_record));
                if let Some(idx) = chat.cell_index_for_history_id(id)
                    && promote_exec_cell_to_explore(chat, idx) {
                        return true;
                    }
                chat.invalidate_height_cache();
                chat.request_redraw();
                return true;
            }

            if let Some(idx) = chat.cell_index_for_history_id(history_id) {
                let cell = history_cell::MergedExecCell::from_state(merged_record.clone());
                chat.history_replace_with_record(
                    idx,
                    Box::new(cell),
                    HistoryDomainRecord::MergedExec(merged_record),
                );
                if promote_exec_cell_to_explore(chat, idx) {
                    return true;
                }
                return true;
            }

            false
        }
        _ => false,
    }
}

pub(in super::super::super) fn handle_exec_begin_now(
    chat: &mut ChatWidget<'_>,
    ev: ExecCommandBeginEvent,
    order: &OrderMeta,
) {
    if chat
        .ended_call_ids
        .contains(&super::ExecCallId(ev.call_id.clone()))
    {
        if try_upgrade_fallback_exec_cell(chat, &ev) {
            return;
        }
        if apply_exec_begin_metadata_to_finished_call(chat, &ev) {
            return;
        }
        return;
    }
    for cell in &chat.history_cells {
        cell.trigger_fade();
    }
    let parsed_command = ev.parsed_cmd.clone();
    let action = history_cell::action_enum_from_parsed(&parsed_command);
    chat.height_manager
        .borrow_mut()
        .record_event(HeightEvent::RunBegin);

    let has_read_command = parsed_command
        .iter()
        .any(|p| matches!(p, ParsedCommand::ReadCommand { .. }));
    let mut upgraded_tool_idx = if let Some(entry) = chat
        .tools_state
        .running_custom_tools
        .remove(&super::ToolCallId(ev.call_id.clone()))
    {
        running_tools::resolve_entry_index(chat, &entry, &ev.call_id)
            .or_else(|| running_tools::find_by_call_id(chat, &ev.call_id))
    } else {
        running_tools::find_by_call_id(chat, &ev.call_id)
    };

    if matches!(
        action,
        ExecAction::Read | ExecAction::Search | ExecAction::List
    ) || has_read_command
    {
        if let Some(idx) = upgraded_tool_idx.take() {
            chat.history_remove_at(idx);
        }
        let mut created_new = false;
        let mut agg_idx = chat.exec.running_explore_agg_index.and_then(|idx| {
            if idx < chat.history_cells.len()
                && chat.history_cells[idx]
                    .as_any()
                    .downcast_ref::<history_cell::ExploreAggregationCell>()
                    .is_some()
            {
                Some(idx)
            } else {
                None
            }
        });

        if agg_idx.is_none() {
            agg_idx = find_trailing_explore_agg(chat);
        }

        if agg_idx.is_none() {
            let key = chat.provider_order_key_from_order_meta(order);
            let record = ExploreRecord {
                id: HistoryId::ZERO,
                entries: Vec::new(),
            };
            let idx = chat.history_insert_with_key_global_tagged(
                Box::new(history_cell::ExploreAggregationCell::from_record(record.clone())),
                key,
                "explore",
                Some(HistoryDomainRecord::Explore(record)),
            );
            created_new = true;
            agg_idx = Some(idx);
        }

        if let Some(idx) = agg_idx {
            if let Some(mut record) = chat.history_cells.get(idx).and_then(|cell| {
                cell.as_any()
                    .downcast_ref::<history_cell::ExploreAggregationCell>()
                    .map(|existing| existing.record().clone())
            })
                && let Some(entry_idx) = history_cell::explore_record_push_from_parsed(
                    &mut record,
                    &parsed_command,
                    history_cell::ExploreEntryStatus::Running,
                    &ev.cwd,
                    &chat.config.cwd,
                    &ev.command,
                ) {
                    let cell = history_cell::ExploreAggregationCell::from_record(record.clone());
                    chat.history_replace_with_record(
                        idx,
                        Box::new(cell),
                        HistoryDomainRecord::Explore(record),
                    );
                    chat.autoscroll_if_near_bottom();
                    chat.exec.running_explore_agg_index = Some(idx);
                    chat.exec.running_commands.insert(
                        super::ExecCallId(ev.call_id.clone()),
                        super::RunningCommand {
                            command: ev.command.clone(),
                            parsed: parsed_command.clone(),
                            history_index: None,
                            history_id: None,
                            explore_entry: Some((idx, entry_idx)),
                            stdout_offset: 0,
                            stderr_offset: 0,
                            wait_total: None,
                            wait_active: false,
                            wait_notes: Vec::new(),
                        },
                    );
                    chat.bottom_pane.set_has_chat_history(true);
                    let status_text = match action {
                        ExecAction::Read => "reading files…",
                        _ => "exploring…",
                    };
                    chat.bottom_pane.update_status_text(status_text.to_string());
                    chat.refresh_auto_drive_visuals();
                    return;
                }

            if created_new {
                chat.history_remove_at(idx);
                chat.autoscroll_if_near_bottom();
                chat.request_redraw();
            }
        }
    }

    let exec_record = exec_record_from_begin(&ev);
    let key = chat.provider_order_key_from_order_meta(order);
    let history_idx = if let Some(idx) = upgraded_tool_idx {
        let replacement = history_cell::ExecCell::from_record(exec_record.clone());
        chat.history_replace_with_record(
            idx,
            Box::new(replacement),
            HistoryDomainRecord::Exec(exec_record),
        );
        if idx < chat.cell_order_seq.len() {
            chat.cell_order_seq[idx] = key;
        }
        if idx < chat.cell_order_dbg.len() {
            chat.cell_order_dbg[idx] = None;
        }
        chat.last_assigned_order = Some(
            chat
                .last_assigned_order
                .map(|prev| prev.max(key))
                .unwrap_or(key),
        );
        chat.bottom_pane.set_has_chat_history(true);
        idx
    } else {
        let cell = history_cell::ExecCell::from_record(exec_record.clone());
        chat.history_insert_with_key_global_tagged(
            Box::new(cell),
            key,
            "exec-begin",
            Some(HistoryDomainRecord::Exec(exec_record)),
        )
    };
    chat.exec.running_commands.insert(
        super::ExecCallId(ev.call_id.clone()),
        super::RunningCommand {
            command: ev.command.clone(),
            parsed: parsed_command,
            history_index: Some(history_idx),
            history_id: None,
            explore_entry: None,
            stdout_offset: 0,
            stderr_offset: 0,
            wait_total: None,
            wait_active: false,
            wait_notes: Vec::new(),
        },
    );
    if let Some(running) = chat
        .exec
        .running_commands
        .get_mut(&super::ExecCallId(ev.call_id.clone()))
    {
        let history_id = chat
            .history_state
            .history_id_for_exec_call(&ev.call_id)
            .or_else(|| chat.history_cell_ids.get(history_idx).and_then(|slot| *slot));
        running.history_id = history_id;
    }
    if !chat.tools_state.web_search_sessions.is_empty() {
        chat.bottom_pane.update_status_text("Search".to_string());
    } else {
        let preview = chat
            .exec
            .running_commands
            .get(&super::ExecCallId(ev.call_id.clone()))
            .map(|rc| rc.command.join(" "))
            .unwrap_or_else(|| "command".to_string());
        let preview_short = if preview.chars().count() > 40 {
            let mut truncated: String = preview.chars().take(40).collect();
            truncated.push('…');
            truncated
        } else {
            preview
        };
        chat.bottom_pane
            .update_status_text(format!("running command: {preview_short}"));
    }
    chat.refresh_auto_drive_visuals();
}
