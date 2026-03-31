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
            } else {
                #[cfg(feature = "managed-network-proxy")]
                {
                    if matches_tui_hotkey(hotkeys.network_settings, &key_event) {
                        Some(ClickableAction::ShowNetworkSettings)
                    } else {
                        None
                    }
                }
                #[cfg(not(feature = "managed-network-proxy"))]
                {
                    None
                }
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
}
