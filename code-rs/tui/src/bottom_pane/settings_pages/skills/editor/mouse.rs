use super::*;

use crate::bottom_pane::settings_ui::buttons::standard_button_at;

pub(super) fn handle_mouse_event_direct_impl(
    view: &mut SkillsSettingsView,
    mouse_event: MouseEvent,
    area: Rect,
    chrome: ChromeMode,
) -> bool {
    if view.mode == Mode::List {
        return match chrome {
            ChromeMode::Framed => view.handle_list_mouse_event_framed(mouse_event, area),
            ChromeMode::ContentOnly => view.handle_list_mouse_event_content_only(mouse_event, area),
        };
    }

    match mouse_event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if !area.contains(Position {
                x: mouse_event.column,
                y: mouse_event.row,
            }) {
                return false;
            }
            handle_edit_click(view, mouse_event, area, chrome)
        }
        MouseEventKind::Moved => {
            if !area.contains(Position {
                x: mouse_event.column,
                y: mouse_event.row,
            }) {
                return set_hovered_button(view, None);
            }
            handle_edit_mouse_move(view, mouse_event, area, chrome)
        }
        MouseEventKind::ScrollUp => {
            if !area.contains(Position {
                x: mouse_event.column,
                y: mouse_event.row,
            }) {
                return false;
            }
            handle_edit_scroll(view, mouse_event, area, chrome, false)
        }
        MouseEventKind::ScrollDown => {
            if !area.contains(Position {
                x: mouse_event.column,
                y: mouse_event.row,
            }) {
                return false;
            }
            handle_edit_scroll(view, mouse_event, area, chrome, true)
        }
        _ => false,
    }
}

fn handle_edit_click(
    view: &mut SkillsSettingsView,
    mouse_event: MouseEvent,
    area: Rect,
    chrome: ChromeMode,
) -> bool {
    let Some(layout) = scroll::compute_form_layout_for_chrome(view, area, chrome) else {
        return false;
    };
    set_hovered_button(view, None);

    if layout.name_field.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        view.editor.focus = Focus::Name;
        view.editor.name_field.handle_mouse_click(
            mouse_event.column,
            mouse_event.row,
            layout.name_field,
        );
        return true;
    }
    if layout.description_field.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        view.editor.focus = Focus::Description;
        view.editor.description_field.handle_mouse_click(
            mouse_event.column,
            mouse_event.row,
            layout.description_field,
        );
        return true;
    }
    if layout.style_field.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        view.editor.focus = Focus::Style;
        view.editor.style_field.handle_mouse_click(
            mouse_event.column,
            mouse_event.row,
            layout.style_field,
        );
        return true;
    }
    if layout.style_profile_row.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        view.editor.focus = Focus::StyleProfile;
        style_profile::cycle_style_profile_mode(view, true);
        return true;
    }
    if layout.style_references_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        view.editor.focus = Focus::StyleReferences;
        if layout.style_references_inner.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            view.editor.style_references_field.handle_mouse_click(
                mouse_event.column,
                mouse_event.row,
                layout.style_references_inner,
            );
        }
        return true;
    }
    if layout.style_skill_roots_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        view.editor.focus = Focus::StyleSkillRoots;
        if layout.style_skill_roots_inner.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            view.editor.style_skill_roots_field.handle_mouse_click(
                mouse_event.column,
                mouse_event.row,
                layout.style_skill_roots_inner,
            );
        }
        return true;
    }
    if layout.style_mcp_include_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        view.editor.focus = Focus::StyleMcpInclude;
        if layout.style_mcp_include_inner.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            view.editor.style_mcp_include_field.handle_mouse_click(
                mouse_event.column,
                mouse_event.row,
                layout.style_mcp_include_inner,
            );
        }
        return true;
    }
    if layout.style_mcp_exclude_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        view.editor.focus = Focus::StyleMcpExclude;
        if layout.style_mcp_exclude_inner.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            view.editor.style_mcp_exclude_field.handle_mouse_click(
                mouse_event.column,
                mouse_event.row,
                layout.style_mcp_exclude_inner,
            );
        }
        return true;
    }
    if layout.examples_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        view.editor.focus = Focus::Examples;
        if layout.examples_inner.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            view.editor.examples_field.handle_mouse_click(
                mouse_event.column,
                mouse_event.row,
                layout.examples_inner,
            );
        }
        return true;
    }
    if layout.body_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        view.editor.focus = Focus::Body;
        if layout.body_inner.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            view.editor.body_field.handle_mouse_click(
                mouse_event.column,
                mouse_event.row,
                layout.body_inner,
            );
        }
        return true;
    }

    handle_edit_button_click(view, mouse_event, layout.buttons_row)
}

fn handle_edit_mouse_move(
    view: &mut SkillsSettingsView,
    mouse_event: MouseEvent,
    area: Rect,
    chrome: ChromeMode,
) -> bool {
    let Some(layout) = scroll::compute_form_layout_for_chrome(view, area, chrome) else {
        return set_hovered_button(view, None);
    };
    set_hovered_button(
        view,
        edit_button_at(view, mouse_event.column, mouse_event.row, layout.buttons_row),
    )
}

fn handle_edit_scroll(
    view: &mut SkillsSettingsView,
    mouse_event: MouseEvent,
    area: Rect,
    chrome: ChromeMode,
    scroll_down: bool,
) -> bool {
    let Some(layout) = scroll::compute_form_layout_for_chrome(view, area, chrome) else {
        return false;
    };
    let container_delta = if scroll_down { 3 } else { -3 };

    if layout.style_references_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        let previous_focus = view.editor.focus;
        view.editor.focus = Focus::StyleReferences;
        let moved = view
            .editor
            .style_references_field
            .handle_mouse_scroll(scroll_down);
        let focus_changed = previous_focus != view.editor.focus;
        let scrolled = scroll::scroll_edit_container_by(view, container_delta, layout.max_scroll);
        return moved || focus_changed || scrolled;
    }
    if layout.style_skill_roots_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        let previous_focus = view.editor.focus;
        view.editor.focus = Focus::StyleSkillRoots;
        let moved = view
            .editor
            .style_skill_roots_field
            .handle_mouse_scroll(scroll_down);
        let focus_changed = previous_focus != view.editor.focus;
        let scrolled = scroll::scroll_edit_container_by(view, container_delta, layout.max_scroll);
        return moved || focus_changed || scrolled;
    }
    if layout.style_mcp_include_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        let previous_focus = view.editor.focus;
        view.editor.focus = Focus::StyleMcpInclude;
        let moved = view
            .editor
            .style_mcp_include_field
            .handle_mouse_scroll(scroll_down);
        let focus_changed = previous_focus != view.editor.focus;
        let scrolled = scroll::scroll_edit_container_by(view, container_delta, layout.max_scroll);
        return moved || focus_changed || scrolled;
    }
    if layout.style_mcp_exclude_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        let previous_focus = view.editor.focus;
        view.editor.focus = Focus::StyleMcpExclude;
        let moved = view
            .editor
            .style_mcp_exclude_field
            .handle_mouse_scroll(scroll_down);
        let focus_changed = previous_focus != view.editor.focus;
        let scrolled = scroll::scroll_edit_container_by(view, container_delta, layout.max_scroll);
        return moved || focus_changed || scrolled;
    }
    if layout.examples_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        let previous_focus = view.editor.focus;
        view.editor.focus = Focus::Examples;
        let moved = view.editor.examples_field.handle_mouse_scroll(scroll_down);
        let focus_changed = previous_focus != view.editor.focus;
        let scrolled = scroll::scroll_edit_container_by(view, container_delta, layout.max_scroll);
        return moved || focus_changed || scrolled;
    }
    if layout.body_outer.contains(Position {
        x: mouse_event.column,
        y: mouse_event.row,
    }) {
        let previous_focus = view.editor.focus;
        view.editor.focus = Focus::Body;
        let moved = view.editor.body_field.handle_mouse_scroll(scroll_down);
        let focus_changed = previous_focus != view.editor.focus;
        let scrolled = scroll::scroll_edit_container_by(view, container_delta, layout.max_scroll);
        return moved || focus_changed || scrolled;
    }

    scroll::scroll_edit_container_by(view, container_delta, layout.max_scroll)
}

fn handle_edit_button_click(
    view: &mut SkillsSettingsView,
    mouse_event: MouseEvent,
    row: Rect,
) -> bool {
    let Some(button) = edit_button_at(view, mouse_event.column, mouse_event.row, row) else {
        return false;
    };
    set_hovered_button(view, Some(button));
    view.editor.focus = button.focus();

    match button {
        ActionButton::Generate => view.generate_draft(),
        ActionButton::Save => view.save_current(),
        ActionButton::Delete => view.delete_current(),
        ActionButton::Cancel => input::cancel_edit(view),
    }
    true
}

fn edit_button_at(view: &SkillsSettingsView, x: u16, y: u16, row: Rect) -> Option<ActionButton> {
    standard_button_at(x, y, row, &view.action_button_specs())
}

fn set_hovered_button(view: &mut SkillsSettingsView, hovered: Option<ActionButton>) -> bool {
    if view.editor.hovered_button == hovered {
        return false;
    }
    view.editor.hovered_button = hovered;
    true
}

