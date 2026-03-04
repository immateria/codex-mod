use std::cell::Cell;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;
use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    redraw_if,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::BottomPane;

const DEFAULT_VISIBLE_ROWS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    PidsMax,
    MemoryMax,
    ResetBothAuto,
    DisableBoth,
    Apply,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditTarget {
    PidsMax,
    MemoryMax,
}

#[derive(Debug)]
enum ViewMode {
    Main,
    Edit {
        target: EditTarget,
        field: FormTextField,
        error: Option<String>,
    },
    Transition,
}

pub(crate) struct ExecLimitsSettingsView {
    settings: code_core::config::ExecLimitsToml,
    last_applied: code_core::config::ExecLimitsToml,
    last_apply_at: Option<Instant>,
    mode: ViewMode,
    state: Cell<ScrollState>,
    viewport_rows: Cell<usize>,
    is_complete: bool,
    app_event_tx: AppEventSender,
}

impl ExecLimitsSettingsView {
    pub(crate) fn new(
        settings: code_core::config::ExecLimitsToml,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        let last_applied = settings.clone();
        Self {
            settings,
            last_applied,
            last_apply_at: None,
            mode: ViewMode::Main,
            state: Cell::new(state),
            viewport_rows: Cell::new(DEFAULT_VISIBLE_ROWS),
            is_complete: false,
            app_event_tx,
        }
    }

    fn build_rows() -> [RowKind; 6] {
        [
            RowKind::PidsMax,
            RowKind::MemoryMax,
            RowKind::ResetBothAuto,
            RowKind::DisableBoth,
            RowKind::Apply,
            RowKind::Close,
        ]
    }

    fn format_limit_pids(limit: code_core::config::ExecLimitToml) -> String {
        match limit {
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Auto) => {
                "Auto".to_string()
            }
            code_core::config::ExecLimitToml::Mode(
                code_core::config::ExecLimitModeToml::Disabled,
            ) => "Disabled".to_string(),
            code_core::config::ExecLimitToml::Value(v) => v.to_string(),
        }
    }

    fn format_limit_memory(limit: code_core::config::ExecLimitToml) -> String {
        match limit {
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Auto) => {
                "Auto".to_string()
            }
            code_core::config::ExecLimitToml::Mode(
                code_core::config::ExecLimitModeToml::Disabled,
            ) => "Disabled".to_string(),
            code_core::config::ExecLimitToml::Value(v) => format!("{v} MiB"),
        }
    }

    fn render_header_lines(&self) -> Vec<Line<'static>> {
        let hint = Style::default().fg(colors::text_dim());
        let mut lines = vec![Line::from(Span::styled(
            "Limits for tool-spawned commands (shell + exec_command).",
            hint,
        ))];

        #[cfg(target_os = "linux")]
        {
            lines.push(Line::from(Span::styled(
                "Linux: enforced via cgroup v2 when available.",
                hint,
            )));

            // Make "Auto" less opaque by showing what it would currently resolve to.
            let auto_pids = code_core::config::exec_limits_auto_pids_max();
            let auto_mem_bytes = code_core::config::exec_limits_auto_memory_max_bytes();
            let auto_mem_mib = auto_mem_bytes.map(|b| (b.saturating_add(1024 * 1024 - 1)) / (1024 * 1024));
            let auto_line = match (auto_pids, auto_mem_mib) {
                (Some(pids), Some(mib)) => format!("Auto currently: pids_max={pids} · memory_max={mib} MiB"),
                (Some(pids), None) => format!("Auto currently: pids_max={pids}"),
                (None, Some(mib)) => format!("Auto currently: memory_max={mib} MiB"),
                (None, None) => "Auto currently: (not available)".to_string(),
            };
            lines.push(Line::from(Span::styled(auto_line, hint)));
        }
        #[cfg(not(target_os = "linux"))]
        lines.push(Line::from(Span::styled(
            "This platform: best-effort (no cgroup enforcement yet).",
            hint,
        )));

        let is_dirty = self.settings != self.last_applied;
        if is_dirty {
            lines.push(Line::from(Span::styled(
                "Pending changes: select Apply to save.",
                Style::default().fg(colors::warning()),
            )));
        } else if self
            .last_apply_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
        {
            lines.push(Line::from(Span::styled(
                "Applied.",
                Style::default().fg(colors::success()),
            )));
        }

        lines.push(Line::from(""));
        lines
    }

    fn render_footer_lines(&self) -> Vec<Line<'static>> {
        vec![Line::from(vec![
            Span::styled("↑↓", Style::default().fg(colors::function())),
            Span::styled(" move  ", Style::default().fg(colors::text_dim())),
            Span::styled("Enter", Style::default().fg(colors::success())),
            Span::styled(" edit/toggle  ", Style::default().fg(colors::text_dim())),
            Span::styled("a", Style::default().fg(colors::success())),
            Span::styled(" auto  ", Style::default().fg(colors::text_dim())),
            Span::styled("d", Style::default().fg(colors::success())),
            Span::styled(" disable  ", Style::default().fg(colors::text_dim())),
            Span::styled("Ctrl+S", Style::default().fg(colors::success())),
            Span::styled(" save  ", Style::default().fg(colors::text_dim())),
            Span::styled("Esc", Style::default().fg(colors::error())),
            Span::styled(" close", Style::default().fg(colors::text_dim())),
        ])]
    }

    fn open_edit_for(&mut self, target: EditTarget) {
        let mut field = FormTextField::new_single_line();
        field.set_placeholder("number, auto, or disabled");
        match target {
            EditTarget::PidsMax => {
                if let code_core::config::ExecLimitToml::Value(v) = self.settings.pids_max {
                    field.set_text(&v.to_string());
                }
            }
            EditTarget::MemoryMax => {
                if let code_core::config::ExecLimitToml::Value(v) = self.settings.memory_max_mb {
                    field.set_text(&v.to_string());
                }
            }
        }
        self.mode = ViewMode::Edit {
            target,
            field,
            error: None,
        };
    }

    fn cycle_limit(&mut self, target: EditTarget) {
        let value = match target {
            EditTarget::PidsMax => self.settings.pids_max,
            EditTarget::MemoryMax => self.settings.memory_max_mb,
        };

        match value {
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Auto) => {
                self.set_limit(target, code_core::config::ExecLimitToml::Mode(
                    code_core::config::ExecLimitModeToml::Disabled,
                ));
            }
            code_core::config::ExecLimitToml::Mode(
                code_core::config::ExecLimitModeToml::Disabled,
            ) => {
                self.open_edit_for(target);
            }
            code_core::config::ExecLimitToml::Value(_) => {
                // When already custom, Enter edits instead of cycling away.
                self.open_edit_for(target);
            }
        }
    }

    fn set_limit(&mut self, target: EditTarget, value: code_core::config::ExecLimitToml) {
        match target {
            EditTarget::PidsMax => self.settings.pids_max = value,
            EditTarget::MemoryMax => self.settings.memory_max_mb = value,
        }
    }

    fn activate_row(&mut self, row: RowKind) {
        match row {
            RowKind::PidsMax => self.cycle_limit(EditTarget::PidsMax),
            RowKind::MemoryMax => self.cycle_limit(EditTarget::MemoryMax),
            RowKind::ResetBothAuto => {
                self.settings.pids_max = code_core::config::ExecLimitToml::Mode(
                    code_core::config::ExecLimitModeToml::Auto,
                );
                self.settings.memory_max_mb = code_core::config::ExecLimitToml::Mode(
                    code_core::config::ExecLimitModeToml::Auto,
                );
            }
            RowKind::DisableBoth => {
                self.settings.pids_max = code_core::config::ExecLimitToml::Mode(
                    code_core::config::ExecLimitModeToml::Disabled,
                );
                self.settings.memory_max_mb = code_core::config::ExecLimitToml::Mode(
                    code_core::config::ExecLimitModeToml::Disabled,
                );
            }
            RowKind::Apply => {
                self.app_event_tx
                    .send(AppEvent::SetExecLimitsSettings(self.settings.clone()));
                self.last_applied = self.settings.clone();
                self.last_apply_at = Some(Instant::now());
            }
            RowKind::Close => self.is_complete = true,
        }
    }

    fn process_key_event_main(&mut self, key_event: KeyEvent) -> bool {
        let rows = Self::build_rows();
        let len = rows.len();
        let mut state = self.state.get();
        state.clamp_selection(len);
        let visible = self.viewport_rows.get().max(1);

        let handled = match key_event.code {
            KeyCode::Esc => {
                self.is_complete = true;
                true
            }
            KeyCode::Up => {
                state.move_up_wrap_visible(len, visible);
                true
            }
            KeyCode::Down => {
                state.move_down_wrap_visible(len, visible);
                true
            }
            KeyCode::Enter => {
                let selected_idx = state.selected_idx.unwrap_or(0).min(len.saturating_sub(1));
                let selected = rows.get(selected_idx).copied().unwrap_or(RowKind::PidsMax);
                self.activate_row(selected);
                true
            }
            KeyCode::Char('a') if key_event.modifiers.is_empty() => {
                let selected_idx = state.selected_idx.unwrap_or(0).min(len.saturating_sub(1));
                let selected = rows.get(selected_idx).copied().unwrap_or(RowKind::PidsMax);
                let target = match selected {
                    RowKind::PidsMax => Some(EditTarget::PidsMax),
                    RowKind::MemoryMax => Some(EditTarget::MemoryMax),
                    _ => None,
                };
                let Some(target) = target else {
                    self.state.set(state);
                    return false;
                };
                self.set_limit(
                    target,
                    code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Auto),
                );
                true
            }
            KeyCode::Char('d') if key_event.modifiers.is_empty() => {
                let selected_idx = state.selected_idx.unwrap_or(0).min(len.saturating_sub(1));
                let selected = rows.get(selected_idx).copied().unwrap_or(RowKind::PidsMax);
                let target = match selected {
                    RowKind::PidsMax => Some(EditTarget::PidsMax),
                    RowKind::MemoryMax => Some(EditTarget::MemoryMax),
                    _ => None,
                };
                let Some(target) = target else {
                    self.state.set(state);
                    return false;
                };
                self.set_limit(
                    target,
                    code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Disabled),
                );
                true
            }
            _ => false,
        };

        self.state.set(state);
        handled
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main => {
                let handled = self.process_key_event_main(key_event);
                if matches!(self.mode, ViewMode::Transition) {
                    self.mode = ViewMode::Main;
                }
                handled
            }
            ViewMode::Edit { target, mut field, mut error } => {
                let handled = match (key_event.code, key_event.modifiers) {
                    (KeyCode::Esc, _) => {
                        self.mode = ViewMode::Main;
                        return true;
                    }
                    (KeyCode::Char('s'), KeyModifiers::CONTROL) | (KeyCode::Enter, _) => {
                        let text = field.text().trim();
                        if text.is_empty() {
                            error = Some("Enter a number, or \"auto\"/\"disabled\"".to_string());
                            self.mode = ViewMode::Edit { target, field, error };
                            return true;
                        }

                        let lowered = text.to_ascii_lowercase();
                        if lowered == "auto" {
                            self.set_limit(
                                target,
                                code_core::config::ExecLimitToml::Mode(
                                    code_core::config::ExecLimitModeToml::Auto,
                                ),
                            );
                            self.mode = ViewMode::Main;
                            return true;
                        }
                        if lowered == "disabled" || lowered == "disable" {
                            self.set_limit(
                                target,
                                code_core::config::ExecLimitToml::Mode(
                                    code_core::config::ExecLimitModeToml::Disabled,
                                ),
                            );
                            self.mode = ViewMode::Main;
                            return true;
                        }

                        let parsed: u64 = match text.parse() {
                            Ok(v) if v >= 1 => v,
                            Ok(_) => {
                                error = Some("Value must be >= 1 (or \"disabled\")".to_string());
                                self.mode = ViewMode::Edit { target, field, error };
                                return true;
                            }
                            Err(_) => {
                                error =
                                    Some("Value must be an integer (or \"auto\"/\"disabled\")".to_string());
                                self.mode = ViewMode::Edit { target, field, error };
                                return true;
                            }
                        };

                        self.set_limit(target, code_core::config::ExecLimitToml::Value(parsed));
                        self.mode = ViewMode::Main;
                        return true;
                    }
                    _ => field.handle_key(key_event),
                };

                self.mode = ViewMode::Edit { target, field, error };
                handled
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.process_key_event(key_event)
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        match &mut self.mode {
            ViewMode::Edit { field, .. } => {
                field.handle_paste(text);
                true
            }
            ViewMode::Main | ViewMode::Transition => false,
        }
    }

    fn selection_index_at(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        let inner = Block::default().borders(Borders::ALL).inner(area);
        if inner.width == 0 || inner.height == 0 {
            return None;
        }

        if x < inner.x
            || x >= inner.x.saturating_add(inner.width)
            || y < inner.y
            || y >= inner.y.saturating_add(inner.height)
        {
            return None;
        }

        let header_lines = self.render_header_lines();
        let footer_lines = self.render_footer_lines();
        let available_height = inner.height as usize;
        let header_height = header_lines.len().min(available_height);
        let footer_height = if available_height > header_height {
            1 + footer_lines.len()
        } else {
            0
        };
        let list_height = available_height.saturating_sub(header_height + footer_height);
        // Even if we can't fit the footer, we still render at least one selectable row.
        let visible_slots = list_height.max(1);

        let rel_y = y.saturating_sub(inner.y) as usize;
        if rel_y < header_height || rel_y >= header_height + visible_slots {
            return None;
        }
        let line_offset = rel_y - header_height;

        let rows = Self::build_rows();
        let total = rows.len();
        if total == 0 {
            return None;
        }
        let idx = self.state.get().scroll_top.saturating_add(line_offset);
        if idx >= total {
            None
        } else {
            Some(idx)
        }
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main => {
                let rows = Self::build_rows();
                let total = rows.len();
                if total == 0 {
                    self.mode = ViewMode::Main;
                    return false;
                }

                // Keep visible-row metrics in sync with current area so mouse navigation
                // can't move selection off-screen.
                let inner = Block::default().borders(Borders::ALL).inner(area);
                let header_lines = self.render_header_lines();
                let footer_lines = self.render_footer_lines();
                let available_height = inner.height as usize;
                let header_height = header_lines.len().min(available_height);
                let footer_height = if available_height > header_height {
                    1 + footer_lines.len()
                } else {
                    0
                };
                let list_height = available_height.saturating_sub(header_height + footer_height);
                let visible_slots = list_height.max(1);
                self.viewport_rows.set(visible_slots);

                let mut state = self.state.get();
                state.clamp_selection(total);
                let mut selected = state.selected_idx.unwrap_or(0);
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    total,
                    |x, y| self.selection_index_at(area, x, y),
                    SelectableListMouseConfig {
                        hover_select: true,
                        require_pointer_hit_for_scroll: true,
                        scroll_behavior: ScrollSelectionBehavior::Clamp,
                        ..SelectableListMouseConfig::default()
                    },
                );
                state.selected_idx = Some(selected);
                state.ensure_visible(total, visible_slots);
                self.state.set(state);

                if matches!(result, SelectableListMouseResult::Activated)
                    && let Some(kind) = rows.get(selected).copied()
                {
                    self.activate_row(kind);
                }

                if matches!(self.mode, ViewMode::Transition) {
                    self.mode = ViewMode::Main;
                }
                result.handled()
            }
            ViewMode::Edit { target, mut field, error } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        // Focus click inside the input field area.
                        let inner = Block::default().borders(Borders::ALL).inner(area);
                        let field_area = Rect {
                            x: inner.x.saturating_add(2),
                            // Matches `render_edit` ("Ctrl+S…" + blank + "Value:" -> 3 lines)
                            y: inner.y.saturating_add(3),
                            width: inner.width.saturating_sub(4),
                            height: 1,
                        };
                        field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                    }
                    _ => false,
                };
                self.mode = ViewMode::Edit { target, field, error };
                handled
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    fn render_main(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::border()))
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .title(" Exec Limits ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let header_lines = self.render_header_lines();
        let footer_lines = self.render_footer_lines();

        let available_height = inner.height as usize;
        let header_height = header_lines.len().min(available_height);
        let footer_height = if available_height > header_height {
            1 + footer_lines.len()
        } else {
            0
        };
        let list_height = available_height.saturating_sub(header_height + footer_height);
        let visible_slots = list_height.max(1);
        self.viewport_rows.set(visible_slots);

        let rows = Self::build_rows();
        let total = rows.len();
        let mut state = self.state.get();
        state.clamp_selection(total);
        let selected_idx = state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
        state.ensure_visible(total, visible_slots);
        self.state.set(state);

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.extend(header_lines);

        // List rows (scrolled)
        let start = state.scroll_top.min(total.saturating_sub(1));
        let end = (start + visible_slots).min(total);
        for (abs_idx, row) in rows
            .iter()
            .copied()
            .enumerate()
            .skip(start)
            .take(end.saturating_sub(start))
        {
            let is_selected = abs_idx == selected_idx;
            let is_dirty = self.settings != self.last_applied;
            let (label, value) = match row {
                RowKind::PidsMax => (
                    "Process limit (pids.max)",
                    Self::format_limit_pids(self.settings.pids_max),
                ),
                RowKind::MemoryMax => (
                    "Memory limit (memory.max)",
                    Self::format_limit_memory(self.settings.memory_max_mb),
                ),
                RowKind::ResetBothAuto => ("Reset both to Auto", "".to_string()),
                RowKind::DisableBoth => ("Disable both", "".to_string()),
                RowKind::Apply => (
                    "Apply",
                    if is_dirty {
                        "Pending".to_string()
                    } else {
                        "Saved".to_string()
                    },
                ),
                RowKind::Close => ("Close", "".to_string()),
            };

            let mut spans = Vec::new();
            spans.push(Span::styled(
                if is_selected { "❯ " } else { "  " },
                Style::default().fg(colors::text_dim()),
            ));
            spans.push(Span::styled(
                format!("{label:<24}"),
                Style::default()
                    .fg(if is_selected {
                        colors::text()
                    } else {
                        colors::text_dim()
                    })
                    .add_modifier(if is_selected { Modifier::BOLD } else { Modifier::empty() }),
            ));
            if !value.is_empty() {
                spans.push(Span::styled(
                    value,
                    Style::default().fg(colors::success()),
                ));
            }

            let line = Line::from(spans).style(if is_selected {
                Style::default().bg(colors::selection()).fg(colors::text())
            } else {
                Style::default()
            });
            lines.push(line);
        }

        // Pad remaining visible slots
        while lines.len() < header_height + visible_slots {
            lines.push(Line::from(" "));
        }

        lines.extend([Line::from(" ")]);
        lines.extend(footer_lines);

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(colors::background()))
            .render(inner, buf);
    }

    fn render_edit(&self, area: Rect, buf: &mut Buffer, target: EditTarget, field: &FormTextField, error: Option<&str>) {
        Clear.render(area, buf);
        let title = match target {
            EditTarget::PidsMax => " Edit Process Limit ",
            EditTarget::MemoryMax => " Edit Memory Limit (MiB) ",
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::border()))
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .title(title)
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let hint = Style::default().fg(colors::text_dim());
        let mut lines: Vec<Line<'static>> = vec![
            Line::from(Span::styled("Enter/Ctrl+S save · Esc cancel", hint)),
            Line::from(""),
            Line::from(Span::styled("Value:", hint)),
        ];

        let field_area = Rect {
            x: inner.x.saturating_add(2),
            y: inner.y.saturating_add(lines.len() as u16),
            width: inner.width.saturating_sub(4),
            height: 1,
        };
        // Render field
        field.render(field_area, buf, true);
        lines.push(Line::from(""));

        if let Some(err) = error {
            lines.push(Line::from(Span::styled(
                err.to_string(),
                Style::default().fg(colors::error()),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "Tip: type \"auto\" or \"disabled\".",
                hint,
            )));
        }

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(colors::background()))
            .render(inner, buf);
    }
}

impl<'a> BottomPaneView<'a> for ExecLimitsSettingsView {
    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.process_key_event(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_mouse_event_direct(mouse_event, area))
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        redraw_if(self.handle_paste_direct(text))
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        match &self.mode {
            ViewMode::Main => {
                let header = self.render_header_lines().len() as u16;
                let total_rows = Self::build_rows().len();
                let visible = total_rows.clamp(1, 10) as u16;
                2 + header + visible
            }
            ViewMode::Edit { .. } => 10,
            ViewMode::Transition => 2 + self.render_header_lines().len() as u16 + 6,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main(area, buf),
            ViewMode::Edit {
                target,
                field,
                error,
            } => self.render_edit(area, buf, *target, field, error.as_deref()),
            ViewMode::Transition => self.render_main(area, buf),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::widgets::{Block, Borders};
    use ratatui::layout::Rect;

    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;

    use super::{ExecLimitsSettingsView, RowKind};

    #[test]
    fn mouse_click_apply_emits_set_exec_limits_event() {
        let (tx, rx) = mpsc::channel();
        let app_event_tx = AppEventSender::new(tx);
        let settings = code_core::config::ExecLimitsToml::default();
        let mut view = ExecLimitsSettingsView::new(settings, app_event_tx);

        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 12,
        };

        let inner = Block::default().borders(Borders::ALL).inner(area);
        let header_height = view.render_header_lines().len() as u16;
        let apply_idx = ExecLimitsSettingsView::build_rows()
            .iter()
            .position(|row| *row == RowKind::Apply)
            .expect("apply row");

        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: inner.x.saturating_add(2),
            row: inner.y.saturating_add(header_height).saturating_add(apply_idx as u16),
            modifiers: KeyModifiers::NONE,
        };

        assert!(view.handle_mouse_event_direct(mouse_event, area));
        match rx.try_recv() {
            Ok(AppEvent::SetExecLimitsSettings(_)) => {}
            other => panic!("expected SetExecLimitsSettings event, got: {other:?}"),
        }
    }
}
