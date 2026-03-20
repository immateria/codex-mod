/// High-level outcome for mouse interaction on selectable vertical lists.
///
/// This lets views implement mouse support with minimal boilerplate:
/// 1) provide `selected` index and item count,
/// 2) provide a row hit-test closure,
/// 3) react to `Activated` for click-to-open/toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectableListMouseResult {
    Ignored,
    SelectionChanged,
    Activated,
}

impl SelectableListMouseResult {
    pub const fn handled(self) -> bool {
        !matches!(self, Self::Ignored)
    }
}

/// How wheel movement should traverse list selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollSelectionBehavior {
    Wrap,
    Clamp,
}

/// Configures common list mouse routing behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SelectableListMouseConfig {
    pub hover_select: bool,
    pub activate_on_left_click: bool,
    pub scroll_select: bool,
    pub require_pointer_hit_for_scroll: bool,
    pub scroll_behavior: ScrollSelectionBehavior,
}

impl Default for SelectableListMouseConfig {
    fn default() -> Self {
        Self {
            hover_select: true,
            activate_on_left_click: true,
            scroll_select: true,
            require_pointer_hit_for_scroll: false,
            scroll_behavior: ScrollSelectionBehavior::Wrap,
        }
    }
}

/// Route common mouse interactions for selectable lists.
///
/// Behavior:
/// - move: hover-select row under cursor
/// - left-click: select row and mark activated
/// - wheel: move selection up/down (wrapping)
pub(crate) fn route_selectable_list_mouse(
    mouse_event: MouseEvent,
    selected: &mut usize,
    item_count: usize,
    row_at_position: impl Fn(u16, u16) -> Option<usize>,
) -> SelectableListMouseResult {
    route_selectable_list_mouse_impl(
        mouse_event,
        selected,
        item_count,
        row_at_position,
        SelectableListMouseConfig::default(),
    )
}

pub(crate) fn route_selectable_list_mouse_with_config(
    mouse_event: MouseEvent,
    selected: &mut usize,
    item_count: usize,
    row_at_position: impl Fn(u16, u16) -> Option<usize>,
    config: SelectableListMouseConfig,
) -> SelectableListMouseResult {
    if config == SelectableListMouseConfig::default() {
        return route_selectable_list_mouse(mouse_event, selected, item_count, row_at_position);
    }

    route_selectable_list_mouse_impl(mouse_event, selected, item_count, row_at_position, config)
}

pub(crate) fn step_index_by_delta(
    current: usize,
    count: usize,
    delta: isize,
    behavior: ScrollSelectionBehavior,
) -> usize {
    if count == 0 || delta == 0 {
        return 0;
    }

    let mut index = clamp_index(current, count);
    let steps = delta.unsigned_abs();
    for _ in 0..steps {
        index = if delta < 0 {
            match behavior {
                ScrollSelectionBehavior::Wrap => wrap_prev(index, count),
                ScrollSelectionBehavior::Clamp => index.saturating_sub(1),
            }
        } else {
            match behavior {
                ScrollSelectionBehavior::Wrap => wrap_next(index, count),
                ScrollSelectionBehavior::Clamp => index.saturating_add(1).min(count - 1),
            }
        };
    }
    index
}

fn route_selectable_list_mouse_impl(
    mouse_event: MouseEvent,
    selected: &mut usize,
    item_count: usize,
    row_at_position: impl Fn(u16, u16) -> Option<usize>,
    config: SelectableListMouseConfig,
) -> SelectableListMouseResult {
    if item_count == 0 {
        *selected = 0;
        return SelectableListMouseResult::Ignored;
    }
    *selected = clamp_index(*selected, item_count);

    let pointer_selection = || {
        row_at_position(mouse_event.column, mouse_event.row).filter(|idx| *idx < item_count)
    };

    match mouse_event.kind {
        MouseEventKind::Moved => {
            if !config.hover_select {
                return SelectableListMouseResult::Ignored;
            }
            let Some(next) = pointer_selection() else {
                return SelectableListMouseResult::Ignored;
            };
            if *selected == next {
                SelectableListMouseResult::Ignored
            } else {
                *selected = next;
                SelectableListMouseResult::SelectionChanged
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if !config.activate_on_left_click {
                return SelectableListMouseResult::Ignored;
            }
            let Some(next) = pointer_selection() else {
                return SelectableListMouseResult::Ignored;
            };
            *selected = next;
            SelectableListMouseResult::Activated
        }
        MouseEventKind::ScrollUp => {
            if !config.scroll_select {
                return SelectableListMouseResult::Ignored;
            }
            if config.require_pointer_hit_for_scroll && pointer_selection().is_none() {
                return SelectableListMouseResult::Ignored;
            }
            let next = match config.scroll_behavior {
                ScrollSelectionBehavior::Wrap => wrap_prev(*selected, item_count),
                ScrollSelectionBehavior::Clamp => selected.saturating_sub(1),
            };
            if next == *selected {
                SelectableListMouseResult::Ignored
            } else {
                *selected = next;
                SelectableListMouseResult::SelectionChanged
            }
        }
        MouseEventKind::ScrollDown => {
            if !config.scroll_select {
                return SelectableListMouseResult::Ignored;
            }
            if config.require_pointer_hit_for_scroll && pointer_selection().is_none() {
                return SelectableListMouseResult::Ignored;
            }
            let next = match config.scroll_behavior {
                ScrollSelectionBehavior::Wrap => wrap_next(*selected, item_count),
                ScrollSelectionBehavior::Clamp => selected.saturating_add(1).min(item_count - 1),
            };
            if next == *selected {
                SelectableListMouseResult::Ignored
            } else {
                *selected = next;
                SelectableListMouseResult::SelectionChanged
            }
        }
        _ => SelectableListMouseResult::Ignored,
    }
}
