use std::cell::Cell;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

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

use crate::bottom_pane::{BottomPaneView, ConditionalUpdate};
use crate::bottom_pane::settings_ui::editor_page::SettingsEditorPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;
use crate::bottom_pane::settings_ui::rows::{KeyValueRow, StyledText};
use crate::bottom_pane::BottomPane;

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
    // Interior mutability so `render_main(&self, ...)` can clamp/scroll the
    // selection as the viewport changes without needing an outer `&mut self`.
    state: Cell<ScrollState>,
    viewport_rows: Cell<usize>,
    is_complete: bool,
    app_event_tx: AppEventSender,
}

pub(crate) type ExecLimitsSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, ExecLimitsSettingsView>;
pub(crate) type ExecLimitsSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, ExecLimitsSettingsView>;
pub(crate) type ExecLimitsSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, ExecLimitsSettingsView>;
pub(crate) type ExecLimitsSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, ExecLimitsSettingsView>;

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
                if let Some(selected) = rows.get(selected_idx).copied() {
                    self.activate_row(selected);
                    true
                } else {
                    false
                }
            }
            KeyCode::Char('a') if key_event.modifiers.is_empty() => {
                let selected_idx = state.selected_idx.unwrap_or(0).min(len.saturating_sub(1));
                let Some(&selected) = rows.get(selected_idx) else {
                    self.state.set(state);
                    return false;
                };
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
                let Some(&selected) = rows.get(selected_idx) else {
                    self.state.set(state);
                    return false;
                };
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

    pub(crate) fn framed(&self) -> ExecLimitsSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> ExecLimitsSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> ExecLimitsSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> ExecLimitsSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
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

    fn handle_mouse_event_direct_content(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main => {
                let rows = Self::build_rows();
                let total = rows.len();
                if total == 0 {
                    self.mode = ViewMode::Main;
                    return false;
                }

                let page = SettingsRowPage::new(
                    " Exec Limits ",
                    self.render_header_lines(),
                    self.render_footer_lines(),
                );
                let Some(layout) = page.content_only().layout(area) else {
                    self.mode = ViewMode::Main;
                    return false;
                };
                let visible_slots = layout.visible_rows().max(1);
                self.viewport_rows.set(visible_slots);

                let mut state = self.state.get();
                state.clamp_selection(total);
                let scroll_top = state.scroll_top;
                let body = layout.body;
                let mut selected = state.selected_idx.unwrap_or(0);
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    total,
                    |x, y| SettingsRowPage::selection_index_at(body, x, y, scroll_top, total),
                    SelectableListMouseConfig {
                        hover_select: false,
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
                        let Some(field_area) =
                            Self::edit_page(target, error.as_deref())
                                .content_only()
                                .layout(area)
                                .map(|layout| layout.field)
                        else {
                            return false;
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

    fn handle_mouse_event_direct_framed(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main => {
                let rows = Self::build_rows();
                let total = rows.len();
                if total == 0 {
                    self.mode = ViewMode::Main;
                    return false;
                }

                let page = SettingsRowPage::new(
                    " Exec Limits ",
                    self.render_header_lines(),
                    self.render_footer_lines(),
                );
                let Some(layout) = page.framed().layout(area) else {
                    self.mode = ViewMode::Main;
                    return false;
                };
                let visible_slots = layout.visible_rows().max(1);
                self.viewport_rows.set(visible_slots);

                let mut state = self.state.get();
                state.clamp_selection(total);
                let scroll_top = state.scroll_top;
                let body = layout.body;
                let mut selected = state.selected_idx.unwrap_or(0);
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    total,
                    |x, y| SettingsRowPage::selection_index_at(body, x, y, scroll_top, total),
                    SelectableListMouseConfig {
                        hover_select: false,
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
                        let Some(field_area) = Self::edit_page(target, error.as_deref())
                            .framed()
                            .layout(area)
                            .map(|layout| layout.field)
                        else {
                            return false;
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
        let rows = Self::build_rows();
        let total = rows.len();
        let mut state = self.state.get();
        state.clamp_selection(total);
        let selected_idx = state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));

        let is_dirty = self.settings != self.last_applied;
        let row_specs: Vec<KeyValueRow<'_>> = rows
            .iter()
            .copied()
            .map(|row| match row {
                RowKind::PidsMax => KeyValueRow::new("Process limit (pids.max)").with_value(
                    StyledText::new(
                        Self::format_limit_pids(self.settings.pids_max),
                        Style::default().fg(colors::success()),
                    ),
                ),
                RowKind::MemoryMax => KeyValueRow::new("Memory limit (memory.max)").with_value(
                    StyledText::new(
                        Self::format_limit_memory(self.settings.memory_max_mb),
                        Style::default().fg(colors::success()),
                    ),
                ),
                RowKind::ResetBothAuto => KeyValueRow::new("Reset both to Auto"),
                RowKind::DisableBoth => KeyValueRow::new("Disable both"),
                RowKind::Apply => KeyValueRow::new("Apply").with_value(StyledText::new(
                    if is_dirty { "Pending" } else { "Saved" },
                    Style::default().fg(colors::success()),
                )),
                RowKind::Close => KeyValueRow::new("Close"),
            })
            .collect();
        let Some(layout) = SettingsRowPage::new(
            " Exec Limits ",
            self.render_header_lines(),
            self.render_footer_lines(),
        )
        .framed()
        .render(area, buf, state.scroll_top, Some(selected_idx), &row_specs)
        else {
            return;
        };
        let visible_slots = layout.visible_rows();
        state.ensure_visible(total, visible_slots);
        self.state.set(state);
        self.viewport_rows.set(visible_slots);
    }

    fn render_main_without_frame(&self, area: Rect, buf: &mut Buffer) {
        let rows = Self::build_rows();
        let total = rows.len();
        let mut state = self.state.get();
        state.clamp_selection(total);
        let selected_idx = state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));

        let is_dirty = self.settings != self.last_applied;
        let row_specs: Vec<KeyValueRow<'_>> = rows
            .iter()
            .copied()
            .map(|row| match row {
                RowKind::PidsMax => KeyValueRow::new("Process limit (pids.max)").with_value(
                    StyledText::new(
                        Self::format_limit_pids(self.settings.pids_max),
                        Style::default().fg(colors::success()),
                    ),
                ),
                RowKind::MemoryMax => KeyValueRow::new("Memory limit (memory.max)").with_value(
                    StyledText::new(
                        Self::format_limit_memory(self.settings.memory_max_mb),
                        Style::default().fg(colors::success()),
                    ),
                ),
                RowKind::ResetBothAuto => KeyValueRow::new("Reset both to Auto"),
                RowKind::DisableBoth => KeyValueRow::new("Disable both"),
                RowKind::Apply => KeyValueRow::new("Apply").with_value(StyledText::new(
                    if is_dirty { "Pending" } else { "Saved" },
                    Style::default().fg(colors::success()),
                )),
                RowKind::Close => KeyValueRow::new("Close"),
            })
            .collect();
        let Some(layout) = SettingsRowPage::new(
            " Exec Limits ",
            self.render_header_lines(),
            self.render_footer_lines(),
        )
        .content_only()
        .render(area, buf, state.scroll_top, Some(selected_idx), &row_specs)
        else {
            return;
        };
        let visible_slots = layout.visible_rows();
        state.ensure_visible(total, visible_slots);
        self.state.set(state);
        self.viewport_rows.set(visible_slots);
    }

    fn edit_page(target: EditTarget, error: Option<&str>) -> SettingsEditorPage<'static> {
        let (title, field_title) = match target {
            EditTarget::PidsMax => (" Edit Process Limit ", "Process limit"),
            EditTarget::MemoryMax => (" Edit Memory Limit (MiB) ", "Memory limit (MiB)"),
        };
        let hint = Style::default().fg(colors::text_dim());
        let mut post_field_lines = Vec::new();
        if let Some(err) = error {
            post_field_lines.push(Line::from(Span::styled(
                err.to_string(),
                Style::default().fg(colors::error()),
            )));
        } else {
            post_field_lines.push(Line::from(Span::styled(
                "Tip: type \"auto\" or \"disabled\".",
                hint,
            )));
        }
        SettingsEditorPage::new(
            title,
            SettingsPanelStyle::bottom_pane(),
            field_title,
            vec![
                Line::from(Span::styled("Enter/Ctrl+S save · Esc cancel", hint)),
                Line::from(""),
            ],
            post_field_lines,
        )
        .with_field_margin(Margin::new(2, 0))
    }

    fn render_edit(
        &self,
        area: Rect,
        buf: &mut Buffer,
        target: EditTarget,
        field: &FormTextField,
        error: Option<&str>,
    ) {
        let _ = Self::edit_page(target, error).framed().render(area, buf, field);
    }

    fn render_edit_without_frame(
        &self,
        area: Rect,
        buf: &mut Buffer,
        target: EditTarget,
        field: &FormTextField,
        error: Option<&str>,
    ) {
        let _ = Self::edit_page(target, error)
            .content_only()
            .render(area, buf, field);
    }

    fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main_without_frame(area, buf),
            ViewMode::Edit {
                target,
                field,
                error,
            } => self.render_edit_without_frame(area, buf, *target, field, error.as_deref()),
            ViewMode::Transition => self.render_main_without_frame(area, buf),
        }
    }

    fn render_framed(&self, area: Rect, buf: &mut Buffer) {
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

impl crate::bottom_pane::chrome_view::ChromeRenderable for ExecLimitsSettingsView {
    fn render_in_framed_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_framed(area, buf);
    }

    fn render_in_content_only_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_content_only(area, buf);
    }
}

impl crate::bottom_pane::chrome_view::ChromeMouseHandler for ExecLimitsSettingsView {
    fn handle_mouse_event_direct_in_framed_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_framed(mouse_event, area)
    }

    fn handle_mouse_event_direct_in_content_only_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_content(mouse_event, area)
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
        redraw_if(
            self.framed_mut()
                .handle_mouse_event_direct(mouse_event, area),
        )
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
                let header = u16::try_from(self.render_header_lines().len()).unwrap_or(u16::MAX);
                let total_rows = Self::build_rows().len();
                let visible = u16::try_from(total_rows.clamp(1, 10)).unwrap_or(u16::MAX);
                2u16.saturating_add(header).saturating_add(visible)
            }
            ViewMode::Edit { .. } => 10,
            ViewMode::Transition => {
                let header = u16::try_from(self.render_header_lines().len()).unwrap_or(u16::MAX);
                2u16.saturating_add(header).saturating_add(6)
            }
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.framed().render(area, buf);
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

        let header_height = u16::try_from(view.render_header_lines().len()).unwrap_or(u16::MAX);
        let footer_height =
            1u16.saturating_add(u16::try_from(view.render_footer_lines().len()).unwrap_or(u16::MAX));
        let rows_len = u16::try_from(ExecLimitsSettingsView::build_rows().len()).unwrap_or(u16::MAX);
        let required_inner_height = header_height.saturating_add(footer_height).saturating_add(rows_len);

        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            // Ensure all rows are visible regardless of OS-specific header lines.
            height: required_inner_height.saturating_add(2),
        };

        let inner = Block::default().borders(Borders::ALL).inner(area);
        let apply_idx = ExecLimitsSettingsView::build_rows()
            .iter()
            .position(|row| *row == RowKind::Apply)
            .expect("apply row");
        let apply_idx = u16::try_from(apply_idx).unwrap_or(u16::MAX);

        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: inner.x.saturating_add(2),
            row: inner.y.saturating_add(header_height).saturating_add(apply_idx),
            modifiers: KeyModifiers::NONE,
        };

        assert!(view.handle_mouse_event_direct_framed(mouse_event, area));
        match rx.try_recv() {
            Ok(AppEvent::SetExecLimitsSettings(_)) => {}
            other => panic!("expected SetExecLimitsSettings event, got: {other:?}"),
        }
    }
}
