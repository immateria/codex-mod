use std::fs;
use std::path::PathBuf;

use code_core::config::find_code_home;
use code_core::protocol::Op;
use code_protocol::custom_prompts::CustomPrompt;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

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
use super::settings_ui::action_page::SettingsActionPage;
use super::settings_ui::buttons::{TextButton, TextButtonAlign};
use super::settings_ui::form_page::{SettingsFormPage, SettingsFormPageLayout, SettingsFormSection};
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::menu_rows::SettingsMenuRow;
use super::settings_ui::panel::SettingsPanelStyle;
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

    fn edit_page(&self) -> SettingsActionPage<'static> {
        let footer_lines = self
            .status
            .as_ref()
            .map(|(msg, style)| vec![Line::from(Span::styled(msg.clone(), *style))])
            .unwrap_or_default();
        SettingsActionPage::new(
            "Custom Prompt",
            super::settings_ui::panel::SettingsPanelStyle::bottom_pane(),
            Vec::new(),
            footer_lines,
        )
    }

    fn edit_form_page(&self) -> SettingsFormPage<'static> {
        SettingsFormPage::new(
            self.edit_page(),
            vec![
                SettingsFormSection::new(
                    self.name_field_title(),
                    matches!(self.focus, Focus::Name),
                    Constraint::Length(3),
                ),
                SettingsFormSection::new(
                    self.body_field_title(),
                    matches!(self.focus, Focus::Body),
                    Constraint::Min(6),
                ),
            ],
        )
    }

    fn list_page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Custom Prompts",
            SettingsPanelStyle::bottom_pane(),
            self.list_header_lines(),
            Vec::new(),
        )
    }

    fn list_rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        let mut rows = self
            .prompts
            .iter()
            .enumerate()
            .map(|(idx, prompt)| {
                let preview = prompt.content.lines().next().unwrap_or("").trim().to_string();
                let mut row = SettingsMenuRow::new(idx, format!("/{}", prompt.name));
                if !preview.is_empty() {
                    row = row.with_detail(super::settings_ui::rows::StyledText::new(
                        preview,
                        Style::new().fg(colors::text_dim()),
                    ));
                }
                row
            })
            .collect::<Vec<_>>();

        rows.push(
            SettingsMenuRow::new(self.prompts.len(), "Add new…").with_detail(
                super::settings_ui::rows::StyledText::new(
                    "Create a custom slash prompt",
                    Style::new().fg(colors::text_dim()),
                ),
            ),
        );
        rows
    }

    fn edit_buttons(&self) -> [TextButton<'static, Focus>; 3] {
        [
            TextButton::new(
                Focus::Save,
                Self::BUTTON_LABELS[0],
                matches!(self.focus, Focus::Save),
                false,
                Style::new().fg(colors::success()).bold(),
            ),
            TextButton::new(
                Focus::Delete,
                Self::BUTTON_LABELS[1],
                matches!(self.focus, Focus::Delete),
                false,
                Style::new().fg(colors::error()).bold(),
            ),
            TextButton::new(
                Focus::Cancel,
                Self::BUTTON_LABELS[2],
                matches!(self.focus, Focus::Cancel),
                false,
                Style::new().fg(colors::text_dim()).bold(),
            ),
        ]
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
        let rows = self.list_rows();
        let Some(_layout) = self
            .list_page()
            .render_menu_rows(area, buf, 0, Some(self.selected), &rows)
        else {
            return;
        };
    }

    fn render_form(&self, area: Rect, buf: &mut Buffer) {
        let page = self.edit_form_page();
        let Some(layout) = page.render(area, buf, &[&self.name_field, &self.body_field])
        else {
            return;
        };
        let buttons = self.edit_buttons();
        page.render_actions(&layout, buf, &buttons, TextButtonAlign::End);
    }

    fn list_selection_at(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        let layout = self.list_page().layout(area)?;
        let rows = self.list_rows();
        SettingsMenuPage::selection_menu_id_in_body(layout.body, x, y, 0, &rows)
    }

    fn button_focus_at(
        &self,
        page: &SettingsFormPage<'_>,
        layout: &SettingsFormPageLayout,
        mouse_event: MouseEvent,
    ) -> Option<Focus> {
        page.action_at(
            layout,
            mouse_event.column,
            mouse_event.row,
            &self.edit_buttons(),
            TextButtonAlign::End,
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
        let page = self.edit_form_page();
        let Some(layout) = page.layout(area)
        else {
            return false;
        };

        match mouse_event.kind {
            MouseEventKind::Moved => {
                if let Some(focus) = self.button_focus_at(&page, &layout, mouse_event) {
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
                if let Some(section_idx) = page.field_index_at(&layout, col, row) {
                    match section_idx {
                        0 => {
                            self.focus = Focus::Name;
                            let _ = self.name_field.handle_mouse_click(
                                col,
                                row,
                                layout.sections[0].inner,
                            );
                        }
                        1 => {
                            self.focus = Focus::Body;
                            let _ = self.body_field.handle_mouse_click(
                                col,
                                row,
                                layout.sections[1].inner,
                            );
                        }
                        _ => {}
                    }
                    return true;
                }
                if let Some(focus) = self.button_focus_at(&page, &layout, mouse_event) {
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
                if layout.sections[1]
                    .outer
                    .contains(ratatui::layout::Position { x: mouse_event.column, y: mouse_event.row })
                {
                    self.focus = Focus::Body;
                    return self.body_field.handle_mouse_scroll(false);
                }
                false
            }
            MouseEventKind::ScrollDown => {
                if layout.sections[1]
                    .outer
                    .contains(ratatui::layout::Position { x: mouse_event.column, y: mouse_event.row })
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
