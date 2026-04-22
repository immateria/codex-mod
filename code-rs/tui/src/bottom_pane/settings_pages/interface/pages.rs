use super::*;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::editor_page::SettingsEditorPage;
use crate::bottom_pane::settings_ui::hints::{
    hint_esc, hint_enter, hint_nav, hint_nav_horizontal, key_ctrl, shortcut_line, KeyHint,
};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::message_page::SettingsMessagePage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;

impl InterfaceSettingsView {
    fn panel_style() -> SettingsPanelStyle {
        SettingsPanelStyle::bottom_pane_padded()
    }

    pub(super) fn main_page(&self) -> SettingsMenuPage<'static> {
        self.main_page_for_selected_row(self.selected_row())
    }

    pub(super) fn main_page_for_selected_row(
        &self,
        selected_row: RowKind,
    ) -> SettingsMenuPage<'static> {
        let shortcuts = vec![
            hint_nav(" navigate"),
            hint_enter(" activate"),
            hint_nav_horizontal(" adjust"),
            KeyHint::new(key_ctrl("S"), " apply"),
            hint_esc(" close"),
        ];
        let footer_lines = vec![self.main_footer_line_for_row(selected_row)];
        SettingsMenuPage::new("Interface", Self::panel_style(), Vec::new(), footer_lines)
            .with_shortcuts(crate::bottom_pane::settings_ui::hints::ShortcutPlacement::Bottom, shortcuts)
    }

    pub(super) fn edit_width_page(error: Option<&str>) -> SettingsEditorPage<'static> {
        let mut post_field_lines = Vec::new();
        if let Some(error) = error {
            post_field_lines.push(Line::styled(
                error.to_owned(),
                Style::new().fg(crate::colors::warning()),
            ));
        }
        post_field_lines.push(shortcut_line(&[
            hint_enter(" save"),
            hint_esc(" cancel"),
        ]));

        SettingsEditorPage::new(
            "Interface",
            Self::panel_style(),
            "Overlay min width",
            Vec::new(),
            post_field_lines,
        )
    }

    pub(super) fn capture_hotkey_page(
        &self,
        row: RowKind,
        error: Option<&str>,
    ) -> SettingsMessagePage<'static> {
        let Some(label) = row.hotkey_capture_label() else {
            unreachable!("capture_hotkey_page called with non-hotkey row: {row:?}");
        };
        let current = self.hotkey_value_label_for_row(row);
        let header_lines = vec![Line::from(Span::styled(
            format!("{label} (current: {current})"),
            Style::new().fg(crate::colors::text()),
        ))];

        let mut body_lines = Vec::new();
        if let Some(error) = error {
            body_lines.push(Line::styled(
                error.to_owned(),
                Style::new().fg(crate::colors::warning()),
            ));
            body_lines.push(Line::from(""));
        }

        let inherit_hint = match self.hotkey_scope {
            HotkeyScope::Global => None,
            HotkeyScope::Macos
            | HotkeyScope::Windows
            | HotkeyScope::Linux
            | HotkeyScope::Android
            | HotkeyScope::Termux
            | HotkeyScope::FreeBsd
            | HotkeyScope::OpenBsd
            | HotkeyScope::NetBsd
            | HotkeyScope::Dragonfly => Some(
                KeyHint::new("i", " inherit"),
            ),
        };
        let legacy_hint = row.supports_legacy_hotkey().then(|| {
            KeyHint::new("l", " legacy")
        });

        let max_key = self.hotkey_scope.max_function_key();
        let ca = crate::platform_caps::ctrl_alt_label();
        body_lines.push(Line::from(Span::styled(
            format!("Press F2-F{max_key} or {ca}+letter (e.g. ctrl+h)."),
            Style::new().fg(crate::colors::text_dim()),
        )));

        let mut footer_hints = vec![
            hint_esc(" cancel"),
            KeyHint::new("d", " disable"),
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

    fn main_footer_line_for_row(&self, row: RowKind) -> Line<'static> {
        if let Some((status, is_error)) = self.status.as_ref() {
            let style = if *is_error {
                Style::new().fg(crate::colors::error())
            } else {
                Style::new().fg(crate::colors::text_dim())
            };
            Line::styled(status.clone(), style)
        } else {
            Line::styled(
                Self::help_for(row),
                Style::new().fg(crate::colors::text_dim()),
            )
        }
    }

    fn help_for(row: RowKind) -> String {
        let ca = crate::platform_caps::ctrl_alt_label();
        match row {
            RowKind::OpenMode => "Auto uses overlay on wide terminals; override with overlay/bottom.".into(),
            RowKind::OverlayMinWidth => "Terminal width (columns) at which auto prefers overlay.".into(),
            RowKind::NerdFonts => {
                "Icon rendering mode. NerdFont uses private-use-area glyphs (requires a patched font). Unicode uses standard symbols. ASCII uses plain characters only.".into()
            }
            RowKind::FuseHintKeyLabels => {
                "When on, hints whose key letter matches the first letter of the action are shown compactly (e.g. `rRefresh` instead of `r refresh`). Turn off for color-blind accessibility.".into()
            }
            RowKind::HotkeyScope => {
                "Choose which scope to edit. Platform scopes write to [tui.hotkeys.<platform>] and can inherit.".into()
            }
            RowKind::ModelSelectorHotkey => {
                format!("Hotkey for opening model selector (F2-F24 or {ca}+letter; macOS supports up to F20 for function keys).")
            }
            RowKind::ReasoningEffortHotkey => {
                format!("Hotkey for cycling reasoning effort (F2-F24 or {ca}+letter; macOS supports up to F20 for function keys).")
            }
            RowKind::ShellSelectorHotkey => {
                format!("Hotkey for opening shell selector (F2-F24 or {ca}+letter; macOS supports up to F20 for function keys).")
            }
            RowKind::NetworkSettingsHotkey => {
                format!("Hotkey for opening Settings -> Network (F2-F24 or {ca}+letter; macOS supports up to F20 for function keys).")
            }
            RowKind::ExecOutputFoldHotkey => {
                format!("Hotkey for folding/unfolding the latest exec output (function key or {ca}+letter). Defaults to legacy `[`, while composer is empty.")
            }
            RowKind::ReplCodeFoldHotkey => {
                format!("Hotkey for folding/unfolding the latest REPL code (function key or {ca}+letter). Defaults to legacy `\\`, while composer is empty.")
            }
            RowKind::JumpToParentCallHotkey => {
                format!("Hotkey for jumping to a parent tool call when a nested call is shown (function key or {ca}+letter). Defaults to legacy `]`, while composer is empty.")
            }
            RowKind::JumpToLatestChildCallHotkey => {
                format!("Hotkey for jumping to the latest tool call spawned by REPL (function key or {ca}+letter). Defaults to legacy `}}`, while composer is empty.")
            }
            RowKind::ShowConfigToml => "Open config.toml in your file manager (Finder/Explorer).".into(),
            RowKind::ShowCodeHome => "Open CODE_HOME in your file manager.".into(),
            RowKind::Apply => "Persist these preferences to config.toml.".into(),
            RowKind::Close => "Close this panel.".into(),
        }
    }
}
