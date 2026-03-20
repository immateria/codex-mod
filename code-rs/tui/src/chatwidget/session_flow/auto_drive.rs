impl ChatWidget<'_> {
    pub(super) fn auto_drive_lines_to_string(lines: Vec<Line<'static>>) -> String {
        let mut rows: Vec<String> = Vec::new();
        for line in lines {
            let mut row = String::new();
            for span in line.spans {
                row.push_str(span.content.as_ref());
            }
            rows.push(row);
        }
        while rows
            .last()
            .map(|line| line.trim().is_empty())
            .unwrap_or(false)
        {
            rows.pop();
        }
        rows.join("\n")
    }

    pub(super) fn auto_drive_role_for_kind(kind: HistoryCellType) -> Option<AutoDriveRole> {
        use AutoDriveRole::{Assistant, User};
        match kind {
            HistoryCellType::User => Some(Assistant),
            HistoryCellType::ProposedPlan => None,
            HistoryCellType::Assistant
            | HistoryCellType::Reasoning
            | HistoryCellType::Error
            | HistoryCellType::Exec { .. }
            | HistoryCellType::Patch { .. }
            | HistoryCellType::PlanUpdate
            | HistoryCellType::BackgroundEvent
            | HistoryCellType::Notice
            | HistoryCellType::CompactionSummary
            | HistoryCellType::Diff
            | HistoryCellType::Plain
            | HistoryCellType::Image => Some(User),
            HistoryCellType::Context => None,
            HistoryCellType::Tool { status } => match status {
                crate::history_cell::ToolCellStatus::Running => None,
                crate::history_cell::ToolCellStatus::Success
                | crate::history_cell::ToolCellStatus::Failed => Some(User),
            },
            HistoryCellType::AnimatedWelcome | HistoryCellType::Loading => None,
            HistoryCellType::JsRepl { .. } => Some(User),
        }
    }

    pub(super) fn auto_drive_cell_text_for_index(&self, idx: usize, cell: &dyn HistoryCell) -> Option<String> {
        let lines = self.cell_lines_for_index(idx, cell);
        let text = Self::auto_drive_lines_to_string(lines);
        if text.trim().is_empty() {
            None
        } else {
            Some(text)
        }
    }

    pub(super) fn auto_drive_make_user_message(
        text: String,
    ) -> Option<code_protocol::models::ResponseItem> {
        if text.trim().is_empty() {
            return None;
        }
        use code_protocol::models::{ContentItem, ResponseItem};
        Some(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text }],
            end_turn: None,
            phase: None,
        })
    }

    pub(super) fn auto_drive_browser_screenshot_items(
        cell: &BrowserSessionCell,
    ) -> Option<Vec<code_protocol::models::ContentItem>> {
        use code_protocol::models::ContentItem;

        let record = cell.screenshot_history().last()?;
        let bytes = match std::fs::read(&record.path) {
            Ok(bytes) if !bytes.is_empty() => bytes,
            Ok(_) => return None,
            Err(err) => {
                tracing::warn!(
                    "Failed to read browser screenshot for Auto Drive export: {} ({err})",
                    record.path.display()
                );
                return None;
            }
        };

        let mime = record
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                let ext_lower = ext.to_ascii_lowercase();
                match ext_lower.as_str() {
                    "png" => "image/png",
                    "jpg" | "jpeg" => "image/jpeg",
                    "gif" => "image/gif",
                    "bmp" => "image/bmp",
                    "webp" => "image/webp",
                    "svg" => "image/svg+xml",
                    "ico" => "image/x-icon",
                    "tif" | "tiff" => "image/tiff",
                    _ => "application/octet-stream",
                }
            })
            .unwrap_or("application/octet-stream")
            .to_string();
        let encoded = BASE64_STANDARD.encode(bytes);

        let timestamp_ms = record.timestamp.as_millis();
        let raw_url = record.url.as_deref().unwrap_or("browser");
        let sanitized = raw_url.replace(['\n', '\r'], " ");
        let trimmed = sanitized.trim();
        let mut url_meta = if trimmed.is_empty() {
            "browser".to_string()
        } else {
            trimmed.to_string()
        };
        if url_meta.len() > 240 {
            let mut truncated: String = url_meta.chars().take(240).collect();
            truncated.push_str("...");
            url_meta = truncated;
        }

        let metadata = format!("browser-screenshot:{timestamp_ms}:{url_meta}");

        let mut items = Vec::with_capacity(2);
        items.push(ContentItem::InputText {
            text: format!("[EPHEMERAL:{metadata}]"),
        });
        items.push(ContentItem::InputImage {
            image_url: format!("data:{mime};base64,{encoded}"),
        });

        Some(items)
    }

    pub(super) fn auto_drive_make_assistant_message(
        text: String,
    ) -> Option<code_protocol::models::ResponseItem> {
        if text.trim().is_empty() {
            return None;
        }
        use code_protocol::models::{ContentItem, ResponseItem};
        Some(ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText { text }],
            end_turn: None,
            phase: None,
        })
    }

    pub(super) fn reset_auto_compaction_overlay(&mut self) {
        self.auto_compaction_overlay = None;
    }

    pub(super) fn auto_drive_normalize_diff_path(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed == "/dev/null" {
            return None;
        }
        let normalized = trimmed
            .strip_prefix("a/")
            .or_else(|| trimmed.strip_prefix("b/"))
            .unwrap_or(trimmed);
        Some(normalized.to_string())
    }

    pub(super) fn auto_drive_diff_summary(record: &DiffRecord) -> Option<String> {
        use std::collections::BTreeMap;

        let mut stats: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        let mut current_file: Option<String> = None;

        for hunk in &record.hunks {
            for line in &hunk.lines {
                match line.kind {
                    DiffLineKind::Context => {
                        let content = line.content.trim();
                        if let Some(rest) = content.strip_prefix("diff --git ") {
                            let mut parts = rest.split_whitespace();
                            let _old = parts.next();
                            if let Some(new_path) = parts.next()
                                && let Some(path) = Self::auto_drive_normalize_diff_path(new_path) {
                                    stats.entry(path.clone()).or_insert((0, 0));
                                    current_file = Some(path);
                                }
                        } else {
                            if let Some(rest) = content.strip_prefix("--- ")
                                && let Some(path) = Self::auto_drive_normalize_diff_path(rest) {
                                    stats.entry(path.clone()).or_insert((0, 0));
                                    current_file = Some(path);
                                }
                            if let Some(rest) = content.strip_prefix("+++ ")
                                && let Some(path) = Self::auto_drive_normalize_diff_path(rest) {
                                    stats.entry(path.clone()).or_insert((0, 0));
                                    current_file = Some(path);
                                }
                        }
                    }
                    DiffLineKind::Addition => {
                        if let Some(file) = current_file.as_ref() {
                            let entry = stats.entry(file.clone()).or_insert((0, 0));
                            entry.0 += 1;
                        }
                    }
                    DiffLineKind::Removal => {
                        if let Some(file) = current_file.as_ref() {
                            let entry = stats.entry(file.clone()).or_insert((0, 0));
                            entry.1 += 1;
                        }
                    }
                }
            }
        }

        if stats.is_empty() {
            return None;
        }

        let mut lines = Vec::with_capacity(stats.len() + 1);
        lines.push("Files changed".to_string());
        for (path, (added, removed)) in stats {
            lines.push(format!("- {path} (+{added} / -{removed})"));
        }
        Some(lines.join("\n"))
    }

    pub(crate) fn export_auto_drive_items(&self) -> Vec<code_protocol::models::ResponseItem> {
        let (items, _) = self.export_auto_drive_items_with_indices();
        items
    }

    pub(super) fn export_auto_drive_items_with_indices(
        &self,
    ) -> (
        Vec<code_protocol::models::ResponseItem>,
        Vec<Option<usize>>,
    ) {
        if let Some(overlay) = &self.auto_compaction_overlay {
            let mut items = overlay.prefix_items.clone();
            let mut indices = vec![None; overlay.prefix_items.len()];
            let tail = self.export_auto_drive_items_from_index_with_indices(overlay.tail_start_cell);
            for (cell_idx, item) in tail {
                indices.push(Some(cell_idx));
                items.push(item);
            }
            (items, indices)
        } else {
            let tail = self.export_auto_drive_items_from_index_with_indices(0);
            let mut items = Vec::with_capacity(tail.len());
            let mut indices = Vec::with_capacity(tail.len());
            for (cell_idx, item) in tail {
                indices.push(Some(cell_idx));
                items.push(item);
            }
            (items, indices)
        }
    }

    pub(super) fn export_auto_drive_items_from_index_with_indices(
        &self,
        start_idx: usize,
    ) -> Vec<(usize, code_protocol::models::ResponseItem)> {
        let mut items = Vec::new();
        for (idx, cell) in self.history_cells.iter().enumerate().skip(start_idx) {
            let Some(role) = Self::auto_drive_role_for_kind(cell.kind()) else {
                continue;
            };

            let text = match cell.kind() {
                HistoryCellType::Reasoning => self
                    .auto_drive_cell_text_for_index(idx, cell.as_ref())
                    .map(|text| (text, true)),
                HistoryCellType::PlanUpdate => {
                    if let Some(plan) = cell.as_any().downcast_ref::<PlanUpdateCell>() {
                        let state = plan.state();
                        let mut lines: Vec<String> = Vec::new();
                        if !state.name.trim().is_empty() {
                            lines.push(format!("Plan update: {}", state.name.trim()));
                        } else {
                            lines.push("Plan update".to_string());
                        }
                        if state.progress.total > 0 {
                            lines.push(format!(
                                "Progress: {}/{}",
                                state.progress.completed, state.progress.total
                            ));
                        }
                        if state.steps.is_empty() {
                            lines.push("(no steps recorded)".to_string());
                        } else {
                            for step in &state.steps {
                                let status_label = match step.status {
                                    StepStatus::Completed => "[completed]",
                                    StepStatus::InProgress => "[in_progress]",
                                    StepStatus::Pending => "[pending]",
                                };
                                lines.push(format!("{} {}", status_label, step.description));
                            }
                        }
                        let text = lines.join("\n");
                        Some((text, false))
                    } else {
                        self.auto_drive_cell_text_for_index(idx, cell.as_ref())
                            .map(|text| (text, false))
                    }
                }
                HistoryCellType::Diff => {
                    if let Some(diff_cell) = cell.as_any().downcast_ref::<DiffCell>() {
                        Self::auto_drive_diff_summary(diff_cell.record()).map(|text| (text, false))
                    } else {
                        self.auto_drive_cell_text_for_index(idx, cell.as_ref())
                            .map(|text| (text, false))
                    }
                }
                _ => self
                    .auto_drive_cell_text_for_index(idx, cell.as_ref())
                    .map(|text| (text, false)),
            };

            let Some((text, is_reasoning)) = text else {
                continue;
            };

            let mut extra_content = None;
            if !is_reasoning && matches!(role, AutoDriveRole::User)
                && let Some(browser_cell) = cell
                    .as_ref()
                    .as_any()
                    .downcast_ref::<BrowserSessionCell>()
                {
                    extra_content = Self::auto_drive_browser_screenshot_items(browser_cell);
                }

            let mut item = if is_reasoning {
                code_protocol::models::ResponseItem::Message {
                    id: Some("auto-drive-reasoning".to_string()),
                    role: "user".to_string(),
                    content: vec![code_protocol::models::ContentItem::InputText { text }],
                    end_turn: None,
                    phase: None,
                }
            } else {
                match role {
                    AutoDriveRole::Assistant => match Self::auto_drive_make_assistant_message(text) {
                        Some(item) => item,
                        None => continue,
                    },
                    AutoDriveRole::User => match Self::auto_drive_make_user_message(text) {
                        Some(item) => item,
                        None => continue,
                    },
                }
            };

            if let Some(extra) = extra_content
                && let code_protocol::models::ResponseItem::Message { content, .. } = &mut item {
                    content.extend(extra);
                }

            items.push((idx, item));
        }
        items
    }

    pub(super) fn derive_compaction_overlay(
        &self,
        previous_items: &[code_protocol::models::ResponseItem],
        previous_indices: &[Option<usize>],
        new_items: &[code_protocol::models::ResponseItem],
    ) -> Option<AutoCompactionOverlay> {
        if previous_items == new_items {
            return self.auto_compaction_overlay.clone();
        }

        if new_items.is_empty() {
            return Some(AutoCompactionOverlay {
                prefix_items: Vec::new(),
                tail_start_cell: self.history_cells.len(),
            });
        }

        let max_prefix = previous_items.len().min(new_items.len());
        let mut prefix_len = 0;
        while prefix_len < max_prefix && previous_items[prefix_len] == new_items[prefix_len] {
            prefix_len += 1;
        }

        let remaining_prev = previous_items.len().saturating_sub(prefix_len);
        let remaining_new = new_items.len().saturating_sub(prefix_len);
        let mut suffix_len = 0;
        while suffix_len < remaining_prev && suffix_len < remaining_new {
            let prev_idx = previous_items.len() - 1 - suffix_len;
            let new_idx = new_items.len() - 1 - suffix_len;
            if previous_items[prev_idx] != new_items[new_idx] {
                break;
            }
            suffix_len += 1;
        }

        let mut prefix_items_end = new_items.len().saturating_sub(suffix_len);

        let tail_start_cell = if suffix_len == 0 {
            self.history_cells.len()
        } else {
            let start = previous_items.len() - suffix_len;
            previous_indices[start..]
                .iter()
                .find_map(|idx| *idx)
                .unwrap_or(self.history_cells.len())
        };

        if suffix_len > 0 && tail_start_cell == self.history_cells.len() {
            // Suffix items no longer map to on-screen cells (e.g., after repeated compactions),
            // so treat the whole conversation as the overlay prefix.
            prefix_items_end = new_items.len();
        }

        let prefix_items = new_items[..prefix_items_end].to_vec();

        Some(AutoCompactionOverlay {
            prefix_items,
            tail_start_cell,
        })
    }

    pub(super) fn rebuild_auto_history(&mut self) -> Vec<code_protocol::models::ResponseItem> {
        let conversation = self.export_auto_drive_items();
        let tail = self
            .auto_history
            .replace_converted(conversation);
        if !tail.is_empty() {
            self.auto_history.append_converted_tail(&tail);
        }
        self.auto_history.raw_snapshot()
    }

    pub(super) fn current_auto_history(&mut self) -> Vec<code_protocol::models::ResponseItem> {
        if self.auto_history.converted_is_empty() {
            return self.rebuild_auto_history();
        }
        self.auto_history.raw_snapshot()
    }

    /// Export current user/assistant messages into ResponseItem list for forking.
    pub(crate) fn export_response_items(&self) -> Vec<code_protocol::models::ResponseItem> {
        use code_protocol::models::ContentItem;
        use code_protocol::models::ResponseItem;
        let mut items = Vec::new();
        for (idx, cell) in self.history_cells.iter().enumerate() {
            match cell.kind() {
                crate::history_cell::HistoryCellType::User => {
                    let text = self
                        .cell_lines_for_index(idx, cell.as_ref())
                        .iter()
                        .map(|l| {
                            l.spans
                                .iter()
                                .map(|s| s.content.to_string())
                                .collect::<String>()
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let prefixed = format!("Coordinator: {text}");
                    let content = ContentItem::InputText { text: prefixed };
                    items.push(ResponseItem::Message {
                        id: None,
                        role: "user".to_string(),
                        content: vec![content],
                        end_turn: None,
                        phase: None,
                    });
                }
                crate::history_cell::HistoryCellType::Assistant => {
                    let text = self
                        .cell_lines_for_index(idx, cell.as_ref())
                        .iter()
                        .map(|l| {
                            l.spans
                                .iter()
                                .map(|s| s.content.to_string())
                                .collect::<String>()
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let prefixed = format!("CLI: {text}");
                    let content = ContentItem::OutputText { text: prefixed };
                    items.push(ResponseItem::Message {
                        id: None,
                        role: "assistant".to_string(),
                        content: vec![content],
                        end_turn: None,
                        phase: None,
                    });
                }
                crate::history_cell::HistoryCellType::PlanUpdate => {
                    if let Some(plan) = cell
                        .as_any()
                        .downcast_ref::<crate::history_cell::PlanUpdateCell>()
                    {
                        let state = plan.state();
                        let mut lines: Vec<String> = Vec::new();
                        if !state.name.trim().is_empty() {
                            lines.push(format!("Plan update: {}", state.name.trim()));
                        } else {
                            lines.push("Plan update".to_string());
                        }

                        if state.progress.total > 0 {
                            lines.push(format!(
                                "Progress: {}/{}",
                                state.progress.completed, state.progress.total
                            ));
                        }

                        if state.steps.is_empty() {
                            lines.push("(no steps recorded)".to_string());
                        } else {
                            for step in &state.steps {
                                let status_label = match step.status {
                                    StepStatus::Completed => "[completed]",
                                    StepStatus::InProgress => "[in_progress]",
                                    StepStatus::Pending => "[pending]",
                                };
                                lines.push(format!("- {} {}", status_label, step.description));
                            }
                        }

                        let text = lines.join("\n");
                        let content = ContentItem::OutputText { text };
                        items.push(ResponseItem::Message {
                            id: None,
                            role: "assistant".to_string(),
                            content: vec![content],
                            end_turn: None,
                            phase: None,
                        });
                    }
                }
                _ => {}
            }
        }
        items
    }
}
