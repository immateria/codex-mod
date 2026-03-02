use super::prelude::*;

fn matches_tui_hotkey(hk: code_core::config_types::TuiHotkey, ev: &KeyEvent) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mods = ev.modifiers.difference(KeyModifiers::SHIFT);
    match hk {
        code_core::config_types::TuiHotkey::Legacy => false,
        code_core::config_types::TuiHotkey::Function(fk) => {
            let Some(n) = fk.as_u8() else {
                return false;
            };
            matches!(ev.code, KeyCode::F(code_n) if code_n == n)
                && !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        }
        code_core::config_types::TuiHotkey::Chord(chord) => {
            let required = match (chord.ctrl, chord.alt) {
                (true, true) => KeyModifiers::CONTROL | KeyModifiers::ALT,
                (true, false) => KeyModifiers::CONTROL,
                (false, true) => KeyModifiers::ALT,
                (false, false) => KeyModifiers::NONE,
            };
            matches!(ev.code, KeyCode::Char(c) if c.to_ascii_lowercase() == chord.key) && mods == required
        }
    }
}

impl ChatWidget<'_> {
    pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) {
        if settings_handlers::handle_settings_key(self, key_event) {
            self.sync_limits_layout_mode_preference();
            return;
        }
        if self.settings.overlay.is_some() {
            return;
        }
        if terminal_handlers::handle_terminal_key(self, key_event) {
            return;
        }
        if self.terminal.overlay.is_some() {
            // Block background input while the terminal overlay is visible.
            return;
        }
        // Intercept keys for overlays when active (help first, then diff)
        if help_handlers::handle_help_key(self, key_event) {
            return;
        }
        if self.help.overlay.is_some() {
            return;
        }
        if diff_handlers::handle_diff_key(self, key_event) {
            return;
        }
        if self.diffs.overlay.is_some() {
            return;
        }
        if self.browser_overlay_visible {
            let is_ctrl_b = matches!(
                key_event,
                KeyEvent {
                    code: crossterm::event::KeyCode::Char('b'),
                    modifiers: crossterm::event::KeyModifiers::CONTROL,
                    kind: KeyEventKind::Press | KeyEventKind::Repeat,
                    ..
                }
            );
            if is_ctrl_b {
                self.toggle_browser_overlay();
                return;
            }
            if self.handle_browser_overlay_key(key_event) {
                return;
            }
        }
        if key_event.kind == KeyEventKind::Press {
            self.bottom_pane.clear_ctrl_c_quit_hint();
        }

        if self.auto_state.awaiting_coordinator_submit()
            && !self.auto_state.is_paused_manual()
            && matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            match key_event.code {
                crossterm::event::KeyCode::Enter
                | crossterm::event::KeyCode::Char(' ') if key_event.modifiers.is_empty() => {
                    if !self.auto_state.should_bypass_coordinator_next_submit() {
                        self.auto_submit_prompt();
                    }
                    return;
                }
                crossterm::event::KeyCode::Char('e') | crossterm::event::KeyCode::Char('E')
                    if key_event.modifiers.is_empty() =>
                {
                    self.auto_pause_for_manual_edit(false);
                    return;
                }
                _ => {}
            }
        }

        // Global overlays (avoid conflicting with common editor keys):
        // - Ctrl+B: toggle Browser overlay
        // - Ctrl+A: toggle Agents terminal mode
        if let KeyEvent {
            code: crossterm::event::KeyCode::Char('b'),
            modifiers: crossterm::event::KeyModifiers::CONTROL,
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        } = key_event
        {
            self.toggle_browser_overlay();
            return;
        }
        if let KeyEvent {
            code: crossterm::event::KeyCode::Char('a'),
            modifiers: crossterm::event::KeyModifiers::CONTROL,
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        } = key_event
        {
            self.toggle_agents_hud();
            return;
        }
        if self.agents_terminal.active {
            use crossterm::event::KeyCode;
            if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                return;
            }

            if let Some(pending) = self.agents_terminal.pending_stop.clone() {
                match key_event.code {
                    KeyCode::Esc | KeyCode::Char('n') => {
                        self.agents_terminal.clear_stop_prompt();
                        self.request_redraw();
                    }
                    KeyCode::Enter | KeyCode::Char('y') => {
                        self.cancel_agent_by_id(&pending.agent_id);
                        self.agents_terminal.clear_stop_prompt();
                    }
                    _ => {}
                }
                return;
            }
            match key_event.code {
                KeyCode::Esc => {
                    if self.agents_terminal.focus() == AgentsTerminalFocus::Detail {
                        self.agents_terminal.focus_sidebar();
                        self.request_redraw();
                    } else {
                        self.exit_agents_terminal_mode();
                    }
                    return;
                }
                KeyCode::Right | KeyCode::Enter => {
                    if self.agents_terminal.focus() == AgentsTerminalFocus::Sidebar {
                        self.agents_terminal.focus_detail();
                        self.request_redraw();
                    }
                    return;
                }
                KeyCode::Left => {
                    if self.agents_terminal.focus() == AgentsTerminalFocus::Detail {
                        self.agents_terminal.focus_sidebar();
                        self.request_redraw();
                    }
                    return;
                }
                KeyCode::Tab => {
                    match self.agents_terminal.focus() {
                        AgentsTerminalFocus::Sidebar => self.agents_terminal.focus_detail(),
                        AgentsTerminalFocus::Detail => self.agents_terminal.focus_sidebar(),
                    }
                    self.request_redraw();
                    return;
                }
                KeyCode::BackTab => {
                    match self.agents_terminal.focus() {
                        AgentsTerminalFocus::Sidebar => self.agents_terminal.focus_detail(),
                        AgentsTerminalFocus::Detail => self.agents_terminal.focus_sidebar(),
                    }
                    self.request_redraw();
                    return;
                }
                KeyCode::Char('1') => {
                    self.agents_terminal.set_tab(AgentsTerminalTab::All);
                    self.request_redraw();
                    return;
                }
                KeyCode::Char('2') => {
                    self.agents_terminal.set_tab(AgentsTerminalTab::Running);
                    self.request_redraw();
                    return;
                }
                KeyCode::Char('3') => {
                    self.agents_terminal.set_tab(AgentsTerminalTab::Failed);
                    self.request_redraw();
                    return;
                }
                KeyCode::Char('4') => {
                    self.agents_terminal.set_tab(AgentsTerminalTab::Completed);
                    self.request_redraw();
                    return;
                }
                KeyCode::Char('5') => {
                    self.agents_terminal.set_tab(AgentsTerminalTab::Review);
                    self.request_redraw();
                    return;
                }
                KeyCode::Char('[') => {
                    self.agents_terminal.jump_batch(-1);
                    self.request_redraw();
                    return;
                }
                KeyCode::Char(']') => {
                    self.agents_terminal.jump_batch(1);
                    self.request_redraw();
                    return;
                }
                KeyCode::Char('s') => {
                    let current = self.agents_terminal.current_sidebar_entry();
                    self.agents_terminal.cycle_sort_mode();
                    self.agents_terminal.reselect_entry(current);
                    self.request_redraw();
                    return;
                }
                KeyCode::Char('S') => {
                    self.prompt_stop_selected_agent();
                    return;
                }
                KeyCode::Char('h') => {
                    self.agents_terminal.toggle_highlights();
                    self.request_redraw();
                    return;
                }
                KeyCode::Char('a') => {
                    self.agents_terminal.toggle_actions();
                    self.request_redraw();
                    return;
                }
                KeyCode::Up => {
                    if self.agents_terminal.focus() == AgentsTerminalFocus::Detail {
                        self.sync_agents_terminal_scroll();
                        layout_scroll::line_up(self);
                        self.record_current_agent_scroll();
                    } else {
                        self.navigate_agents_terminal_selection(-1);
                    }
                    return;
                }
                KeyCode::Down => {
                    if self.agents_terminal.focus() == AgentsTerminalFocus::Detail {
                        self.sync_agents_terminal_scroll();
                        layout_scroll::line_down(self);
                        self.record_current_agent_scroll();
                    } else {
                        self.navigate_agents_terminal_selection(1);
                    }
                    return;
                }
                KeyCode::PageUp => {
                    if self.agents_terminal.focus() == AgentsTerminalFocus::Detail {
                        self.sync_agents_terminal_scroll();
                        layout_scroll::page_up(self);
                        self.record_current_agent_scroll();
                    } else {
                        self.navigate_agents_terminal_page(-1);
                    }
                    return;
                }
                KeyCode::PageDown => {
                    if self.agents_terminal.focus() == AgentsTerminalFocus::Detail {
                        self.sync_agents_terminal_scroll();
                        layout_scroll::page_down(self);
                        self.record_current_agent_scroll();
                    } else {
                        self.navigate_agents_terminal_page(1);
                    }
                    return;
                }
                KeyCode::Home => {
                    if self.agents_terminal.focus() == AgentsTerminalFocus::Detail {
                        self.sync_agents_terminal_scroll();
                        layout_scroll::to_top(self);
                        self.record_current_agent_scroll();
                    } else {
                        self.navigate_agents_terminal_home();
                    }
                    return;
                }
                KeyCode::End => {
                    if self.agents_terminal.focus() == AgentsTerminalFocus::Detail {
                        self.sync_agents_terminal_scroll();
                        layout_scroll::to_bottom(self);
                        self.record_current_agent_scroll();
                    } else {
                        self.navigate_agents_terminal_end();
                    }
                    return;
                }
                _ => {
                    return;
                }
            }
        }

        // Status line shortcuts (mirror clickable status line segments).
        //
        // These are configured under `[tui.hotkeys]` and may be function keys
        // or modifier chords (e.g. ctrl+h), so we match against the resolved
        // `TuiHotkey` values instead of hard-coding KeyCode::F(n).
        if !self.bottom_pane.has_active_modal_view()
            && matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            let hotkeys = self.config.tui.hotkeys.effective_for_runtime();
            let action = if matches_tui_hotkey(hotkeys.model_selector, &key_event) {
                Some(ClickableAction::ShowModelSelector)
            } else if matches_tui_hotkey(hotkeys.reasoning_effort, &key_event) {
                Some(ClickableAction::ShowReasoningSelector)
            } else if matches_tui_hotkey(hotkeys.shell_selector, &key_event) {
                Some(ClickableAction::ShowShellSelector)
            } else if matches_tui_hotkey(hotkeys.network_settings, &key_event) {
                Some(ClickableAction::ShowNetworkSettings)
            } else {
                None
            };
            if let Some(action) = action {
                self.handle_clickable_action(action);
                return;
            }
        }

        if let KeyEvent {
            code: crossterm::event::KeyCode::Char('g'),
            modifiers: crossterm::event::KeyModifiers::CONTROL,
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        } = key_event
        {
            if !self.bottom_pane.has_active_modal_view() {
                let initial = self.bottom_pane.composer_text();
                self.app_event_tx
                    .send(AppEvent::OpenExternalEditor { initial });
            }
            return;
        }

        // Fast-path PageUp/PageDown to scroll the transcript by a viewport at a time.
        if let crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::PageUp,
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        } = key_event
        {
            layout_scroll::page_up(self);
            return;
        }
        if let crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::PageDown,
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        } = key_event
        {
            layout_scroll::page_down(self);
            return;
        }
        // Home/End: when the composer is empty, jump the history to start/end
        if let crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Home,
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        } = key_event
            && self.composer_is_empty() {
                layout_scroll::to_top(self);
                return;
            }
        if let crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::End,
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        } = key_event
            && self.composer_is_empty() {
                layout_scroll::to_bottom(self);
                return;
            }

        // History shortcuts (fold/jump) configured under `[tui.hotkeys]`.
        // Only intercept when the composer is empty so normal typing is unaffected.
        if !self.bottom_pane.has_active_modal_view()
            && self.bottom_pane.composer_is_empty()
            && matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            use crossterm::event::{KeyCode, KeyModifiers};

            fn matches_legacy_char(ev: &KeyEvent, expected: char, allow_shift: bool) -> bool {
                if !matches!(ev.code, KeyCode::Char(c) if c == expected) {
                    return false;
                }
                if allow_shift {
                    ev.modifiers.difference(KeyModifiers::SHIFT).is_empty()
                } else {
                    ev.modifiers.is_empty()
                }
            }

            let hotkeys = self.config.tui.hotkeys.effective_for_runtime();
            let matches_binding = |binding: code_core::config_types::TuiHotkey,
                                   legacy_char: char,
                                   legacy_allows_shift: bool| {
                if binding.is_legacy() {
                    matches_legacy_char(&key_event, legacy_char, legacy_allows_shift)
                } else {
                    matches_tui_hotkey(binding, &key_event)
                }
            };

            if matches_binding(hotkeys.exec_output_fold, '[', false) {
                self.toggle_bottommost_exec_fold();
                return;
            }

            if matches_binding(hotkeys.js_repl_code_fold, '\\', false) {
                self.toggle_bottommost_js_repl_code_fold();
                return;
            }

            if matches_binding(hotkeys.jump_to_parent_call, ']', false) {
                let visible_parent = self
                    .visible_history_cell_range_for_shortcuts()
                    .and_then(|(start, end)| {
                        self.history_cells[start..end]
                            .iter()
                            .rev()
                            .find_map(|cell| cell.parent_call_id().map(str::to_owned))
                    });
                let Some(parent_call_id) = visible_parent.or_else(|| {
                    self.history_cells
                        .iter()
                        .rev()
                        .find_map(|cell| cell.parent_call_id().map(str::to_owned))
                }) else {
                    self.bottom_pane
                        .update_status_text("no parent tool call to jump to".to_string());
                    self.request_redraw();
                    return;
                };
                self.jump_to_call_id(&parent_call_id);
                return;
            }

            if matches_binding(hotkeys.jump_to_latest_child_call, '}', true) {
                use crate::history_cell::JsReplCell;
                let visible_child = self
                    .visible_history_cell_range_for_shortcuts()
                    .and_then(|(start, end)| {
                        self.history_cells[start..end].iter().rev().find_map(|cell| {
                            cell.as_any().downcast_ref::<JsReplCell>().and_then(|js_cell| {
                                js_cell.latest_child_call_id().map(str::to_owned)
                            })
                        })
                    });
                let Some(child_call_id) = visible_child.or_else(|| {
                    self.history_cells.iter().rev().find_map(|cell| {
                        cell.as_any()
                            .downcast_ref::<JsReplCell>()
                            .and_then(|js_cell| js_cell.latest_child_call_id().map(str::to_owned))
                    })
                }) else {
                    self.bottom_pane
                        .update_status_text("no spawned tool call to jump to".to_string());
                    self.request_redraw();
                    return;
                };
                self.jump_to_call_id(&child_call_id);
                return;
            }
        }

        let composer_was_empty = self.bottom_pane.composer_is_empty();
        let input_result = self.bottom_pane.handle_key_event(key_event);
        let composer_is_empty = self.bottom_pane.composer_is_empty();
        if composer_was_empty && !composer_is_empty {
            for cell in &self.history_cells {
                cell.trigger_fade();
            }
            self.request_redraw();
        }
        self.auto_sync_goal_escape_state_from_composer();

        match input_result {
            InputResult::Submitted(text) => {
                if let Some(pending) = self.pending_request_user_input.take() {
                    self.submit_request_user_input_answer(pending, text);
                    return;
                }
                self.pending_turn_origin = Some(TurnOrigin::User);
                let cleaned = Self::strip_context_sections(&text);
                self.last_user_message = (!cleaned.trim().is_empty()).then_some(cleaned);
                if self.auto_state.should_show_goal_entry() {
                    for cell in &self.history_cells {
                        cell.trigger_fade();
                    }
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        self.bottom_pane.set_task_running(true);
                        self.bottom_pane
                            .update_status_text("Auto Drive Goal".to_string());
                        self.clear_composer();
                        self.request_redraw();
                        return;
                    }
                    self.clear_composer();
                    self.bottom_pane.update_status_text(String::new());
                    self.bottom_pane.set_task_running(false);
                    self.handle_auto_command(Some(trimmed.to_string()));
                    return;
                }
                if self.try_handle_terminal_shortcut(&text) {
                    return;
                }
                let user_message = self.parse_message_with_images(text);
                self.submit_user_message(user_message);
            }
            InputResult::Command(_cmd) => {
                // Command was dispatched at the App layer; request redraw.
                self.app_event_tx.send(AppEvent::RequestRedraw);
            }
            InputResult::ScrollUp => {
                let before = self.layout.scroll_offset.get();
                // Only allow Up to navigate command history when the top view
                // cannot be scrolled at all (no scrollback available).
                if self.layout.last_max_scroll.get() == 0
                    && self.bottom_pane.try_history_up() {
                        self.perf_track_scroll_delta(before, self.layout.scroll_offset.get());
                        return;
                    }
                // Scroll up in chat history (increase offset, towards older content)
                // Use last_max_scroll computed during the previous render to avoid overshoot
                let new_offset = self
                    .layout
                    .scroll_offset
                    .get()
                    .saturating_add(3)
                    .min(self.layout.last_max_scroll.get());
                self.layout.scroll_offset.set(new_offset);
                self.flash_scrollbar();
                self.sync_history_virtualization();
                // Enable compact mode so history can use the spacer line
                if self.layout.scroll_offset.get() > 0 {
                    self.bottom_pane.set_compact_compose(true);
                    self.height_manager
                        .borrow_mut()
                        .record_event(HeightEvent::ComposerModeChange);
                    // Mark that the very next Down should continue scrolling chat (sticky)
                    self.bottom_pane.mark_next_down_scrolls_history();
                }
                self.app_event_tx.send(AppEvent::RequestRedraw);
                self.height_manager
                    .borrow_mut()
                    .record_event(HeightEvent::UserScroll);
                self.maybe_show_history_nav_hint_on_first_scroll();
                self.perf_track_scroll_delta(before, self.layout.scroll_offset.get());
            }
            InputResult::ScrollDown => {
                let before = self.layout.scroll_offset.get();
                // Only allow Down to navigate command history when the top view
                // cannot be scrolled at all (no scrollback available).
                if self.layout.last_max_scroll.get() == 0 && self.bottom_pane.history_is_browsing()
                    && self.bottom_pane.try_history_down() {
                        self.perf_track_scroll_delta(before, self.layout.scroll_offset.get());
                        return;
                    }
                // Scroll down in chat history (decrease offset, towards bottom)
                if self.layout.scroll_offset.get() == 0 {
                    // Already at bottom: ensure spacer above input is enabled.
                    self.bottom_pane.set_compact_compose(false);
                    self.sync_history_virtualization();
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                    self.height_manager
                        .borrow_mut()
                        .record_event(HeightEvent::UserScroll);
                    self.maybe_show_history_nav_hint_on_first_scroll();
                    self.height_manager
                        .borrow_mut()
                        .record_event(HeightEvent::ComposerModeChange);
                    self.perf_track_scroll_delta(before, self.layout.scroll_offset.get());
                } else if self.layout.scroll_offset.get() >= 3 {
                    // Move towards bottom but do NOT toggle spacer yet; wait until
                    // the user confirms by pressing Down again at bottom.
                    self.layout
                        .scroll_offset
                        .set(self.layout.scroll_offset.get().saturating_sub(3));
                    self.sync_history_virtualization();
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                    self.height_manager
                        .borrow_mut()
                        .record_event(HeightEvent::UserScroll);
                    self.maybe_show_history_nav_hint_on_first_scroll();
                    self.perf_track_scroll_delta(before, self.layout.scroll_offset.get());
                } else if self.layout.scroll_offset.get() > 0 {
                    // Land exactly at bottom without toggling spacer yet; require
                    // a subsequent Down to re-enable the spacer so the input
                    // doesn't move when scrolling into the line above it.
                    self.layout.scroll_offset.set(0);
                    self.sync_history_virtualization();
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                    self.height_manager
                        .borrow_mut()
                        .record_event(HeightEvent::UserScroll);
                    self.maybe_show_history_nav_hint_on_first_scroll();
                    self.perf_track_scroll_delta(before, self.layout.scroll_offset.get());
                }
                self.flash_scrollbar();
            }
            InputResult::None => {
                // Trigger redraw so input wrapping/height reflects immediately
                self.app_event_tx.send(AppEvent::RequestRedraw);
            }
        }
    }

    fn visible_history_cell_range_for_shortcuts(&self) -> Option<(usize, usize)> {
        let viewport_height = self.layout.last_history_viewport_height.get();
        if viewport_height == 0 {
            return None;
        }

        let ps_ref = self.history_render.prefix_sums.borrow();
        if ps_ref.len() < 2 {
            return None;
        }
        let request_count = ps_ref.len().saturating_sub(1);

        let history_len = self.history_cells.len();
        if history_len == 0 || request_count == 0 {
            return None;
        }

        let max_scroll = self.layout.last_max_scroll.get();
        let clamped_offset = self.layout.scroll_offset.get().min(max_scroll);

        // Reproduce the scroll-from-top calculation from the renderer so our
        // "visible" window matches what the user is actually seeing.
        let base_total_height = self.history_render.last_total_height();
        let total_height = if max_scroll > 0 {
            max_scroll.saturating_add(viewport_height)
        } else {
            base_total_height
        };
        let overscan_extra = total_height.saturating_sub(base_total_height);

        let mut scroll_pos = max_scroll.saturating_sub(clamped_offset);
        if overscan_extra > 0 && clamped_offset == 0 {
            scroll_pos = scroll_pos.saturating_sub(overscan_extra);
        }
        if clamped_offset > 0 {
            scroll_pos = self.history_render.adjust_scroll_to_content(scroll_pos);
        }

        let viewport_bottom = scroll_pos.saturating_add(viewport_height);
        let ps: &Vec<u16> = &ps_ref;

        let mut start_idx = match ps.binary_search(&scroll_pos) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        start_idx = start_idx.min(request_count);

        let mut end_idx = match ps.binary_search(&viewport_bottom) {
            Ok(i) => i,
            Err(i) => i,
        };
        end_idx = end_idx.saturating_add(1).min(request_count);
        drop(ps_ref);

        let start = start_idx.min(history_len);
        let end = end_idx.min(history_len);
        (start < end).then_some((start, end))
    }

    fn toggle_bottommost_exec_fold(&mut self) {
        use crate::history_cell::{
            ExecCell,
            JsReplCell,
            RunningToolCallCell,
            ToolCallCell,
            WebFetchToolCell,
        };

        let (start, end) = self
            .visible_history_cell_range_for_shortcuts()
            .unwrap_or((0, self.history_cells.len()));
        for cell_box in self.history_cells[start..end].iter().rev() {
            let cell = cell_box.as_ref();
            if let Some(exec_cell) = cell.as_any().downcast_ref::<ExecCell>()
                && exec_cell.output.is_some()
            {
                exec_cell.toggle_output_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
            if let Some(js_cell) = cell.as_any().downcast_ref::<JsReplCell>()
                && js_cell.output.is_some()
            {
                js_cell.toggle_output_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
            if let Some(tool_cell) = cell.as_any().downcast_ref::<ToolCallCell>() {
                tool_cell.toggle_details_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
            if let Some(tool_cell) = cell.as_any().downcast_ref::<RunningToolCallCell>() {
                tool_cell.toggle_details_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
            if let Some(web_fetch_cell) = cell.as_any().downcast_ref::<WebFetchToolCell>() {
                web_fetch_cell.toggle_body_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
        }
    }

    fn toggle_bottommost_js_repl_code_fold(&mut self) {
        use crate::history_cell::JsReplCell;
        let (start, end) = self
            .visible_history_cell_range_for_shortcuts()
            .unwrap_or((0, self.history_cells.len()));
        for cell_box in self.history_cells[start..end].iter().rev() {
            let cell = cell_box.as_ref();
            if let Some(js_cell) = cell.as_any().downcast_ref::<JsReplCell>() {
                js_cell.toggle_code_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
        }
    }

    pub(in super::super) fn toggle_agents_hud(&mut self) {
        if self.agents_terminal.active {
            self.exit_agents_terminal_mode();
        } else {
            self.enter_agents_terminal_mode();
        }
    }

    pub(crate) fn handle_paste(&mut self, text: String) {
        if settings_handlers::handle_settings_paste(self, text.clone()) {
            return;
        }
        // Check if the pasted text is a file path to an image
        let trimmed = text.trim();

        tracing::info!("Paste received: {:?}", trimmed);

        const IMAGE_EXTENSIONS: &[&str] = &[
            ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".svg", ".ico", ".tiff", ".tif",
        ];

        // Check if it looks like a file path
        let is_likely_path = trimmed.starts_with("file://")
            || trimmed.starts_with("/")
            || trimmed.starts_with("~/")
            || trimmed.starts_with("./");

        if is_likely_path {
            // Remove escape backslashes that terminals add for special characters
            let unescaped = trimmed
                .replace("\\ ", " ")
                .replace("\\(", "(")
                .replace("\\)", ")");

            // Handle file:// URLs (common when dragging from Finder)
            let path_str = if unescaped.starts_with("file://") {
                // URL decode to handle spaces and special characters
                // Simple decoding for common cases (spaces as %20, etc.)
                unescaped
                    .strip_prefix("file://")
                    .map(|s| {
                        s.replace("%20", " ")
                            .replace("%28", "(")
                            .replace("%29", ")")
                            .replace("%5B", "[")
                            .replace("%5D", "]")
                            .replace("%2C", ",")
                            .replace("%27", "'")
                            .replace("%26", "&")
                            .replace("%23", "#")
                            .replace("%40", "@")
                            .replace("%2B", "+")
                            .replace("%3D", "=")
                            .replace("%24", "$")
                            .replace("%21", "!")
                            .replace("%2D", "-")
                            .replace("%2E", ".")
                    })
                    .unwrap_or_else(|| unescaped.clone())
            } else {
                unescaped
            };

            tracing::info!("Decoded path: {:?}", path_str);

            // Check if it has an image extension
            let is_image = IMAGE_EXTENSIONS
                .iter()
                .any(|ext| path_str.to_lowercase().ends_with(ext));

            if is_image {
                let path = PathBuf::from(&path_str);
                tracing::info!("Checking if path exists: {:?}", path);
                if path.exists() {
                    tracing::info!("Image file dropped/pasted: {:?}", path);
                    // Get just the filename for display
                    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");

                    // Add a placeholder to the compose field instead of submitting
                    let placeholder = format!("[image: {filename}]");

                    let persisted = self.persist_user_image_if_needed(&path).unwrap_or(path);

                    // Store the image path for later submission
                    self.pending_images.insert(placeholder.clone(), persisted);

                    // Add the placeholder text to the compose field
                    self.bottom_pane.handle_paste(placeholder);
                    self.auto_sync_goal_escape_state_from_composer();
                    // Force immediate redraw to reflect input growth/wrap
                    self.request_redraw();
                    return;
                } else {
                    tracing::warn!("Image path does not exist: {:?}", path);
                }
            } else {
                // For non-image files, paste the decoded path as plain text.
                let path = PathBuf::from(&path_str);
                if path.exists() && path.is_file() {
                    self.bottom_pane.handle_paste(path_str);
                    self.auto_sync_goal_escape_state_from_composer();
                    self.request_redraw();
                    return;
                }
            }
        }

        // Otherwise handle as regular text paste
        self.bottom_pane.handle_paste(text);
        self.auto_sync_goal_escape_state_from_composer();
        // Force immediate redraw so compose height matches new content
        self.request_redraw();
    }
}
