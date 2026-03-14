use code_core::config::{load_config_as_toml, set_account_store_paths};
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;

use crate::bottom_pane::settings_ui::action_page::SettingsActionPage;
use crate::bottom_pane::settings_ui::buttons::{
    standard_button_specs, SettingsButtonKind, StandardButtonSpec,
};
use crate::bottom_pane::settings_ui::form_page::{SettingsFormPage, SettingsFormSection};
use crate::bottom_pane::settings_ui::hints::{status_and_shortcuts_split, title_line, KeyHint};
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::components::form_text_field::FormTextField;

use super::super::shared::Feedback;
use super::{
    LoginAccountsState, StorePathEditorAction, StorePathEditorState, ViewMode,
};

impl StorePathEditorState {
    pub(super) fn new(read_paths_text: &str, write_path_text: &str) -> Self {
        let mut read_paths_field = FormTextField::new_multi_line();
        read_paths_field.set_placeholder("auth_accounts.json\nlegacy/auth_accounts.json");
        read_paths_field.set_text(read_paths_text);

        let mut write_path_field = FormTextField::new_single_line();
        write_path_field.set_placeholder("auth_accounts.json");
        write_path_field.set_text(write_path_text);

        Self {
            selected_row: 0,
            read_paths_field,
            write_path_field,
        }
    }

    fn parse_read_paths(&self) -> Vec<String> {
        self.read_paths_field
            .text()
            .lines()
            .flat_map(|line| line.split(','))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(std::string::ToString::to_string)
            .collect()
    }

    fn write_path(&self) -> Option<String> {
        let trimmed = self.write_path_field.text().trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

impl LoginAccountsState {
    fn load_store_path_inputs(&self) -> (String, String) {
        let mut read_paths = vec!["auth_accounts.json".to_string()];
        let mut write_path = "auth_accounts.json".to_string();

        if let Ok(root) = load_config_as_toml(&self.code_home)
            && let Some(accounts) = root.get("accounts").and_then(|value| value.as_table())
        {
            if let Some(values) = accounts.get("read_paths").and_then(|value| value.as_array()) {
                let parsed = values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>();
                if !parsed.is_empty() {
                    read_paths = parsed;
                }
            }

            if let Some(value) = accounts.get("write_path").and_then(|value| value.as_str()) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    write_path = trimmed.to_string();
                }
            }
        }

        (read_paths.join("\n"), write_path)
    }

    pub(super) fn open_store_paths_editor(&mut self) {
        let (read_paths, write_path) = self.load_store_path_inputs();
        self.mode = ViewMode::EditStorePaths(Box::new(StorePathEditorState::new(
            &read_paths,
            &write_path,
        )));
    }

    fn save_store_paths_editor(&mut self, editor: &StorePathEditorState) -> bool {
        let read_paths = editor.parse_read_paths();
        let write_path = editor.write_path();

        match set_account_store_paths(&self.code_home, &read_paths, write_path.as_deref()) {
            Ok(()) => {
                self.feedback = Some(Feedback {
                    message: "Account store paths updated".to_string(),
                    is_error: false,
                });
                self.reload_accounts();
                true
            }
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to save account store paths: {err}"),
                    is_error: true,
                });
                false
            }
        }
    }

    pub(super) fn handle_store_paths_editor_key(
        &mut self,
        key_event: KeyEvent,
        editor: &mut StorePathEditorState,
    ) -> (bool, bool) {
        const ROW_COUNT: usize = 4;
        match key_event.code {
            KeyCode::Esc => (false, true),
            KeyCode::Up => {
                if editor.selected_row == 0 {
                    editor.selected_row = ROW_COUNT - 1;
                } else {
                    editor.selected_row = editor.selected_row.saturating_sub(1);
                }
                (true, true)
            }
            KeyCode::Down | KeyCode::Tab => {
                editor.selected_row = (editor.selected_row + 1) % ROW_COUNT;
                (true, true)
            }
            KeyCode::BackTab => {
                if editor.selected_row == 0 {
                    editor.selected_row = ROW_COUNT - 1;
                } else {
                    editor.selected_row = editor.selected_row.saturating_sub(1);
                }
                (true, true)
            }
            KeyCode::Enter => match editor.selected_row {
                0 | 1 => {
                    editor.selected_row = (editor.selected_row + 1) % ROW_COUNT;
                    (true, true)
                }
                2 => {
                    if self.save_store_paths_editor(editor) {
                        (false, true)
                    } else {
                        (true, true)
                    }
                }
                3 => (false, true),
                _ => (true, false),
            },
            KeyCode::Char('s') | KeyCode::Char('S') if editor.selected_row >= 2 => {
                if self.save_store_paths_editor(editor) {
                    (false, true)
                } else {
                    (true, true)
                }
            }
            _ => match editor.selected_row {
                0 => {
                    (true, editor.read_paths_field.handle_key(key_event))
                }
                1 => {
                    (true, editor.write_path_field.handle_key(key_event))
                }
                _ => (true, false),
            },
        }
    }

    pub(super) fn render_store_paths_editor(
        &self,
        area: Rect,
        buf: &mut Buffer,
        editor: &StorePathEditorState,
    ) {
        let page = self.store_paths_editor_form_page();
        let buttons = self.store_paths_editor_button_specs(editor.selected_row);
        let Some(_layout) = page.framed().render_with_standard_actions_end(
            area,
            buf,
            &[&editor.read_paths_field, &editor.write_path_field],
            &buttons,
        ) else {
            return;
        };
    }

    fn store_paths_editor_page(&self) -> SettingsActionPage<'static> {
        let header_lines = vec![
            title_line("Account Store Paths"),
            Line::from("Set where account records are read/written."),
            Line::from(""),
        ];
        let status = self.feedback.as_ref().map(|feedback| {
            let style = if feedback.is_error {
                Style::new().fg(crate::colors::error()).bold()
            } else {
                Style::new().fg(crate::colors::success()).bold()
            };
            StyledText::new(feedback.message.clone(), style)
        });
        let (status_lines, footer_lines) = status_and_shortcuts_split(
            status,
            &[
                KeyHint::new("Tab", " next"),
                KeyHint::new("S", " save")
                    .with_key_style(Style::new().fg(crate::colors::success()).bold()),
                KeyHint::new("Esc", " back")
                    .with_key_style(Style::new().fg(crate::colors::error()).bold()),
            ],
        );

        SettingsActionPage::new(
            "Manage Accounts",
            super::super::panel_style(),
            header_lines,
            footer_lines,
        )
        .with_status_lines(status_lines)
        .with_action_rows(1)
        .with_min_body_rows(6)
    }

    fn store_paths_editor_form_page(&self) -> SettingsFormPage<'static> {
        SettingsFormPage::new(
            self.store_paths_editor_page(),
            vec![
                SettingsFormSection::new(
                    "Read paths (one per line)",
                    false,
                    Constraint::Length(4),
                ),
                SettingsFormSection::new("Write path", false, Constraint::Length(1)),
            ],
        )
        .with_section_gap_rows(1)
    }

    fn store_paths_editor_button_specs(
        &self,
        selected_row: usize,
    ) -> Vec<StandardButtonSpec<StorePathEditorAction>> {
        standard_button_specs(
            &[
                (StorePathEditorAction::Save, SettingsButtonKind::Save),
                (StorePathEditorAction::Cancel, SettingsButtonKind::Cancel),
            ],
            match selected_row {
                2 => Some(StorePathEditorAction::Save),
                3 => Some(StorePathEditorAction::Cancel),
                _ => None,
            },
            None,
        )
    }

    pub(super) fn handle_store_paths_editor_mouse(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        editor: &mut StorePathEditorState,
    ) -> (bool, bool) {
        let page = self.store_paths_editor_form_page();
        let Some(layout) = page.framed().layout(area) else {
            return (true, false);
        };
        let buttons = self.store_paths_editor_button_specs(editor.selected_row);

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(action) = page.standard_action_at_end(
                    &layout,
                    mouse_event.column,
                    mouse_event.row,
                    &buttons,
                ) {
                    return match action {
                        StorePathEditorAction::Save => {
                            if self.save_store_paths_editor(editor) {
                                (false, true)
                            } else {
                                (true, true)
                            }
                        }
                        StorePathEditorAction::Cancel => (false, true),
                    };
                }
                if let Some(section_idx) =
                    page.field_index_at(&layout, mouse_event.column, mouse_event.row)
                {
                    match section_idx {
                        0 => {
                            editor.selected_row = 0;
                            return (
                                true,
                                editor.read_paths_field.handle_mouse_click(
                                    mouse_event.column,
                                    mouse_event.row,
                                    layout.sections[0].inner,
                                ),
                            );
                        }
                        1 => {
                            editor.selected_row = 1;
                            return (
                                true,
                                editor.write_path_field.handle_mouse_click(
                                    mouse_event.column,
                                    mouse_event.row,
                                    layout.sections[1].inner,
                                ),
                            );
                        }
                        _ => {}
                    }
                }
                (true, false)
            }
            MouseEventKind::ScrollUp => {
                let pos = ratatui::layout::Position {
                    x: mouse_event.column,
                    y: mouse_event.row,
                };
                if layout.sections[0].outer.contains(pos) {
                    editor.selected_row = 0;
                    return (true, editor.read_paths_field.handle_mouse_scroll(false));
                }
                if layout.sections[1].outer.contains(pos) {
                    editor.selected_row = 1;
                    return (true, editor.write_path_field.handle_mouse_scroll(false));
                }
                (true, false)
            }
            MouseEventKind::ScrollDown => {
                let pos = ratatui::layout::Position {
                    x: mouse_event.column,
                    y: mouse_event.row,
                };
                if layout.sections[0].outer.contains(pos) {
                    editor.selected_row = 0;
                    return (true, editor.read_paths_field.handle_mouse_scroll(true));
                }
                if layout.sections[1].outer.contains(pos) {
                    editor.selected_row = 1;
                    return (true, editor.write_path_field.handle_mouse_scroll(true));
                }
                (true, false)
            }
            _ => (true, false),
        }
    }
}
