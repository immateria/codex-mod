use super::*;

use ratatui::style::Modifier;

use crate::bottom_pane::settings_ui::rows::{KeyValueRow, StyledText};
use crate::bottom_pane::settings_ui::toggle;

impl ReplSettingsView {
    pub(super) fn row_count(&self) -> usize {
        // Enabled + per-runtime toggles + default runtime selector.
        let mut count = 5;
        // Runtime path for the selected default runtime.
        count += 1;
        // Optional: runtime picker action.
        if crate::platform_caps::supports_native_picker() {
            count += 1;
        }
        // Optional: clear runtime path.
        if self.settings.runtime_path.is_some() {
            count += 1;
        }
        // Runtime args.
        count += 1;
        // Node-only: module dirs.
        if matches!(self.settings.runtime, ReplRuntimeKindToml::Node) {
            count += 1;
            // Optional: add module dir picker.
            if crate::platform_caps::supports_native_picker() {
                count += 1;
            }
        }
        // Deno-only: permission toggles (8 categories).
        if matches!(self.settings.runtime, ReplRuntimeKindToml::Deno) {
            count += 8;
        }
        // Apply, close.
        count += 2;
        count
    }

    pub(super) fn runtime_label(kind: ReplRuntimeKindToml) -> &'static str {
        kind.label()
    }

    pub(super) fn enabled_value(enabled: bool) -> StyledText<'static> {
        let mut status = toggle::enabled_word_warning_off(enabled);
        status.style = status.style.add_modifier(Modifier::BOLD);
        status
    }

    fn perm_value(allowed: bool) -> StyledText<'static> {
        if allowed {
            StyledText::new("allow", crate::colors::style_success())
        } else {
            StyledText::new("deny", crate::colors::style_text_dim())
        }
    }

    pub(super) fn build_rows(&self) -> Vec<RowKind> {
        let mut rows = Vec::with_capacity(self.row_count());
        rows.extend([
            RowKind::Enabled,
            RowKind::NodeEnabled,
            RowKind::DenoEnabled,
            RowKind::PythonEnabled,
            RowKind::RuntimeKind,
            RowKind::RuntimePath,
        ]);
        if crate::platform_caps::supports_native_picker() {
            rows.push(RowKind::PickRuntimePath);
        }
        if self.settings.runtime_path.is_some() {
            rows.push(RowKind::ClearRuntimePath);
        }

        rows.push(RowKind::RuntimeArgs);
        if matches!(self.settings.runtime, ReplRuntimeKindToml::Node) {
            rows.push(RowKind::NodeModuleDirs);
            if crate::platform_caps::supports_native_picker() {
                rows.push(RowKind::AddNodeModuleDir);
            }
        }

        if matches!(self.settings.runtime, ReplRuntimeKindToml::Deno) {
            rows.extend([
                RowKind::DenoPermRead,
                RowKind::DenoPermWrite,
                RowKind::DenoPermNet,
                RowKind::DenoPermEnv,
                RowKind::DenoPermRun,
                RowKind::DenoPermSys,
                RowKind::DenoPermFfi,
                RowKind::DenoPermAll,
            ]);
        }

        rows.push(RowKind::Apply);
        rows.push(RowKind::Close);
        debug_assert_eq!(rows.len(), self.row_count());
        rows
    }

    pub(super) fn main_row_specs(&self, rows: &[RowKind]) -> Vec<KeyValueRow<'static>> {
        let nf = crate::icons::nerd_fonts_enabled();
        let runtime_label = Self::runtime_label(self.settings.runtime);
        let runtime_path = self
            .settings
            .runtime_path
            .as_ref().map_or_else(|| "auto (PATH)".to_owned(), |path| path.to_string_lossy().into_owned());
        let runtime_args = if self.settings.runtime_args.is_empty() {
            "(none)".to_owned()
        } else {
            format!("{} entries", self.settings.runtime_args.len())
        };
        let module_dirs = if self.settings.node_module_dirs.is_empty() {
            "(none)".to_owned()
        } else {
            format!("{} entries", self.settings.node_module_dirs.len())
        };
        let apply_suffix = if self.dirty { " *" } else { "" };
        let dp = &self.settings.deno_permissions;

        let node_label = if nf {
            format!("  {} Node", crate::icons::nodejs_icon())
        } else {
            "  Node".to_owned()
        };
        let deno_label = if nf {
            format!("  {} Deno", crate::icons::denojs_icon())
        } else {
            "  Deno".to_owned()
        };
        let python_label = if nf {
            format!("  {} Python", crate::icons::python_icon())
        } else {
            "  Python".to_owned()
        };

        rows.iter()
            .copied()
            .map(|kind| match kind {
                RowKind::Enabled => KeyValueRow::new("REPL")
                    .with_value(Self::enabled_value(self.settings.enabled)),
                RowKind::NodeEnabled => KeyValueRow::new(node_label.clone())
                    .with_value(Self::enabled_value(self.settings.node_enabled)),
                RowKind::DenoEnabled => KeyValueRow::new(deno_label.clone())
                    .with_value(Self::enabled_value(self.settings.deno_enabled)),
                RowKind::PythonEnabled => KeyValueRow::new(python_label.clone())
                    .with_value(Self::enabled_value(self.settings.python_enabled)),
                RowKind::RuntimeKind => {
                    let rt_icon = if nf {
                        match self.settings.runtime {
                            ReplRuntimeKindToml::Node => format!("{} ", crate::icons::nodejs_icon()),
                            ReplRuntimeKindToml::Deno => format!("{} ", crate::icons::denojs_icon()),
                            ReplRuntimeKindToml::Python => format!("{} ", crate::icons::python_icon()),
                        }
                    } else {
                        String::new()
                    };
                    KeyValueRow::new("Configure runtime").with_value(StyledText::new(
                        format!("{rt_icon}{runtime_label}"),
                        crate::colors::style_info(),
                    ))
                }
                RowKind::RuntimePath => KeyValueRow::new("  Path").with_value(
                    StyledText::new(
                        runtime_path.clone(),
                        crate::colors::style_text_dim(),
                    ),
                ),
                RowKind::PickRuntimePath => KeyValueRow::new("  Pick path (file picker)"),
                RowKind::ClearRuntimePath => KeyValueRow::new("  Clear path (use PATH)"),
                RowKind::RuntimeArgs => KeyValueRow::new("  Args").with_value(
                    StyledText::new(
                        runtime_args.clone(),
                        crate::colors::style_text_dim(),
                    ),
                ),
                RowKind::NodeModuleDirs => KeyValueRow::new("  Module dirs").with_value(
                    StyledText::new(
                        module_dirs.clone(),
                        crate::colors::style_text_dim(),
                    ),
                ),
                RowKind::AddNodeModuleDir => {
                    KeyValueRow::new("  Add module dir (folder picker)")
                }
                RowKind::DenoPermRead => KeyValueRow::new("  allow-read")
                    .with_value(Self::perm_value(dp.allow_read)),
                RowKind::DenoPermWrite => KeyValueRow::new("  allow-write")
                    .with_value(Self::perm_value(dp.allow_write)),
                RowKind::DenoPermNet => KeyValueRow::new("  allow-net")
                    .with_value(Self::perm_value(dp.allow_net)),
                RowKind::DenoPermEnv => KeyValueRow::new("  allow-env")
                    .with_value(Self::perm_value(dp.allow_env)),
                RowKind::DenoPermRun => KeyValueRow::new("  allow-run")
                    .with_value(Self::perm_value(dp.allow_run)),
                RowKind::DenoPermSys => KeyValueRow::new("  allow-sys")
                    .with_value(Self::perm_value(dp.allow_sys)),
                RowKind::DenoPermFfi => KeyValueRow::new("  allow-ffi")
                    .with_value(Self::perm_value(dp.allow_ffi)),
                RowKind::DenoPermAll => KeyValueRow::new("  allow-all (⚠ full access)")
                    .with_value(Self::perm_value(dp.allow_all)),
                RowKind::Apply => KeyValueRow::new("Apply changes").with_value(StyledText::new(
                    apply_suffix,
                    crate::colors::style_warning(),
                )),
                RowKind::Close => KeyValueRow::new("Close"),
            })
            .collect()
    }
}
