#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use crate::bottom_pane::ConditionalUpdate;

    use super::{
        HeaderBodyFooterLayout,
        ListWindow,
        PinnedFooterLayout,
        ScrollSelectionBehavior,
        SelectableListMouseConfig,
        SelectableListMouseResult,
        VerticalScrollbarHit,
        centered_scroll_top,
        clamp_index,
        clipped_vertical_rect_with_scroll,
        contains_point,
        inset_rect_right,
        next_scroll_top_with_delta,
        redraw_if,
        render_vertical_scrollbar,
        route_selectable_list_mouse,
        route_selectable_list_mouse_with_config,
        scroll_top_to_keep_visible,
        split_header_body_footer,
        split_pinned_footer_layout,
        split_two_pane_when_room,
        step_index_by_delta,
        vertical_scrollbar_metrics,
        vertical_scrollbar_position_for_thumb_drag,
        vertical_scrollbar_right_hit_test,
        wrap_next,
        wrap_prev,
    };
    #[cfg(feature = "browser-automation")]
    use super::hit_test_repeating_rows;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::buffer::Buffer;

    #[cfg(feature = "browser-automation")]
    #[test]
    fn repeating_rows_skip_spacer_lines() {
        let area = Rect::new(0, 0, 80, 30);
        // first row=4, each group is 3 rows where top 2 are interactive.
        assert_eq!(
            hit_test_repeating_rows(area, 5, 4, 4, 2, 3, 3),
            Some(0)
        );
        assert_eq!(
            hit_test_repeating_rows(area, 5, 5, 4, 2, 3, 3),
            Some(0)
        );
        assert_eq!(
            hit_test_repeating_rows(area, 5, 6, 4, 2, 3, 3),
            None
        );
        assert_eq!(
            hit_test_repeating_rows(area, 5, 7, 4, 2, 3, 3),
            Some(1)
        );
    }

    #[test]
    fn selection_helpers_wrap_and_clamp() {
        assert_eq!(wrap_prev(0, 3), 2);
        assert_eq!(wrap_prev(2, 3), 1);
        assert_eq!(wrap_next(2, 3), 0);
        assert_eq!(wrap_next(1, 3), 2);
        assert_eq!(clamp_index(9, 3), 2);
        assert_eq!(clamp_index(0, 0), 0);
    }

    #[test]
    fn contains_point_checks_bounds() {
        let area = Rect::new(2, 3, 4, 5);
        assert!(contains_point(area, 2, 3));
        assert!(contains_point(area, 5, 7));
        assert!(!contains_point(area, 6, 7));
        assert!(!contains_point(area, 5, 8));
    }

    #[test]
    fn pinned_footer_layout_reserves_viewport_and_rows() {
        let area = Rect::new(10, 20, 80, 16);
        let layout = split_pinned_footer_layout(area, 1, 1, 4);

        assert_eq!(
            layout,
            PinnedFooterLayout {
                viewport: Rect::new(10, 20, 80, 14),
                status_row: Rect::new(10, 34, 80, 1),
                action_row: Rect::new(10, 35, 80, 1),
            }
        );
    }

    #[test]
    fn pinned_footer_layout_drops_status_when_height_is_tight() {
        let area = Rect::new(0, 0, 40, 4);
        let layout = split_pinned_footer_layout(area, 1, 1, 4);

        assert_eq!(layout.viewport, Rect::new(0, 0, 40, 3));
        assert_eq!(layout.status_row, Rect::default());
        assert_eq!(layout.action_row, Rect::new(0, 3, 40, 1));
    }

    #[test]
    fn header_body_footer_layout_splits_with_min_body() {
        let area = Rect::new(4, 5, 80, 20);
        let layout = split_header_body_footer(area, 2, 3, 2).expect("layout");
        assert_eq!(
            layout,
            HeaderBodyFooterLayout {
                header: Rect::new(4, 5, 80, 2),
                body: Rect::new(4, 7, 80, 15),
                footer: Rect::new(4, 22, 80, 3),
            }
        );
    }

    #[test]
    fn header_body_footer_layout_returns_none_when_too_short() {
        let area = Rect::new(0, 0, 40, 6);
        assert!(split_header_body_footer(area, 2, 2, 2).is_none());
    }

    #[test]
    fn split_two_pane_when_room_honors_minimums() {
        let area = Rect::new(10, 20, 100, 12);
        let (left, right) = split_two_pane_when_room(area, 96, 10, 42).expect("split");
        assert_eq!(left.height, area.height);
        assert_eq!(right.height, area.height);
        assert_eq!(left.width.saturating_add(right.width), area.width);

        let too_narrow = Rect::new(10, 20, 90, 12);
        assert!(split_two_pane_when_room(too_narrow, 96, 10, 42).is_none());

        let too_short = Rect::new(10, 20, 100, 8);
        assert!(split_two_pane_when_room(too_short, 96, 10, 42).is_none());
    }

    #[test]
    fn clipped_vertical_rect_with_scroll_clips_virtual_section() {
        let viewport = Rect::new(5, 10, 20, 6);
        let clipped = clipped_vertical_rect_with_scroll(viewport, 4, 5, 2);
        assert_eq!(clipped, Rect::new(5, 12, 20, 4));
    }

    #[test]
    fn scroll_top_to_keep_visible_moves_when_item_outside_viewport() {
        assert_eq!(scroll_top_to_keep_visible(0, 40, 10, 25, 3), 18);
        assert_eq!(scroll_top_to_keep_visible(20, 40, 10, 5, 3), 5);
        assert_eq!(scroll_top_to_keep_visible(6, 40, 10, 8, 2), 6);
    }

    #[test]
    fn next_scroll_top_with_delta_clamps_both_directions() {
        assert_eq!(next_scroll_top_with_delta(5, 20, 3), 8);
        assert_eq!(next_scroll_top_with_delta(19, 20, 5), 20);
        assert_eq!(next_scroll_top_with_delta(5, 20, -3), 2);
        assert_eq!(next_scroll_top_with_delta(1, 20, -8), 0);
    }

    #[test]
    fn inset_rect_right_reduces_width_only() {
        let area = Rect::new(4, 6, 12, 8);
        assert_eq!(inset_rect_right(area, 1), Rect::new(4, 6, 11, 8));
        assert_eq!(inset_rect_right(area, 32), Rect::new(4, 6, 0, 8));
    }

    #[test]
    fn render_vertical_scrollbar_is_noop_when_no_overflow() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 8, 6));
        let baseline = buf.clone();
        render_vertical_scrollbar(&mut buf, Rect::new(0, 0, 8, 6), 0, 0, 5);
        assert_eq!(buf, baseline);
    }

    #[test]
    fn redraw_if_maps_handled_state() {
        assert!(matches!(redraw_if(true), ConditionalUpdate::NeedsRedraw));
        assert!(matches!(redraw_if(false), ConditionalUpdate::NoRedraw));
    }

    #[test]
    fn selectable_list_router_handles_move_click_and_wheel() {
        let mut selected = 0usize;
        let row_at = |x: u16, y: u16| {
            if x == 1 && y <= 2 {
                Some(y as usize)
            } else {
                None
            }
        };

        let moved = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 1,
            row: 2,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse(moved, &mut selected, 3, row_at),
            SelectableListMouseResult::SelectionChanged
        );
        assert_eq!(selected, 2);

        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse(click, &mut selected, 3, row_at),
            SelectableListMouseResult::Activated
        );
        assert_eq!(selected, 1);

        let wheel_down = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse(wheel_down, &mut selected, 3, row_at),
            SelectableListMouseResult::SelectionChanged
        );
        assert_eq!(selected, 2);
    }

    #[test]
    fn selectable_list_router_honors_scroll_clamp_and_pointer_gate() {
        let mut selected = 1usize;
        let row_at = |x: u16, y: u16| {
            if x == 2 && y <= 2 {
                Some(y as usize)
            } else {
                None
            }
        };
        let config = SelectableListMouseConfig {
            require_pointer_hit_for_scroll: true,
            scroll_behavior: ScrollSelectionBehavior::Clamp,
            ..SelectableListMouseConfig::default()
        };

        let off_target_wheel = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 9,
            row: 9,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse_with_config(
                off_target_wheel,
                &mut selected,
                3,
                row_at,
                config
            ),
            SelectableListMouseResult::Ignored
        );
        assert_eq!(selected, 1);

        let on_target_wheel = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 2,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse_with_config(
                on_target_wheel,
                &mut selected,
                3,
                row_at,
                config
            ),
            SelectableListMouseResult::SelectionChanged
        );
        assert_eq!(selected, 2);

        let clamp_end = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 2,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse_with_config(clamp_end, &mut selected, 3, row_at, config),
            SelectableListMouseResult::Ignored
        );
        assert_eq!(selected, 2);
    }

    #[test]
    fn selectable_list_router_handles_empty_and_out_of_range_rows() {
        let mut selected = 7usize;
        let row_at = |_x: u16, _y: u16| Some(9usize);
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse(click, &mut selected, 0, row_at),
            SelectableListMouseResult::Ignored
        );
        assert_eq!(selected, 0);

        selected = 0;
        assert_eq!(
            route_selectable_list_mouse(click, &mut selected, 3, row_at),
            SelectableListMouseResult::Ignored
        );
        assert_eq!(selected, 0);
    }

    #[test]
    fn centered_list_window_keeps_selected_visible() {
        let window = ListWindow::centered(20, 5, 0);
        assert_eq!(window.start, 0);
        assert_eq!(window.end, 5);

        let window = ListWindow::centered(20, 5, 10);
        assert_eq!(window.start, 8);
        assert_eq!(window.end, 13);

        let window = ListWindow::centered(20, 5, 19);
        assert_eq!(window.start, 15);
        assert_eq!(window.end, 20);
    }

    #[test]
    fn centered_list_window_maps_relative_rows() {
        let window = ListWindow::centered(10, 4, 6);
        assert_eq!(window.start, 4);
        assert_eq!(window.index_for_relative_row(0), Some(4));
        assert_eq!(window.index_for_relative_row(3), Some(7));
        assert_eq!(window.index_for_relative_row(4), None);
    }

    #[test]
    fn centered_scroll_top_matches_window_start() {
        assert_eq!(centered_scroll_top(0, 20, 5), 0);
        assert_eq!(centered_scroll_top(10, 20, 5), 8);
        assert_eq!(centered_scroll_top(19, 20, 5), 15);
    }

    #[test]
	    fn step_index_by_delta_supports_wrap_and_clamp() {
	        assert_eq!(
	            step_index_by_delta(0, 3, -1, ScrollSelectionBehavior::Wrap),
	            2
	        );
        assert_eq!(
            step_index_by_delta(0, 3, -1, ScrollSelectionBehavior::Clamp),
            0
        );
        assert_eq!(
            step_index_by_delta(1, 3, 3, ScrollSelectionBehavior::Wrap),
            1
        );
	        assert_eq!(
	            step_index_by_delta(1, 3, 3, ScrollSelectionBehavior::Clamp),
	            2
	        );
	    }

	    #[test]
	    fn vertical_scrollbar_hit_test_and_drag_round_trip() {
	        let area = Rect::new(10, 20, 4, 10);
	        let metrics = vertical_scrollbar_metrics(area, 6, 0, 3).expect("metrics");
	        let x = area.x + area.width - 1;

	        assert_eq!(
	            vertical_scrollbar_right_hit_test(area, x, area.y, metrics),
	            Some(VerticalScrollbarHit::BeginArrow)
	        );
	        assert_eq!(
	            vertical_scrollbar_right_hit_test(area, x, area.y + area.height - 1, metrics),
	            Some(VerticalScrollbarHit::EndArrow)
	        );

	        assert!(matches!(
	            vertical_scrollbar_right_hit_test(area, x, area.y + 2, metrics),
	            Some(VerticalScrollbarHit::Thumb { .. })
	        ));

	        let pos = vertical_scrollbar_position_for_thumb_drag(area, area.y + 8, metrics, 0);
	        assert_eq!(pos, metrics.max_position);
	    }
	}
