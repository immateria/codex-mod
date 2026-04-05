use super::*;

use std::collections::HashSet;

use crate::app_event::AppEvent;
use crate::native_picker::{pick_path, NativePickerKind};

impl JsReplSettingsView {
    fn toggle_enabled(&mut self) {
        self.settings.enabled = !self.settings.enabled;
        self.dirty = true;
    }

    fn cycle_runtime(&mut self) {
        self.settings.runtime = match self.settings.runtime {
            JsReplRuntimeKindToml::Node => JsReplRuntimeKindToml::Deno,
            JsReplRuntimeKindToml::Deno => JsReplRuntimeKindToml::Node,
        };
        self.dirty = true;
    }

    fn open_text_editor(&mut self, target: TextTarget) {
        let mut field = FormTextField::new_single_line();
        match target {
            TextTarget::RuntimePath => {
                field.set_placeholder("node (or /path/to/node)");
                field.set_text(
                    self.settings
                        .runtime_path
                        .as_ref()
                        .map(|path| path.to_string_lossy().to_string())
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
                field.set_placeholder("--flag (one per line)");
                field.set_text(&self.settings.runtime_args.join("\n"));
            }
            ListTarget::NodeModuleDirs => {
                field.set_placeholder("/path/to/node_modules (one per line)");
                let lines = self
                    .settings
                    .node_module_dirs
                    .iter()
                    .map(|path| path.to_string_lossy().to_string())
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
                    self.settings.runtime_path = None;
                } else {
                    self.settings.runtime_path = Some(PathBuf::from(raw));
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
                self.settings.runtime_args = lines;
            }
            ListTarget::NodeModuleDirs => {
                self.settings.node_module_dirs = lines.into_iter().map(PathBuf::from).collect();
            }
        }
        self.dirty = true;
        Ok(())
    }

    fn pick_runtime_path(&mut self) {
        if !crate::platform_caps::supports_native_picker() {
            self.app_event_tx.send_background_event_with_ticket(
                &self.ticket,
                "Native picker not supported on Android; type the path.".to_string(),
            );
            return;
        }
        let result = pick_path(NativePickerKind::File, "Select js_repl runtime executable");
        match result {
            Ok(Some(path)) => {
                self.settings.runtime_path = Some(path);
                self.dirty = true;
            }
            Ok(None) => {}
            Err(err) => {
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    format!("JS REPL picker failed: {err:#}"),
                );
            }
        }
    }

    fn clear_runtime_path(&mut self) {
        self.settings.runtime_path = None;
        self.dirty = true;
    }

    fn add_node_module_dir(&mut self) {
        if !crate::platform_caps::supports_native_picker() {
            self.app_event_tx.send_background_event_with_ticket(
                &self.ticket,
                "Native picker not supported on Android; type the path.".to_string(),
            );
            return;
        }
        let result = pick_path(NativePickerKind::Folder, "Select node_modules folder");
        match result {
            Ok(Some(path)) => {
                let rendered = path.to_string_lossy().to_string();
                if !self
                    .settings
                    .node_module_dirs
                    .iter()
                    .any(|existing| existing.to_string_lossy() == rendered)
                {
                    self.settings.node_module_dirs.push(path);
                    self.dirty = true;
                }
            }
            Ok(None) => {}
            Err(err) => {
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    format!("JS REPL picker failed: {err:#}"),
                );
            }
        }
    }

    fn apply_settings(&mut self) {
        self.app_event_tx
            .send(AppEvent::SetJsReplSettings(self.settings.clone()));
        self.app_event_tx.send_background_event_with_ticket(
            &self.ticket,
            "JS REPL: applying…".to_string(),
        );
        self.dirty = false;
    }

    pub(super) fn activate_row(&mut self, kind: RowKind) {
        match kind {
            RowKind::Enabled => self.toggle_enabled(),
            RowKind::RuntimeKind => self.cycle_runtime(),
            RowKind::RuntimePath => self.open_text_editor(TextTarget::RuntimePath),
            RowKind::PickRuntimePath => self.pick_runtime_path(),
            RowKind::ClearRuntimePath => self.clear_runtime_path(),
            RowKind::RuntimeArgs => self.open_list_editor(ListTarget::RuntimeArgs),
            RowKind::NodeModuleDirs => self.open_list_editor(ListTarget::NodeModuleDirs),
            RowKind::AddNodeModuleDir => self.add_node_module_dir(),
            RowKind::Apply => self.apply_settings(),
            RowKind::Close => self.is_complete = true,
        }
    }
}
