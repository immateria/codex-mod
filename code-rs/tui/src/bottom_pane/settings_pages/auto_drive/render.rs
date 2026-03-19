use super::*;

use crate::bottom_pane::chrome::ChromeMode;
use crate::colors;
use crate::bottom_pane::settings_ui::buttons::{
    standard_button_specs,
    SettingsButtonKind,
    StandardButtonSpec,
};
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

    fn render_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            AutoDriveSettingsMode::Main => {
                let rows = self.main_menu_rows();
                let selected_id = self
                    .main_state
                    .selected_idx
                    .map(|idx| idx.min(Self::option_count().saturating_sub(1)));
                let _layout = self
                    .page()
                    .render_menu_rows_in_chrome(chrome, area, buf, 0, selected_id, &rows);
            }
            AutoDriveSettingsMode::RoutingList => {
                let rows = self.routing_list_menu_rows();
                let total = rows.len();
                let state = self.routing_state.clamped(total);
                let selected_id = state.selected_idx;
                let scroll_top = state.scroll_top;

                let Some(layout) = self.page().render_menu_rows_in_chrome(
                    chrome,
                    area,
                    buf,
                    scroll_top,
                    selected_id,
                    &rows,
                ) else {
                    return;
                };

                self.routing_viewport_rows
                    .set(layout.body.height.max(1) as usize);
            }
            AutoDriveSettingsMode::RoutingEditor(editor) => {
                let page = self.routing_editor_page(editor);
                let buttons = self.routing_editor_action_buttons(editor.selected_field);
                let Some(layout) = page
                    .render_with_standard_actions_end_in_chrome(chrome, area, buf, &buttons)
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

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::Framed, area, buf);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::ContentOnly, area, buf);
    }

}
