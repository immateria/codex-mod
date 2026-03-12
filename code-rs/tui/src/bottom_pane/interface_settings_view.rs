use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyEventKind,
    KeyModifiers,
    MouseButton,
    MouseEvent,
    MouseEventKind,
};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use code_core::config_types::{
    FunctionKeyHotkey,
    ResolvedTuiHotkeys,
    SettingsMenuConfig,
    SettingsMenuOpenMode,
    TuiHotkey,
    TuiHotkeysConfig,
    TuiHotkeysEnv,
    TuiHotkeysOverrides,
    TuiHotkeysPlatform,
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
use std::cell::Cell;
use std::path::PathBuf;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::editor_page::SettingsEditorPage;
use super::settings_ui::hints::{shortcut_line, KeyHint};
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::menu_rows::SettingsMenuRow;
use super::settings_ui::message_page::SettingsMessagePage;
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::rows::{selection_index_at, StyledText};
use super::BottomPane;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    OpenMode,
    OverlayMinWidth,
    HotkeyScope,
    ModelSelectorHotkey,
    ReasoningEffortHotkey,
    ShellSelectorHotkey,
    NetworkSettingsHotkey,
    ExecOutputFoldHotkey,
    JsReplCodeFoldHotkey,
    JumpToParentCallHotkey,
    JumpToLatestChildCallHotkey,
    ShowConfigToml,
    ShowCodeHome,
    Apply,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HotkeyScope {
    Global,
    Macos,
    Windows,
    Linux,
    Android,
    Termux,
    FreeBsd,
    OpenBsd,
    NetBsd,
    Dragonfly,
}

impl HotkeyScope {
    fn label(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Android => "android",
            Self::Termux => "termux",
            Self::FreeBsd => "freebsd",
            Self::OpenBsd => "openbsd",
            Self::NetBsd => "netbsd",
            Self::Dragonfly => "dragonfly",
        }
    }

    fn env(self) -> TuiHotkeysEnv {
        match self {
            Self::Global => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Other,
                termux: false,
            },
            Self::Macos => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Macos,
                termux: false,
            },
            Self::Windows => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Windows,
                termux: false,
            },
            Self::Linux => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Linux,
                termux: false,
            },
            Self::Android => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Android,
                termux: false,
            },
            Self::Termux => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Android,
                termux: true,
            },
            Self::FreeBsd => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::FreeBsd,
                termux: false,
            },
            Self::OpenBsd => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::OpenBsd,
                termux: false,
            },
            Self::NetBsd => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::NetBsd,
                termux: false,
            },
            Self::Dragonfly => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Dragonfly,
                termux: false,
            },
        }
    }

    fn max_function_key(self) -> u8 {
        match self {
            Self::Macos => 20,
            _ => 24,
        }
    }

    fn platform_override(self) -> Option<TuiHotkeysPlatform> {
        match self {
            Self::Global | Self::Termux => None,
            Self::Macos => Some(TuiHotkeysPlatform::Macos),
            Self::Windows => Some(TuiHotkeysPlatform::Windows),
            Self::Linux => Some(TuiHotkeysPlatform::Linux),
            Self::Android => Some(TuiHotkeysPlatform::Android),
            Self::FreeBsd => Some(TuiHotkeysPlatform::FreeBsd),
            Self::OpenBsd => Some(TuiHotkeysPlatform::OpenBsd),
            Self::NetBsd => Some(TuiHotkeysPlatform::NetBsd),
            Self::Dragonfly => Some(TuiHotkeysPlatform::Dragonfly),
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Global => Self::Macos,
            Self::Macos => Self::Windows,
            Self::Windows => Self::Linux,
            Self::Linux => Self::Android,
            Self::Android => Self::Termux,
            Self::Termux => Self::FreeBsd,
            Self::FreeBsd => Self::OpenBsd,
            Self::OpenBsd => Self::NetBsd,
            Self::NetBsd => Self::Dragonfly,
            Self::Dragonfly => Self::Global,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Global => Self::Dragonfly,
            Self::Macos => Self::Global,
            Self::Windows => Self::Macos,
            Self::Linux => Self::Windows,
            Self::Android => Self::Linux,
            Self::Termux => Self::Android,
            Self::FreeBsd => Self::Termux,
            Self::OpenBsd => Self::FreeBsd,
            Self::NetBsd => Self::OpenBsd,
            Self::Dragonfly => Self::NetBsd,
        }
    }
}

#[derive(Debug)]
enum ViewMode {
    Transition,
    Main,
    EditWidth { field: FormTextField, error: Option<String> },
    CaptureHotkey { row: RowKind, error: Option<String> },
}

pub(crate) struct InterfaceSettingsView {
    settings: SettingsMenuConfig,
    hotkeys: TuiHotkeysConfig,
    hotkey_scope: HotkeyScope,
    code_home: PathBuf,
    app_event_tx: AppEventSender,
    is_complete: bool,
    dirty_settings: bool,
    dirty_hotkeys: bool,
    status: Option<(String, bool)>,
    mode: ViewMode,
    state: ScrollState,
    viewport_rows: Cell<usize>,
}

impl InterfaceSettingsView {
    fn panel_style() -> SettingsPanelStyle {
        SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0))
    }

    fn main_page(&self) -> SettingsMenuPage<'static> {
        let header_lines = vec![shortcut_line(&[
            KeyHint::new("↑↓", " navigate").with_key_style(Style::new().fg(crate::colors::function())),
            KeyHint::new("Enter", " activate").with_key_style(Style::new().fg(crate::colors::success())),
            KeyHint::new("←→", " adjust").with_key_style(Style::new().fg(crate::colors::function())),
            KeyHint::new("Esc", " close").with_key_style(Style::new().fg(crate::colors::error()).bold()),
        ])];
        let footer_lines = vec![self.main_footer_line()];
        SettingsMenuPage::new("Interface", Self::panel_style(), header_lines, footer_lines)
    }

    fn edit_width_page(error: Option<&str>) -> SettingsEditorPage<'static> {
        let mut post_field_lines = Vec::new();
        if let Some(error) = error {
            post_field_lines.push(Line::from(Span::styled(
                error.to_string(),
                Style::new().fg(crate::colors::warning()),
            )));
        }
        post_field_lines.push(shortcut_line(&[
            KeyHint::new("Enter", " save").with_key_style(Style::new().fg(crate::colors::success())),
            KeyHint::new("Ctrl+S", " save").with_key_style(Style::new().fg(crate::colors::success())),
            KeyHint::new("Esc", " cancel").with_key_style(Style::new().fg(crate::colors::error()).bold()),
        ]));

        SettingsEditorPage::new(
            "Interface",
            Self::panel_style(),
            "Overlay min width",
            Vec::new(),
            post_field_lines,
        )
    }

    fn capture_hotkey_page(&self, row: RowKind, error: Option<&str>) -> SettingsMessagePage<'static> {
        let label = match row {
            RowKind::ModelSelectorHotkey => "Hotkey: model selector",
            RowKind::ReasoningEffortHotkey => "Hotkey: reasoning effort",
            RowKind::ShellSelectorHotkey => "Hotkey: shell selector",
            RowKind::NetworkSettingsHotkey => "Hotkey: network settings",
            RowKind::ExecOutputFoldHotkey => "Hotkey: fold output/details",
            RowKind::JsReplCodeFoldHotkey => "Hotkey: fold JS REPL code",
            RowKind::JumpToParentCallHotkey => "Hotkey: jump to parent call",
            RowKind::JumpToLatestChildCallHotkey => "Hotkey: jump to child call",
            _ => "Hotkey",
        };
        let current = self.hotkey_value_label_for_row(row);
        let header_lines = vec![Line::from(Span::styled(
            format!("{label} (current: {current})"),
            Style::new().fg(crate::colors::text()),
        ))];

        let mut body_lines = Vec::new();
        if let Some(error) = error {
            body_lines.push(Line::from(Span::styled(
                error.to_string(),
                Style::new().fg(crate::colors::warning()),
            )));
            body_lines.push(Line::from(""));
        }

        let inherit_hint = match self.hotkey_scope {
            HotkeyScope::Global => None,
            _ => Some(KeyHint::new("i", " inherit").with_key_style(Style::new().fg(crate::colors::function()))),
        };
        let legacy_hint = match row {
            RowKind::ExecOutputFoldHotkey
            | RowKind::JsReplCodeFoldHotkey
            | RowKind::JumpToParentCallHotkey
            | RowKind::JumpToLatestChildCallHotkey => Some(KeyHint::new("l", " legacy").with_key_style(Style::new().fg(crate::colors::function()))),
            _ => None,
        };

        let max_key = self.hotkey_scope.max_function_key();
        body_lines.push(Line::from(Span::styled(
            format!("Press F2-F{max_key} or Ctrl/Alt+letter (e.g. ctrl+h)."),
            Style::new().fg(crate::colors::text_dim()),
        )));

        let mut footer_hints = vec![
            KeyHint::new("Esc", " cancel").with_key_style(Style::new().fg(crate::colors::error()).bold()),
            KeyHint::new("d", " disable").with_key_style(Style::new().fg(crate::colors::function())),
        ];
        if let Some(hint) = legacy_hint {
            footer_hints.push(hint);
        }
        if let Some(hint) = inherit_hint {
            footer_hints.push(hint);
        }
        let footer_lines = vec![shortcut_line(&footer_hints)];

        SettingsMessagePage::new(
            "Interface",
            Self::panel_style(),
            header_lines,
            body_lines,
            footer_lines,
        )
        .with_min_body_rows(3)
    }

    fn main_footer_line(&self) -> Line<'static> {
        let (footer_text, footer_style) = if let Some((status, is_error)) = self.status.as_ref() {
            let style = if *is_error {
                Style::new().fg(crate::colors::error())
            } else {
                Style::new().fg(crate::colors::text_dim())
            };
            (status.as_str(), style)
        } else {
            (
                Self::help_for(self.selected_row()),
                Style::new().fg(crate::colors::text_dim()),
            )
        };
        Line::from(vec![Span::styled(footer_text.to_string(), footer_style)])
    }

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
            hotkey_scope: HotkeyScope::Global,
            code_home,
            app_event_tx,
            is_complete: false,
            dirty_settings: false,
            dirty_hotkeys: false,
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

    fn build_rows(&self) -> [RowKind; 15] {
        [
            RowKind::OpenMode,
            RowKind::OverlayMinWidth,
            RowKind::HotkeyScope,
            RowKind::ModelSelectorHotkey,
            RowKind::ReasoningEffortHotkey,
            RowKind::ShellSelectorHotkey,
            RowKind::NetworkSettingsHotkey,
            RowKind::ExecOutputFoldHotkey,
            RowKind::JsReplCodeFoldHotkey,
            RowKind::JumpToParentCallHotkey,
            RowKind::JumpToLatestChildCallHotkey,
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
        self.dirty_settings = true;
    }

    fn adjust_min_width(&mut self, delta: i16) {
        let current = self.settings.overlay_min_width as i16;
        let next = (current + delta).clamp(40, 300) as u16;
        if next != self.settings.overlay_min_width {
            self.settings.overlay_min_width = next;
            self.dirty_settings = true;
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
        self.dirty_settings = true;
        Ok(())
    }

    fn cycle_hotkey_scope_next(&mut self) {
        self.hotkey_scope = self.hotkey_scope.next();
    }

    fn cycle_hotkey_scope_prev(&mut self) {
        self.hotkey_scope = self.hotkey_scope.prev();
    }

    fn prune_empty_overrides(overrides: &mut Option<TuiHotkeysOverrides>) {
        let Some(value) = overrides.as_ref() else {
            return;
        };
        if value.model_selector.is_none()
            && value.reasoning_effort.is_none()
            && value.shell_selector.is_none()
            && value.network_settings.is_none()
        {
            *overrides = None;
        }
    }

    fn cycle_function_hotkey_next(hotkey: FunctionKeyHotkey, max_key: u8) -> FunctionKeyHotkey {
        match hotkey {
            FunctionKeyHotkey::Disabled => FunctionKeyHotkey::F2,
            _ => match hotkey.as_u8() {
                Some(n) if n < 2 || n >= max_key => FunctionKeyHotkey::Disabled,
                Some(n) => FunctionKeyHotkey::from_u8(n.saturating_add(1))
                    .unwrap_or(FunctionKeyHotkey::Disabled),
                None => FunctionKeyHotkey::Disabled,
            },
        }
    }

    fn cycle_function_hotkey_prev(hotkey: FunctionKeyHotkey, max_key: u8) -> FunctionKeyHotkey {
        match hotkey {
            FunctionKeyHotkey::Disabled => {
                FunctionKeyHotkey::from_u8(max_key).unwrap_or(FunctionKeyHotkey::Disabled)
            }
            _ => match hotkey.as_u8() {
                Some(n) if n <= 2 => FunctionKeyHotkey::Disabled,
                Some(n) => FunctionKeyHotkey::from_u8(n.saturating_sub(1))
                    .unwrap_or(FunctionKeyHotkey::Disabled),
                None => FunctionKeyHotkey::Disabled,
            },
        }
    }

    fn cycle_hotkey_next(hotkey: TuiHotkey, max_key: u8) -> TuiHotkey {
        match hotkey {
            TuiHotkey::Function(hk) => TuiHotkey::Function(Self::cycle_function_hotkey_next(hk, max_key)),
            // Cycling is function-key focused. Chords are set via the capture UI.
            TuiHotkey::Chord(_) | TuiHotkey::Legacy => TuiHotkey::Function(FunctionKeyHotkey::Disabled),
        }
    }

    fn cycle_hotkey_prev(hotkey: TuiHotkey, max_key: u8) -> TuiHotkey {
        match hotkey {
            TuiHotkey::Function(hk) => TuiHotkey::Function(Self::cycle_function_hotkey_prev(hk, max_key)),
            // Cycling is function-key focused. Chords are set via the capture UI.
            TuiHotkey::Chord(_) | TuiHotkey::Legacy => TuiHotkey::Function(FunctionKeyHotkey::Disabled),
        }
    }

    fn cycle_optional_hotkey_next(hotkey: Option<TuiHotkey>, max_key: u8) -> Option<TuiHotkey> {
        match hotkey {
            None => Some(TuiHotkey::disabled()),
            Some(hk) if hk.is_disabled() => Some(TuiHotkey::Function(FunctionKeyHotkey::F2)),
            Some(TuiHotkey::Legacy) => Some(TuiHotkey::disabled()),
            Some(TuiHotkey::Function(value)) => match value.as_u8() {
                Some(n) if n < 2 => None,
                Some(n) if n >= max_key => None,
                Some(n) => FunctionKeyHotkey::from_u8(n.saturating_add(1)).map(TuiHotkey::Function),
                None => None,
            },
            Some(TuiHotkey::Chord(_)) => Some(TuiHotkey::disabled()),
        }
    }

    fn cycle_optional_hotkey_prev(hotkey: Option<TuiHotkey>, max_key: u8) -> Option<TuiHotkey> {
        match hotkey {
            None => FunctionKeyHotkey::from_u8(max_key).map(TuiHotkey::Function),
            Some(hk) if hk.is_disabled() => None,
            Some(TuiHotkey::Legacy) => None,
            Some(TuiHotkey::Function(value)) => match value.as_u8() {
                Some(n) if n <= 2 => Some(TuiHotkey::disabled()),
                Some(n) => FunctionKeyHotkey::from_u8(n.saturating_sub(1)).map(TuiHotkey::Function),
                None => None,
            },
            Some(TuiHotkey::Chord(_)) => Some(TuiHotkey::disabled()),
        }
    }

    fn adjust_overrides_for_row(
        overrides: &mut Option<TuiHotkeysOverrides>,
        row: RowKind,
        forward: bool,
        max_key: u8,
    ) -> bool {
        let next = |hk| {
            if forward {
                Self::cycle_optional_hotkey_next(hk, max_key)
            } else {
                Self::cycle_optional_hotkey_prev(hk, max_key)
            }
        };

        let mut changed = false;
        match row {
            RowKind::ModelSelectorHotkey => {
                let table = overrides.get_or_insert_with(TuiHotkeysOverrides::default);
                table.model_selector = next(table.model_selector);
                changed = true;
            }
            RowKind::ReasoningEffortHotkey => {
                let table = overrides.get_or_insert_with(TuiHotkeysOverrides::default);
                table.reasoning_effort = next(table.reasoning_effort);
                changed = true;
            }
            RowKind::ShellSelectorHotkey => {
                let table = overrides.get_or_insert_with(TuiHotkeysOverrides::default);
                table.shell_selector = next(table.shell_selector);
                changed = true;
            }
            RowKind::NetworkSettingsHotkey => {
                let table = overrides.get_or_insert_with(TuiHotkeysOverrides::default);
                table.network_settings = next(table.network_settings);
                changed = true;
            }
            RowKind::ExecOutputFoldHotkey => {
                let table = overrides.get_or_insert_with(TuiHotkeysOverrides::default);
                table.exec_output_fold = next(table.exec_output_fold);
                changed = true;
            }
            RowKind::JsReplCodeFoldHotkey => {
                let table = overrides.get_or_insert_with(TuiHotkeysOverrides::default);
                table.js_repl_code_fold = next(table.js_repl_code_fold);
                changed = true;
            }
            RowKind::JumpToParentCallHotkey => {
                let table = overrides.get_or_insert_with(TuiHotkeysOverrides::default);
                table.jump_to_parent_call = next(table.jump_to_parent_call);
                changed = true;
            }
            RowKind::JumpToLatestChildCallHotkey => {
                let table = overrides.get_or_insert_with(TuiHotkeysOverrides::default);
                table.jump_to_latest_child_call = next(table.jump_to_latest_child_call);
                changed = true;
            }
            _ => {}
        }

        if changed {
            Self::prune_empty_overrides(overrides);
        }
        changed
    }

    fn adjust_hotkey_for_row(&mut self, row: RowKind, forward: bool) {
        let max_key = self.hotkey_scope.max_function_key();

        match self.hotkey_scope {
            HotkeyScope::Global => {
                let next = |hk| {
                    if forward {
                        Self::cycle_hotkey_next(hk, max_key)
                    } else {
                        Self::cycle_hotkey_prev(hk, max_key)
                    }
                };
                match row {
                    RowKind::ModelSelectorHotkey => {
                        self.hotkeys.model_selector = next(self.hotkeys.model_selector);
                        self.dirty_hotkeys = true;
                    }
                    RowKind::ReasoningEffortHotkey => {
                        self.hotkeys.reasoning_effort = next(self.hotkeys.reasoning_effort);
                        self.dirty_hotkeys = true;
                    }
                    RowKind::ShellSelectorHotkey => {
                        self.hotkeys.shell_selector = next(self.hotkeys.shell_selector);
                        self.dirty_hotkeys = true;
                    }
                    RowKind::NetworkSettingsHotkey => {
                        self.hotkeys.network_settings = next(self.hotkeys.network_settings);
                        self.dirty_hotkeys = true;
                    }
                    RowKind::ExecOutputFoldHotkey => {
                        self.hotkeys.exec_output_fold = next(self.hotkeys.exec_output_fold);
                        self.dirty_hotkeys = true;
                    }
                    RowKind::JsReplCodeFoldHotkey => {
                        self.hotkeys.js_repl_code_fold = next(self.hotkeys.js_repl_code_fold);
                        self.dirty_hotkeys = true;
                    }
                    RowKind::JumpToParentCallHotkey => {
                        self.hotkeys.jump_to_parent_call = next(self.hotkeys.jump_to_parent_call);
                        self.dirty_hotkeys = true;
                    }
                    RowKind::JumpToLatestChildCallHotkey => {
                        self.hotkeys.jump_to_latest_child_call =
                            next(self.hotkeys.jump_to_latest_child_call);
                        self.dirty_hotkeys = true;
                    }
                    _ => {}
                }
            }
            HotkeyScope::Termux => {
                let changed =
                    Self::adjust_overrides_for_row(&mut self.hotkeys.termux, row, forward, max_key);
                if changed {
                    self.dirty_hotkeys = true;
                }
            }
            scope => {
                let Some(platform) = scope.platform_override() else {
                    return;
                };
                let changed = {
                    let Some(overrides) = self.hotkeys.overrides_for_platform_mut(platform) else {
                        return;
                    };
                    Self::adjust_overrides_for_row(overrides, row, forward, max_key)
                };
                if changed {
                    self.dirty_hotkeys = true;
                }
            }
        }
    }

    fn validate_hotkeys_for_env(
        &self,
        label: &str,
        env: TuiHotkeysEnv,
        max_key: u8,
    ) -> Result<(), String> {
        use std::collections::HashMap;

        let resolved = self.hotkeys.resolved_for_env(env);
        let pairs = [
            ("model_selector", resolved.model_selector, false),
            ("reasoning_effort", resolved.reasoning_effort, false),
            ("shell_selector", resolved.shell_selector, false),
            ("network_settings", resolved.network_settings, false),
            ("exec_output_fold", resolved.exec_output_fold, true),
            ("js_repl_code_fold", resolved.js_repl_code_fold, true),
            ("jump_to_parent_call", resolved.jump_to_parent_call, true),
            (
                "jump_to_latest_child_call",
                resolved.jump_to_latest_child_call,
                true,
            ),
        ];

        let mut seen: HashMap<TuiHotkey, &'static str> = HashMap::new();
        for (field, hk, allow_legacy) in pairs {
            if hk.is_legacy() {
                if allow_legacy {
                    continue;
                }
                return Err(format!(
                    "{label}: {field} does not support the legacy mapping."
                ));
            }
            if matches!(hk.function_key(), Some(FunctionKeyHotkey::F1)) {
                return Err(format!("{label}: F1 is reserved for the Help overlay."));
            }
            if hk.is_disabled() {
                continue;
            }
            if let Some(fk) = hk.function_key() {
                let Some(n) = fk.as_u8() else {
                    continue;
                };
                if n > max_key {
                    let key = hk.display_name();
                    return Err(format!(
                        "{label}: {field} uses {key}, but this platform supports up to F{max_key}.",
                        key = key.as_ref()
                    ));
                }
            }
            if hk.is_reserved_for_statusline_shortcuts() {
                let key = hk.display_name();
                return Err(format!("{label}: {key} is reserved and cannot be remapped.", key = key.as_ref()));
            }
            if let Some(prev) = seen.insert(hk, field) {
                let key = hk.display_name();
                return Err(format!(
                    "{label}: hotkeys must be unique (both {prev} and {field} use {key}).",
                    key = key.as_ref()
                ));
            }
        }
        Ok(())
    }

    fn validate_hotkeys(&self) -> Result<(), String> {
        let global_max = HotkeyScope::Global.max_function_key();
        self.validate_hotkeys_for_env("global", HotkeyScope::Global.env(), global_max)?;

        if self.hotkeys.macos.is_some() {
            self.validate_hotkeys_for_env(
                "macos",
                HotkeyScope::Macos.env(),
                HotkeyScope::Macos.max_function_key(),
            )?;
        }
        if self.hotkeys.windows.is_some() {
            self.validate_hotkeys_for_env(
                "windows",
                HotkeyScope::Windows.env(),
                HotkeyScope::Windows.max_function_key(),
            )?;
        }
        if self.hotkeys.linux.is_some() {
            self.validate_hotkeys_for_env(
                "linux",
                HotkeyScope::Linux.env(),
                HotkeyScope::Linux.max_function_key(),
            )?;
        }
        if self.hotkeys.android.is_some() {
            self.validate_hotkeys_for_env(
                "android",
                HotkeyScope::Android.env(),
                HotkeyScope::Android.max_function_key(),
            )?;
        }
        if self.hotkeys.termux.is_some() {
            self.validate_hotkeys_for_env(
                "termux",
                HotkeyScope::Termux.env(),
                HotkeyScope::Termux.max_function_key(),
            )?;
        }
        if self.hotkeys.freebsd.is_some() {
            self.validate_hotkeys_for_env(
                "freebsd",
                HotkeyScope::FreeBsd.env(),
                HotkeyScope::FreeBsd.max_function_key(),
            )?;
        }
        if self.hotkeys.openbsd.is_some() {
            self.validate_hotkeys_for_env(
                "openbsd",
                HotkeyScope::OpenBsd.env(),
                HotkeyScope::OpenBsd.max_function_key(),
            )?;
        }
        if self.hotkeys.netbsd.is_some() {
            self.validate_hotkeys_for_env(
                "netbsd",
                HotkeyScope::NetBsd.env(),
                HotkeyScope::NetBsd.max_function_key(),
            )?;
        }
        if self.hotkeys.dragonfly.is_some() {
            self.validate_hotkeys_for_env(
                "dragonfly",
                HotkeyScope::Dragonfly.env(),
                HotkeyScope::Dragonfly.max_function_key(),
            )?;
        }

        Ok(())
    }

    fn apply_settings(&mut self) {
        let mut saved_any = false;

        if self.dirty_settings {
            self.app_event_tx
                .send(AppEvent::SetTuiSettingsMenuConfig(self.settings.clone()));
            self.dirty_settings = false;
            saved_any = true;
        }

        if self.dirty_hotkeys {
            if let Err(err) = self.validate_hotkeys() {
                let msg = if saved_any {
                    format!("Saved settings menu. Hotkeys not saved: {err}")
                } else {
                    err
                };
                self.status = Some((msg, true));
                return;
            }

            self.app_event_tx
                .send(AppEvent::SetTuiHotkeysConfig(self.hotkeys.clone()));
            self.dirty_hotkeys = false;
            saved_any = true;
        }

        if saved_any {
            self.status = Some(("Saved interface settings".to_string(), false));
        } else {
            self.status = Some(("No changes to save".to_string(), false));
        }
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

    fn scoped_hotkeys_resolved(&self) -> ResolvedTuiHotkeys {
        self.hotkeys.effective_for_env(self.hotkey_scope.env())
    }

    fn override_value_for_row(&self, row: RowKind) -> Option<TuiHotkey> {
        let overrides = match self.hotkey_scope {
            HotkeyScope::Global => return None,
            HotkeyScope::Termux => self.hotkeys.termux.as_ref(),
            scope => scope
                .platform_override()
                .and_then(|platform| self.hotkeys.overrides_for_platform(platform)),
        };

        let overrides = overrides?;
        match row {
            RowKind::ModelSelectorHotkey => overrides.model_selector,
            RowKind::ReasoningEffortHotkey => overrides.reasoning_effort,
            RowKind::ShellSelectorHotkey => overrides.shell_selector,
            RowKind::NetworkSettingsHotkey => overrides.network_settings,
            RowKind::ExecOutputFoldHotkey => overrides.exec_output_fold,
            RowKind::JsReplCodeFoldHotkey => overrides.js_repl_code_fold,
            RowKind::JumpToParentCallHotkey => overrides.jump_to_parent_call,
            RowKind::JumpToLatestChildCallHotkey => overrides.jump_to_latest_child_call,
            _ => None,
        }
    }

    fn effective_value_for_row(resolved: ResolvedTuiHotkeys, row: RowKind) -> TuiHotkey {
        match row {
            RowKind::ModelSelectorHotkey => resolved.model_selector,
            RowKind::ReasoningEffortHotkey => resolved.reasoning_effort,
            RowKind::ShellSelectorHotkey => resolved.shell_selector,
            RowKind::NetworkSettingsHotkey => resolved.network_settings,
            RowKind::ExecOutputFoldHotkey => resolved.exec_output_fold,
            RowKind::JsReplCodeFoldHotkey => resolved.js_repl_code_fold,
            RowKind::JumpToParentCallHotkey => resolved.jump_to_parent_call,
            RowKind::JumpToLatestChildCallHotkey => resolved.jump_to_latest_child_call,
            _ => TuiHotkey::disabled(),
        }
    }

    fn hotkey_value_label_for_row(&self, row: RowKind) -> String {
        fn legacy_label_for_row(row: RowKind) -> Option<&'static str> {
            match row {
                RowKind::ExecOutputFoldHotkey => Some("["),
                RowKind::JsReplCodeFoldHotkey => Some("\\"),
                RowKind::JumpToParentCallHotkey => Some("]"),
                RowKind::JumpToLatestChildCallHotkey => Some("}"),
                _ => None,
            }
        }

        let format_hotkey = |hk: TuiHotkey| -> String {
            if hk.is_legacy() {
                if let Some(label) = legacy_label_for_row(row) {
                    format!("legacy ({label})")
                } else {
                    "legacy".to_string()
                }
            } else {
                hk.display_name().into_owned()
            }
        };

        match self.hotkey_scope {
            HotkeyScope::Global => {
                let hk = match row {
                    RowKind::ModelSelectorHotkey => self.hotkeys.model_selector,
                    RowKind::ReasoningEffortHotkey => self.hotkeys.reasoning_effort,
                    RowKind::ShellSelectorHotkey => self.hotkeys.shell_selector,
                    RowKind::NetworkSettingsHotkey => self.hotkeys.network_settings,
                    RowKind::ExecOutputFoldHotkey => self.hotkeys.exec_output_fold,
                    RowKind::JsReplCodeFoldHotkey => self.hotkeys.js_repl_code_fold,
                    RowKind::JumpToParentCallHotkey => self.hotkeys.jump_to_parent_call,
                    RowKind::JumpToLatestChildCallHotkey => self.hotkeys.jump_to_latest_child_call,
                    _ => TuiHotkey::disabled(),
                };
                let effective_hk =
                    Self::effective_value_for_row(self.hotkeys.effective_for_runtime(), row);
                let configured_name = format_hotkey(hk);
                let effective_name = format_hotkey(effective_hk);
                if hk == effective_hk {
                    configured_name
                } else {
                    format!("{configured_name} (here: {effective_name})")
                }
            }
            _ => {
                let resolved = self.scoped_hotkeys_resolved();
                let effective = Self::effective_value_for_row(resolved, row);
                let effective_name = format_hotkey(effective);
                match self.override_value_for_row(row) {
                    Some(_) => effective_name,
                    None => format!("inherit ({effective_name})"),
                }
            }
        }
    }

    fn open_hotkey_capture(&mut self, row: RowKind) {
        self.mode = ViewMode::CaptureHotkey { row, error: None };
    }

    fn activate_selected_row(&mut self) {
        match self.selected_row() {
            RowKind::OpenMode => self.cycle_open_mode_next(),
            RowKind::OverlayMinWidth => self.open_width_editor(),
            RowKind::HotkeyScope => self.cycle_hotkey_scope_next(),
            RowKind::ModelSelectorHotkey
            | RowKind::ReasoningEffortHotkey
            | RowKind::ShellSelectorHotkey
            | RowKind::NetworkSettingsHotkey
            | RowKind::ExecOutputFoldHotkey
            | RowKind::JsReplCodeFoldHotkey
            | RowKind::JumpToParentCallHotkey
            | RowKind::JumpToLatestChildCallHotkey => {
                let row = self.selected_row();
                self.open_hotkey_capture(row);
            }
            RowKind::ShowConfigToml => self.show_config_toml(),
            RowKind::ShowCodeHome => self.show_code_home(),
            RowKind::Apply => self.apply_settings(),
            RowKind::Close => self.is_complete = true,
        }
    }

    fn handle_mouse_event_main(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let rows = self.build_rows();
        let total = rows.len();
        if total == 0 {
            return false;
        }

        let Some(layout) = self.main_page().framed().layout(area) else {
            return false;
        };
        let visible = layout.body.height.max(1) as usize;
        self.viewport_rows.set(visible);

        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        let scroll_top = self.state.scroll_top.min(total.saturating_sub(1));
        let mut selected = self.state.selected_idx.unwrap_or(0);
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            total,
            |x, y| selection_index_at(layout.body, x, y, scroll_top, total),
            SelectableListMouseConfig {
                hover_select: false,
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

    fn handle_mouse_event_main_content(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let rows = self.build_rows();
        let total = rows.len();
        if total == 0 {
            return false;
        }

        let Some(layout) = self.main_page().content_only().layout(area) else {
            return false;
        };
        let visible = layout.body.height.max(1) as usize;
        self.viewport_rows.set(visible);

        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        let scroll_top = self.state.scroll_top.min(total.saturating_sub(1));
        let mut selected = self.state.selected_idx.unwrap_or(0);
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            total,
            |x, y| selection_index_at(layout.body, x, y, scroll_top, total),
            SelectableListMouseConfig {
                hover_select: false,
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
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let ViewMode::EditWidth { field, error } = &mut self.mode {
                    let Some(field_area) = Self::edit_width_page(error.as_deref())
                        .layout(area)
                        .map(|layout| layout.field)
                    else {
                        return false;
                    };
                    return field.handle_mouse_click(
                        mouse_event.column,
                        mouse_event.row,
                        field_area,
                    );
                }
                false
            }
            _ => false,
        }
    }

    fn handle_mouse_event_edit_content(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let ViewMode::EditWidth { field, error } = &mut self.mode {
                    let Some(field_area) = Self::edit_width_page(error.as_deref())
                        .layout_content(area)
                        .map(|layout| layout.field)
                    else {
                        return false;
                    };
                    return field.handle_mouse_click(
                        mouse_event.column,
                        mouse_event.row,
                        field_area,
                    );
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
                        self.dirty_settings = true;
                    }
                    Some(RowKind::OverlayMinWidth) => self.adjust_min_width(-5),
                    Some(RowKind::HotkeyScope) => self.cycle_hotkey_scope_prev(),
                    Some(row @ RowKind::ModelSelectorHotkey)
                    | Some(row @ RowKind::ReasoningEffortHotkey)
                    | Some(row @ RowKind::ShellSelectorHotkey)
                    | Some(row @ RowKind::NetworkSettingsHotkey)
                    | Some(row @ RowKind::ExecOutputFoldHotkey)
                    | Some(row @ RowKind::JsReplCodeFoldHotkey)
                    | Some(row @ RowKind::JumpToParentCallHotkey)
                    | Some(row @ RowKind::JumpToLatestChildCallHotkey) => {
                        self.adjust_hotkey_for_row(row, false);
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
                    Some(RowKind::HotkeyScope) => self.cycle_hotkey_scope_next(),
                    Some(row @ RowKind::ModelSelectorHotkey)
                    | Some(row @ RowKind::ReasoningEffortHotkey)
                    | Some(row @ RowKind::ShellSelectorHotkey)
                    | Some(row @ RowKind::NetworkSettingsHotkey)
                    | Some(row @ RowKind::ExecOutputFoldHotkey)
                    | Some(row @ RowKind::JsReplCodeFoldHotkey)
                    | Some(row @ RowKind::JumpToParentCallHotkey)
                    | Some(row @ RowKind::JumpToLatestChildCallHotkey) => {
                        self.adjust_hotkey_for_row(row, true);
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
                ViewMode::Main | ViewMode::Transition | ViewMode::CaptureHotkey { .. } => false,
            },
        }
    }

    fn set_hotkey_for_row(&mut self, row: RowKind, value: TuiHotkey) {
        match self.hotkey_scope {
            HotkeyScope::Global => {
                match row {
                    RowKind::ModelSelectorHotkey => self.hotkeys.model_selector = value,
                    RowKind::ReasoningEffortHotkey => self.hotkeys.reasoning_effort = value,
                    RowKind::ShellSelectorHotkey => self.hotkeys.shell_selector = value,
                    RowKind::NetworkSettingsHotkey => self.hotkeys.network_settings = value,
                    RowKind::ExecOutputFoldHotkey => self.hotkeys.exec_output_fold = value,
                    RowKind::JsReplCodeFoldHotkey => self.hotkeys.js_repl_code_fold = value,
                    RowKind::JumpToParentCallHotkey => self.hotkeys.jump_to_parent_call = value,
                    RowKind::JumpToLatestChildCallHotkey => {
                        self.hotkeys.jump_to_latest_child_call = value
                    }
                    _ => {}
                }
            }
            HotkeyScope::Termux => {
                let table = self.hotkeys.termux.get_or_insert_with(TuiHotkeysOverrides::default);
                match row {
                    RowKind::ModelSelectorHotkey => table.model_selector = Some(value),
                    RowKind::ReasoningEffortHotkey => table.reasoning_effort = Some(value),
                    RowKind::ShellSelectorHotkey => table.shell_selector = Some(value),
                    RowKind::NetworkSettingsHotkey => table.network_settings = Some(value),
                    RowKind::ExecOutputFoldHotkey => table.exec_output_fold = Some(value),
                    RowKind::JsReplCodeFoldHotkey => table.js_repl_code_fold = Some(value),
                    RowKind::JumpToParentCallHotkey => table.jump_to_parent_call = Some(value),
                    RowKind::JumpToLatestChildCallHotkey => {
                        table.jump_to_latest_child_call = Some(value)
                    }
                    _ => {}
                }
                Self::prune_empty_overrides(&mut self.hotkeys.termux);
            }
            scope => {
                let Some(platform) = scope.platform_override() else {
                    return;
                };
                let Some(overrides) = self.hotkeys.overrides_for_platform_mut(platform) else {
                    return;
                };
                let table = overrides.get_or_insert_with(TuiHotkeysOverrides::default);
                match row {
                    RowKind::ModelSelectorHotkey => table.model_selector = Some(value),
                    RowKind::ReasoningEffortHotkey => table.reasoning_effort = Some(value),
                    RowKind::ShellSelectorHotkey => table.shell_selector = Some(value),
                    RowKind::NetworkSettingsHotkey => table.network_settings = Some(value),
                    RowKind::ExecOutputFoldHotkey => table.exec_output_fold = Some(value),
                    RowKind::JsReplCodeFoldHotkey => table.js_repl_code_fold = Some(value),
                    RowKind::JumpToParentCallHotkey => table.jump_to_parent_call = Some(value),
                    RowKind::JumpToLatestChildCallHotkey => {
                        table.jump_to_latest_child_call = Some(value)
                    }
                    _ => {}
                }
                Self::prune_empty_overrides(overrides);
            }
        }
        self.dirty_hotkeys = true;
    }

    fn clear_hotkey_override_for_row(&mut self, row: RowKind) {
        let overrides: Option<&mut Option<TuiHotkeysOverrides>> = match self.hotkey_scope {
            HotkeyScope::Global => None,
            HotkeyScope::Termux => Some(&mut self.hotkeys.termux),
            scope => scope
                .platform_override()
                .and_then(|platform| self.hotkeys.overrides_for_platform_mut(platform)),
        };
        let Some(overrides) = overrides else {
            return;
        };
        let Some(table) = overrides.as_mut() else {
            return;
        };
        match row {
            RowKind::ModelSelectorHotkey => table.model_selector = None,
            RowKind::ReasoningEffortHotkey => table.reasoning_effort = None,
            RowKind::ShellSelectorHotkey => table.shell_selector = None,
            RowKind::NetworkSettingsHotkey => table.network_settings = None,
            RowKind::ExecOutputFoldHotkey => table.exec_output_fold = None,
            RowKind::JsReplCodeFoldHotkey => table.js_repl_code_fold = None,
            RowKind::JumpToParentCallHotkey => table.jump_to_parent_call = None,
            RowKind::JumpToLatestChildCallHotkey => table.jump_to_latest_child_call = None,
            _ => {}
        }
        Self::prune_empty_overrides(overrides);
        self.dirty_hotkeys = true;
    }

    fn process_key_event_capture_hotkey(&mut self, row: RowKind, key_event: KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.mode = ViewMode::Main;
                true
            }
            KeyEvent { code: KeyCode::Char('d'), modifiers, .. }
                if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
            {
                self.set_hotkey_for_row(row, TuiHotkey::disabled());
                self.mode = ViewMode::Main;
                true
            }
            KeyEvent { code: KeyCode::Char('l'), modifiers, .. }
                if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
            {
                match row {
                    RowKind::ExecOutputFoldHotkey
                    | RowKind::JsReplCodeFoldHotkey
                    | RowKind::JumpToParentCallHotkey
                    | RowKind::JumpToLatestChildCallHotkey => {
                        self.set_hotkey_for_row(row, TuiHotkey::legacy());
                        self.mode = ViewMode::Main;
                    }
                    _ => {
                        self.mode = ViewMode::CaptureHotkey {
                            row,
                            error: Some("Legacy is only available for history shortcuts.".to_string()),
                        };
                    }
                }
                true
            }
            KeyEvent { code: KeyCode::Char('i'), modifiers, .. }
                if (modifiers.is_empty() || modifiers == KeyModifiers::SHIFT)
                    && !matches!(self.hotkey_scope, HotkeyScope::Global) =>
            {
                self.clear_hotkey_override_for_row(row);
                self.mode = ViewMode::Main;
                true
            }
            KeyEvent {
                code: KeyCode::F(n),
                modifiers,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                if n == 1 {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some("F1 is reserved for the Help overlay.".to_string()),
                    };
                    return true;
                }
                let max_key = self.hotkey_scope.max_function_key();
                if n > max_key {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some(format!("This scope supports up to F{max_key}.")),
                    };
                    return true;
                }
                let Some(fk) = FunctionKeyHotkey::from_u8(n) else {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some("Unsupported function key.".to_string()),
                    };
                    return true;
                };
                let hk = TuiHotkey::Function(fk);
                self.set_hotkey_for_row(row, hk);
                self.mode = ViewMode::Main;
                true
            }
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                let mods = modifiers.difference(KeyModifiers::SHIFT);
                if mods.intersects(KeyModifiers::SUPER) {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some("Super modifier is not supported for hotkeys.".to_string()),
                    };
                    return true;
                }
                let ctrl = mods.contains(KeyModifiers::CONTROL);
                let alt = mods.contains(KeyModifiers::ALT);
                if !ctrl && !alt {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some("Use Ctrl/Alt+letter or a function key.".to_string()),
                    };
                    return true;
                }
                if !c.is_ascii_alphabetic() {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some("Hotkey chords currently support ASCII letters only.".to_string()),
                    };
                    return true;
                }

                let hk = TuiHotkey::Chord(code_core::config_types::TuiHotkeyChord {
                    ctrl,
                    alt,
                    key: c.to_ascii_lowercase(),
                });
                if hk.is_reserved_for_statusline_shortcuts() {
                    let label = hk.display_name();
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some(format!(
                            "{label} is reserved and cannot be remapped.",
                            label = label.as_ref()
                        )),
                    };
                    return true;
                }
                self.set_hotkey_for_row(row, hk);
                self.mode = ViewMode::Main;
                true
            }
            _ => {
                self.mode = ViewMode::CaptureHotkey { row, error: None };
                true
            }
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
            ViewMode::CaptureHotkey { row, error } => {
                self.mode = ViewMode::CaptureHotkey { row, error };
                self.process_key_event_capture_hotkey(row, key_event)
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
            ViewMode::Main | ViewMode::Transition | ViewMode::CaptureHotkey { .. } => false,
        }
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match &self.mode {
            ViewMode::Main => self.handle_mouse_event_main_content(mouse_event, area),
            ViewMode::EditWidth { .. } => self.handle_mouse_event_edit_content(mouse_event, area),
            ViewMode::CaptureHotkey { .. } | ViewMode::Transition => false,
        }
    }

    fn handle_mouse_event_direct_framed(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match &self.mode {
            ViewMode::Main => self.handle_mouse_event_main(mouse_event, area),
            ViewMode::EditWidth { .. } => self.handle_mouse_event_edit(mouse_event, area),
            ViewMode::CaptureHotkey { .. } | ViewMode::Transition => false,
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn render_without_frame(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main_without_frame(area, buf),
            ViewMode::EditWidth { field, error } => {
                let _ = Self::edit_width_page(error.as_deref()).render_content(area, buf, field);
            }
            ViewMode::CaptureHotkey { row, error } => {
                let _ = self
                    .capture_hotkey_page(*row, error.as_deref())
                    .render_content(area, buf);
            }
            ViewMode::Transition => self.render_main_without_frame(area, buf),
        }
    }

    fn help_for(row: RowKind) -> &'static str {
        match row {
            RowKind::OpenMode => "Auto uses overlay on wide terminals; override with overlay/bottom.",
            RowKind::OverlayMinWidth => "Terminal width (columns) at which auto prefers overlay.",
            RowKind::HotkeyScope => {
                "Choose which scope to edit. Platform scopes write to [tui.hotkeys.<platform>] and can inherit."
            }
            RowKind::ModelSelectorHotkey => {
                "Hotkey for opening model selector (F2-F24 or Ctrl/Alt+letter; macOS supports up to F20 for function keys)."
            }
            RowKind::ReasoningEffortHotkey => {
                "Hotkey for cycling reasoning effort (F2-F24 or Ctrl/Alt+letter; macOS supports up to F20 for function keys)."
            }
            RowKind::ShellSelectorHotkey => {
                "Hotkey for opening shell selector (F2-F24 or Ctrl/Alt+letter; macOS supports up to F20 for function keys)."
            }
            RowKind::NetworkSettingsHotkey => {
                "Hotkey for opening Settings -> Network (F2-F24 or Ctrl/Alt+letter; macOS supports up to F20 for function keys)."
            }
            RowKind::ExecOutputFoldHotkey => {
                "Hotkey for folding/unfolding the latest exec output (function key or Ctrl/Alt+letter). Defaults to legacy `[`, while composer is empty."
            }
            RowKind::JsReplCodeFoldHotkey => {
                "Hotkey for folding/unfolding the latest JS REPL code (function key or Ctrl/Alt+letter). Defaults to legacy `\\`, while composer is empty."
            }
            RowKind::JumpToParentCallHotkey => {
                "Hotkey for jumping to a parent tool call when a nested call is shown (function key or Ctrl/Alt+letter). Defaults to legacy `]`, while composer is empty."
            }
            RowKind::JumpToLatestChildCallHotkey => {
                "Hotkey for jumping to the latest tool call spawned by JS REPL (function key or Ctrl/Alt+letter). Defaults to legacy `}`, while composer is empty."
            }
            RowKind::ShowConfigToml => "Open config.toml in your file manager (Finder/Explorer).",
            RowKind::ShowCodeHome => "Open CODE_HOME in your file manager.",
            RowKind::Apply => "Persist these preferences to config.toml.",
            RowKind::Close => "Close this panel.",
        }
    }

    fn main_menu_rows(&self, kinds: &[RowKind]) -> Vec<SettingsMenuRow<'static, usize>> {
        fn label_for_row(kind: RowKind) -> &'static str {
            match kind {
                RowKind::OpenMode => "Settings menu",
                RowKind::OverlayMinWidth => "Overlay min width",
                RowKind::HotkeyScope => "Hotkey scope",
                RowKind::ModelSelectorHotkey => "Hotkey: model selector",
                RowKind::ReasoningEffortHotkey => "Hotkey: reasoning effort",
                RowKind::ShellSelectorHotkey => "Hotkey: shell selector",
                RowKind::NetworkSettingsHotkey => "Hotkey: network settings",
                RowKind::ExecOutputFoldHotkey => "Hotkey: fold output/details",
                RowKind::JsReplCodeFoldHotkey => "Hotkey: fold JS REPL code",
                RowKind::JumpToParentCallHotkey => "Hotkey: jump to parent call",
                RowKind::JumpToLatestChildCallHotkey => "Hotkey: jump to child call",
                RowKind::ShowConfigToml => "Show config.toml",
                RowKind::ShowCodeHome => "Show CODE_HOME",
                RowKind::Apply => "Apply",
                RowKind::Close => "Close",
            }
        }

        let label_pad_cols = kinds
            .iter()
            .map(|kind| label_for_row(*kind).width())
            .max()
            .map(|cols| u16::try_from(cols).unwrap_or(u16::MAX))
            .unwrap_or(0);

        kinds
            .iter()
            .enumerate()
            .map(|(idx, kind)| {
                let label = label_for_row(*kind);
                let mut row = SettingsMenuRow::new(idx, label).with_label_pad_cols(label_pad_cols);

                let value = match kind {
                    RowKind::OpenMode => {
                        let mode = Self::open_mode_label(self.settings.open_mode);
                        let desc = Self::open_mode_description(
                            self.settings.open_mode,
                            self.settings.overlay_min_width,
                        );
                        Some(StyledText::new(
                            format!("{mode} ({desc})"),
                            Style::new().fg(crate::colors::function()),
                        ))
                    }
                    RowKind::OverlayMinWidth => Some(StyledText::new(
                        self.settings.overlay_min_width.to_string(),
                        Style::new().fg(crate::colors::function()),
                    )),
                    RowKind::HotkeyScope => Some(StyledText::new(
                        self.hotkey_scope.label(),
                        Style::new().fg(crate::colors::function()),
                    )),
                    RowKind::ModelSelectorHotkey
                    | RowKind::ReasoningEffortHotkey
                    | RowKind::ShellSelectorHotkey
                    | RowKind::NetworkSettingsHotkey
                    | RowKind::ExecOutputFoldHotkey
                    | RowKind::JsReplCodeFoldHotkey
                    | RowKind::JumpToParentCallHotkey
                    | RowKind::JumpToLatestChildCallHotkey => Some(StyledText::new(
                        self.hotkey_value_label_for_row(*kind),
                        Style::new().fg(crate::colors::function()),
                    )),
                    RowKind::ShowConfigToml | RowKind::ShowCodeHome | RowKind::Close => None,
                    RowKind::Apply => {
                        let is_dirty = self.dirty_settings || self.dirty_hotkeys;
                        Some(StyledText::new(
                            if is_dirty { "Pending" } else { "Saved" },
                            if is_dirty {
                                Style::new().fg(crate::colors::warning()).bold()
                            } else {
                                Style::new().fg(crate::colors::success())
                            },
                        ))
                    }
                };

                if let Some(value) = value {
                    row = row.with_value(value);
                }
                row
            })
            .collect()
    }

    fn render_main(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.build_rows();
        let total = rows.len();
        let selected_idx = self
            .state
            .selected_idx
            .unwrap_or(0)
            .min(total.saturating_sub(1));
        let scroll_top = self.state.scroll_top.min(total.saturating_sub(1));

        let menu_rows = self.main_menu_rows(&rows);
        let selected_id = (total > 0).then_some(selected_idx);

        let Some(layout) = self
            .main_page()
            .framed()
            .render_menu_rows(area, buf, scroll_top, selected_id, &menu_rows)
        else {
            return;
        };

        self.viewport_rows.set(layout.body.height.max(1) as usize);
    }

    fn render_main_without_frame(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.build_rows();
        let total = rows.len();
        let selected_idx = self
            .state
            .selected_idx
            .unwrap_or(0)
            .min(total.saturating_sub(1));
        let scroll_top = self.state.scroll_top.min(total.saturating_sub(1));

        let menu_rows = self.main_menu_rows(&rows);
        let selected_id = (total > 0).then_some(selected_idx);

        let Some(layout) = self
            .main_page()
            .content_only()
            .render_menu_rows(area, buf, scroll_top, selected_id, &menu_rows)
        else {
            return;
        };

        self.viewport_rows.set(layout.body.height.max(1) as usize);
    }

    fn render_edit_width(
        &self,
        area: Rect,
        buf: &mut Buffer,
        field: &FormTextField,
        error: Option<&str>,
    ) {
        let _ = Self::edit_width_page(error).render(area, buf, field);
    }

    fn render_capture_hotkey(
        &self,
        area: Rect,
        buf: &mut Buffer,
        row: RowKind,
        error: Option<&str>,
    ) {
        let _ = self.capture_hotkey_page(row, error).render(area, buf);
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
        redraw_if(self.handle_mouse_event_direct_framed(mouse_event, area))
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
                base.clamp(12, 20)
            }
            ViewMode::EditWidth { .. } => 8,
            ViewMode::CaptureHotkey { .. } => 8,
            ViewMode::Transition => 8,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main(area, buf),
            ViewMode::EditWidth { field, error } => {
                self.render_edit_width(area, buf, field, error.as_deref())
            }
            ViewMode::CaptureHotkey { row, error } => {
                self.render_capture_hotkey(area, buf, *row, error.as_deref())
            }
            ViewMode::Transition => self.render_main(area, buf),
        }
    }
}
