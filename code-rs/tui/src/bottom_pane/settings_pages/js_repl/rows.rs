use super::*;

use ratatui::style::{Modifier, Style};

use crate::bottom_pane::settings_ui::rows::{KeyValueRow, StyledText};
use crate::bottom_pane::settings_ui::toggle;

impl JsReplSettingsView {
    pub(super) fn row_count(&self) -> usize {
        // Enabled, runtime kind, runtime path, picker action.
        let mut count = 4;
        // Optional: clear runtime path.
        if self.settings.runtime_path.is_some() {
            count += 1;
        }
        // Runtime args.
        count += 1;
        // Node-only: module dirs + add module dir.
        if matches!(self.settings.runtime, JsReplRuntimeKindToml::Node) {
            count += 2;
        }
        // Apply, close.
        count += 2;
        count
    }

    pub(super) fn runtime_label(kind: JsReplRuntimeKindToml) -> &'static str {
        match kind {
            JsReplRuntimeKindToml::Node => "node",
            JsReplRuntimeKindToml::Deno => "deno",
        }
    }

    pub(super) fn enabled_value(enabled: bool) -> StyledText<'static> {
        let mut status = toggle::enabled_word_warning_off(enabled);
        status.style = status.style.add_modifier(Modifier::BOLD);
        status
    }

    pub(super) fn build_rows(&self) -> Vec<RowKind> {
        let mut rows = Vec::with_capacity(self.row_count());
        rows.extend([
            RowKind::Enabled,
            RowKind::RuntimeKind,
            RowKind::RuntimePath,
            RowKind::PickRuntimePath,
        ]);
        if self.settings.runtime_path.is_some() {
            rows.push(RowKind::ClearRuntimePath);
        }

        rows.push(RowKind::RuntimeArgs);
        if matches!(self.settings.runtime, JsReplRuntimeKindToml::Node) {
            rows.push(RowKind::NodeModuleDirs);
            rows.push(RowKind::AddNodeModuleDir);
        }

        rows.push(RowKind::Apply);
        rows.push(RowKind::Close);
        debug_assert_eq!(rows.len(), self.row_count());
        rows
    }

    pub(super) fn main_row_specs(&self, rows: &[RowKind]) -> Vec<KeyValueRow<'static>> {
        let runtime_label = Self::runtime_label(self.settings.runtime);
        let runtime_path = self
            .settings
            .runtime_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "auto (PATH)".to_string());
        let runtime_args = if self.settings.runtime_args.is_empty() {
            "(none)".to_string()
        } else {
            format!("{} entries", self.settings.runtime_args.len())
        };
        let module_dirs = if self.settings.node_module_dirs.is_empty() {
            "(none)".to_string()
        } else {
            format!("{} entries", self.settings.node_module_dirs.len())
        };
        let apply_suffix = if self.dirty { " *" } else { "" };

        rows.iter()
            .copied()
            .map(|kind| match kind {
                RowKind::Enabled => KeyValueRow::new("Enabled")
                    .with_value(Self::enabled_value(self.settings.enabled)),
                RowKind::RuntimeKind => KeyValueRow::new("Runtime").with_value(StyledText::new(
                    runtime_label,
                    Style::default().fg(crate::colors::info()),
                )),
                RowKind::RuntimePath => KeyValueRow::new("Runtime path").with_value(
                    StyledText::new(
                        runtime_path.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ),
                ),
                RowKind::PickRuntimePath => KeyValueRow::new("Pick runtime path (file picker)"),
                RowKind::ClearRuntimePath => KeyValueRow::new("Clear runtime path (use PATH)"),
                RowKind::RuntimeArgs => KeyValueRow::new("Runtime args").with_value(
                    StyledText::new(
                        runtime_args.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ),
                ),
                RowKind::NodeModuleDirs => KeyValueRow::new("Node module dirs").with_value(
                    StyledText::new(
                        module_dirs.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ),
                ),
                RowKind::AddNodeModuleDir => {
                    KeyValueRow::new("Add node module dir (folder picker)")
                }
                RowKind::Apply => KeyValueRow::new("Apply changes").with_value(StyledText::new(
                    apply_suffix,
                    Style::default().fg(crate::colors::warning()),
                )),
                RowKind::Close => KeyValueRow::new("Close"),
            })
            .collect()
    }
}
