impl ChatWidget<'_> {
    pub(crate) fn config_ref(&self) -> &Config {
        &self.config
    }

    /// Check if there are any animations and trigger redraw if needed
    pub fn check_for_initial_animations(&mut self) {
        if self
            .history_cells
            .iter()
            .any(crate::history_cell::HistoryCell::is_animating)
        {
            if Self::auto_reduced_motion_preference() {
                return;
            }
            tracing::info!("Initial animation detected, scheduling frame");
            // Schedule initial frame for animations to ensure they start properly.
            // Use ScheduleFrameIn to avoid debounce issues with immediate RequestRedraw.
            self.app_event_tx
                .send(AppEvent::ScheduleFrameIn(HISTORY_ANIMATION_FRAME_INTERVAL));
        }
    }

    /// Format model name with proper capitalization (e.g., "gpt-4" -> "GPT-4")
    pub(super) fn format_model_name(&self, model_name: &str) -> String {
        fn format_segment(segment: &str) -> String {
            if segment.eq_ignore_ascii_case("codex") {
                return "Codex".to_string();
            }

            let mut chars = segment.chars();
            match chars.next() {
                Some(first) if first.is_ascii_alphabetic() => {
                    let mut formatted = String::new();
                    formatted.push(first.to_ascii_uppercase());
                    formatted.push_str(chars.as_str());
                    formatted
                }
                Some(first) => {
                    let mut formatted = String::new();
                    formatted.push(first);
                    formatted.push_str(chars.as_str());
                    formatted
                }
                None => String::new(),
            }
        }

        if let Some(rest) = model_name.strip_prefix("gpt-") {
            let formatted_rest = rest
                .split('-')
                .map(format_segment)
                .collect::<Vec<_>>()
                .join("-");
            format!("GPT-{formatted_rest}")
        } else {
            model_name.to_string()
        }
    }

    pub(super) fn try_append_prefix_fast(
        &self,
        render_requests: &[RenderRequest<'_>],
        render_settings: RenderSettings,
        prefix_width: u16,
    ) -> bool {
        if !self.history_prefix_append_only.get() {
            return false;
        }
        if !self
            .history_render
            .can_append_prefix(prefix_width, render_requests.len())
        {
            return false;
        }
        let prev_count = self.history_render.last_prefix_count();
        if prev_count == 0 || render_requests.len() != prev_count.saturating_add(1) {
            return false;
        }
        let history_count = self.history_cells.len();
        if history_count < 2 {
            return false;
        }
        if history_count != self
            .history_render
            .last_history_count()
            .saturating_add(1)
        {
            return false;
        }
        if render_requests.len() != history_count {
            return false;
        }
        let history_tail_start = history_count - 2;
        let tail = &render_requests[history_tail_start..history_count];
        if tail.len() != 2 {
            return false;
        }
        let cells = self
            .history_render
            .visible_cells(&self.history_state, tail, render_settings);
        if cells.len() != 2 {
            return false;
        }
        let prev = &cells[0];
        let next = &cells[1];
        if prev.height == 0 || next.height == 0 {
            return false;
        }
        let prev_is_reasoning = prev
            .cell
            .and_then(|cell| cell.as_any().downcast_ref::<crate::history_cell::CollapsibleReasoningCell>())
            .is_some();
        let next_is_reasoning = next
            .cell
            .and_then(|cell| cell.as_any().downcast_ref::<crate::history_cell::CollapsibleReasoningCell>())
            .is_some();
        if prev_is_reasoning || next_is_reasoning {
            return false;
        }
        let spacing = 1u16;
        let spacing_range = self
            .history_render
            .extend_prefix_for_append(prefix_width, spacing, next.height, history_count);
        if let Some(range) = spacing_range {
            self.history_render.append_spacing_range(range);
        }
        if next.height >= 2
            && next
                .cell
                .and_then(|cell| {
                    cell.as_any()
                        .downcast_ref::<crate::history_cell::AssistantMarkdownCell>()
                })
                .is_some()
        {
            let cell_end = self.history_render.last_total_height();
            let cell_start = cell_end.saturating_sub(next.height);
            self.history_render
                .append_spacing_range((cell_start, cell_start.saturating_add(1)));
            self.history_render
                .append_spacing_range((cell_end.saturating_sub(1), cell_end));
        }
        true
    }
}
