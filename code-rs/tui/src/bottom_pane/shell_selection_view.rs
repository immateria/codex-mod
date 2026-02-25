use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use super::settings_panel::{render_panel, PanelFrameStyle};
use super::BottomPane;
use super::SettingsSection;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crate::components::form_text_field::FormTextField;
use crate::native_picker::{pick_path, NativePickerKind};
use crate::util::buffer::write_line;
use code_common::shell_presets::ShellPreset;
use code_core::config_types::ShellConfig;
use code_core::config_types::ShellScriptStyle;
use code_core::split_command_and_args;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::cell::RefCell;

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
    hovered_index: Option<usize>,
    current_shell: Option<ShellConfig>,
    app_event_tx: AppEventSender,
    is_complete: bool,
    custom_input_mode: bool,
    custom_field: FormTextField,
    custom_style_override: Option<ShellScriptStyle>,
    native_picker_notice: Option<String>,
    /// Cached item rects from last render for mouse hit testing
    item_rects: RefCell<Vec<Rect>>,
    /// Cached rect for the custom input field for mouse hit testing
    custom_field_rect: RefCell<Option<Rect>>,
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
            hovered_index: None,
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
            item_rects: RefCell::new(Vec::new()),
            custom_field_rect: RefCell::new(None),
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

    fn set_hovered_index(&mut self, hovered: Option<usize>) -> bool {
        if self.hovered_index == hovered {
            return false;
        }
        self.hovered_index = hovered;
        true
    }

    fn move_selection_up(&mut self) {
        if self.selected_index == 0 {
            self.selected_index = self.item_count().saturating_sub(1);
        } else {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
        self.hovered_index = None; // Clear hover when using keyboard
    }

    fn move_selection_down(&mut self) {
        self.selected_index = (self.selected_index + 1) % self.item_count();
        self.hovered_index = None; // Clear hover when using keyboard
    }

    fn confirm_selection(&mut self) {
        self.select_item(self.selected_index);
    }

    fn open_custom_input_with_prefill(&mut self, prefill: String, style: Option<ShellScriptStyle>) {
        self.custom_input_mode = true;
        self.custom_field.set_text(&prefill);
        self.custom_style_override = style;
        self.native_picker_notice = None;
        self.hovered_index = None;
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

    /// Find which item index a screen position corresponds to
    fn hit_test(&self, x: u16, y: u16) -> Option<usize> {
        // Check cached item rects in render order (auto + presets + custom).
        let item_rects = self.item_rects.borrow();
        for (idx, rect) in item_rects.iter().enumerate() {
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                return Some(idx);
            }
        }

        None
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent) -> bool {
        if self.custom_input_mode {
            if let Some(area) = *self.custom_field_rect.borrow() {
                match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        return self.custom_field.handle_mouse_click(
                            mouse_event.column,
                            mouse_event.row,
                            area,
                        );
                    }
                    _ => return false,
                }
            }
            return false;
        }

        let mut selected = self.selected_index;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.item_count(),
            |x, y| self.hit_test(x, y),
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

        if matches!(mouse_event.kind, MouseEventKind::Moved) {
            handled |= self.set_hovered_index(self.hit_test(mouse_event.column, mouse_event.row));
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
        _area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_mouse_event_direct(mouse_event))
    }

    fn update_hover(&mut self, mouse_pos: (u16, u16), _area: Rect) -> bool {
        if self.custom_input_mode {
            return false;
        }

        self.set_hovered_index(self.hit_test(mouse_pos.0, mouse_pos.1))
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
        let title = if self.custom_input_mode {
            "Edit Shell Command"
        } else {
            "Select Shell"
        };

        render_panel(area, buf, title, PanelFrameStyle::bottom_pane(), |content_area, buf| {
            if self.custom_input_mode {
                self.render_custom_input(content_area, buf);
            } else {
                self.render_shell_list(content_area, buf);
            }
        });
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
                    true
                }
                (KeyCode::Enter, _) => {
                    self.submit_custom_path();
                    true
                }
                (KeyCode::Char('o'), mods) if mods.contains(KeyModifiers::CONTROL) => {
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
                (KeyCode::Tab, _) | (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                    self.custom_style_override = match self.custom_style_override {
                        None => Some(ShellScriptStyle::PosixSh),
                        Some(ShellScriptStyle::PosixSh) => Some(ShellScriptStyle::BashZshCompatible),
                        Some(ShellScriptStyle::BashZshCompatible) => Some(ShellScriptStyle::Zsh),
                        Some(ShellScriptStyle::Zsh) => None,
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
                _ => self.custom_field.handle_key(key_event),
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

        self.custom_field.handle_paste(text);
        true
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn render_custom_input(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height < 4 {
            return;
        }

        let label_area = Rect::new(area.x, area.y, area.width, 1);
        let field_area = Rect::new(area.x, area.y.saturating_add(1), area.width, 1);
        let style_area = Rect::new(area.x, area.y.saturating_add(2), area.width, 1);
        let help_area = Rect::new(area.x, area.y.saturating_add(3), area.width, 1);

        write_line(
            buf,
            label_area.x,
            label_area.y,
            label_area.width,
            &Line::from(vec![Span::styled(
                "Shell command (path + args):".to_string(),
                Style::default().fg(colors::text_dim()),
            )]),
            Style::default().bg(colors::background()),
        );

        *self.custom_field_rect.borrow_mut() = Some(field_area);
        self.custom_field.render(field_area, buf, true);

        let inferred = {
            let (path, _args) = split_command_and_args(self.custom_field.text());
            ShellScriptStyle::infer_from_shell_program(&path)
        };
        let style_label = match (self.custom_style_override, inferred) {
            (Some(style), _) => format!("Style: {style} (explicit)"),
            (None, Some(style)) => format!("Style: auto (inferred: {style})"),
            (None, None) => "Style: auto".to_string(),
        };

        write_line(
            buf,
            style_area.x,
            style_area.y,
            style_area.width,
            &Line::from(Span::styled(style_label, Style::default().fg(colors::text_dim()))),
            Style::default().bg(colors::background()),
        );

        let status = {
            let (path, _args) = split_command_and_args(self.custom_field.text());
            let trimmed = path.trim();
            if trimmed.is_empty() {
                "Status: enter a shell path or command".to_string()
            } else if trimmed.contains('/') || trimmed.contains('\\') {
                let exists = std::path::Path::new(trimmed).exists();
                if exists {
                    format!("Status: OK ({trimmed})")
                } else {
                    format!("Status: not found ({trimmed})")
                }
            } else {
                match which::which(trimmed) {
                    Ok(resolved) => format!("Status: OK ({})", resolved.to_string_lossy()),
                    Err(_) => format!("Status: not found in PATH ({trimmed})"),
                }
            }
        };
        let notice = self.native_picker_notice.as_deref().unwrap_or_default();
        let notice = notice.trim();
        let notice = if notice.is_empty() {
            String::new()
        } else {
            format!("  •  {notice}")
        };
        write_line(
            buf,
            help_area.x,
            help_area.y,
            help_area.width,
            &Line::from(Span::styled(
                format!("{status}{notice}  •  Enter apply  •  Ctrl+O pick  •  Ctrl+R resolve  •  Ctrl+T style  •  Esc back"),
                Style::default().fg(colors::text_dim()),
            )),
            Style::default().bg(colors::background()),
        );
    }

    fn render_shell_list(&self, area: Rect, buf: &mut Buffer) {
        let mut item_rects = self.item_rects.borrow_mut();
        item_rects.clear();
        *self.custom_field_rect.borrow_mut() = None;

        let mut lines = Vec::new();
        let mut current_y = area.y;

        // Show current shell (or auto) at the top.
        {
            let current_label = match self.current_shell.as_ref() {
                Some(current) => {
                    let label = Self::display_shell(current);
                    let style = current
                        .script_style
                        .or_else(|| ShellScriptStyle::infer_from_shell_program(&current.path))
                        .map(|style| style.to_string())
                        .unwrap_or_else(|| "auto".to_string());
                    format!("{label} (style: {style})")
                }
                None => "auto-detected".to_string(),
            };
            lines.push(Line::from(vec![
                Span::styled("Current: ", Style::default()),
                Span::styled(
                    current_label,
                    Style::default().fg(colors::text_bright()).add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::raw(""));
            current_y += 2;
        }

        // Auto option
        let is_auto_selected = self.selected_index == 0;
        let is_auto_hovered = self.hovered_index == Some(0);
        let auto_prefix = if is_auto_selected { "▶ " } else { "  " };
        let auto_style = if is_auto_selected || is_auto_hovered {
            Style::default()
                .bg(colors::selection())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let auto_start_y = current_y;
        lines.push(Line::from(vec![
            Span::styled(auto_prefix, auto_style),
            Span::styled("Auto-detect shell", auto_style),
            Span::styled(" - ", Style::default().fg(colors::text_dim())),
            Span::styled("use system default", Style::default().fg(colors::text_dim())),
        ]));
        current_y += 1;
        if is_auto_selected {
            lines.push(Line::from(Span::styled(
                "    Clears the override and follows your default shell.",
                Style::default().fg(colors::text_dim()).bg(colors::selection()),
            )));
            current_y += 1;
        }
        item_rects.push(Rect {
            x: area.x,
            y: auto_start_y,
            width: area.width,
            height: current_y.saturating_sub(auto_start_y),
        });

        // Render shell options
        for (idx, shell) in self.shells.iter().enumerate() {
            let item_idx = idx.saturating_add(1);
            let is_selected = item_idx == self.selected_index;
            let is_hovered = self.hovered_index == Some(item_idx);
            let prefix = if is_selected { "▶ " } else { "  " };

            let name_style = if shell.available {
                if is_selected || is_hovered {
                    Style::default().bg(colors::selection()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                }
            } else {
                Style::default().fg(colors::text_dim())
            };

            let status = if shell.available { "" } else { " (not found)" };
            let item_start_y = current_y;

            let style_label = shell
                .preset
                .script_style
                .as_deref()
                .and_then(ShellScriptStyle::parse)
                .or_else(|| ShellScriptStyle::infer_from_shell_program(&shell.preset.command))
                .map(|style| style.to_string())
                .unwrap_or_else(|| "auto".to_string());

            lines.push(Line::from(vec![
                Span::styled(prefix, name_style),
                Span::styled(format!("{}{}", shell.preset.display_name, status), name_style),
                Span::styled(format!(" [{style_label}]"), Style::default().fg(colors::text_dim())),
                Span::styled(" - ", Style::default().fg(colors::text_dim())),
                Span::styled(&shell.preset.command, Style::default().fg(colors::text_dim())),
            ]));
            current_y += 1;

            if is_selected {
                let info_style = Style::default().fg(colors::text_dim()).bg(colors::selection());
                lines.push(Line::from(Span::styled(
                    format!("    {}", shell.preset.description),
                    info_style,
                )));
                current_y += 1;
                let resolved = shell
                    .resolved_path
                    .as_deref()
                    .unwrap_or("not found in PATH");
                lines.push(Line::from(Span::styled(
                    format!("    Binary: {resolved} (press p to pin, e/→ to edit)"),
                    info_style,
                )));
                current_y += 1;
            }

            item_rects.push(Rect {
                x: area.x,
                y: item_start_y,
                width: area.width,
                height: current_y.saturating_sub(item_start_y),
            });
        }

        // Custom path option
        let custom_idx = self.shells.len().saturating_add(1);
        let is_custom_selected = self.selected_index == custom_idx;
        let is_custom_hovered = self.hovered_index == Some(custom_idx);
        let custom_prefix = if is_custom_selected { "▶ " } else { "  " };
        let custom_style = if is_custom_selected || is_custom_hovered {
            Style::default().bg(colors::selection()).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::raw(""));
        current_y += 1;

        let custom_start_y = current_y;

        lines.push(Line::from(vec![
            Span::styled(custom_prefix, custom_style),
            Span::styled("Custom / pinned path...", custom_style),
        ]));
        current_y += 1;
        item_rects.push(Rect {
            x: area.x,
            y: custom_start_y,
            width: area.width,
            height: current_y.saturating_sub(custom_start_y),
        });

        lines.push(Line::from(Span::styled(
            "Keys: Enter apply  •  e/→ edit/pin path  •  p pin resolved  •  Ctrl+P profiles  •  Esc close".to_string(),
            Style::default().fg(colors::text_dim()),
        )));

        let para = Paragraph::new(lines).alignment(Alignment::Left);
        para.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
