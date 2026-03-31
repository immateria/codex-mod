use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::SettingsSection;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test_no_ensure_visible;
use crate::components::scroll_state::ScrollState;
use crate::ui_interaction::{
    contains_point,
    ListWindow,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use unicode_width::UnicodeWidthStr;

use super::SettingsOverlayView;

impl SettingsOverlayView {
    /// Handle mouse events in the settings overlay.
    /// Returns true if the event was handled and requires a redraw.
    pub(crate) fn handle_mouse_event(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        // Check if mouse is within overlay area
        if !contains_point(area, mouse_event.column, mouse_event.row) {
            if self.hovered_section.borrow().is_some() {
                *self.hovered_section.borrow_mut() = None;
                return true;
            }
            return false;
        }

        if self.is_menu_active() {
            self.handle_menu_mouse_event(mouse_event)
        } else {
            self.handle_section_mouse_event(mouse_event)
        }
    }

    fn handle_menu_mouse_event(&mut self, mouse_event: MouseEvent) -> bool {
        let item_count = self.sidebar_section_count();
        if item_count == 0 {
            return false;
        }

        let mut state = ScrollState {
            selected_idx: self.sidebar_selected_index(),
            scroll_top: 0,
        };
        let kind = mouse_event.kind;
        let outcome = route_scroll_state_mouse_with_hit_test_no_ensure_visible(
            mouse_event,
            &mut state,
            item_count,
            |x, y, _scroll_top| {
                if matches!(kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) {
                    let overview_area = *self.last_overview_list_area.borrow();
                    contains_point(overview_area, x, y).then_some(0)
                } else {
                    self.hit_test_menu_index(x, y)
                }
            },
            SelectableListMouseConfig {
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );
        let selected_idx = state.selected_idx.unwrap_or(0);

        match outcome.result {
            SelectableListMouseResult::Ignored => false,
            SelectableListMouseResult::SelectionChanged => {
                let Some(section) = self.sidebar_section_at(selected_idx) else {
                    return false;
                };
                self.set_section(section)
            }
            SelectableListMouseResult::Activated => {
                let Some(section) = self.sidebar_section_at(selected_idx) else {
                    return false;
                };
                let changed = self.active_section() != section || self.is_menu_active();
                self.set_mode_section(section);
                changed
            }
        }
    }

    fn handle_section_mouse_event(&mut self, mouse_event: MouseEvent) -> bool {
        match mouse_event.kind {
            MouseEventKind::Moved => {
                // Ignore movement in the sidebar itself to avoid expensive
                // full-overlay redraws from hover-only state churn.
                let new_hover = self.hit_test_sidebar(mouse_event.column, mouse_event.row);
                let current_hover = *self.hovered_section.borrow();
                if new_hover.is_some() {
                    if new_hover != current_hover {
                        *self.hovered_section.borrow_mut() = new_hover;
                        return true;
                    }
                    return false;
                }

                // Cursor moved out of sidebar: clear hover state once.
                if current_hover.is_some() {
                    *self.hovered_section.borrow_mut() = None;
                    return true;
                }
                // Forward move events only when cursor is in content panel.
                self.forward_mouse_to_content(mouse_event)
            }
            MouseEventKind::Down(_) | MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let item_count = self.sidebar_section_count();
                let mut state = ScrollState {
                    selected_idx: self.sidebar_selected_index(),
                    scroll_top: 0,
                };
                let kind = mouse_event.kind;
                let outcome = route_scroll_state_mouse_with_hit_test_no_ensure_visible(
                    mouse_event,
                    &mut state,
                    item_count,
                    |x, y, _scroll_top| {
                        if matches!(kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) {
                            let sidebar_area = *self.last_sidebar_area.borrow();
                            contains_point(sidebar_area, x, y).then_some(0)
                        } else {
                            self.hit_test_sidebar_index(x, y)
                        }
                    },
                    SelectableListMouseConfig {
                        hover_select: false,
                        require_pointer_hit_for_scroll: true,
                        scroll_behavior: ScrollSelectionBehavior::Clamp,
                        ..SelectableListMouseConfig::default()
                    },
                );
                let selected_idx = state.selected_idx.unwrap_or(0);

                match outcome.result {
                    SelectableListMouseResult::Ignored => {
                        let handled = self.forward_mouse_to_content(mouse_event);
                        if handled {
                            self.set_focus_content();
                        }
                        handled
                    }
                    SelectableListMouseResult::SelectionChanged
                    | SelectableListMouseResult::Activated => {
                        let Some(section) = self.sidebar_section_at(selected_idx) else {
                            return false;
                        };
                        if section != self.active_section() {
                            self.set_mode_section(section);
                            self.set_focus_sidebar();
                            true
                        } else {
                            let activated =
                                matches!(outcome.result, SelectableListMouseResult::Activated);
                            if activated {
                                self.set_focus_sidebar();
                            }
                            activated
                        }
                    }
                }
            }
            _ => false,
        }
    }

    fn hit_test_menu_index(&self, x: u16, y: u16) -> Option<usize> {
        let overview_area = *self.last_overview_list_area.borrow();
        if overview_area.width == 0 || overview_area.height == 0 {
            return None;
        }
        if !contains_point(overview_area, x, y) {
            return None;
        }

        let rel_y = y.saturating_sub(overview_area.y) as usize;
        let abs_y = rel_y.saturating_add(*self.last_overview_scroll.borrow());
        let hit_ranges = self
            .last_overview_line_hit_ranges
            .borrow()
            .get(abs_y)
            .copied()?;
        let mut hit = false;
        for range in hit_ranges {
            if let Some((start, end)) = range
                && x >= start
                && x < end
            {
                hit = true;
                break;
            }
        }
        if !hit {
            return None;
        }
        let section = self
            .last_overview_line_sections
            .borrow()
            .get(abs_y)
            .copied()
            .flatten()?;
        self.sidebar_index_for_section(section)
    }

    /// Hit test the sidebar to find which section is at the given coordinates.
    /// Returns None if not in sidebar area or no section at that position.
    fn hit_test_sidebar(&self, x: u16, y: u16) -> Option<SettingsSection> {
        self.hit_test_sidebar_index(x, y)
            .and_then(|idx| self.sidebar_section_at(idx))
    }

    fn hit_test_sidebar_index(&self, x: u16, y: u16) -> Option<usize> {
        let sidebar_area = *self.last_sidebar_area.borrow();
        if sidebar_area.width == 0 || sidebar_area.height == 0 {
            return None;
        }

        if x < sidebar_area.x || x >= sidebar_area.x.saturating_add(sidebar_area.width) {
            return None;
        }

        if y < sidebar_area.y || y >= sidebar_area.y.saturating_add(sidebar_area.height) {
            return None;
        }

        let rel_y = y.saturating_sub(sidebar_area.y);
        let total = self.sidebar_section_count();
        if total == 0 {
            return None;
        }
        let visible = sidebar_area.height as usize;
        let selected_idx = self.sidebar_selected_index().unwrap_or(0);
        let window = ListWindow::centered(total, visible, selected_idx);
        let idx = window.index_for_relative_row(rel_y as usize)?;
        let section = self.sidebar_section_at(idx)?;

        // Only treat the row as "hit" when the pointer is over the section label
        // text, not just anywhere in the sidebar padding.
        let label_start = sidebar_area.x.saturating_add(3);
        let label_width = UnicodeWidthStr::width(section.label()) as u16;
        if label_width == 0 {
            return None;
        }
        let label_end = label_start.saturating_add(label_width);
        if x < label_start || x >= label_end {
            return None;
        }
        Some(idx)
    }

    /// Forward mouse event to the active content view.
    /// Returns true if the content handled the event and needs a redraw.
    fn forward_mouse_to_content(&mut self, mouse_event: MouseEvent) -> bool {
        let panel_area = *self.last_panel_inner_area.borrow();
        if panel_area.width == 0 || panel_area.height == 0 {
            return false;
        }
        self.active_content_mut()
            .is_some_and(|content| content.handle_mouse(mouse_event, panel_area))
    }

    fn sidebar_section_count(&self) -> usize {
        if self.overview_rows.is_empty() {
            SettingsSection::ALL.len()
        } else {
            self.overview_rows.len()
        }
    }

    fn sidebar_section_at(&self, idx: usize) -> Option<SettingsSection> {
        if self.overview_rows.is_empty() {
            SettingsSection::ALL.get(idx).copied()
        } else {
            self.overview_rows.get(idx).map(|row| row.section)
        }
    }

    fn sidebar_index_for_section(&self, section: SettingsSection) -> Option<usize> {
        if self.overview_rows.is_empty() {
            SettingsSection::ALL.iter().position(|candidate| *candidate == section)
        } else {
            self.overview_rows
                .iter()
                .position(|row| row.section == section)
        }
    }

    fn sidebar_selected_index(&self) -> Option<usize> {
        if self.overview_rows.is_empty() {
            Some(self.index_of(self.active_section()))
        } else {
            self.overview_rows
                .iter()
                .position(|row| row.section == self.active_section())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bottom_pane::SettingsSection;
    use ratatui::layout::Rect;

    #[test]
    fn sidebar_hit_test_requires_pointer_over_text() {
        let overlay = SettingsOverlayView::new(SettingsSection::Model);
        *overlay.last_sidebar_area.borrow_mut() = Rect::new(0, 0, 22, 10);

        // "Model" is drawn starting at x+3, so pointing at the far-right padding should not hit.
        assert_eq!(overlay.hit_test_sidebar_index(20, 0), None);
        assert_eq!(overlay.hit_test_sidebar_index(3, 0), Some(0));
    }
}
