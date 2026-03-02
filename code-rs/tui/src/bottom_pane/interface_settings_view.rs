use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use code_core::config_types::{
    FunctionKeyHotkey,
    SettingsMenuConfig,
    SettingsMenuOpenMode,
    TuiHotkeysConfig,
};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use crate::util::buffer::{fill_rect, write_line};
use std::cell::Cell;
use std::path::PathBuf;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_panel::{panel_content_rect, render_panel, PanelFrameStyle};
use super::BottomPane;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    OpenMode,
    OverlayMinWidth,
    ModelSelectorHotkey,
    ReasoningEffortHotkey,
    ShellSelectorHotkey,
    NetworkSettingsHotkey,
    ShowConfigToml,
    ShowCodeHome,
    Apply,
    Close,
}

#[derive(Debug)]
enum ViewMode {
    Transition,
    Main,
    EditWidth { field: FormTextField, error: Option<String> },
}

pub(crate) struct InterfaceSettingsView {
    settings: SettingsMenuConfig,
    hotkeys: TuiHotkeysConfig,
    code_home: PathBuf,
    app_event_tx: AppEventSender,
    is_complete: bool,
    dirty: bool,
    status: Option<(String, bool)>,
    mode: ViewMode,
    state: ScrollState,
    viewport_rows: Cell<usize>,
}

impl InterfaceSettingsView {
    pub fn new(
        code_home: PathBuf,
        settings: SettingsMenuConfig,
        hotkeys: TuiHotkeysConfig,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        Self {
            settings,
            hotkeys,
            code_home,
            app_event_tx,
            is_complete: false,
            dirty: false,
            status: None,
            mode: ViewMode::Main,
            state,
            viewport_rows: Cell::new(0),
        }
    }

    fn open_mode_label(mode: SettingsMenuOpenMode) -> &'static str {
        match mode {
            SettingsMenuOpenMode::Auto => "auto",
            SettingsMenuOpenMode::Overlay => "overlay",
            SettingsMenuOpenMode::Bottom => "bottom",
        }
    }

    fn open_mode_description(mode: SettingsMenuOpenMode, width: u16) -> String {
        match mode {
            SettingsMenuOpenMode::Auto => format!("overlay when width >= {width}"),
            SettingsMenuOpenMode::Overlay => "always show overlay".to_string(),
            SettingsMenuOpenMode::Bottom => "prefer bottom pane".to_string(),
        }
    }

    fn build_rows(&self) -> [RowKind; 10] {
        [
            RowKind::OpenMode,
            RowKind::OverlayMinWidth,
            RowKind::ModelSelectorHotkey,
            RowKind::ReasoningEffortHotkey,
            RowKind::ShellSelectorHotkey,
            RowKind::NetworkSettingsHotkey,
            RowKind::ShowConfigToml,
            RowKind::ShowCodeHome,
            RowKind::Apply,
            RowKind::Close,
        ]
    }

    fn selected_row(&self) -> RowKind {
        let rows = self.build_rows();
        let idx = self.state.selected_idx.unwrap_or(0).min(rows.len().saturating_sub(1));
        rows[idx]
    }

    fn cycle_open_mode_next(&mut self) {
        self.settings.open_mode = match self.settings.open_mode {
            SettingsMenuOpenMode::Auto => SettingsMenuOpenMode::Overlay,
            SettingsMenuOpenMode::Overlay => SettingsMenuOpenMode::Bottom,
            SettingsMenuOpenMode::Bottom => SettingsMenuOpenMode::Auto,
        };
        self.dirty = true;
    }

    fn adjust_min_width(&mut self, delta: i16) {
        let current = self.settings.overlay_min_width as i16;
        let next = (current + delta).clamp(40, 300) as u16;
        if next != self.settings.overlay_min_width {
            self.settings.overlay_min_width = next;
            self.dirty = true;
        }
    }

    fn open_width_editor(&mut self) {
        let mut field = FormTextField::new_single_line();
        field.set_placeholder("100");
        field.set_text(&self.settings.overlay_min_width.to_string());
        self.mode = ViewMode::EditWidth { field, error: None };
    }

    fn save_width_editor(&mut self, field: &FormTextField) -> Result<(), String> {
        let raw = field.text().trim();
        let parsed: u16 = raw
            .parse()
            .map_err(|_| "Enter a number (columns), e.g. 100.".to_string())?;
        // Keep sane bounds so accidental paste doesn't make it unusable.
        let clamped = parsed.clamp(40, 300);
        self.settings.overlay_min_width = clamped;
        self.dirty = true;
        Ok(())
    }

    fn cycle_function_hotkey_next(hotkey: FunctionKeyHotkey) -> FunctionKeyHotkey {
        match hotkey {
            FunctionKeyHotkey::Disabled => FunctionKeyHotkey::F2,
            FunctionKeyHotkey::F2 => FunctionKeyHotkey::F3,
            FunctionKeyHotkey::F3 => FunctionKeyHotkey::F4,
            FunctionKeyHotkey::F4 => FunctionKeyHotkey::F5,
            FunctionKeyHotkey::F5 => FunctionKeyHotkey::F6,
            FunctionKeyHotkey::F6 => FunctionKeyHotkey::F7,
            FunctionKeyHotkey::F7 => FunctionKeyHotkey::F8,
            FunctionKeyHotkey::F8 => FunctionKeyHotkey::F9,
            FunctionKeyHotkey::F9 => FunctionKeyHotkey::F10,
            FunctionKeyHotkey::F10 => FunctionKeyHotkey::F11,
            FunctionKeyHotkey::F11 => FunctionKeyHotkey::F12,
            FunctionKeyHotkey::F12 => FunctionKeyHotkey::Disabled,
        }
    }

    fn cycle_function_hotkey_prev(hotkey: FunctionKeyHotkey) -> FunctionKeyHotkey {
        match hotkey {
            FunctionKeyHotkey::Disabled => FunctionKeyHotkey::F12,
            FunctionKeyHotkey::F2 => FunctionKeyHotkey::Disabled,
            FunctionKeyHotkey::F3 => FunctionKeyHotkey::F2,
            FunctionKeyHotkey::F4 => FunctionKeyHotkey::F3,
            FunctionKeyHotkey::F5 => FunctionKeyHotkey::F4,
            FunctionKeyHotkey::F6 => FunctionKeyHotkey::F5,
            FunctionKeyHotkey::F7 => FunctionKeyHotkey::F6,
            FunctionKeyHotkey::F8 => FunctionKeyHotkey::F7,
            FunctionKeyHotkey::F9 => FunctionKeyHotkey::F8,
            FunctionKeyHotkey::F10 => FunctionKeyHotkey::F9,
            FunctionKeyHotkey::F11 => FunctionKeyHotkey::F10,
            FunctionKeyHotkey::F12 => FunctionKeyHotkey::F11,
        }
    }

    fn adjust_hotkey_for_row(&mut self, row: RowKind, forward: bool) {
        let next = |hk| if forward { Self::cycle_function_hotkey_next(hk) } else { Self::cycle_function_hotkey_prev(hk) };
        match row {
            RowKind::ModelSelectorHotkey => {
                self.hotkeys.model_selector = next(self.hotkeys.model_selector);
                self.dirty = true;
            }
            RowKind::ReasoningEffortHotkey => {
                self.hotkeys.reasoning_effort = next(self.hotkeys.reasoning_effort);
                self.dirty = true;
            }
            RowKind::ShellSelectorHotkey => {
                self.hotkeys.shell_selector = next(self.hotkeys.shell_selector);
                self.dirty = true;
            }
            RowKind::NetworkSettingsHotkey => {
                self.hotkeys.network_settings = next(self.hotkeys.network_settings);
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn validate_hotkeys(&self) -> Result<(), String> {
        use std::collections::HashMap;

        let mut seen: HashMap<u8, &'static str> = HashMap::new();
        let pairs = [
            ("model_selector", self.hotkeys.model_selector),
            ("reasoning_effort", self.hotkeys.reasoning_effort),
            ("shell_selector", self.hotkeys.shell_selector),
            ("network_settings", self.hotkeys.network_settings),
        ];
        for (label, hk) in pairs {
            let Some(n) = hk.as_u8() else { continue };
            if let Some(prev) = seen.insert(n, label) {
                return Err(format!(
                    "Hotkeys must be unique (both {prev} and {label} use {key}).",
                    key = hk.display_name()
                ));
            }
        }
        Ok(())
    }

    fn apply_settings(&mut self) {
        if let Err(err) = self.validate_hotkeys() {
            self.status = Some((err, true));
            return;
        }

        self.app_event_tx
            .send(AppEvent::SetTuiSettingsMenuConfig(self.settings.clone()));
        self.app_event_tx
            .send(AppEvent::SetTuiHotkeysConfig(self.hotkeys.clone()));
        self.dirty = false;
        self.status = Some(("Saved interface settings".to_string(), false));
    }

    fn show_path(&mut self, path: &std::path::Path, label: &str) {
        match crate::native_file_manager::reveal_path(path) {
            Ok(()) => self.status = Some((format!("Opened {label} in file manager"), false)),
            Err(err) => {
                self.status = Some((format!("Failed to open {label}: {err:#}"), true));
            }
        }
    }

    fn show_config_toml(&mut self) {
        let path = self.code_home.join("config.toml");
        let target = if path.exists() { path } else { self.code_home.clone() };
        self.show_path(&target, "config.toml");
    }

    fn show_code_home(&mut self) {
        let code_home = self.code_home.clone();
        self.show_path(&code_home, "CODE_HOME");
    }

    fn activate_selected_row(&mut self) {
        match self.selected_row() {
            RowKind::OpenMode => self.cycle_open_mode_next(),
            RowKind::OverlayMinWidth => self.open_width_editor(),
            RowKind::ModelSelectorHotkey
            | RowKind::ReasoningEffortHotkey
            | RowKind::ShellSelectorHotkey
            | RowKind::NetworkSettingsHotkey => {
                let row = self.selected_row();
                self.adjust_hotkey_for_row(row, true);
            }
            RowKind::ShowConfigToml => self.show_config_toml(),
            RowKind::ShowCodeHome => self.show_code_home(),
            RowKind::Apply => self.apply_settings(),
            RowKind::Close => self.is_complete = true,
        }
    }

    fn content_area(area: Rect) -> Rect {
        panel_content_rect(area, PanelFrameStyle::bottom_pane().with_margin(Margin::new(1, 0)))
    }

    fn layout_main(area: Rect) -> Option<(Rect, Rect, Rect)> {
        let content = Self::content_area(area);
        if content.width == 0 || content.height < 3 {
            return None;
        }

        let header = Rect::new(content.x, content.y, content.width, 1);
        let footer = Rect::new(
            content.x,
            content.y.saturating_add(content.height.saturating_sub(1)),
            content.width,
            1,
        );
        let list = Rect::new(
            content.x,
            content.y.saturating_add(1),
            content.width,
            content.height.saturating_sub(2),
        );
        Some((header, list, footer))
    }

    fn selection_index_at(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        let (_header, list, _footer) = Self::layout_main(area)?;
        if list.width == 0 || list.height == 0 {
            return None;
        }
        if x < list.x || x >= list.x.saturating_add(list.width) {
            return None;
        }
        if y < list.y || y >= list.y.saturating_add(list.height) {
            return None;
        }
        let rel = y.saturating_sub(list.y) as usize;
        let rows = self.build_rows();
        let scroll_top = self.state.scroll_top.min(rows.len().saturating_sub(1));
        let actual = scroll_top.saturating_add(rel);
        if actual >= rows.len() {
            return None;
        }
        Some(actual)
    }

    fn handle_mouse_event_main(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let rows = self.build_rows();
        let total = rows.len();
        if total == 0 {
            return false;
        }

        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        let mut selected = self.state.selected_idx.unwrap_or(0);
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

        match result {
            SelectableListMouseResult::Ignored => false,
            SelectableListMouseResult::SelectionChanged => {
                self.state.selected_idx = Some(selected);
                let visible = self.viewport_rows.get().max(1);
                self.state.ensure_visible(total, visible);
                true
            }
            SelectableListMouseResult::Activated => {
                self.state.selected_idx = Some(selected);
                let visible = self.viewport_rows.get().max(1);
                self.state.ensure_visible(total, visible);
                self.activate_selected_row();
                true
            }
        }
    }

    fn handle_mouse_event_edit(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let content = Self::content_area(area);
        if content.width == 0 || content.height < 4 {
            return false;
        }

        // Layout: label (1), field (1), hint (1), optional error line (1).
        let label_y = content.y;
        let field_y = label_y.saturating_add(1);
        let field_area = Rect::new(content.x, field_y, content.width, 1);

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let ViewMode::EditWidth { field, .. } = &mut self.mode
                    && mouse_event.row == field_area.y
                    && mouse_event.column >= field_area.x
                    && mouse_event.column < field_area.x.saturating_add(field_area.width)
                {
                    return field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area);
                }
                false
            }
            _ => false,
        }
    }

    fn process_key_event_main(&mut self, key_event: KeyEvent) -> bool {
        let rows = self.build_rows();
        let total = rows.len();
        if total == 0 {
            if matches!(key_event.code, KeyCode::Esc) {
                self.is_complete = true;
                return true;
            }
            return false;
        }

        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        let selected = self.state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
        let current_row = rows.get(selected).copied();

        let visible = self.viewport_rows.get().max(1);

        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                self.status = None;
                self.state.move_up_wrap_visible(total, visible);
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                self.status = None;
                self.state.move_down_wrap_visible(total, visible);
                true
            }
            KeyEvent { code: KeyCode::Left, modifiers: KeyModifiers::NONE, .. } => {
                self.status = None;
                match current_row {
                    Some(RowKind::OpenMode) => {
                        // Reverse cycle.
                        self.settings.open_mode = match self.settings.open_mode {
                            SettingsMenuOpenMode::Auto => SettingsMenuOpenMode::Bottom,
                            SettingsMenuOpenMode::Overlay => SettingsMenuOpenMode::Auto,
                            SettingsMenuOpenMode::Bottom => SettingsMenuOpenMode::Overlay,
                        };
                        self.dirty = true;
                    }
                    Some(RowKind::OverlayMinWidth) => self.adjust_min_width(-5),
                    Some(RowKind::ModelSelectorHotkey)
                    | Some(RowKind::ReasoningEffortHotkey)
                    | Some(RowKind::ShellSelectorHotkey)
                    | Some(RowKind::NetworkSettingsHotkey) => {
                        self.adjust_hotkey_for_row(current_row.unwrap(), false);
                    }
                    _ => {}
                }
                true
            }
            KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE, .. } => {
                self.status = None;
                match current_row {
                    Some(RowKind::OpenMode) => self.cycle_open_mode_next(),
                    Some(RowKind::OverlayMinWidth) => self.adjust_min_width(5),
                    Some(RowKind::ModelSelectorHotkey)
                    | Some(RowKind::ReasoningEffortHotkey)
                    | Some(RowKind::ShellSelectorHotkey)
                    | Some(RowKind::NetworkSettingsHotkey) => {
                        self.adjust_hotkey_for_row(current_row.unwrap(), true);
                    }
                    _ => {}
                }
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
            | KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                if current_row.is_some() {
                    self.activate_selected_row();
                    self.state.ensure_visible(total, visible);
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }

    fn process_key_event_edit(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.mode = ViewMode::Main;
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
                if let ViewMode::EditWidth { field, .. } = mode {
                    match self.save_width_editor(&field) {
                        Ok(()) => self.mode = ViewMode::Main,
                        Err(err) => self.mode = ViewMode::EditWidth {
                            field,
                            error: Some(err),
                        },
                    }
                } else {
                    self.mode = ViewMode::Main;
                }
                true
            }
            KeyEvent { code: KeyCode::Char('s'), modifiers, .. }
                if modifiers.contains(KeyModifiers::CONTROL) =>
            {
                let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
                if let ViewMode::EditWidth { field, .. } = mode {
                    match self.save_width_editor(&field) {
                        Ok(()) => self.mode = ViewMode::Main,
                        Err(err) => self.mode = ViewMode::EditWidth {
                            field,
                            error: Some(err),
                        },
                    }
                } else {
                    self.mode = ViewMode::Main;
                }
                true
            }
            _ => match &mut self.mode {
                ViewMode::EditWidth { field, .. } => field.handle_key(key_event),
                ViewMode::Main | ViewMode::Transition => false,
            },
        }
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main => {
                self.mode = ViewMode::Main;
                self.process_key_event_main(key_event)
            }
            ViewMode::EditWidth { field, error } => {
                self.mode = ViewMode::EditWidth { field, error };
                self.process_key_event_edit(key_event)
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
            ViewMode::EditWidth { field, .. } => {
                field.handle_paste(text);
                true
            }
            ViewMode::Main | ViewMode::Transition => false,
        }
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match &self.mode {
            ViewMode::Main => self.handle_mouse_event_main(mouse_event, area),
            ViewMode::EditWidth { .. } => self.handle_mouse_event_edit(mouse_event, area),
            ViewMode::Transition => false,
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn help_for(row: RowKind) -> &'static str {
        match row {
            RowKind::OpenMode => "Auto uses overlay on wide terminals; override with overlay/bottom.",
            RowKind::OverlayMinWidth => "Terminal width (columns) at which auto prefers overlay.",
            RowKind::ModelSelectorHotkey => "Hotkey for opening model selector (F2-F12 or disabled).",
            RowKind::ReasoningEffortHotkey => "Hotkey for cycling reasoning effort (F2-F12 or disabled).",
            RowKind::ShellSelectorHotkey => "Hotkey for opening shell selector (F2-F12 or disabled).",
            RowKind::NetworkSettingsHotkey => "Hotkey for opening Settings -> Network (F2-F12 or disabled).",
            RowKind::ShowConfigToml => "Open config.toml in your file manager (Finder/Explorer).",
            RowKind::ShowCodeHome => "Open CODE_HOME in your file manager.",
            RowKind::Apply => "Persist these preferences to config.toml.",
            RowKind::Close => "Close this panel.",
        }
    }

    fn render_main(&self, area: Rect, buf: &mut Buffer) {
        let style = PanelFrameStyle::bottom_pane().with_margin(Margin::new(1, 0));
        render_panel(area, buf, "Interface", style, |_content, buf| {
            let Some((header, list, footer)) = Self::layout_main(area) else {
                return;
            };

            let header_line = Line::from(vec![
                Span::styled("Enter", Style::default().fg(crate::colors::function())),
                Span::styled(" activate  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("←/→", Style::default().fg(crate::colors::function())),
                Span::styled(" adjust  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Esc", Style::default().fg(crate::colors::function())),
                Span::styled(" close", Style::default().fg(crate::colors::text_dim())),
            ]);
            write_line(
                buf,
                header.x,
                header.y,
                header.width,
                &header_line,
                Style::default().bg(crate::colors::background()),
            );

            let rows = self.build_rows();
            let total = rows.len();
            let selected = self.state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
            let scroll_top = self.state.scroll_top.min(total.saturating_sub(1));
            let visible_rows = list.height.max(1) as usize;
            self.viewport_rows.set(visible_rows);

            let mut abs_idx = scroll_top;
            let mut rel_idx = 0usize;
            while rel_idx < visible_rows && abs_idx < rows.len() {
                let kind = rows[abs_idx];
                let y = list.y.saturating_add(rel_idx as u16);
                let row_area = Rect::new(list.x, y, list.width, 1);
                let is_selected = abs_idx == selected;
                let base = if is_selected {
                    Style::default()
                        .bg(crate::colors::selection())
                        .fg(crate::colors::text_bright())
                } else {
                    Style::default().bg(crate::colors::background()).fg(crate::colors::text())
                };
                fill_rect(buf, row_area, Some(' '), base);

                let (label, value) = match kind {
                    RowKind::OpenMode => {
                        let mode = Self::open_mode_label(self.settings.open_mode);
                        let desc = Self::open_mode_description(
                            self.settings.open_mode,
                            self.settings.overlay_min_width,
                        );
                        ("Settings menu", format!("{mode} ({desc})"))
                    }
                    RowKind::OverlayMinWidth => (
                        "Overlay min width",
                        format!("{}", self.settings.overlay_min_width),
                    ),
                    RowKind::ModelSelectorHotkey => (
                        "Hotkey: model selector",
                        self.hotkeys.model_selector.display_name().to_string(),
                    ),
                    RowKind::ReasoningEffortHotkey => (
                        "Hotkey: reasoning effort",
                        self.hotkeys.reasoning_effort.display_name().to_string(),
                    ),
                    RowKind::ShellSelectorHotkey => (
                        "Hotkey: shell selector",
                        self.hotkeys.shell_selector.display_name().to_string(),
                    ),
                    RowKind::NetworkSettingsHotkey => (
                        "Hotkey: network settings",
                        self.hotkeys.network_settings.display_name().to_string(),
                    ),
                    RowKind::ShowConfigToml => ("Show config.toml", String::new()),
                    RowKind::ShowCodeHome => ("Show CODE_HOME", String::new()),
                    RowKind::Apply => {
                        let suffix = if self.dirty { " *" } else { "" };
                        ("Apply", suffix.to_string())
                    }
                    RowKind::Close => ("Close", String::new()),
                };

                let mut parts = vec![
                    Span::styled(
                        format!("{} {label}: ", if is_selected { ">" } else { " " }),
                        base.add_modifier(Modifier::BOLD),
                    ),
                ];
                if !value.is_empty() {
                    let value_style = if is_selected {
                        base.add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .bg(crate::colors::background())
                            .fg(crate::colors::text_dim())
                    };
                    parts.push(Span::styled(value, value_style));
                }
                let line = Line::from(parts);
                write_line(buf, row_area.x, row_area.y, row_area.width, &line, base);

                abs_idx = abs_idx.saturating_add(1);
                rel_idx = rel_idx.saturating_add(1);
            }

            let (footer_text, footer_style) = if let Some((status, is_error)) = self.status.as_ref()
            {
                let style = if *is_error {
                    Style::default().fg(crate::colors::error())
                } else {
                    Style::default().fg(crate::colors::text_dim())
                };
                (status.as_str(), style)
            } else {
                (
                    Self::help_for(self.selected_row()),
                    Style::default().fg(crate::colors::text_dim()),
                )
            };
            let footer_line = Line::from(vec![Span::styled(footer_text.to_string(), footer_style)]);
            write_line(
                buf,
                footer.x,
                footer.y,
                footer.width,
                &footer_line,
                Style::default().bg(crate::colors::background()),
            );
        });
    }

    fn render_edit_width(&self, area: Rect, buf: &mut Buffer, field: &FormTextField, error: Option<&str>) {
        let style = PanelFrameStyle::bottom_pane().with_margin(Margin::new(1, 0));
        render_panel(area, buf, "Interface", style, |content, buf| {
            if content.width == 0 || content.height == 0 {
                return;
            }
            let base = Style::default().bg(crate::colors::background()).fg(crate::colors::text());

            let label = Line::from(vec![Span::styled(
                "Overlay min width (columns):".to_string(),
                Style::default().fg(crate::colors::text()),
            )]);
            write_line(buf, content.x, content.y, content.width, &label, base);

            let field_area = Rect::new(content.x, content.y.saturating_add(1), content.width, 1);
            fill_rect(buf, field_area, Some(' '), base);
            field.render(field_area, buf, true);

            let hint_y = content.y.saturating_add(3);
            let hint_line = Line::from(vec![
                Span::styled("Enter", Style::default().fg(crate::colors::function())),
                Span::styled("/", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Ctrl+S", Style::default().fg(crate::colors::function())),
                Span::styled(" save  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Esc", Style::default().fg(crate::colors::function())),
                Span::styled(" cancel", Style::default().fg(crate::colors::text_dim())),
            ]);
            if hint_y < content.y.saturating_add(content.height) {
                write_line(buf, content.x, hint_y, content.width, &hint_line, base);
            }

            if let Some(error) = error {
                let err_y = content.y.saturating_add(2);
                if err_y < content.y.saturating_add(content.height) {
                    let err_line = Line::from(vec![Span::styled(
                        error.to_string(),
                        Style::default().fg(crate::colors::warning()),
                    )]);
                    write_line(buf, content.x, err_y, content.width, &err_line, base);
                }
            }
        });
    }
}

impl<'a> BottomPaneView<'a> for InterfaceSettingsView {
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
                let base = self.build_rows().len() as u16 + 4;
                base.max(12).min(20)
            }
            ViewMode::EditWidth { .. } => 8,
            ViewMode::Transition => 8,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main(area, buf),
            ViewMode::EditWidth { field, error } => {
                self.render_edit_width(area, buf, field, error.as_deref())
            }
            ViewMode::Transition => self.render_main(area, buf),
        }
    }
}
