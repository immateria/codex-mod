use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::action_page::SettingsActionPage;
use super::settings_ui::buttons::{
    standard_button_specs,
    SettingsButtonKind,
    StandardButtonSpec,
};
use super::settings_ui::fields::BorderedField;
use super::settings_ui::hints::{self, KeyHint};
use super::settings_ui::line_runs::{
    selection_id_at as selection_run_id_at,
    SelectableLineRun,
};
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::menu_rows::SettingsMenuRow;
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::rows::StyledText;
use super::BottomPane;
use super::SettingsSection;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crate::components::form_text_field::FormTextField;
use crate::native_picker::{pick_path, NativePickerKind};
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use code_common::shell_presets::ShellPreset;
use code_core::config_types::ShellConfig;
use code_core::config_types::ShellScriptStyle;
use code_core::split_command_and_args;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditFocus {
    Field,
    Actions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditAction {
    Apply,
    Pick,
    Show,
    Resolve,
    Style,
    Back,
}

const EDIT_ACTION_ITEMS: [(EditAction, SettingsButtonKind); 6] = [
    (EditAction::Apply, SettingsButtonKind::Apply),
    (EditAction::Pick, SettingsButtonKind::Pick),
    (EditAction::Show, SettingsButtonKind::Show),
    (EditAction::Resolve, SettingsButtonKind::Resolve),
    (EditAction::Style, SettingsButtonKind::Style),
    (EditAction::Back, SettingsButtonKind::Back),
];

/// A shell option with availability status
#[derive(Clone, Debug)]
struct ShellOption {
    preset: ShellPreset,
    available: bool,
    resolved_path: Option<String>,
}

pub(crate) struct ShellSelectionView {
    shells: Vec<ShellOption>,
    selected_index: usize,
    current_shell: Option<ShellConfig>,
    app_event_tx: AppEventSender,
    is_complete: bool,
    custom_input_mode: bool,
    custom_field: FormTextField,
    custom_style_override: Option<ShellScriptStyle>,
    native_picker_notice: Option<String>,
    edit_focus: EditFocus,
    selected_action: EditAction,
    hovered_action: Option<EditAction>,
}

impl ShellSelectionView {
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

    fn pick_shell_binary_from_dialog(&mut self) -> bool {
        self.native_picker_notice = None;
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

    fn show_custom_shell_in_file_manager(&mut self) -> bool {
        self.native_picker_notice = None;
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

    fn current_matches_preset(current: &ShellConfig, preset: &ShellPreset) -> bool {
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

    fn display_shell(shell: &ShellConfig) -> String {
        if shell.args.is_empty() {
            shell.path.clone()
        } else {
            let path = &shell.path;
            let args = shell.args.join(" ");
            format!("{path} {args}")
        }
    }

    fn item_count(&self) -> usize {
        // Auto option + presets + custom path option
        self.shells.len() + 2
    }

    fn move_selection_up(&mut self) {
        if self.selected_index == 0 {
            self.selected_index = self.item_count().saturating_sub(1);
        } else {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
    }

    fn move_selection_down(&mut self) {
        self.selected_index = (self.selected_index + 1) % self.item_count();
    }

    fn confirm_selection(&mut self) {
        self.select_item(self.selected_index);
    }

    fn open_custom_input_with_prefill(&mut self, prefill: String, style: Option<ShellScriptStyle>) {
        self.custom_input_mode = true;
        self.custom_field.set_text(&prefill);
        self.custom_style_override = style;
        self.native_picker_notice = None;
        self.edit_focus = EditFocus::Field;
        self.selected_action = EditAction::Apply;
        self.hovered_action = None;
    }

    fn prefill_for_selection(&self, index: usize) -> (String, Option<ShellScriptStyle>) {
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

    fn select_item(&mut self, index: usize) {
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

    fn submit_custom_path(&mut self) {
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

    fn pin_selected_shell_binary(&mut self) {
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

    fn resolve_custom_shell_path_in_place(&mut self) -> bool {
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

    fn cycle_custom_style_override(&mut self) {
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

    fn send_closed(&mut self, confirmed: bool) {
        self.is_complete = true;
        self.app_event_tx.send(AppEvent::ShellSelectionClosed { confirmed });
    }

    fn open_shell_profiles_settings(&mut self) {
        // Close this picker before opening settings to avoid stacked modals.
        self.send_closed(false);
        self.app_event_tx.send(AppEvent::OpenSettings {
            section: Some(SettingsSection::ShellProfiles),
        });
    }

    fn list_page(&self) -> SettingsMenuPage<'_> {
        let mut current_label = match self.current_shell.as_ref() {
            Some(current) => Self::display_shell(current),
            None => "auto-detected".to_string(),
        };
        if let Some(current) = self.current_shell.as_ref() {
            let style = current
                .script_style
                .or_else(|| ShellScriptStyle::infer_from_shell_program(&current.path))
                .map(|style| style.to_string())
                .unwrap_or_else(|| "auto".to_string());
            current_label.push_str(&format!(" (style: {style})"));
        }

        let header_lines = vec![
            Line::from(vec![
                Span::styled("Current: ", Style::new().fg(colors::text_dim())),
                Span::styled(
                    current_label,
                    Style::new().fg(colors::text_bright()).bold(),
                ),
            ]),
            Line::from(""),
        ];

        let footer_lines = vec![hints::shortcut_line(&[
            KeyHint::new("↑↓", " select"),
            KeyHint::new("Enter", " apply"),
            KeyHint::new("e/→", " edit"),
            KeyHint::new("p", " pin"),
            KeyHint::new("Ctrl+P", " profiles"),
            KeyHint::new("Esc", " close"),
        ])];

        SettingsMenuPage::new(
            "Select Shell",
            SettingsPanelStyle::bottom_pane(),
            header_lines,
            footer_lines,
        )
    }

    fn list_runs(&self) -> Vec<SelectableLineRun<'_, usize>> {
        let selected_id = Some(self.selected_index);
        let mut runs = Vec::new();

        let mut auto_row = SettingsMenuRow::new(0usize, "Auto-detect shell")
            .with_detail(StyledText::new(
                "use system default",
                Style::new().fg(colors::text_dim()),
            ));
        auto_row.selected_hint = Some("clears override".into());
        let mut run = auto_row.into_run(selected_id);
        if selected_id == Some(0) {
            run.lines.push(Line::from(Span::styled(
                "    Clears the override and follows your default shell.",
                Style::new().fg(colors::text_dim()),
            )));
        }
        runs.push(run);

        for (idx, shell) in self.shells.iter().enumerate() {
            let item_idx = idx.saturating_add(1);
            let status = if shell.available { "" } else { " (not found)" };
            let display_name = shell.preset.display_name.as_str();
            let label = format!("{display_name}{status}");
            let style_label = shell
                .preset
                .script_style
                .as_deref()
                .and_then(ShellScriptStyle::parse)
                .or_else(|| ShellScriptStyle::infer_from_shell_program(&shell.preset.command))
                .map(|style| style.to_string())
                .unwrap_or_else(|| "auto".to_string());

            let mut row = SettingsMenuRow::new(item_idx, label)
                .with_value(StyledText::new(
                    format!("[{style_label}]"),
                    Style::new().fg(colors::text_dim()),
                ))
                .with_detail(StyledText::new(
                    shell.preset.command.as_str(),
                    Style::new().fg(colors::text_dim()),
                ));
            if selected_id == Some(item_idx) {
                row = row.with_selected_hint("Enter to apply, e/→ to edit");
            }
            let mut run = row.into_run(selected_id);

            if selected_id == Some(item_idx) {
                let desc = shell.preset.description.trim();
                if !desc.is_empty() {
                    run.lines.push(Line::from(Span::styled(
                        format!("    {desc}"),
                        Style::new().fg(colors::text_dim()),
                    )));
                }

                let resolved = shell
                    .resolved_path
                    .as_deref()
                    .unwrap_or("not found in PATH");
                run.lines.push(Line::from(Span::styled(
                    format!("    Binary: {resolved}"),
                    Style::new().fg(colors::text_dim()),
                )));

                if !shell.available {
                    run.lines.push(Line::from(Span::styled(
                        "    Not found. Press Enter to edit an explicit command.",
                        Style::new().fg(colors::text_dim()),
                    )));
                }
            }

            runs.push(run);
        }

        runs.push(SelectableLineRun::plain(vec![Line::from("")]));

        let custom_idx = self.shells.len().saturating_add(1);
        let mut custom_run =
            SettingsMenuRow::new(custom_idx, "Custom / pinned path...").into_run(selected_id);
        if selected_id == Some(custom_idx) {
            custom_run.lines.push(Line::from(Span::styled(
                "    Edit a custom command (or pin a resolved binary path).",
                Style::new().fg(colors::text_dim()),
            )));
        }
        runs.push(custom_run);

        runs
    }

    fn update_action_hover(&mut self, area: Rect, mouse_pos: (u16, u16)) -> bool {
        if !self.custom_input_mode {
            return false;
        }

        let page = self.edit_page();
        let Some(layout) = page.layout(area) else {
            return false;
        };
        let buttons = self.edit_buttons();
        let hovered =
            page.standard_action_at_end(&layout, mouse_pos.0, mouse_pos.1, &buttons);
        if hovered == self.hovered_action {
            return false;
        }
        self.hovered_action = hovered;
        true
    }

    fn edit_buttons(&self) -> Vec<StandardButtonSpec<EditAction>> {
        let focused = match self.edit_focus {
            EditFocus::Field => None,
            EditFocus::Actions => Some(self.selected_action),
        };
        standard_button_specs(&EDIT_ACTION_ITEMS, focused, self.hovered_action)
    }

    fn edit_page(&self) -> SettingsActionPage<'_> {
        let status_lines = vec![self.edit_status_line()];
        let footer_lines = vec![hints::shortcut_line(&[
            KeyHint::new("Tab", " focus"),
            KeyHint::new("Enter", " apply"),
            KeyHint::new("Ctrl+O", " pick"),
            KeyHint::new("Ctrl+V", " show"),
            KeyHint::new("Ctrl+R", " resolve"),
            KeyHint::new("Ctrl+T", " style"),
            KeyHint::new("Ctrl+P", " profiles"),
            KeyHint::new("Esc", " back"),
        ])];

        SettingsActionPage::new(
            "Edit Shell Command",
            SettingsPanelStyle::bottom_pane(),
            Vec::new(),
            footer_lines,
        )
        .with_status_lines(status_lines)
        .with_min_body_rows(6)
        .with_wrap_lines(true)
    }

    fn edit_status_line(&self) -> Line<'static> {
        let (status, status_style) = {
            let (path, _args) = split_command_and_args(self.custom_field.text());
            let trimmed = path.trim();
            if trimmed.is_empty() {
                (
                    "Enter a shell path or command".to_string(),
                    Style::new().fg(colors::text_dim()),
                )
            } else if trimmed.contains('/') || trimmed.contains('\\') {
                if std::path::Path::new(trimmed).exists() {
                    (
                        format!("OK ({trimmed})"),
                        Style::new().fg(colors::success()),
                    )
                } else {
                    (
                        format!("Not found ({trimmed})"),
                        Style::new().fg(colors::warning()),
                    )
                }
            } else {
                match which::which(trimmed) {
                    Ok(resolved) => {
                        let resolved = resolved.to_string_lossy();
                        (
                            format!("OK ({resolved})"),
                            Style::new().fg(colors::success()),
                        )
                    }
                    Err(_) => (
                        format!("Not found in PATH ({trimmed})"),
                        Style::new().fg(colors::warning()),
                    ),
                }
            }
        };

        let mut spans = vec![Span::styled(status, status_style)];
        if let Some(notice) = self.native_picker_notice.as_deref() {
            let notice = notice.trim();
            if !notice.is_empty() {
                spans.push(Span::styled(
                    format!("  •  {notice}"),
                    Style::new().fg(colors::warning()),
                ));
            }
        }
        Line::from(spans)
    }

    fn activate_edit_action(&mut self, action: EditAction) {
        match action {
            EditAction::Apply => self.submit_custom_path(),
            EditAction::Pick => {
                let _ = self.pick_shell_binary_from_dialog();
            }
            EditAction::Show => {
                let _ = self.show_custom_shell_in_file_manager();
            }
            EditAction::Resolve => {
                let _ = self.resolve_custom_shell_path_in_place();
            }
            EditAction::Style => self.cycle_custom_style_override(),
            EditAction::Back => {
                self.custom_input_mode = false;
                self.custom_field.set_text("");
                self.custom_style_override = None;
                self.native_picker_notice = None;
                self.edit_focus = EditFocus::Field;
                self.hovered_action = None;
            }
        }
    }

    pub(crate) fn handle_mouse_event_direct(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        if self.custom_input_mode {
            let page = self.edit_page();
            let Some(layout) = page.layout(area) else {
                return false;
            };
            let buttons = self.edit_buttons();
            return match mouse_event.kind {
                MouseEventKind::Moved => {
                    let hovered = page.standard_action_at_end(
                        &layout,
                        mouse_event.column,
                        mouse_event.row,
                        &buttons,
                    );
                    if hovered == self.hovered_action {
                        return false;
                    }
                    self.hovered_action = hovered;
                    return true;
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = page.standard_action_at_end(
                        &layout,
                        mouse_event.column,
                        mouse_event.row,
                        &buttons,
                    ) {
                        self.selected_action = action;
                        self.edit_focus = EditFocus::Actions;
                        self.activate_edit_action(action);
                        return true;
                    }

                    let field_outer = Rect::new(layout.body.x, layout.body.y, layout.body.width, 3);
                    if field_outer.contains(ratatui::layout::Position {
                        x: mouse_event.column,
                        y: mouse_event.row,
                    }) {
                        let focus_changed = self.edit_focus != EditFocus::Field;
                        self.edit_focus = EditFocus::Field;
                        self.hovered_action = None;
                        let inner = BorderedField::new(
                            "Shell command",
                            matches!(self.edit_focus, EditFocus::Field),
                        )
                        .inner(field_outer);
                        let handled = self.custom_field.handle_mouse_click(
                            mouse_event.column,
                            mouse_event.row,
                            inner,
                        );
                        return focus_changed || handled;
                    }

                    false
                }
                _ => false,
            }
        }

        let page = self.list_page();
        let Some(layout) = page.layout(area) else {
            return false;
        };
        let runs = self.list_runs();

        let mut selected = self.selected_index;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.item_count(),
            |x, y| selection_run_id_at(layout.body, x, y, 0, &runs),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_behavior: ScrollSelectionBehavior::Wrap,
                ..SelectableListMouseConfig::default()
            },
        );

        let mut handled = false;
        if selected != self.selected_index {
            self.selected_index = selected;
            handled = true;
        }

        if matches!(result, SelectableListMouseResult::Activated) {
            self.select_item(self.selected_index);
            handled = true;
        }

        handled || result.handled()
    }
}

impl<'a> BottomPaneView<'a> for ShellSelectionView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_direct(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_direct(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_mouse_event_direct(mouse_event, area))
    }

    fn update_hover(&mut self, mouse_pos: (u16, u16), area: Rect) -> bool {
        self.update_action_hover(area, mouse_pos)
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        // Options + current display + padding (description lines are handled by min height).
        let lines = self.item_count() + 6;
        (lines as u16).max(12)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if self.custom_input_mode {
            self.render_custom_mode(area, buf);
        } else {
            self.render_list_mode(area, buf);
        }
    }
}

impl ShellSelectionView {
    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        if self.custom_input_mode {
            return match (key_event.code, key_event.modifiers) {
                (KeyCode::Esc, _) => {
                    self.custom_input_mode = false;
                    self.custom_field.set_text("");
                    self.custom_style_override = None;
                    self.native_picker_notice = None;
                    self.edit_focus = EditFocus::Field;
                    self.hovered_action = None;
                    true
                }
                (KeyCode::Enter, _) => {
                    match self.edit_focus {
                        EditFocus::Field => self.submit_custom_path(),
                        EditFocus::Actions => self.activate_edit_action(self.selected_action),
                    }
                    true
                }
                (KeyCode::Char('o'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    self.pick_shell_binary_from_dialog()
                }
                (KeyCode::Char('v'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    self.show_custom_shell_in_file_manager()
                }
                (KeyCode::Tab, _) => {
                    self.edit_focus = match self.edit_focus {
                        EditFocus::Field => EditFocus::Actions,
                        EditFocus::Actions => EditFocus::Field,
                    };
                    true
                }
                (KeyCode::Char('p'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    self.open_shell_profiles_settings();
                    true
                }
                (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                    self.resolve_custom_shell_path_in_place()
                }
                (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                    self.cycle_custom_style_override();
                    true
                }
                (KeyCode::Left, _) if matches!(self.edit_focus, EditFocus::Actions) => {
                    let len = EDIT_ACTION_ITEMS.len();
                    let idx = EDIT_ACTION_ITEMS
                        .iter()
                        .position(|(id, _)| *id == self.selected_action)
                        .unwrap_or(0);
                    let next = if idx == 0 { len.saturating_sub(1) } else { idx - 1 };
                    self.selected_action = EDIT_ACTION_ITEMS[next].0;
                    true
                }
                (KeyCode::Right, _) if matches!(self.edit_focus, EditFocus::Actions) => {
                    let idx = EDIT_ACTION_ITEMS
                        .iter()
                        .position(|(id, _)| *id == self.selected_action)
                        .unwrap_or(0);
                    let next = (idx + 1) % EDIT_ACTION_ITEMS.len();
                    self.selected_action = EDIT_ACTION_ITEMS[next].0;
                    true
                }
                _ => match self.edit_focus {
                    EditFocus::Field => self.custom_field.handle_key(key_event),
                    EditFocus::Actions => false,
                },
            };
        }

        match (key_event.code, key_event.modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Char('q'), KeyModifiers::NONE) => {
                self.send_closed(false);
                true
            }
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                self.move_selection_up();
                true
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                self.move_selection_down();
                true
            }
            (KeyCode::Char('p'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                self.open_shell_profiles_settings();
                true
            }
            (KeyCode::Char('e'), KeyModifiers::NONE) => {
                let (prefill, style) = self.prefill_for_selection(self.selected_index);
                self.open_custom_input_with_prefill(prefill, style);
                true
            }
            (KeyCode::Char('p'), KeyModifiers::NONE) => {
                self.pin_selected_shell_binary();
                true
            }
            (KeyCode::Right, _) => {
                let (prefill, style) = self.prefill_for_selection(self.selected_index);
                self.open_custom_input_with_prefill(prefill, style);
                true
            }
            (KeyCode::Enter, _) => {
                self.confirm_selection();
                true
            }
            _ => false,
        }
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        if !self.custom_input_mode {
            return false;
        }

        if text.is_empty() {
            return false;
        }

        self.edit_focus = EditFocus::Field;
        self.hovered_action = None;
        self.custom_field.handle_paste(text);
        true
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn render_list_mode(&self, area: Rect, buf: &mut Buffer) {
        let page = self.list_page();
        let runs = self.list_runs();
        let mut rects = Vec::new();
        let _ = page.render_runs(area, buf, 0, &runs, &mut rects);
    }

    fn render_custom_mode(&self, area: Rect, buf: &mut Buffer) {
        let page = self.edit_page();
        let buttons = self.edit_buttons();
        let Some(layout) = page.render_with_standard_actions_end(area, buf, &buttons) else {
            return;
        };

        if layout.body.width == 0 || layout.body.height == 0 {
            return;
        }

        let field_outer = Rect::new(layout.body.x, layout.body.y, layout.body.width, 3);
        let field = BorderedField::new(
            "Shell command",
            matches!(self.edit_focus, EditFocus::Field),
        );
        field.render(field_outer, buf, &self.custom_field);

        let style_outer = Rect::new(
            layout.body.x,
            layout.body.y.saturating_add(3),
            layout.body.width,
            3,
        );
        let style_inner = BorderedField::new("Script style", false).render_block(style_outer, buf);
        let inferred = {
            let (path, _args) = split_command_and_args(self.custom_field.text());
            ShellScriptStyle::infer_from_shell_program(&path)
        };
        let (style_text, style_style) = match (self.custom_style_override, inferred) {
            (Some(style), _) => (
                format!("{style} (explicit)"),
                Style::new().fg(colors::primary()).bold(),
            ),
            (None, Some(style)) => (
                format!("auto (inferred: {style})"),
                Style::new().fg(colors::text_dim()),
            ),
            (None, None) => ("auto".to_string(), Style::new().fg(colors::text_dim())),
        };
        Paragraph::new(Line::from(Span::styled(style_text, style_style)))
            .render(style_inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn preset(command: &str) -> ShellPreset {
        ShellPreset {
            id: command.to_string(),
            command: command.to_string(),
            display_name: command.to_string(),
            description: String::new(),
            default_args: Vec::new(),
            script_style: None,
            show_in_picker: true,
        }
    }

    fn shell(path: &str) -> ShellConfig {
        ShellConfig {
            path: path.to_string(),
            args: Vec::new(),
            script_style: None,
            command_safety: code_core::config_types::CommandSafetyProfileConfig::default(),
            dangerous_command_detection: None,
        }
    }

    #[test]
    fn matches_termux_bash_path_to_bash_preset() {
        assert!(ShellSelectionView::current_matches_preset(
            &shell("/data/data/com.termux/files/usr/bin/bash"),
            &preset("bash"),
        ));
    }

    #[test]
    fn does_not_match_unrelated_basename() {
        assert!(!ShellSelectionView::current_matches_preset(
            &shell("/usr/bin/bashful"),
            &preset("bash"),
        ));
    }

    #[test]
    fn matches_windows_exe_suffix() {
        assert!(ShellSelectionView::current_matches_preset(
            &shell("C:\\Program Files\\PowerShell\\7\\pwsh.exe"),
            &preset("pwsh"),
        ));
    }

    #[test]
    fn list_mode_enter_on_auto_clears_override_and_closes_confirmed() {
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let mut view = ShellSelectionView::new(
            Some(shell("bash")),
            vec![preset("bash")],
            AppEventSender::new(tx),
        );
        view.selected_index = 0;
        assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Enter)));

        match rx.recv().expect("update selection") {
            AppEvent::UpdateShellSelection { path, .. } => {
                assert_eq!(path, "-");
            }
            other => panic!("unexpected event: {other:?}"),
        }
        match rx.recv().expect("closed") {
            AppEvent::ShellSelectionClosed { confirmed } => assert!(confirmed),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn ctrl_p_opens_shell_profiles_settings_and_closes_picker() {
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let mut view = ShellSelectionView::new(None, vec![preset("bash")], AppEventSender::new(tx));
        assert!(view.handle_key_event_direct(KeyEvent::new(
            KeyCode::Char('p'),
            KeyModifiers::CONTROL,
        )));

        match rx.recv().expect("closed") {
            AppEvent::ShellSelectionClosed { confirmed } => assert!(!confirmed),
            other => panic!("unexpected event: {other:?}"),
        }
        match rx.recv().expect("open settings") {
            AppEvent::OpenSettings { section } => {
                assert_eq!(section, Some(SettingsSection::ShellProfiles));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn edit_mode_tab_switches_focus_and_enter_activates_back_action() {
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let mut view = ShellSelectionView::new(None, vec![preset("bash")], AppEventSender::new(tx));
        view.open_custom_input_with_prefill("bash".to_string(), None);
        assert!(view.custom_input_mode);
        assert_eq!(view.edit_focus, EditFocus::Field);

        assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Tab)));
        assert_eq!(view.edit_focus, EditFocus::Actions);

        // Move selection to Back.
        for _ in 0..5 {
            assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Right)));
        }
        assert_eq!(view.selected_action, EditAction::Back);
        assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Enter)));
        assert!(!view.custom_input_mode);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn edit_mode_apply_action_submits_and_closes() {
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let mut view = ShellSelectionView::new(None, vec![preset("bash")], AppEventSender::new(tx));
        view.open_custom_input_with_prefill("bash".to_string(), None);
        assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Tab)));
        assert_eq!(view.edit_focus, EditFocus::Actions);
        assert_eq!(view.selected_action, EditAction::Apply);
        assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Enter)));

        match rx.recv().expect("update selection") {
            AppEvent::UpdateShellSelection { path, args, .. } => {
                assert_eq!(path, "bash");
                assert!(args.is_empty());
            }
            other => panic!("unexpected event: {other:?}"),
        }
        match rx.recv().expect("closed") {
            AppEvent::ShellSelectionClosed { confirmed } => assert!(confirmed),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
