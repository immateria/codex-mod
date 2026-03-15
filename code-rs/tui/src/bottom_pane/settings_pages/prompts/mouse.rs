use super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::layout::Position;

use crate::bottom_pane::settings_ui::form_page::SettingsFormPageLayout;
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

impl PromptsSettingsView {
    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        if self.is_complete || area.width == 0 || area.height == 0 {
            return false;
        }
        self.handle_mouse_event_direct_content(mouse_event, area)
    }

    fn handle_mouse_event_direct_content(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match self.mode {
            Mode::List => self.handle_list_mouse_event_content(mouse_event, area),
            Mode::Edit => self.handle_edit_mouse_event_content(mouse_event, area),
        }
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        match self.mode {
            Mode::List => self.handle_list_mouse_event(mouse_event, area),
            Mode::Edit => self.handle_edit_mouse_event(mouse_event, area),
        }
    }

    fn list_selection_at(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        let layout = self.list_page().framed().layout(area)?;
        let rows = self.list_rows();
        SettingsMenuPage::selection_menu_id_in_body(layout.body, x, y, 0, &rows)
    }

    fn list_selection_at_content(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        let layout = self.list_page().content_only().layout(area)?;
        let rows = self.list_rows();
        SettingsMenuPage::selection_menu_id_in_body(layout.body, x, y, 0, &rows)
    }

    fn button_focus_at(
        &self,
        page: &crate::bottom_pane::settings_ui::form_page::SettingsFormPage<'_>,
        layout: &SettingsFormPageLayout,
        mouse_event: MouseEvent,
    ) -> Option<Focus> {
        page.standard_action_at_end(
            layout,
            mouse_event.column,
            mouse_event.row,
            &self.edit_button_specs(),
        )
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

    fn handle_list_mouse_event_content(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mut selected = self.selected;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.prompts.len().saturating_add(1),
            |x, y| self.list_selection_at_content(area, x, y),
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
        let Some(layout) = page.framed().layout(area) else {
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
                            let _ = self.name_field.handle_mouse_click(col, row, layout.sections[0].inner);
                        }
                        1 => {
                            self.focus = Focus::Body;
                            let _ = self.body_field.handle_mouse_click(col, row, layout.sections[1].inner);
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
                if layout.sections[1].outer.contains(Position { x: mouse_event.column, y: mouse_event.row }) {
                    self.focus = Focus::Body;
                    return self.body_field.handle_mouse_scroll(false);
                }
                false
            }
            MouseEventKind::ScrollDown => {
                if layout.sections[1].outer.contains(Position { x: mouse_event.column, y: mouse_event.row }) {
                    self.focus = Focus::Body;
                    return self.body_field.handle_mouse_scroll(true);
                }
                false
            }
            _ => false,
        }
    }

    fn handle_edit_mouse_event_content(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let page = self.edit_form_page();
        let Some(layout) = page.content_only().layout(area) else {
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
                            let _ = self.name_field.handle_mouse_click(col, row, layout.sections[0].inner);
                        }
                        1 => {
                            self.focus = Focus::Body;
                            let _ = self.body_field.handle_mouse_click(col, row, layout.sections[1].inner);
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
                if layout.sections[1].outer.contains(Position { x: mouse_event.column, y: mouse_event.row }) {
                    self.focus = Focus::Body;
                    return self.body_field.handle_mouse_scroll(false);
                }
                false
            }
            MouseEventKind::ScrollDown => {
                if layout.sections[1].outer.contains(Position { x: mouse_event.column, y: mouse_event.row }) {
                    self.focus = Focus::Body;
                    return self.body_field.handle_mouse_scroll(true);
                }
                false
            }
            _ => false,
        }
    }
}

