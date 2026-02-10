use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::SettingsSection;

use super::SettingsOverlayView;

impl SettingsOverlayView {
    /// Handle mouse events in the settings overlay.
    /// Returns true if the event was handled and requires a redraw.
    pub(crate) fn handle_mouse_event(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let x: u16 = mouse_event.column;
        let y: u16 = mouse_event.row;

        // Check if mouse is within overlay area
        if x < area.x || x >= area.x + area.width || y < area.y || y >= area.y + area.height {
            if self.hovered_section.borrow().is_some() {
                *self.hovered_section.borrow_mut() = None;
                return true;
            }
            return false;
        }

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.is_menu_active() {
                    // In menu mode - the content area is within the overlay block
                    let content_area = *self.last_content_area.borrow();
                    if content_area.width == 0 || content_area.height == 0 {
                        return false;
                    }

                    // Check if click is within the list area (not the footer hint)
                    if y < content_area.y
                        || y >= content_area.y + content_area.height.saturating_sub(1)
                    {
                        return false;
                    }

                    // Each menu item takes ~3 lines on average
                    let rel_y: usize = y.saturating_sub(content_area.y) as usize;
                    let approx_section_idx: usize = rel_y / 3;

                    if approx_section_idx < self.overview_rows.len() {
                        let section: SettingsSection = self.overview_rows[approx_section_idx].section;
                        return self.set_section(section);
                    }
                } else {
                    // In section mode - check sidebar clicks first
                    if let Some(section) = self.hit_test_sidebar(x, y) {
                        if section != self.active_section() {
                            self.set_mode_section(section);
                            return true;
                        }
                        return false;
                    }
                    // Not in sidebar - forward to content
                    return self.forward_mouse_to_content(mouse_event);
                }
                false
            }
            MouseEventKind::Moved => {
                if !self.is_menu_active() {
                    // Ignore movement in the sidebar itself to avoid expensive
                    // full-overlay redraws from hover-only state churn.
                    let new_hover = self.hit_test_sidebar(x, y);
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
                    return self.forward_mouse_to_content(mouse_event);
                }
                false
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                // Forward scroll to content if in section mode
                if !self.is_menu_active() {
                    return self.forward_mouse_to_content(mouse_event);
                }
                // In menu mode, consume scroll to prevent outer scroll
                true
            }
            _ => false,
        }
    }

    /// Hit test the sidebar to find which section is at the given coordinates.
    /// Returns None if not in sidebar area or no section at that position.
    fn hit_test_sidebar(&self, x: u16, y: u16) -> Option<SettingsSection> {
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
        let mut start = 0usize;
        if total > visible && visible > 0 {
            let half = visible / 2;
            if selected_idx > half {
                start = selected_idx - half;
            }
            if start + visible > total {
                start = total - visible;
            }
        }

        let clicked_idx = start + (rel_y as usize);
        self.sidebar_section_at(clicked_idx)
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
