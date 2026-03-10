use std::fs;
use std::path::PathBuf;

use code_core::config::find_code_home;
use code_core::protocol::Op;
use code_protocol::custom_prompts::CustomPrompt;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::prelude::Widget;
use ratatui::widgets::Paragraph;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crate::slash_command::built_in_slash_commands;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use crate::components::form_text_field::{FormTextField, InputFilter};
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use super::settings_ui::buttons::{render_text_button_strip, text_button_at, TextButton};
use super::settings_ui::fields::BorderedField;
use super::settings_ui::frame::SettingsFrame;
use super::settings_ui::rows::selection_index_at as row_selection_index_at;
use super::BottomPane;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    List,
    Name,
    Body,
    Save,
    Delete,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    List,
    Edit,
}

pub(crate) struct PromptsSettingsView {
    prompts: Vec<CustomPrompt>,
    selected: usize,
    focus: Focus,
    name_field: FormTextField,
    body_field: FormTextField,
    status: Option<(String, Style)>,
    app_event_tx: AppEventSender,
    is_complete: bool,
    mode: Mode,
}

impl PromptsSettingsView {
    const DEFAULT_HEIGHT: u16 = 20;
    const BUTTON_LABELS: [&str; 3] = ["Save", "Delete", "Cancel"];

    pub fn new(prompts: Vec<CustomPrompt>, app_event_tx: AppEventSender) -> Self {
        let mut name_field = FormTextField::new_single_line();
        name_field.set_filter(InputFilter::Id);
        let body_field = FormTextField::new_multi_line();
        
        Self {
            prompts,
            selected: 0,
            focus: Focus::List,
            name_field,
            body_field,
            status: None,
            app_event_tx,
            is_complete: false,
            mode: Mode::List,
        }
    }

    fn name_field_title(&self) -> &'static str {
        if matches!(self.focus, Focus::Name) {
            "Name (slug) • Enter to save"
        } else {
            "Name (slug)"
        }
    }

    fn body_field_title(&self) -> &'static str {
        if matches!(self.focus, Focus::Body) {
            "Content (multiline)"
        } else {
            "Content"
        }
    }

    pub fn handle_key_event_direct(&mut self, key: KeyEvent) -> bool {
        if self.is_complete {
            return true;
        }
        match self.mode {
            Mode::List => match key {
                KeyEvent { code: KeyCode::Esc, .. } => {
                    self.is_complete = true;
                    true
                }
                KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                    self.enter_editor();
                    true
                }
                KeyEvent { code: KeyCode::Char('n'), modifiers, .. }
                    if modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.start_new_prompt();
                    true
                }
                other => self.handle_list_key(other),
            },
            Mode::Edit => match key {
                KeyEvent { code: KeyCode::Esc, .. } => {
                    self.mode = Mode::List;
                    self.focus = Focus::List;
                    self.status = None;
                    true
                }
                KeyEvent { code: KeyCode::Tab, .. } => {
                    self.cycle_focus(true);
                    true
                }
                KeyEvent { code: KeyCode::BackTab, .. } => {
                    self.cycle_focus(false);
                    true
                }
                KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                    match self.focus {
                        Focus::Save => self.save_current(),
                        Focus::Delete => self.delete_current(),
                        Focus::Cancel => {
                            self.mode = Mode::List;
                            self.focus = Focus::List;
                            self.status = None;
                        }
                        _ => {}
                    }
                    true
                }
                KeyEvent { code: KeyCode::Char('n'), modifiers, .. }
                    if modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.start_new_prompt();
                    true
                }
                _ => match self.focus {
                    Focus::Name => {
                        self.name_field.handle_key(key);
                        true
                    }
                    Focus::Body => {
                        self.body_field.handle_key(key);
                        true
                    }
                    Focus::Save | Focus::Delete | Focus::Cancel => false,
                    Focus::List => self.handle_list_key(key),
                },
            },
        }
    }

    pub fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        if self.is_complete || area.width == 0 || area.height == 0 {
            return false;
        }

        match self.mode {
            Mode::List => self.handle_list_mouse_event(mouse_event, area),
            Mode::Edit => self.handle_edit_mouse_event(mouse_event, area),
        }
    }

    pub fn is_complete(&self) -> bool { self.is_complete }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 { return; }
        self.render_body(area, buf);
    }

    fn render_body(&self, area: Rect, buf: &mut Buffer) {
        match self.mode {
            Mode::List => self.render_list(area, buf),
            Mode::Edit => self.render_form(area, buf),
        }
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer) {
        let Some(layout) = SettingsFrame::new("Custom Prompts", self.list_header_lines(), Vec::new())
            .render(area, buf)
        else {
            return;
        };

        let mut lines: Vec<Line<'static>> = Vec::new();
        for (idx, p) in self.prompts.iter().enumerate() {
            let preview = p.content.lines().next().unwrap_or("").trim();
            let arrow = if idx == self.selected { "›" } else { " " };
            let name_style = if idx == self.selected {
                Style::default().fg(colors::primary()).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::text())
            };
            let name_span = Span::styled(format!("{arrow} /{}", p.name), name_style);
            let preview_span = Span::styled(
                format!("  {preview}"),
                Style::default().fg(colors::text_dim()),
            );
            let mut spans = vec![name_span];
            if !preview.is_empty() { spans.push(preview_span); }
            let line = Line::from(spans);
            lines.push(line);
        }
        if lines.is_empty() {
            lines.push(Line::from("No prompts yet. Press Enter or Ctrl+N to create one."));
        }

        // Add new row
        let add_arrow = if self.selected == self.prompts.len() { "›" } else { " " };
        let add_style = if self.selected == self.prompts.len() {
            Style::default().fg(colors::primary()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::success()).add_modifier(Modifier::BOLD)
        };
        let add_line = Line::from(vec![Span::styled(format!("{add_arrow} Add new…"), add_style)]);
        lines.push(add_line);

        let list = Paragraph::new(lines)
            .style(Style::default().bg(colors::background()));
        list.render(layout.body, buf);
    }

    fn render_form(&self, area: Rect, buf: &mut Buffer) {
        let Some(layout) = SettingsFrame::new("Custom Prompt", Vec::new(), vec![Line::from("")])
            .render(area, buf)
        else {
            return;
        };
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // name block
                Constraint::Min(6),    // body block
                Constraint::Length(1), // status
            ])
            .split(layout.body);

        let name_title = self.name_field_title();
        let name_block = BorderedField::new(name_title, matches!(self.focus, Focus::Name));
        let _ = name_block.render(vertical[0], buf, &self.name_field);

        let body_title = self.body_field_title();
        let body_block = BorderedField::new(body_title, matches!(self.focus, Focus::Body));
        let _ = body_block.render(vertical[1], buf, &self.body_field);

        if let Some((msg, style)) = &self.status {
            Paragraph::new(Line::from(Span::styled(msg.clone(), *style)))
                .render(vertical[2], buf);
        }

        render_text_button_strip(
            layout.footer,
            buf,
            &[
                TextButton::new(
                    Focus::Save,
                    Self::BUTTON_LABELS[0],
                    matches!(self.focus, Focus::Save),
                    false,
                    Style::default().fg(colors::success()).add_modifier(Modifier::BOLD),
                ),
                TextButton::new(
                    Focus::Delete,
                    Self::BUTTON_LABELS[1],
                    matches!(self.focus, Focus::Delete),
                    false,
                    Style::default().fg(colors::error()).add_modifier(Modifier::BOLD),
                ),
                TextButton::new(
                    Focus::Cancel,
                    Self::BUTTON_LABELS[2],
                    matches!(self.focus, Focus::Cancel),
                    false,
                    Style::default().fg(colors::text_dim()).add_modifier(Modifier::BOLD),
                ),
            ],
        );
    }

    fn list_selection_at(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        let Some(layout) = SettingsFrame::new("Custom Prompts", self.list_header_lines(), Vec::new())
            .layout(area)
        else {
            return None;
        };
        let rel_y = row_selection_index_at(layout.body, x, y, 0, layout.visible_rows())?;
        if self.prompts.is_empty() {
            (rel_y == 1).then_some(0)
        } else if rel_y <= self.prompts.len() {
            Some(rel_y)
        } else {
            None
        }
    }

    fn button_focus_at(&self, buttons_area: Rect, mouse_event: MouseEvent) -> Option<Focus> {
        text_button_at(
            mouse_event.column,
            mouse_event.row,
            buttons_area,
            &[
                TextButton::new(Focus::Save, Self::BUTTON_LABELS[0], false, false, Style::new()),
                TextButton::new(Focus::Delete, Self::BUTTON_LABELS[1], false, false, Style::new()),
                TextButton::new(Focus::Cancel, Self::BUTTON_LABELS[2], false, false, Style::new()),
            ],
        )
    }

    fn list_header_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                "Custom prompts allow you to save reusable prompts initiated with a simple slash command. They are invoked with /name. Create and update your custom prompts below.",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(""),
        ]
    }

    fn handle_list_mouse_event(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mut selected = self.selected;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.prompts.len().saturating_add(1),
            |x, y| self.list_selection_at(area, x, y),
            SelectableListMouseConfig {
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );
        self.selected = selected;
        if matches!(result, SelectableListMouseResult::Activated) {
            self.enter_editor();
        }
        result.handled()
    }

    fn handle_edit_mouse_event(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let Some(layout) =
            SettingsFrame::new("Custom Prompt", Vec::new(), vec![Line::from("")]).layout(area)
        else {
            return false;
        };
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(6),
                Constraint::Length(1),
            ])
            .split(layout.body);
        let name_rect = vertical[0];
        let body_rect = vertical[1];
        let status_rect = vertical[2];
        let buttons_rect = layout.footer;

        let name_inner =
            BorderedField::new(self.name_field_title(), matches!(self.focus, Focus::Name))
                .inner(name_rect);
        let body_inner =
            BorderedField::new(self.body_field_title(), matches!(self.focus, Focus::Body))
                .inner(body_rect);

        match mouse_event.kind {
            MouseEventKind::Moved => {
                if let Some(focus) = self.button_focus_at(buttons_rect, mouse_event) {
                    if self.focus == focus {
                        return false;
                    }
                    self.focus = focus;
                    return true;
                }
                false
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let col = mouse_event.column;
                let row = mouse_event.row;
                if col >= name_rect.x
                    && col < name_rect.x.saturating_add(name_rect.width)
                    && row >= name_rect.y
                    && row < name_rect.y.saturating_add(name_rect.height)
                {
                    self.focus = Focus::Name;
                    let _ = self.name_field.handle_mouse_click(col, row, name_inner);
                    return true;
                }
                if col >= body_rect.x
                    && col < body_rect.x.saturating_add(body_rect.width)
                    && row >= body_rect.y
                    && row < body_rect.y.saturating_add(body_rect.height)
                {
                    self.focus = Focus::Body;
                    let _ = self.body_field.handle_mouse_click(col, row, body_inner);
                    return true;
                }
                if status_rect.contains(ratatui::layout::Position { x: col, y: row }) {
                    return false;
                }
                if let Some(focus) = self.button_focus_at(buttons_rect, mouse_event) {
                    self.focus = focus;
                    match focus {
                        Focus::Save => self.save_current(),
                        Focus::Delete => self.delete_current(),
                        Focus::Cancel => {
                            self.mode = Mode::List;
                            self.focus = Focus::List;
                            self.status = None;
                        }
                        Focus::List | Focus::Name | Focus::Body => {}
                    }
                    return true;
                }
                false
            }
            MouseEventKind::ScrollUp => {
                if mouse_event.column >= body_rect.x
                    && mouse_event.column < body_rect.x.saturating_add(body_rect.width)
                    && mouse_event.row >= body_rect.y
                    && mouse_event.row < body_rect.y.saturating_add(body_rect.height)
                {
                    self.focus = Focus::Body;
                    return self.body_field.handle_mouse_scroll(false);
                }
                false
            }
            MouseEventKind::ScrollDown => {
                if mouse_event.column >= body_rect.x
                    && mouse_event.column < body_rect.x.saturating_add(body_rect.width)
                    && mouse_event.row >= body_rect.y
                    && mouse_event.row < body_rect.y.saturating_add(body_rect.height)
                {
                    self.focus = Focus::Body;
                    return self.body_field.handle_mouse_scroll(true);
                }
                false
            }
            _ => false,
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => {
                if self.selected > 0 { self.selected -= 1; }
                true
            }
            KeyCode::Down => {
                let max = self.prompts.len();
                if self.selected < max { self.selected += 1; }
                true
            }
            _ => false,
        }
    }

    fn start_new_prompt(&mut self) {
        self.selected = self.prompts.len();
        self.name_field.set_text("");
        self.body_field.set_text("");
        self.focus = Focus::Name;
        self.status = Some(("New prompt".to_string(), Style::default().fg(colors::info())));
        self.mode = Mode::Edit;
    }

    fn load_selected_into_form(&mut self) {
        if let Some(p) = self.prompts.get(self.selected) {
            self.name_field.set_text(&p.name);
            self.body_field.set_text(&p.content);
            self.focus = Focus::Name;
            self.status = None;
        }
    }

    fn enter_editor(&mut self) {
        if self.selected >= self.prompts.len() {
            self.start_new_prompt();
        } else {
            self.load_selected_into_form();
            self.mode = Mode::Edit;
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        let order = [Focus::List, Focus::Name, Focus::Body, Focus::Save, Focus::Delete, Focus::Cancel];
        let mut idx = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        if forward {
            idx = (idx + 1) % order.len();
        } else {
            idx = idx.checked_sub(1).unwrap_or(order.len() - 1);
        }
        self.focus = order[idx];
    }

    fn validate(&self, name: &str) -> Result<(), String> {
        let slug = name.trim();
        if slug.is_empty() {
            return Err("Name is required".to_string());
        }
        if !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err("Name must use letters, numbers, '-', '_' or '.'".to_string());
        }

        let slug_lower = slug.to_ascii_lowercase();
        if built_in_slash_commands()
            .into_iter()
            .map(|(name, _)| name)
            .any(|name| name.eq_ignore_ascii_case(&slug_lower))
        {
            return Err("Name conflicts with a built-in slash command".to_string());
        }

        let dup = self
            .prompts
            .iter()
            .enumerate()
            .any(|(idx, p)| idx != self.selected && p.name.eq_ignore_ascii_case(slug));
        if dup {
            return Err("A prompt with this name already exists".to_string());
        }
        Ok(())
    }

    fn save_current(&mut self) {
        let name = self.name_field.text().trim().to_string();
        let body = self.body_field.text().to_string();
        match self.validate(&name) {
            Ok(()) => {}
            Err(msg) => {
                self.status = Some((msg, Style::default().fg(colors::error())));
                return;
            }
        }

        let code_home = match find_code_home() {
            Ok(path) => path,
            Err(e) => {
                self.status = Some((format!("CODE_HOME unavailable: {e}"), Style::default().fg(colors::error())));
                return;
            }
        };
        let mut dir = code_home;
        dir.push("prompts");
        if let Err(e) = fs::create_dir_all(&dir) {
            self.status = Some((format!("Failed to create prompts dir: {e}"), Style::default().fg(colors::error())));
            return;
        }
        let mut path = PathBuf::from(&dir);
        path.push(format!("{name}.md"));
        if let Err(e) = fs::write(&path, &body) {
            self.status = Some((format!("Failed to save: {e}"), Style::default().fg(colors::error())));
            return;
        }

        // Update local list
        let new_entry = CustomPrompt { name, path, content: body, description: None, argument_hint: None };
        if self.selected < self.prompts.len() {
            self.prompts[self.selected] = new_entry;
        } else {
            self.prompts.push(new_entry);
            self.selected = self.prompts.len() - 1;
        }
        self.status = Some(("Saved.".to_string(), Style::default().fg(colors::success())));

        // Trigger reload so composer autocomplete picks it up.
        self.app_event_tx.send(AppEvent::codex_op(Op::ListCustomPrompts));
    }

    fn delete_current(&mut self) {
        if self.selected >= self.prompts.len() {
            self.status = Some(("Nothing to delete".to_string(), Style::default().fg(colors::warning())));
            self.mode = Mode::List;
            self.focus = Focus::List;
            return;
        }
        let prompt = self.prompts[self.selected].clone();
        if let Err(e) = fs::remove_file(&prompt.path) {
            // Ignore missing file but surface other errors
            if e.kind() != std::io::ErrorKind::NotFound {
                self.status = Some((format!("Delete failed: {e}"), Style::default().fg(colors::error())));
                return;
            }
        }
        self.prompts.remove(self.selected);
        if self.selected > 0 && self.selected >= self.prompts.len() {
            self.selected -= 1;
        }
        self.mode = Mode::List;
        self.focus = Focus::List;
        self.status = Some(("Deleted.".to_string(), Style::default().fg(colors::success())));
        self.app_event_tx.send(AppEvent::codex_op(Op::ListCustomPrompts));
    }
}

impl<'a> BottomPaneView<'a> for PromptsSettingsView {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        self.handle_key_event_with_result(pane, key_event);
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

    fn is_complete(&self) -> bool {
        self.is_complete()
    }

    fn desired_height(&self, _width: u16) -> u16 {
        Self::DEFAULT_HEIGHT
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_body(area, buf);
    }
}
