use super::*;

use crate::app_event::AppEvent;
use crate::bottom_pane::SettingsSection;
use crate::native_picker::{pick_path, NativePickerKind};
use code_core::split_command_and_args;

impl ShellSelectionView {
    pub(super) fn edit_action_items(&self) -> &'static [(EditAction, SettingsButtonKind)] {
        if crate::platform_caps::supports_native_picker()
            && crate::platform_caps::supports_reveal_in_file_manager()
        {
            &EDIT_ACTION_ITEMS
        } else {
            &EDIT_ACTION_ITEMS_NO_PICKER
        }
    }

    pub fn new(
        current_shell: Option<ShellConfig>,
        presets: Vec<ShellPreset>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let shells: Vec<ShellOption> = presets
            .into_iter()
            .filter(|p| p.show_in_picker)
            .map(|preset| {
                let resolved_path =
                    which::which(&preset.command).ok().map(|path| path.to_string_lossy().to_string());
                let available = resolved_path.is_some();
                ShellOption {
                    preset,
                    available,
                    resolved_path,
                }
            })
            .collect();

        let custom_index = shells.len().saturating_add(1);
        let initial_index = match current_shell.as_ref() {
            None => 0,
            Some(current) => shells
                .iter()
                .position(|s| Self::current_matches_preset(current, &s.preset))
                .map(|idx| idx.saturating_add(1))
                .unwrap_or(custom_index),
        };

        Self {
            shells,
            selected_index: initial_index,
            current_shell,
            app_event_tx,
            is_complete: false,
            custom_input_mode: false,
            custom_field: {
                let mut field = FormTextField::new_single_line();
                field.set_placeholder("/bin/zsh -l");
                field
            },
            custom_style_override: None,
            native_picker_notice: None,
            edit_focus: EditFocus::Field,
            selected_action: EditAction::Apply,
            hovered_action: None,
        }
    }

    pub(crate) fn framed(&self) -> ShellSelectionViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> ShellSelectionViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> ShellSelectionViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> ShellSelectionViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(super) fn pick_shell_binary_from_dialog(&mut self) -> bool {
        self.native_picker_notice = None;
        if !crate::platform_caps::supports_native_picker() {
            self.native_picker_notice =
                Some("Not supported on Android; type the path.".to_string());
            return true;
        }
        match pick_path(NativePickerKind::File, "Select shell binary") {
            Ok(Some(path)) => {
                self.set_custom_path_from_picker(&path);
                true
            }
            Ok(None) => true,
            Err(err) => {
                self.native_picker_notice = Some(format!("Picker failed: {err:#}"));
                true
            }
        }
    }

    pub(super) fn show_custom_shell_in_file_manager(&mut self) -> bool {
        self.native_picker_notice = None;
        if !crate::platform_caps::supports_reveal_in_file_manager() {
            self.native_picker_notice =
                Some("Not supported on Android; copy the path manually.".to_string());
            return true;
        }
        let (path, _args) = split_command_and_args(self.custom_field.text());
        let trimmed = path.trim();
        if trimmed.is_empty() {
            self.native_picker_notice = Some("No shell path to show".to_string());
            return true;
        }

        let resolved = if trimmed.contains('/') || trimmed.contains('\\') {
            if std::path::Path::new(trimmed).exists() {
                Some(trimmed.to_string())
            } else {
                None
            }
        } else {
            which::which(trimmed).ok().map(|path| path.to_string_lossy().to_string())
        };

        let Some(resolved) = resolved else {
            self.native_picker_notice = Some(format!("Not found: {trimmed}"));
            return true;
        };

        match crate::native_file_manager::reveal_path(std::path::Path::new(&resolved)) {
            Ok(()) => {
                self.native_picker_notice = Some("Opened in file manager".to_string());
                true
            }
            Err(err) => {
                self.native_picker_notice = Some(format!("Open failed: {err:#}"));
                true
            }
        }
    }

    pub(super) fn current_matches_preset(current: &ShellConfig, preset: &ShellPreset) -> bool {
        Self::normalized_command_name(&current.path) == Self::normalized_command_name(&preset.command)
    }

    fn normalized_command_name(command: &str) -> String {
        let raw = command
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(command)
            .trim_matches('"')
            .trim_matches('\'')
            .to_ascii_lowercase();
        raw.strip_suffix(".exe").unwrap_or(raw.as_str()).to_string()
    }

    pub(super) fn display_shell(shell: &ShellConfig) -> String {
        if shell.args.is_empty() {
            shell.path.clone()
        } else {
            let path = &shell.path;
            let args = shell.args.join(" ");
            format!("{path} {args}")
        }
    }

    pub(super) fn item_count(&self) -> usize {
        // Auto option + presets + custom path option
        self.shells.len() + 2
    }

    pub(super) fn move_selection_up(&mut self) {
        if self.selected_index == 0 {
            self.selected_index = self.item_count().saturating_sub(1);
        } else {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
    }

    pub(super) fn move_selection_down(&mut self) {
        self.selected_index = (self.selected_index + 1) % self.item_count();
    }

    pub(super) fn confirm_selection(&mut self) {
        self.select_item(self.selected_index);
    }

    pub(super) fn open_custom_input_with_prefill(
        &mut self,
        prefill: String,
        style: Option<ShellScriptStyle>,
    ) {
        self.custom_input_mode = true;
        self.custom_field.set_text(&prefill);
        self.custom_style_override = style;
        self.native_picker_notice = None;
        self.edit_focus = EditFocus::Field;
        self.selected_action = EditAction::Apply;
        self.hovered_action = None;
    }

    pub(super) fn prefill_for_selection(
        &self,
        index: usize,
    ) -> (String, Option<ShellScriptStyle>) {
        let custom_index = self.shells.len().saturating_add(1);
        let current_prefill = || {
            let Some(current) = self.current_shell.as_ref() else {
                return (String::new(), None);
            };
            let inferred = current
                .script_style
                .or_else(|| ShellScriptStyle::infer_from_shell_program(&current.path));
            (Self::display_shell(current), inferred)
        };

        match index {
            0 => current_prefill(),
            idx if idx == custom_index => current_prefill(),
            idx => {
                let preset_idx = idx.saturating_sub(1);
                let Some(shell) = self.shells.get(preset_idx) else {
                    return current_prefill();
                };

                if let Some(current) = self.current_shell.as_ref()
                    && Self::current_matches_preset(current, &shell.preset)
                {
                    return (
                        Self::display_shell(current),
                        current
                            .script_style
                            .or_else(|| ShellScriptStyle::infer_from_shell_program(&current.path)),
                    );
                }

                let resolved_command = shell
                    .resolved_path
                    .clone()
                    .unwrap_or_else(|| shell.preset.command.clone());

                let mut input = resolved_command.clone();
                if !shell.preset.default_args.is_empty() {
                    input.push(' ');
                    input.push_str(shell.preset.default_args.join(" ").as_str());
                }

                let style = shell
                    .preset
                    .script_style
                    .as_deref()
                    .and_then(ShellScriptStyle::parse)
                    .or_else(|| ShellScriptStyle::infer_from_shell_program(&resolved_command));
                (input, style)
            }
        }
    }

    pub(super) fn select_item(&mut self, index: usize) {
        if index == 0 {
            self.app_event_tx.send(AppEvent::UpdateShellSelection {
                // Keep parity with `/shell -` which clears the override.
                path: "-".to_string(),
                args: Vec::new(),
                script_style: None,
            });
            self.send_closed(true);
            return;
        }

        let custom_index = self.shells.len().saturating_add(1);
        if index == custom_index {
            // Custom path option selected
            let (prefill, style) = self.prefill_for_selection(index);
            self.open_custom_input_with_prefill(prefill, style);
            return;
        }

        let preset_index = index.saturating_sub(1);
        if let Some(shell) = self.shells.get(preset_index) {
            if !shell.available {
                // Show notice that shell is not available - enter custom mode with command pre-filled
                let (prefill, style) = self.prefill_for_selection(index);
                self.open_custom_input_with_prefill(prefill, style);
                return;
            }

            self.app_event_tx.send(AppEvent::UpdateShellSelection {
                path: shell.preset.command.clone(),
                args: shell.preset.default_args.clone(),
                script_style: shell.preset.script_style.clone(),
            });
            self.send_closed(true);
        }
    }

    pub(super) fn submit_custom_path(&mut self) {
        let (path, args) = split_command_and_args(self.custom_field.text());
        let path = path.trim().to_string();
        if path.is_empty() {
            return;
        }

        self.app_event_tx.send(AppEvent::UpdateShellSelection {
            path,
            args,
            script_style: self.custom_style_override.map(|style| style.to_string()),
        });
        self.send_closed(true);
    }

    fn set_custom_path_from_picker(&mut self, selected_path: &std::path::Path) {
        let selected = selected_path.to_string_lossy().to_string();
        let selected = match shlex::try_quote(&selected) {
            Ok(quoted) => quoted.into_owned(),
            Err(_) => {
                self.native_picker_notice = Some("Picker returned an invalid path".to_string());
                return;
            }
        };
        let (_current_path, current_args) = split_command_and_args(self.custom_field.text());
        let mut args: Vec<String> = Vec::new();
        for arg in current_args {
            match shlex::try_quote(&arg) {
                Ok(quoted) => args.push(quoted.into_owned()),
                Err(_) => {
                    self.native_picker_notice =
                        Some("Shell args contain invalid characters".to_string());
                    return;
                }
            }
        }
        let args = args.join(" ");

        let mut next = selected;
        if !args.is_empty() {
            next.push(' ');
            next.push_str(&args);
        }
        self.custom_field.set_text(&next);
    }

    pub(super) fn pin_selected_shell_binary(&mut self) {
        let index = self.selected_index;
        if index == 0 {
            // Auto-detect can't be pinned.
            return;
        }

        let custom_index = self.shells.len().saturating_add(1);
        if index == custom_index {
            let (prefill, style) = self.prefill_for_selection(index);
            self.open_custom_input_with_prefill(prefill, style);
            return;
        }

        let preset_index = index.saturating_sub(1);
        let Some(shell) = self.shells.get(preset_index) else {
            return;
        };

        let Some(resolved) = shell.resolved_path.clone() else {
            // If the binary isn't discoverable, fall back to the editor so the user can
            // provide an explicit path.
            let (prefill, style) = self.prefill_for_selection(index);
            self.open_custom_input_with_prefill(prefill, style);
            return;
        };

        self.app_event_tx.send(AppEvent::UpdateShellSelection {
            path: resolved,
            args: shell.preset.default_args.clone(),
            script_style: shell.preset.script_style.clone(),
        });
        self.send_closed(true);
    }

    pub(super) fn resolve_custom_shell_path_in_place(&mut self) -> bool {
        let (path, args) = split_command_and_args(self.custom_field.text());
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return false;
        }

        let resolved = if trimmed.contains('/') || trimmed.contains('\\') {
            // Already a path; only normalize if it exists.
            if std::path::Path::new(trimmed).exists() {
                trimmed.to_string()
            } else {
                return false;
            }
        } else {
            match which::which(trimmed) {
                Ok(path) => path.to_string_lossy().to_string(),
                Err(_) => return false,
            }
        };

        let mut cmd = resolved;
        if !args.is_empty() {
            cmd.push(' ');
            cmd.push_str(&args.join(" "));
        }
        self.custom_field.set_text(&cmd);
        true
    }

    pub(super) fn cycle_custom_style_override(&mut self) {
        self.custom_style_override = match self.custom_style_override {
            None => Some(ShellScriptStyle::PosixSh),
            Some(ShellScriptStyle::PosixSh) => Some(ShellScriptStyle::BashZshCompatible),
            Some(ShellScriptStyle::BashZshCompatible) => Some(ShellScriptStyle::Zsh),
            Some(ShellScriptStyle::Zsh) => Some(ShellScriptStyle::PowerShell),
            Some(ShellScriptStyle::PowerShell) => Some(ShellScriptStyle::Cmd),
            Some(ShellScriptStyle::Cmd) => Some(ShellScriptStyle::Nushell),
            Some(ShellScriptStyle::Nushell) => Some(ShellScriptStyle::Elvish),
            Some(ShellScriptStyle::Elvish) => None,
        };
    }

    pub(super) fn send_closed(&mut self, confirmed: bool) {
        self.is_complete = true;
        self.app_event_tx.send(AppEvent::ShellSelectionClosed { confirmed });
    }

    pub(super) fn open_shell_profiles_settings(&mut self) {
        // Close this picker before opening settings to avoid stacked modals.
        self.send_closed(false);
        self.app_event_tx.send(AppEvent::OpenSettings {
            section: Some(SettingsSection::ShellProfiles),
        });
    }

}
