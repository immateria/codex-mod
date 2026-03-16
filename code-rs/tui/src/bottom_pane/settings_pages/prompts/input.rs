use super::*;

use std::fs;
use std::path::PathBuf;

use code_core::config::find_code_home;
use code_core::protocol::Op;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Style;

use crate::app_event::AppEvent;
use crate::colors;
use crate::slash_command::built_in_slash_commands;

impl PromptsSettingsView {
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

    fn handle_list_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => {
                self.list_state.clamp_selection(self.list_row_count());
                let selected = self.selected_list_idx();
                if selected > 0 {
                    self.list_state.selected_idx = Some(selected.saturating_sub(1));
                    self.clamp_list_state();
                }
                true
            }
            KeyCode::Down => {
                self.list_state.clamp_selection(self.list_row_count());
                let selected = self.selected_list_idx();
                let max_idx = self.prompts.len();
                if selected < max_idx {
                    self.list_state.selected_idx = Some(selected.saturating_add(1));
                    self.clamp_list_state();
                }
                true
            }
            _ => false,
        }
    }

    fn start_new_prompt(&mut self) {
        self.list_state.selected_idx = Some(self.prompts.len());
        self.clamp_list_state();
        self.name_field.set_text("");
        self.body_field.set_text("");
        self.focus = Focus::Name;
        self.status = Some((
            "New prompt".to_string(),
            Style::default().fg(colors::info()),
        ));
        self.mode = Mode::Edit;
    }

    fn load_selected_into_form(&mut self) {
        let Some(selected) = self.selected_prompt_index() else {
            return;
        };
        if let Some(p) = self.prompts.get(selected) {
            self.name_field.set_text(&p.name);
            self.body_field.set_text(&p.content);
            self.focus = Focus::Name;
            self.status = None;
        }
    }

    pub(super) fn enter_editor(&mut self) {
        if self.selected_prompt_index().is_none() {
            self.start_new_prompt();
        } else {
            self.load_selected_into_form();
            self.mode = Mode::Edit;
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        let order = [
            Focus::List,
            Focus::Name,
            Focus::Body,
            Focus::Save,
            Focus::Delete,
            Focus::Cancel,
        ];
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
            .any(|(idx, p)| Some(idx) != self.selected_prompt_index() && p.name.eq_ignore_ascii_case(slug));
        if dup {
            return Err("A prompt with this name already exists".to_string());
        }
        Ok(())
    }

    pub(super) fn save_current(&mut self) {
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
                self.status = Some((
                    format!("CODE_HOME unavailable: {e}"),
                    Style::default().fg(colors::error()),
                ));
                return;
            }
        };
        let mut dir = code_home;
        dir.push("prompts");
        if let Err(e) = fs::create_dir_all(&dir) {
            self.status = Some((
                format!("Failed to create prompts dir: {e}"),
                Style::default().fg(colors::error()),
            ));
            return;
        }
        let mut path = PathBuf::from(&dir);
        path.push(format!("{name}.md"));
        if let Err(e) = fs::write(&path, &body) {
            self.status = Some((
                format!("Failed to save: {e}"),
                Style::default().fg(colors::error()),
            ));
            return;
        }

        // Update local list
        let new_entry = CustomPrompt {
            name,
            path,
            content: body,
            description: None,
            argument_hint: None,
        };
        if let Some(selected) = self.selected_prompt_index() {
            if selected < self.prompts.len() {
                self.prompts[selected] = new_entry;
                self.list_state.selected_idx = Some(selected);
            }
        } else {
            let new_idx = self.prompts.len();
            self.prompts.push(new_entry);
            self.list_state.selected_idx = Some(new_idx);
        }
        self.clamp_list_state();
        self.status = Some((
            "Saved.".to_string(),
            Style::default().fg(colors::success()),
        ));

        // Trigger reload so composer autocomplete picks it up.
        self.app_event_tx
            .send(AppEvent::codex_op(Op::ListCustomPrompts));
    }

    pub(super) fn delete_current(&mut self) {
        let Some(selected) = self.selected_prompt_index() else {
            self.status = Some((
                "Nothing to delete".to_string(),
                Style::default().fg(colors::warning()),
            ));
            self.mode = Mode::List;
            self.focus = Focus::List;
            return;
        };
        let prompt = self.prompts[selected].clone();
        if let Err(e) = fs::remove_file(&prompt.path) {
            // Ignore missing file but surface other errors
            if e.kind() != std::io::ErrorKind::NotFound {
                self.status = Some((
                    format!("Delete failed: {e}"),
                    Style::default().fg(colors::error()),
                ));
                return;
            }
        }
        self.prompts.remove(selected);
        let mut next_selected = selected;
        if next_selected > 0 && next_selected >= self.prompts.len() {
            next_selected = next_selected.saturating_sub(1);
        }
        self.list_state.selected_idx = Some(next_selected);
        self.clamp_list_state();
        self.mode = Mode::List;
        self.focus = Focus::List;
        self.status = Some((
            "Deleted.".to_string(),
            Style::default().fg(colors::success()),
        ));
        self.app_event_tx
            .send(AppEvent::codex_op(Op::ListCustomPrompts));
    }
}
