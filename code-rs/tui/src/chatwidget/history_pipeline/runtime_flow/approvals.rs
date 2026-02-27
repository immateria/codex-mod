use super::*;

impl ChatWidget<'_> {
    /// Clear memoized cell heights (called when history/content changes)
    /// Handle exec approval request immediately
    pub(in super::super::super) fn handle_exec_approval_now(&mut self, _id: String, ev: ExecApprovalRequestEvent) {
        // Use approval_id when present, otherwise fall back to call_id.
        let approval_id = ev.effective_approval_id();
        let ticket = self.make_background_before_next_output_ticket();
        if let Some(ctx) = ev.network_approval_context {
            self.bottom_pane.push_approval_request(
                ApprovalRequest::Network {
                    id: approval_id,
                    command: ev.command,
                    reason: ev.reason,
                    host: ctx.host,
                    protocol: ctx.protocol,
                },
                ticket,
            );
        } else {
            self.bottom_pane.push_approval_request(
                ApprovalRequest::Exec {
                    id: approval_id,
                    command: ev.command,
                    reason: ev.reason,
                },
                ticket,
            );
        }
    }

    /// Handle apply patch approval request immediately
    pub(in super::super::super) fn handle_apply_patch_approval_now(&mut self, _id: String, ev: ApplyPatchApprovalRequestEvent) {
        let ApplyPatchApprovalRequestEvent {
            call_id,
            changes,
            reason,
            grant_root,
        } = ev;

        // Clone for session storage before moving into history
        let changes_clone = changes.clone();
        // Surface the patch summary in the main conversation
        let key = self.next_internal_key();
        let _ = self.history_insert_with_key_global(
            Box::new(history_cell::new_patch_event(
                history_cell::PatchEventType::ApprovalRequest,
                changes,
            )),
            key,
        );
        // Record change set for session diff popup (latest last)
        self.diffs.session_patch_sets.push(changes_clone);
        // For any new paths, capture an original baseline snapshot the first time we see them
        if let Some(last) = self.diffs.session_patch_sets.last() {
            for (src_path, chg) in last.iter() {
                match chg {
                    code_core::protocol::FileChange::Update {
                        move_path: Some(dest_path),
                        ..
                    } => {
                        if let Some(baseline) =
                            self.diffs.baseline_file_contents.get(src_path).cloned()
                        {
                            // Mirror baseline under destination so tabs use the new path
                            self.diffs
                                .baseline_file_contents
                                .entry(dest_path.clone())
                                .or_insert(baseline);
                        } else if !self.diffs.baseline_file_contents.contains_key(dest_path) {
                            // Snapshot from source (pre-apply)
                            let baseline = std::fs::read_to_string(src_path).unwrap_or_default();
                            self.diffs
                                .baseline_file_contents
                                .insert(dest_path.clone(), baseline);
                        }
                    }
                    _ => {
                        if !self.diffs.baseline_file_contents.contains_key(src_path) {
                            let baseline = std::fs::read_to_string(src_path).unwrap_or_default();
                            self.diffs
                                .baseline_file_contents
                                .insert(src_path.clone(), baseline);
                        }
                    }
                }
            }
        }
        // Enable Ctrl+D footer hint now that we have diffs to show
        self.bottom_pane.set_diffs_hint(true);

        // Push the approval request to the bottom pane, keyed by call_id
        let request = ApprovalRequest::ApplyPatch {
            id: call_id,
            reason,
            grant_root,
        };
        let ticket = self.make_background_before_next_output_ticket();
        self.bottom_pane.push_approval_request(request, ticket);
    }

    pub(in super::super::super) fn build_patch_failure_metadata(stdout: &str, stderr: &str) -> PatchFailureMetadata {
        fn sanitize(text: &str) -> String {
            let normalized = history_cell::normalize_overwrite_sequences(text);
            sanitize_for_tui(
                &normalized,
                SanitizeMode::AnsiPreserving,
                SanitizeOptions {
                    expand_tabs: true,
                    tabstop: 4,
                    debug_markers: false,
                },
            )
        }

        fn excerpt(input: &str) -> Option<String> {
            let trimmed = input.trim();
            if trimmed.is_empty() {
                return None;
            }
            const MAX_CHARS: usize = 2_000;
            const MAX_LINES: usize = 20;
            let mut excerpt = String::new();
            let mut remaining = MAX_CHARS;
            for (idx, line) in trimmed.lines().enumerate() {
                if idx >= MAX_LINES || remaining == 0 {
                    break;
                }
                let line = line.trim_end_matches('\r');
                let mut line_chars = line.chars();
                let mut chunk = String::new();
                while remaining > 0 {
                    if let Some(ch) = line_chars.next() {
                        let width = ch.len_utf8();
                        if width > remaining {
                            break;
                        }
                        chunk.push(ch);
                        remaining -= width;
                    } else {
                        break;
                    }
                }
                if chunk.len() < line.len() {
                    chunk.push('…');
                    remaining = 0;
                }
                if !excerpt.is_empty() {
                    excerpt.push('\n');
                }
                excerpt.push_str(&chunk);
                if remaining == 0 {
                    break;
                }
            }
            Some(excerpt)
        }

        let sanitized_stdout = sanitize(stdout);
        let sanitized_stderr = sanitize(stderr);
        let message = sanitized_stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| "Patch application failed".to_string());

        PatchFailureMetadata {
            message,
            stdout_excerpt: excerpt(&sanitized_stdout),
            stderr_excerpt: excerpt(&sanitized_stderr),
        }
    }
}
