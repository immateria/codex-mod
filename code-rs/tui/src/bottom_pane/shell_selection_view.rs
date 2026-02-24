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
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use code_common::shell_presets::ShellPreset;
use code_core::config_types::ShellConfig;
use code_core::config_types::ShellScriptStyle;
use code_core::split_command_and_args;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
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
}

pub(crate) struct ShellSelectionView {
    shells: Vec<ShellOption>,
    selected_index: usize,
    hovered_index: Option<usize>,
    current_shell: Option<ShellConfig>,
    app_event_tx: AppEventSender,
    is_complete: bool,
    custom_input_mode: bool,
    custom_input: String,
    custom_style_override: Option<ShellScriptStyle>,
    /// Cached item rects from last render for mouse hit testing
    item_rects: RefCell<Vec<Rect>>,
    /// Rect for custom option
    custom_rect: RefCell<Option<Rect>>,
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
                let available = which::which(&preset.command).is_ok();
                ShellOption { preset, available }
            })
            .collect();

        let initial_index = if let Some(ref current) = current_shell {
            shells
                .iter()
                .position(|s| Self::current_matches_preset(current, &s.preset))
                .unwrap_or(0)
        } else {
            0
        };

        Self {
            shells,
            selected_index: initial_index,
            hovered_index: None,
            current_shell,
            app_event_tx,
            is_complete: false,
            custom_input_mode: false,
            custom_input: String::new(),
            custom_style_override: None,
            item_rects: RefCell::new(Vec::new()),
            custom_rect: RefCell::new(None),
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
        self.shells.len() + 1
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
            self.selected_index = self.shells.len();
        } else {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
        self.hovered_index = None; // Clear hover when using keyboard
    }

    fn move_selection_down(&mut self) {
        self.selected_index = (self.selected_index + 1) % (self.shells.len() + 1);
        self.hovered_index = None; // Clear hover when using keyboard
    }

    fn confirm_selection(&mut self) {
        self.select_item(self.selected_index);
    }

    fn select_item(&mut self, index: usize) {
        if index == self.shells.len() {
            // Custom path option selected
            self.custom_input_mode = true;
            self.custom_style_override = None;
            return;
        }

        if let Some(shell) = self.shells.get(index) {
            if !shell.available {
                // Show notice that shell is not available - enter custom mode with command pre-filled
                self.custom_input_mode = true;
                self.custom_input = shell.preset.command.clone();
                self.custom_style_override = shell
                    .preset
                    .script_style
                    .as_deref()
                    .and_then(ShellScriptStyle::parse)
                    .or_else(|| ShellScriptStyle::infer_from_shell_program(&self.custom_input));
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
        let (path, args) = split_command_and_args(self.custom_input.as_str());
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

    fn send_closed(&mut self, confirmed: bool) {
        self.is_complete = true;
        self.app_event_tx.send(AppEvent::ShellSelectionClosed { confirmed });
    }

    /// Find which item index a screen position corresponds to
    fn hit_test(&self, x: u16, y: u16) -> Option<usize> {
        // Check shell items
        let item_rects = self.item_rects.borrow();
        for (idx, rect) in item_rects.iter().enumerate() {
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                return Some(idx);
            }
        }

        // Check custom option
        if let Some(rect) = *self.custom_rect.borrow()
            && x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                return Some(self.shells.len());
            }

        None
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent) -> bool {
        if self.custom_input_mode {
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
        // Shell options + custom option + current display + padding
        let shell_count = self.shells.len();
        let has_current = self.current_shell.is_some();
        let extra_lines = if has_current { 3 } else { 1 };
        let lines = shell_count + 2 + extra_lines;
        (lines as u16).max(12)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let title = if self.custom_input_mode {
            "Enter Custom Shell Path"
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
                    self.custom_input.clear();
                    self.custom_style_override = None;
                    true
                }
                (KeyCode::Enter, _) => {
                    self.submit_custom_path();
                    true
                }
                (KeyCode::Tab, _) => {
                    self.custom_style_override = match self.custom_style_override {
                        None => Some(ShellScriptStyle::PosixSh),
                        Some(ShellScriptStyle::PosixSh) => Some(ShellScriptStyle::BashZshCompatible),
                        Some(ShellScriptStyle::BashZshCompatible) => Some(ShellScriptStyle::Zsh),
                        Some(ShellScriptStyle::Zsh) => None,
                    };
                    true
                }
                (KeyCode::Backspace, _) => {
                    self.custom_input.pop();
                    true
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.custom_input.push(c);
                    true
                }
                _ => false,
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

        self.custom_input.push_str(&text);
        true
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn render_custom_input(&self, area: Rect, buf: &mut Buffer) {
        let prompt_line = Line::from(vec![
            Span::styled("Enter shell path: ", Style::default()),
            Span::styled(&self.custom_input, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("█", Style::default()),
        ]);

        let inferred = {
            let (path, _args) = split_command_and_args(self.custom_input.as_str());
            ShellScriptStyle::infer_from_shell_program(&path)
        };
        let style_label = match (self.custom_style_override, inferred) {
            (Some(style), _) => format!("Style: {style} (explicit)"),
            (None, Some(style)) => format!("Style: auto (inferred: {style})"),
            (None, None) => "Style: auto".to_string(),
        };

        let style_line = Line::from(Span::styled(
            style_label,
            Style::default().fg(colors::text_dim()),
        ));

        let help_line = Line::from(Span::styled(
            "Tip: Tab cycles script style. Enter applies. Esc cancels.",
            Style::default().fg(colors::text_dim()),
        ));

        let para = Paragraph::new(vec![prompt_line, style_line, Line::raw(""), help_line])
            .alignment(Alignment::Left);
        para.render(area, buf);
    }

    fn render_shell_list(&self, area: Rect, buf: &mut Buffer) {
        let mut item_rects = self.item_rects.borrow_mut();
        item_rects.clear();
        *self.custom_rect.borrow_mut() = None;

        let mut lines = Vec::new();
        let mut current_y = area.y;

        // Show current shell if set
        if let Some(ref current) = self.current_shell {
            let current_label = Self::display_shell(current);
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

        // Render shell options
        for (idx, shell) in self.shells.iter().enumerate() {
            let is_selected = idx == self.selected_index;
            let is_hovered = self.hovered_index == Some(idx);
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

            // Store the rect for this item
            item_rects.push(Rect {
                x: area.x,
                y: current_y,
                width: area.width,
                height: 1,
            });

            lines.push(Line::from(vec![
                Span::styled(prefix, name_style),
                Span::styled(format!("{}{}", shell.preset.display_name, status), name_style),
                Span::styled(" - ", Style::default().fg(colors::text_dim())),
                Span::styled(&shell.preset.command, Style::default().fg(colors::text_dim())),
            ]));
            current_y += 1;

            if is_selected {
                lines.push(Line::from(Span::styled(
                    format!("    {}", shell.preset.description),
                    Style::default().fg(colors::text_dim()),
                )));
                current_y += 1;
            }
        }

        // Custom path option
        let is_custom_selected = self.selected_index == self.shells.len();
        let is_custom_hovered = self.hovered_index == Some(self.shells.len());
        let custom_prefix = if is_custom_selected { "▶ " } else { "  " };
        let custom_style = if is_custom_selected || is_custom_hovered {
            Style::default().bg(colors::selection()).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::raw(""));
        current_y += 1;

        // Store the rect for custom option
        *self.custom_rect.borrow_mut() = Some(Rect {
            x: area.x,
            y: current_y,
            width: area.width,
            height: 1,
        });

        lines.push(Line::from(vec![
            Span::styled(custom_prefix, custom_style),
            Span::styled("Custom path...", custom_style),
        ]));

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
