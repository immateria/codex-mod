use super::*;

pub(super) type RunningExecEntry = (ExecCallId, Option<usize>, Option<(usize, usize)>);

pub(super) fn find_trailing_explore_agg(chat: &ChatWidget<'_>) -> Option<usize> {
    if chat.is_reasoning_shown() {
        return None;
    }
    let mut idx = chat.history_cells.len();
    while idx > 0 {
        idx -= 1;
        let cell = &chat.history_cells[idx];
        if cell
            .as_any()
            .downcast_ref::<history_cell::CollapsibleReasoningCell>()
            .is_some()
        {
            continue;
        }
        if cell
            .as_any()
            .downcast_ref::<history_cell::ExploreAggregationCell>()
            .is_some()
        {
            return Some(idx);
        }
        break;
    }
    None
}

pub(super) fn stream_chunks_to_text(chunks: &[ExecStreamChunk]) -> String {
    if chunks.is_empty() {
        return String::new();
    }
    let mut ordered: Vec<&ExecStreamChunk> = chunks.iter().collect();
    ordered.sort_by_key(|chunk| chunk.offset);
    let mut combined = String::new();
    for chunk in ordered {
        combined.push_str(&chunk.content);
    }
    combined
}

pub(super) fn stream_chunks_tail_to_text(chunks: &[ExecStreamChunk], max_bytes: usize) -> String {
    if chunks.is_empty() || max_bytes == 0 {
        return String::new();
    }

    let mut ordered: Vec<&ExecStreamChunk> = chunks.iter().collect();
    ordered.sort_by_key(|chunk| chunk.offset);

    let mut pieces: Vec<String> = Vec::new();
    let mut total_bytes: usize = 0;
    for chunk in ordered.into_iter().rev() {
        if total_bytes >= max_bytes {
            break;
        }
        let remaining = max_bytes - total_bytes;
        let content = chunk.content.as_str();
        if content.len() <= remaining {
            pieces.push(content.to_string());
            total_bytes = total_bytes.saturating_add(content.len());
            continue;
        }

        let mut start = content.len().saturating_sub(remaining);
        while start < content.len() && !content.is_char_boundary(start) {
            start = start.saturating_add(1);
        }
        pieces.push(content[start..].to_string());
        break;
    }

    pieces.reverse();
    pieces.concat()
}

pub(super) fn explore_status_from_exec(action: ExecAction, record: &ExecRecord) -> history_cell::ExploreEntryStatus {
    match record.status {
        ExecStatus::Running => history_cell::ExploreEntryStatus::Running,
        ExecStatus::Success => history_cell::ExploreEntryStatus::Success,
        ExecStatus::Error => match (record.exit_code, action) {
            (Some(1), ExecAction::Search | ExecAction::List) => history_cell::ExploreEntryStatus::NotFound,
            _ => history_cell::ExploreEntryStatus::Error {
                exit_code: record.exit_code,
            },
        },
    }
}

pub(super) fn promote_exec_cell_to_explore(chat: &mut ChatWidget<'_>, idx: usize) -> bool {
    if idx >= chat.history_cells.len() {
        return false;
    }

    let (segments, action) = match history_record_for_cell(chat, idx) {
        Some(HistoryRecord::Exec(exec_record)) => {
            let action = exec_record.action;
            (vec![exec_record], action)
        }
        Some(HistoryRecord::MergedExec(merged)) => {
            let action = merged.action;
            (merged.segments, action)
        }
        _ => return false,
    };

    if !matches!(
        action,
        ExecAction::Read | ExecAction::Search | ExecAction::List
    ) {
        return false;
    }

    let session_root = chat.config.cwd.clone();

    let mut target_idx = chat.exec.running_explore_agg_index.and_then(|candidate| {
        if candidate < chat.history_cells.len()
            && chat.history_cells[candidate]
                .as_any()
                .downcast_ref::<history_cell::ExploreAggregationCell>()
                .is_some()
        {
            Some(candidate)
        } else {
            None
        }
    });

    if target_idx.is_none() {
        target_idx = find_trailing_explore_agg(chat);
    }

    let push_segments = |record: &mut ExploreRecord| -> bool {
        let mut added_any = false;
        for segment in &segments {
            if segment.parsed.is_empty() {
                continue;
            }
            let status = explore_status_from_exec(segment.action, segment);
            let cwd_buf: PathBuf = segment
                .working_dir
                .clone()
                .unwrap_or_else(|| chat.config.cwd.clone());
            if history_cell::explore_record_push_from_parsed(
                record,
                &segment.parsed,
                status,
                cwd_buf.as_path(),
                session_root.as_path(),
                &segment.command,
            )
            .is_some()
            {
                added_any = true;
            }
        }
        added_any
    };

    if let Some(agg_idx) = target_idx {
        let Some(mut record) = chat.history_cells.get(agg_idx).and_then(|cell| {
            cell.as_any()
                .downcast_ref::<history_cell::ExploreAggregationCell>()
                .map(|existing| existing.record().clone())
        }) else {
            return false;
        };

        if !push_segments(&mut record) {
            return false;
        }

        let replacement = history_cell::ExploreAggregationCell::from_record(record.clone());
        chat.history_replace_with_record(
            agg_idx,
            Box::new(replacement),
            HistoryDomainRecord::Explore(record),
        );

        if agg_idx != idx {
            chat.history_remove_at(idx);
        }

        chat.bottom_pane.set_has_chat_history(true);
        chat.autoscroll_if_near_bottom();
        chat.exec.running_explore_agg_index = None;
        return true;
    }

    let mut record = ExploreRecord {
        id: HistoryId::ZERO,
        entries: Vec::new(),
    };

    if !push_segments(&mut record) {
        return false;
    }

    let cell = history_cell::ExploreAggregationCell::from_record(record.clone());
    chat.history_replace_with_record(
        idx,
        Box::new(cell),
        HistoryDomainRecord::Explore(record),
    );
    chat.bottom_pane.set_has_chat_history(true);
    chat.autoscroll_if_near_bottom();
    chat.exec.running_explore_agg_index = None;
    true
}

pub(super) fn update_explore_entry_status(
    chat: &mut ChatWidget<'_>,
    preferred_index: Option<usize>,
    entry_idx: usize,
    status: history_cell::ExploreEntryStatus,
) -> Option<usize> {
    fn try_update_at(
        chat: &mut ChatWidget<'_>,
        idx: usize,
        entry_idx: usize,
        status: &history_cell::ExploreEntryStatus,
    ) -> Option<usize> {
        if idx >= chat.history_cells.len() {
            return None;
        }
        let cell = chat.history_cells[idx]
            .as_any()
            .downcast_ref::<history_cell::ExploreAggregationCell>()?;
        if entry_idx >= cell.record().entries.len() {
            return None;
        }
        let mut record = cell.record().clone();
        history_cell::explore_record_update_status(&mut record, entry_idx, status.clone());
        let replacement = history_cell::ExploreAggregationCell::from_record(record.clone());
        chat.history_replace_with_record(
            idx,
            Box::new(replacement),
            HistoryDomainRecord::Explore(record),
        );
        chat.autoscroll_if_near_bottom();
        Some(idx)
    }

    if let Some(idx) = preferred_index.and_then(|i| try_update_at(chat, i, entry_idx, &status)) {
        return Some(idx);
    }
    if let Some(idx) = chat
        .exec
        .running_explore_agg_index
        .and_then(|i| try_update_at(chat, i, entry_idx, &status))
    {
        return Some(idx);
    }
    for i in (0..chat.history_cells.len()).rev() {
        let Some(cell) = chat.history_cells[i]
            .as_any()
            .downcast_ref::<history_cell::ExploreAggregationCell>()
        else {
            continue;
        };
        if entry_idx >= cell.record().entries.len() {
            continue;
        }
        if !matches!(
            cell.record().entries[entry_idx].status,
            history_cell::ExploreEntryStatus::Running
                | history_cell::ExploreEntryStatus::Error { .. }
                | history_cell::ExploreEntryStatus::NotFound
        ) {
            continue;
        }
        if let Some(idx) = try_update_at(chat, i, entry_idx, &status) {
            return Some(idx);
        }
    }
    None
}

pub(super) fn exec_record_from_begin(ev: &ExecCommandBeginEvent) -> ExecRecord {
    let action = history_cell::action_enum_from_parsed(&ev.parsed_cmd);
    ExecRecord {
        id: crate::history::state::HistoryId::ZERO,
        call_id: Some(ev.call_id.clone()),
        command: ev.command.clone(),
        parsed: ev.parsed_cmd.clone(),
        action,
        status: ExecStatus::Running,
        stdout_chunks: Vec::new(),
        stderr_chunks: Vec::new(),
        exit_code: None,
        wait_total: None,
        wait_active: false,
        wait_notes: Vec::new(),
        started_at: std::time::SystemTime::now(),
        completed_at: None,
        working_dir: Some(ev.cwd.clone()),
        env: Vec::new(),
        tags: Vec::new(),
    }
}

pub(super) fn exec_wait_notes_from_pairs(pairs: &[(String, bool)]) -> Vec<ExecWaitNote> {
    pairs
        .iter()
        .map(|(message, is_error)| ExecWaitNote {
            message: message.clone(),
            tone: if *is_error {
                TextTone::Error
            } else {
                TextTone::Info
            },
            timestamp: SystemTime::now(),
        })
        .collect()
}

pub(super) fn stream_tail(full: &str, streamed: &str) -> Option<String> {
    if full.is_empty() {
        return None;
    }
    if streamed.is_empty() {
        return Some(full.to_string());
    }
    if let Some(tail) = full.strip_prefix(streamed) {
        if tail.is_empty() {
            None
        } else {
            Some(tail.to_string())
        }
    } else {
        Some(full.to_string())
    }
}

pub(super) fn history_record_for_cell(chat: &ChatWidget<'_>, idx: usize) -> Option<HistoryRecord> {
    if let Some(Some(id)) = chat.history_cell_ids.get(idx)
        && let Some(record) = chat.history_state.record(*id).cloned() {
            return Some(record);
        }
    chat.history_cells
        .get(idx)
        .and_then(|cell| history_cell::record_from_cell(cell.as_ref()))
}

pub(super) fn exec_record_has_output(record: &ExecRecord) -> bool {
    !record.stdout_chunks.is_empty() || !record.stderr_chunks.is_empty()
}
