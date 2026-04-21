use super::*;

use std::collections::HashSet;

use crate::app_event::AppEvent;
use crate::native_picker::{pick_path, NativePickerKind};

impl ReplSettingsView {
    fn toggle_enabled(&mut self) {
        self.settings.enabled = !self.settings.enabled;
        self.dirty = true;
    }

    fn cycle_runtime(&mut self) {
        self.settings.runtime = self.settings.runtime.next();
        self.settings.runtimes.entry(self.settings.runtime).or_default();
        self.dirty = true;
    }

    fn open_text_editor(&mut self, target: TextTarget) {
        let mut field = FormTextField::new_single_line();
        match target {
            TextTarget::RuntimePath => {
                let spec = self.current_runtime_spec();
                let placeholder = format!(
                    "{exe} (or /path/to/{exe})",
                    exe = self.settings.runtime.default_executable()
                );
                field.set_placeholder(&placeholder);
                field.set_text(
                    spec.path
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_default()
                        .as_str(),
                );
            }
        }
        self.mode = ViewMode::EditText { target, field };
    }

    fn open_list_editor(&mut self, target: ListTarget) {
        let mut field = FormTextField::new_multi_line();
        match target {
            ListTarget::RuntimeArgs => {
                let spec = self.current_runtime_spec();
                field.set_placeholder("--flag (one per line)");
                field.set_text(&spec.args.join("\n"));
            }
            ListTarget::ModuleDirs => {
                let spec = self.current_runtime_spec();
                field.set_placeholder("/path/to/node_modules (one per line)");
                let lines = spec
                    .module_dirs
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("\n");
                field.set_text(&lines);
            }
        }
        self.mode = ViewMode::EditList { target, field };
    }

    pub(super) fn save_text_editor(
        &mut self,
        target: TextTarget,
        field: &FormTextField,
    ) -> Result<(), String> {
        match target {
            TextTarget::RuntimePath => {
                let raw = field.text().trim();
                if raw.is_empty() {
                    self.current_runtime_spec_mut().path = None;
                } else {
                    self.current_runtime_spec_mut().path = Some(PathBuf::from(raw));
                }
            }
        }
        self.dirty = true;
        Ok(())
    }

    pub(super) fn save_list_editor(
        &mut self,
        target: ListTarget,
        field: &FormTextField,
    ) -> Result<(), String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut lines: Vec<String> = Vec::new();
        for line in field.text().lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let owned = trimmed.to_owned();
            if seen.insert(owned.clone()) {
                lines.push(owned);
            }
        }

        match target {
            ListTarget::RuntimeArgs => {
                self.current_runtime_spec_mut().args = lines;
            }
            ListTarget::ModuleDirs => {
                self.current_runtime_spec_mut().module_dirs =
                    lines.into_iter().map(PathBuf::from).collect();
            }
        }
        self.dirty = true;
        Ok(())
    }

    fn pick_runtime_path(&mut self) {
        if !crate::platform_caps::supports_native_picker() {
            self.app_event_tx.send_background_event_with_ticket(
                &self.ticket,
                "Native picker not supported on Android; type the path.".to_owned(),
            );
            return;
        }
        let result = pick_path(NativePickerKind::File, "Select repl runtime executable");
        match result {
            Ok(Some(path)) => {
                self.current_runtime_spec_mut().path = Some(path);
                self.dirty = true;
            }
            Ok(None) => {}
            Err(err) => {
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    format!("REPL picker failed: {err:#}"),
                );
            }
        }
    }

    fn clear_runtime_path(&mut self) {
        self.current_runtime_spec_mut().path = None;
        self.dirty = true;
    }

    fn add_module_dir(&mut self) {
        if !crate::platform_caps::supports_native_picker() {
            self.app_event_tx.send_background_event_with_ticket(
                &self.ticket,
                "Native picker not supported on Android; type the path.".to_owned(),
            );
            return;
        }
        let result = pick_path(NativePickerKind::Folder, "Select node_modules folder");
        match result {
            Ok(Some(path)) => {
                let rendered = path.to_string_lossy().into_owned();
                let spec = self.current_runtime_spec_mut();
                if !spec
                    .module_dirs
                    .iter()
                    .any(|existing| existing.to_string_lossy() == rendered)
                {
                    spec.module_dirs.push(path);
                    self.dirty = true;
                }
            }
            Ok(None) => {}
            Err(err) => {
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    format!("REPL picker failed: {err:#}"),
                );
            }
        }
    }

    fn apply_settings(&mut self) {
        self.app_event_tx
            .send(AppEvent::SetReplSettings(self.settings.clone()));
        self.app_event_tx.send_background_event_with_ticket(
            &self.ticket,
            "REPL: applying…".to_owned(),
        );
        self.dirty = false;
    }

    pub(super) fn activate_row(&mut self, kind: RowKind) {
        match kind {
            RowKind::Enabled => self.toggle_enabled(),
            RowKind::NodeEnabled => {
                self.settings.node_enabled = !self.settings.node_enabled;
                self.dirty = true;
            }
            RowKind::DenoEnabled => {
                self.settings.deno_enabled = !self.settings.deno_enabled;
                self.dirty = true;
            }
            RowKind::PythonEnabled => {
                self.settings.python_enabled = !self.settings.python_enabled;
                self.dirty = true;
            }
            RowKind::RuntimeKind => self.cycle_runtime(),
            RowKind::RuntimePath => self.open_text_editor(TextTarget::RuntimePath),
            RowKind::PickRuntimePath => self.pick_runtime_path(),
            RowKind::ClearRuntimePath => self.clear_runtime_path(),
            RowKind::RuntimeArgs => self.open_list_editor(ListTarget::RuntimeArgs),
            RowKind::ModuleDirs => self.open_list_editor(ListTarget::ModuleDirs),
            RowKind::AddModuleDir => self.add_module_dir(),
            RowKind::DenoPermRead => self.toggle_deno_perm(|dp| &mut dp.allow_read),
            RowKind::DenoPermWrite => self.toggle_deno_perm(|dp| &mut dp.allow_write),
            RowKind::DenoPermNet => self.toggle_deno_perm(|dp| &mut dp.allow_net),
            RowKind::DenoPermEnv => self.toggle_deno_perm(|dp| &mut dp.allow_env),
            RowKind::DenoPermRun => self.toggle_deno_perm(|dp| &mut dp.allow_run),
            RowKind::DenoPermSys => self.toggle_deno_perm(|dp| &mut dp.allow_sys),
            RowKind::DenoPermFfi => self.toggle_deno_perm(|dp| &mut dp.allow_ffi),
            RowKind::DenoPermAll => self.toggle_deno_perm(|dp| &mut dp.allow_all),
            RowKind::Apply => self.apply_settings(),
            RowKind::Close => self.is_complete = true,
        }
    }

    fn toggle_deno_perm(&mut self, accessor: impl FnOnce(&mut code_core::config::DenoPermissions) -> &mut bool) {
        let field = accessor(&mut self.settings.deno_permissions);
        *field = !*field;
        self.dirty = true;
    }
}
