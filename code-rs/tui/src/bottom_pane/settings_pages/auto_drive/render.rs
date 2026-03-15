use super::*;

use crate::colors;
use crate::bottom_pane::settings_ui::buttons::{standard_button_specs, SettingsButtonKind, StandardButtonSpec};
use crate::bottom_pane::settings_ui::menu_rows::render_menu_rows;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

impl AutoDriveSettingsView {
    fn routing_editor_selected_body_field(
        &self,
        editor: &RoutingEditorState,
    ) -> Option<RoutingEditorField> {
        match editor.selected_field {
            RoutingEditorField::Model
            | RoutingEditorField::Enabled
            | RoutingEditorField::Reasoning
            | RoutingEditorField::Description => Some(editor.selected_field),
            RoutingEditorField::Save | RoutingEditorField::Cancel => None,
        }
    }

    pub(super) fn routing_editor_action_buttons(
        &self,
        selected_field: RoutingEditorField,
    ) -> Vec<StandardButtonSpec<RoutingEditorField>> {
        standard_button_specs(
            &[
                (RoutingEditorField::Save, SettingsButtonKind::Save),
                (RoutingEditorField::Cancel, SettingsButtonKind::Cancel),
            ],
            match selected_field {
                RoutingEditorField::Save | RoutingEditorField::Cancel => Some(selected_field),
                _ => None,
            },
            match self.hovered {
                Some(HoverTarget::RoutingEditor(
                    RoutingEditorField::Save | RoutingEditorField::Cancel,
                )) => self.hovered.and_then(|target| match target {
                    HoverTarget::RoutingEditor(field) => Some(field),
                    _ => None,
                }),
                _ => None,
            },
        )
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        let base = Style::new().bg(colors::background()).fg(colors::text());
        match &self.mode {
            AutoDriveSettingsMode::Main => {
                let rows = self.main_menu_rows();
                render_menu_rows(
                    area,
                    buf,
                    0,
                    Some(self.selected_index),
                    &rows,
                    base,
                );
            }
            AutoDriveSettingsMode::RoutingList => {
                let rows = self.routing_list_menu_rows();
                render_menu_rows(
                    area,
                    buf,
                    0,
                    Some(self.routing_selected_index),
                    &rows,
                    base,
                );
            }
            AutoDriveSettingsMode::RoutingEditor(editor) => {
                let selected = self.routing_editor_selected_body_field(editor);
                let rows = self.routing_editor_menu_rows(editor);
                render_menu_rows(
                    area,
                    buf,
                    0,
                    selected,
                    &rows,
                    base,
                );
            }
        }
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            AutoDriveSettingsMode::Main => {
                let rows = self.main_menu_rows();
                let _ = self
                    .page()
                    .framed()
                    .render_menu_rows(area, buf, 0, Some(self.selected_index), &rows);
            }
            AutoDriveSettingsMode::RoutingList => {
                let rows = self.routing_list_menu_rows();
                let _ = self
                    .page()
                    .framed()
                    .render_menu_rows(area, buf, 0, Some(self.routing_selected_index), &rows);
            }
            AutoDriveSettingsMode::RoutingEditor(editor) => {
                let page = self.routing_editor_page(editor);
                let buttons = self.routing_editor_action_buttons(editor.selected_field);
                let Some(layout) = page
                    .framed()
                    .render_with_standard_actions_end(area, buf, &buttons)
                else {
                    return;
                };
                let rows = self.routing_editor_menu_rows(editor);
                render_menu_rows(
                    layout.body,
                    buf,
                    0,
                    self.routing_editor_selected_body_field(editor),
                    &rows,
                    Style::new().bg(colors::background()).fg(colors::text()),
                );
            }
        }
    }

}
