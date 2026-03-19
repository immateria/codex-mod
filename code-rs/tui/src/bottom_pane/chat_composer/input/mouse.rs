use super::*;

trait PopupMouseTarget {
    fn select_visible_index(&mut self, rel_y: usize) -> bool;
    fn move_up(&mut self);
    fn move_down(&mut self);
}

impl PopupMouseTarget for CommandPopup {
    fn select_visible_index(&mut self, rel_y: usize) -> bool {
        self.select_visible_index(rel_y)
    }

    fn move_up(&mut self) {
        self.move_up();
    }

    fn move_down(&mut self) {
        self.move_down();
    }
}

impl PopupMouseTarget for FileSearchPopup {
    fn select_visible_index(&mut self, rel_y: usize) -> bool {
        self.select_visible_index(rel_y)
    }

    fn move_up(&mut self) {
        self.move_up();
    }

    fn move_down(&mut self) {
        self.move_down();
    }
}

pub(super) fn handle_mouse_event_inner(
    view: &mut ChatComposer,
    mouse_event: MouseEvent,
    area: Rect,
) -> (InputResult, bool) {
    let (mx, my) = (mouse_event.column, mouse_event.row);

    // Only handle left clicks and scroll
    match mouse_event.kind {
        MouseEventKind::Down(MouseButton::Left) => {}
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {}
        _ => return (InputResult::None, false),
    }

    // Calculate footer area (where popups live)
    let footer_height = view.footer_height();
    let footer_area = if footer_height > 0 {
        Some(Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(footer_height),
            width: area.width,
            height: footer_height,
        })
    } else {
        None
    };

    // Check if click/scroll is in footer area for popup handling
    let hit_footer = footer_area.filter(|fa| {
        mx >= fa.x && mx < fa.x + fa.width && my >= fa.y && my < fa.y + fa.height
    });

    // First, check if there's an active popup and handle events for it
    if let Some(footer_rect) = hit_footer {
        let rel_y = my.saturating_sub(footer_rect.y) as usize;
        match &mut view.active_popup {
            ActivePopup::Command(popup) => {
                if handle_popup_mouse(popup, mouse_event.kind, rel_y) {
                    return view.confirm_slash_popup_selection();
                }
                return (InputResult::None, false);
            }
            ActivePopup::File(popup) => {
                if handle_popup_mouse(popup, mouse_event.kind, rel_y) {
                    return view.confirm_file_popup_selection();
                }
                return (InputResult::None, false);
            }
            ActivePopup::None => {}
        }
    }

    // Not in popup area - check if click is on the textarea
    if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind
        && let Some(textarea_rect) = *view.last_textarea_rect.borrow()
    {
        let state = *view.textarea_state.borrow();
        if view.textarea.handle_mouse_click(mx, my, textarea_rect, state) {
            return (InputResult::None, true);
        }
    }

    (InputResult::None, false)
}

fn handle_popup_mouse(popup: &mut impl PopupMouseTarget, kind: MouseEventKind, rel_y: usize) -> bool {
    match kind {
        MouseEventKind::Down(MouseButton::Left) => popup.select_visible_index(rel_y),
        MouseEventKind::ScrollUp => {
            popup.move_up();
            false
        }
        MouseEventKind::ScrollDown => {
            popup.move_down();
            false
        }
        _ => false,
    }
}

