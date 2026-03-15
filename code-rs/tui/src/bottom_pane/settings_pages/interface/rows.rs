use super::*;

use ratatui::style::{Style, Stylize};
use unicode_width::UnicodeWidthStr;

use code_core::config_types::SettingsMenuOpenMode;

use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::rows::StyledText;

const MAIN_ROWS: [RowKind; 15] = [
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
];

impl InterfaceSettingsView {
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
            SettingsMenuOpenMode::Overlay => "always show overlay".to_owned(),
            SettingsMenuOpenMode::Bottom => "prefer bottom pane".to_owned(),
        }
    }

    pub(super) fn build_rows(&self) -> &'static [RowKind] {
        &MAIN_ROWS
    }

    pub(super) fn selected_row(&self) -> RowKind {
        let rows = self.build_rows();
        let idx = self.state.selected_idx.unwrap_or(0).min(rows.len().saturating_sub(1));
        rows[idx]
    }

    pub(super) fn main_menu_rows(&self, kinds: &[RowKind]) -> Vec<SettingsMenuRow<'static, usize>> {
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
}
