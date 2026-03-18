use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::components::scroll_state::ScrollState;
use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

pub(crate) struct ScrollStateMouseOutcome {
    pub(crate) result: SelectableListMouseResult,
    pub(crate) changed: bool,
}

/// Route mouse events for a scrollable selectable list backed by `ScrollState`.
///
/// - Normalizes `ScrollState` before hit-testing (selection + scroll clamp)
/// - Returns whether anything changed (including state normalization)
/// - Leaves activation handling to the caller (use `outcome.result`)
pub(crate) fn route_scroll_state_mouse_with_hit_test(
    mouse_event: MouseEvent,
    state: &mut ScrollState,
    item_count: usize,
    visible_rows: usize,
    row_at_position: impl Fn(u16, u16, usize) -> Option<usize>,
    config: SelectableListMouseConfig,
) -> ScrollStateMouseOutcome {
    let before_selected = state.selected_idx;
    let before_scroll_top = state.scroll_top;

    state.clamp_selection(item_count);

    if item_count == 0 {
        return ScrollStateMouseOutcome {
            result: SelectableListMouseResult::Ignored,
            changed: state.selected_idx != before_selected || state.scroll_top != before_scroll_top,
        };
    }

    let scroll_top = state.scroll_top;

    let mut selected = state
        .selected_idx
        .expect("selected_idx must be Some for non-empty lists");
    let result = route_selectable_list_mouse_with_config(
        mouse_event,
        &mut selected,
        item_count,
        |x, y| row_at_position(x, y, scroll_top),
        config,
    );

    state.selected_idx = Some(selected);
    state.ensure_visible(item_count, visible_rows.max(1));

    let state_changed = state.selected_idx != before_selected || state.scroll_top != before_scroll_top;
    ScrollStateMouseOutcome {
        result,
        changed: result.handled() || state_changed,
    }
}

pub(crate) fn route_scroll_state_mouse_with_hit_test_no_ensure_visible(
    mouse_event: MouseEvent,
    state: &mut ScrollState,
    item_count: usize,
    row_at_position: impl Fn(u16, u16, usize) -> Option<usize>,
    config: SelectableListMouseConfig,
) -> ScrollStateMouseOutcome {
    let before_selected = state.selected_idx;
    let before_scroll_top = state.scroll_top;

    state.clamp_selection(item_count);

    if item_count == 0 {
        return ScrollStateMouseOutcome {
            result: SelectableListMouseResult::Ignored,
            changed: state.selected_idx != before_selected || state.scroll_top != before_scroll_top,
        };
    }

    let scroll_top = state.scroll_top;

    let mut selected = state
        .selected_idx
        .expect("selected_idx must be Some for non-empty lists");
    let result = route_selectable_list_mouse_with_config(
        mouse_event,
        &mut selected,
        item_count,
        |x, y| row_at_position(x, y, scroll_top),
        config,
    );

    state.selected_idx = Some(selected);

    let state_changed = state.selected_idx != before_selected || state.scroll_top != before_scroll_top;
    ScrollStateMouseOutcome {
        result,
        changed: result.handled() || state_changed,
    }
}

pub(crate) fn route_scroll_state_mouse_in_body(
    mouse_event: MouseEvent,
    body: Rect,
    state: &mut ScrollState,
    item_count: usize,
    config: SelectableListMouseConfig,
) -> ScrollStateMouseOutcome {
    route_scroll_state_mouse_with_hit_test(
        mouse_event,
        state,
        item_count,
        body.height as usize,
        |x, y, scroll_top| super::rows::selection_index_at(body, x, y, scroll_top, item_count),
        config,
    )
}
