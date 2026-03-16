use super::*;

use code_core::config_types::{FunctionKeyHotkey, ResolvedTuiHotkeys, SettingsMenuOpenMode, TuiHotkey};

use crate::app_event::AppEvent;

const OVERLAY_MIN_WIDTH_MIN: u16 = 40;
const OVERLAY_MIN_WIDTH_MAX: u16 = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HotkeyRow {
    ModelSelector,
    ReasoningEffort,
    ShellSelector,
    NetworkSettings,
    ExecOutputFold,
    JsReplCodeFold,
    JumpToParentCall,
    JumpToLatestChildCall,
}

const HOTKEY_ROWS: [HotkeyRow; 8] = [
    HotkeyRow::ModelSelector,
    HotkeyRow::ReasoningEffort,
    HotkeyRow::ShellSelector,
    HotkeyRow::NetworkSettings,
    HotkeyRow::ExecOutputFold,
    HotkeyRow::JsReplCodeFold,
    HotkeyRow::JumpToParentCall,
    HotkeyRow::JumpToLatestChildCall,
];

impl HotkeyRow {
    fn field_name(self) -> &'static str {
        match self {
            Self::ModelSelector => "model_selector",
            Self::ReasoningEffort => "reasoning_effort",
            Self::ShellSelector => "shell_selector",
            Self::NetworkSettings => "network_settings",
            Self::ExecOutputFold => "exec_output_fold",
            Self::JsReplCodeFold => "js_repl_code_fold",
            Self::JumpToParentCall => "jump_to_parent_call",
            Self::JumpToLatestChildCall => "jump_to_latest_child_call",
        }
    }

    fn allows_legacy(self) -> bool {
        matches!(
            self,
            Self::ExecOutputFold
                | Self::JsReplCodeFold
                | Self::JumpToParentCall
                | Self::JumpToLatestChildCall
        )
    }

    fn hotkey_value(self, hotkeys: &TuiHotkeysConfig) -> TuiHotkey {
        match self {
            Self::ModelSelector => hotkeys.model_selector,
            Self::ReasoningEffort => hotkeys.reasoning_effort,
            Self::ShellSelector => hotkeys.shell_selector,
            Self::NetworkSettings => hotkeys.network_settings,
            Self::ExecOutputFold => hotkeys.exec_output_fold,
            Self::JsReplCodeFold => hotkeys.js_repl_code_fold,
            Self::JumpToParentCall => hotkeys.jump_to_parent_call,
            Self::JumpToLatestChildCall => hotkeys.jump_to_latest_child_call,
        }
    }

    fn hotkey_field_mut<'a>(self, hotkeys: &'a mut TuiHotkeysConfig) -> &'a mut TuiHotkey {
        match self {
            Self::ModelSelector => &mut hotkeys.model_selector,
            Self::ReasoningEffort => &mut hotkeys.reasoning_effort,
            Self::ShellSelector => &mut hotkeys.shell_selector,
            Self::NetworkSettings => &mut hotkeys.network_settings,
            Self::ExecOutputFold => &mut hotkeys.exec_output_fold,
            Self::JsReplCodeFold => &mut hotkeys.js_repl_code_fold,
            Self::JumpToParentCall => &mut hotkeys.jump_to_parent_call,
            Self::JumpToLatestChildCall => &mut hotkeys.jump_to_latest_child_call,
        }
    }

    fn override_value(self, overrides: &TuiHotkeysOverrides) -> Option<TuiHotkey> {
        match self {
            Self::ModelSelector => overrides.model_selector,
            Self::ReasoningEffort => overrides.reasoning_effort,
            Self::ShellSelector => overrides.shell_selector,
            Self::NetworkSettings => overrides.network_settings,
            Self::ExecOutputFold => overrides.exec_output_fold,
            Self::JsReplCodeFold => overrides.js_repl_code_fold,
            Self::JumpToParentCall => overrides.jump_to_parent_call,
            Self::JumpToLatestChildCall => overrides.jump_to_latest_child_call,
        }
    }

    fn override_field_mut<'a>(
        self,
        overrides: &'a mut TuiHotkeysOverrides,
    ) -> &'a mut Option<TuiHotkey> {
        match self {
            Self::ModelSelector => &mut overrides.model_selector,
            Self::ReasoningEffort => &mut overrides.reasoning_effort,
            Self::ShellSelector => &mut overrides.shell_selector,
            Self::NetworkSettings => &mut overrides.network_settings,
            Self::ExecOutputFold => &mut overrides.exec_output_fold,
            Self::JsReplCodeFold => &mut overrides.js_repl_code_fold,
            Self::JumpToParentCall => &mut overrides.jump_to_parent_call,
            Self::JumpToLatestChildCall => &mut overrides.jump_to_latest_child_call,
        }
    }

    fn resolved_value(self, resolved: &ResolvedTuiHotkeys) -> TuiHotkey {
        match self {
            Self::ModelSelector => resolved.model_selector,
            Self::ReasoningEffort => resolved.reasoning_effort,
            Self::ShellSelector => resolved.shell_selector,
            Self::NetworkSettings => resolved.network_settings,
            Self::ExecOutputFold => resolved.exec_output_fold,
            Self::JsReplCodeFold => resolved.js_repl_code_fold,
            Self::JumpToParentCall => resolved.jump_to_parent_call,
            Self::JumpToLatestChildCall => resolved.jump_to_latest_child_call,
        }
    }
}

impl RowKind {
    fn as_hotkey_row(self) -> Option<HotkeyRow> {
        match self {
            Self::ModelSelectorHotkey => Some(HotkeyRow::ModelSelector),
            Self::ReasoningEffortHotkey => Some(HotkeyRow::ReasoningEffort),
            Self::ShellSelectorHotkey => Some(HotkeyRow::ShellSelector),
            Self::NetworkSettingsHotkey => Some(HotkeyRow::NetworkSettings),
            Self::ExecOutputFoldHotkey => Some(HotkeyRow::ExecOutputFold),
            Self::JsReplCodeFoldHotkey => Some(HotkeyRow::JsReplCodeFold),
            Self::JumpToParentCallHotkey => Some(HotkeyRow::JumpToParentCall),
            Self::JumpToLatestChildCallHotkey => Some(HotkeyRow::JumpToLatestChildCall),
            _ => None,
        }
    }

    pub(super) fn is_hotkey_row(self) -> bool {
        self.as_hotkey_row().is_some()
    }

    pub(super) fn supports_legacy_hotkey(self) -> bool {
        self.as_hotkey_row()
            .map(HotkeyRow::allows_legacy)
            .unwrap_or(false)
    }

    pub(super) fn hotkey_capture_label(self) -> Option<&'static str> {
        match self {
            Self::ModelSelectorHotkey => Some("Hotkey: model selector"),
            Self::ReasoningEffortHotkey => Some("Hotkey: reasoning effort"),
            Self::ShellSelectorHotkey => Some("Hotkey: shell selector"),
            Self::NetworkSettingsHotkey => Some("Hotkey: network settings"),
            Self::ExecOutputFoldHotkey => Some("Hotkey: fold output/details"),
            Self::JsReplCodeFoldHotkey => Some("Hotkey: fold JS REPL code"),
            Self::JumpToParentCallHotkey => Some("Hotkey: jump to parent call"),
            Self::JumpToLatestChildCallHotkey => Some("Hotkey: jump to child call"),
            _ => None,
        }
    }

    pub(super) fn legacy_hotkey_label(self) -> Option<&'static str> {
        match self.as_hotkey_row()? {
            HotkeyRow::ExecOutputFold => Some("["),
            HotkeyRow::JsReplCodeFold => Some("\\"),
            HotkeyRow::JumpToParentCall => Some("]"),
            HotkeyRow::JumpToLatestChildCall => Some("}"),
            _ => None,
        }
    }
}

impl InterfaceSettingsView {
    pub(super) fn cycle_open_mode_next(&mut self) {
        self.settings.open_mode = match self.settings.open_mode {
            SettingsMenuOpenMode::Auto => SettingsMenuOpenMode::Overlay,
            SettingsMenuOpenMode::Overlay => SettingsMenuOpenMode::Bottom,
            SettingsMenuOpenMode::Bottom => SettingsMenuOpenMode::Auto,
        };
        self.dirty_settings = true;
    }

    pub(super) fn adjust_min_width(&mut self, delta: i16) {
        let current = self.settings.overlay_min_width as i16;
        let next =
            (current + delta).clamp(OVERLAY_MIN_WIDTH_MIN as i16, OVERLAY_MIN_WIDTH_MAX as i16)
                as u16;
        if next != self.settings.overlay_min_width {
            self.settings.overlay_min_width = next;
            self.dirty_settings = true;
        }
    }

    pub(super) fn open_width_editor(&mut self) {
        let mut field = FormTextField::new_single_line();
        field.set_placeholder("100");
        field.set_text(&self.settings.overlay_min_width.to_string());
        self.mode = ViewMode::EditWidth { field, error: None };
    }

    pub(super) fn save_width_editor(&mut self, field: &FormTextField) -> Result<(), String> {
        let raw = field.text().trim();
        let parsed: u16 = raw
            .parse()
            .map_err(|_| {
                format!(
                    "Enter a number ({}-{} columns), e.g. 100.",
                    OVERLAY_MIN_WIDTH_MIN, OVERLAY_MIN_WIDTH_MAX
                )
            })?;
        // Keep sane bounds so accidental paste doesn't make it unusable.
        let clamped = parsed.clamp(OVERLAY_MIN_WIDTH_MIN, OVERLAY_MIN_WIDTH_MAX);
        self.settings.overlay_min_width = clamped;
        self.dirty_settings = true;
        Ok(())
    }

    pub(super) fn cycle_hotkey_scope_next(&mut self) {
        self.hotkey_scope = self.hotkey_scope.next();
    }

    pub(super) fn cycle_hotkey_scope_prev(&mut self) {
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
            && value.exec_output_fold.is_none()
            && value.js_repl_code_fold.is_none()
            && value.jump_to_parent_call.is_none()
            && value.jump_to_latest_child_call.is_none()
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
            TuiHotkey::Chord(_) | TuiHotkey::Legacy => {
                TuiHotkey::Function(FunctionKeyHotkey::Disabled)
            }
        }
    }

    fn cycle_hotkey_prev(hotkey: TuiHotkey, max_key: u8) -> TuiHotkey {
        match hotkey {
            TuiHotkey::Function(hk) => TuiHotkey::Function(Self::cycle_function_hotkey_prev(hk, max_key)),
            // Cycling is function-key focused. Chords are set via the capture UI.
            TuiHotkey::Chord(_) | TuiHotkey::Legacy => {
                TuiHotkey::Function(FunctionKeyHotkey::Disabled)
            }
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
        let Some(row) = row.as_hotkey_row() else {
            return false;
        };
        let next = |hk| {
            if forward {
                Self::cycle_optional_hotkey_next(hk, max_key)
            } else {
                Self::cycle_optional_hotkey_prev(hk, max_key)
            }
        };

        let table = overrides.get_or_insert_with(TuiHotkeysOverrides::default);
        let field = row.override_field_mut(table);
        *field = next(*field);
        Self::prune_empty_overrides(overrides);
        true
    }

    pub(super) fn adjust_hotkey_for_row(&mut self, row: RowKind, forward: bool) {
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
                if let Some(row) = row.as_hotkey_row() {
                    let field = row.hotkey_field_mut(&mut self.hotkeys);
                    *field = next(*field);
                    self.dirty_hotkeys = true;
                }
            }
            HotkeyScope::Termux => {
                let changed = Self::adjust_overrides_for_row(
                    &mut self.hotkeys.termux,
                    row,
                    forward,
                    max_key,
                );
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

        let mut seen: HashMap<TuiHotkey, &'static str> = HashMap::new();
        for row in HOTKEY_ROWS {
            let field = row.field_name();
            let hk = row.resolved_value(&resolved);
            let allow_legacy = row.allows_legacy();
            if hk.is_legacy() {
                if allow_legacy {
                    continue;
                }
                return Err(format!("{label}: {field} does not support the legacy mapping."));
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
                return Err(format!(
                    "{label}: {key} is reserved and cannot be remapped.",
                    key = key.as_ref()
                ));
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

        for (label, scope, has_overrides) in [
            ("macos", HotkeyScope::Macos, self.hotkeys.macos.is_some()),
            ("windows", HotkeyScope::Windows, self.hotkeys.windows.is_some()),
            ("linux", HotkeyScope::Linux, self.hotkeys.linux.is_some()),
            ("android", HotkeyScope::Android, self.hotkeys.android.is_some()),
            ("termux", HotkeyScope::Termux, self.hotkeys.termux.is_some()),
            ("freebsd", HotkeyScope::FreeBsd, self.hotkeys.freebsd.is_some()),
            ("openbsd", HotkeyScope::OpenBsd, self.hotkeys.openbsd.is_some()),
            ("netbsd", HotkeyScope::NetBsd, self.hotkeys.netbsd.is_some()),
            (
                "dragonfly",
                HotkeyScope::Dragonfly,
                self.hotkeys.dragonfly.is_some(),
            ),
        ] {
            if has_overrides {
                self.validate_hotkeys_for_env(label, scope.env(), scope.max_function_key())?;
            }
        }

        Ok(())
    }

    pub(super) fn apply_settings(&mut self) {
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
            self.status = Some(("Saved interface settings".to_owned(), false));
        } else {
            self.status = Some(("No changes to save".to_owned(), false));
        }
    }

    fn show_path_result(path: &std::path::Path, label: &str) -> (String, bool) {
        match crate::native_file_manager::reveal_path(path) {
            Ok(()) => (format!("Opened {label} in file manager"), false),
            Err(err) => (format!("Failed to open {label}: {err:#}"), true),
        }
    }

    pub(super) fn show_config_toml(&mut self) {
        let path = self.code_home.join("config.toml");
        let (message, is_error) = {
            let target: &std::path::Path = if path.exists() { &path } else { &self.code_home };
            Self::show_path_result(target, "config.toml")
        };
        self.status = Some((message, is_error));
    }

    pub(super) fn show_code_home(&mut self) {
        let (message, is_error) = Self::show_path_result(&self.code_home, "CODE_HOME");
        self.status = Some((message, is_error));
    }

    fn scoped_hotkeys_resolved(&self) -> ResolvedTuiHotkeys {
        self.hotkeys.effective_for_env(self.hotkey_scope.env())
    }

    fn override_value_for_row(&self, row: RowKind) -> Option<TuiHotkey> {
        let row = row.as_hotkey_row()?;
        let overrides = match self.hotkey_scope {
            HotkeyScope::Global => return None,
            HotkeyScope::Termux => self.hotkeys.termux.as_ref(),
            scope => scope
                .platform_override()
                .and_then(|platform| self.hotkeys.overrides_for_platform(platform)),
        };

        let overrides = overrides?;
        row.override_value(overrides)
    }

    pub(super) fn hotkey_value_label_for_row(&self, row: RowKind) -> String {
        let row_kind = row;
        let Some(row) = row_kind.as_hotkey_row() else {
            return "disabled".to_owned();
        };

        let format_hotkey = |hk: TuiHotkey| -> String {
            if hk.is_legacy() {
                if let Some(label) = row_kind.legacy_hotkey_label() {
                    format!("legacy ({label})")
                } else {
                    "legacy".to_owned()
                }
            } else {
                hk.display_name().into_owned()
            }
        };

        match self.hotkey_scope {
            HotkeyScope::Global => {
                let hk = row.hotkey_value(&self.hotkeys);
                let runtime_resolved = self.hotkeys.effective_for_runtime();
                let effective_hk = row.resolved_value(&runtime_resolved);
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
                let effective = row.resolved_value(&resolved);
                let effective_name = format_hotkey(effective);
                match self.override_value_for_row(row_kind) {
                    Some(_) => effective_name,
                    None => format!("inherit ({effective_name})"),
                }
            }
        }
    }

    pub(super) fn set_hotkey_for_row(&mut self, row: RowKind, value: TuiHotkey) {
        let Some(row) = row.as_hotkey_row() else {
            return;
        };
        match self.hotkey_scope {
            HotkeyScope::Global => *row.hotkey_field_mut(&mut self.hotkeys) = value,
            HotkeyScope::Termux => {
                let table = self.hotkeys.termux.get_or_insert_with(TuiHotkeysOverrides::default);
                *row.override_field_mut(table) = Some(value);
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
                *row.override_field_mut(table) = Some(value);
                Self::prune_empty_overrides(overrides);
            }
        }
        self.dirty_hotkeys = true;
    }

    pub(super) fn clear_hotkey_override_for_row(&mut self, row: RowKind) {
        let Some(row) = row.as_hotkey_row() else {
            return;
        };
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
        *row.override_field_mut(table) = None;
        Self::prune_empty_overrides(overrides);
        self.dirty_hotkeys = true;
    }
}
