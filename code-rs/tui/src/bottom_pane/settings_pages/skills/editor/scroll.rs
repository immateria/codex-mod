use super::*;

pub(super) fn scroll_edit_container_by(
    view: &mut SkillsSettingsView,
    delta: isize,
    max_scroll: usize,
) -> bool {
    if max_scroll == 0 || delta == 0 {
        return false;
    }
    let next = next_scroll_top_with_delta(view.editor.edit_scroll_top, max_scroll, delta);
    if next == view.editor.edit_scroll_top {
        false
    } else {
        view.editor.edit_scroll_top = next;
        true
    }
}

fn ensure_edit_focus_visible(view: &mut SkillsSettingsView, layout: &SkillsFormLayout) -> bool {
    if layout.max_scroll == 0 || layout.viewport_inner.height == 0 {
        return false;
    }
    let Some((focus_top, focus_h)) = layout.focus_bounds(view.editor.focus) else {
        return false;
    };
    if focus_h == 0 {
        return false;
    }

    let viewport_h = layout.viewport_inner.height as usize;
    let next = scroll_top_to_keep_visible(
        view.editor.edit_scroll_top,
        layout.max_scroll,
        viewport_h,
        focus_top,
        focus_h,
    );
    if next == view.editor.edit_scroll_top {
        false
    } else {
        view.editor.edit_scroll_top = next;
        true
    }
}

pub(super) fn ensure_edit_focus_visible_from_last_render(view: &mut SkillsSettingsView) -> bool {
    let Some((area, chrome)) = view.last_render.get() else {
        return false;
    };
    let Some(layout) = (match chrome {
        ChromeMode::Framed => view.compute_form_layout_framed(area),
        ChromeMode::ContentOnly => view.compute_form_layout_content_only(area),
    }) else {
        return false;
    };
    ensure_edit_focus_visible(view, &layout)
}

pub(super) fn compute_form_layout_for_chrome(
    view: &SkillsSettingsView,
    area: Rect,
    chrome: ChromeMode,
) -> Option<SkillsFormLayout> {
    match chrome {
        ChromeMode::Framed => view.compute_form_layout_framed(area),
        ChromeMode::ContentOnly => view.compute_form_layout_content_only(area),
    }
}

