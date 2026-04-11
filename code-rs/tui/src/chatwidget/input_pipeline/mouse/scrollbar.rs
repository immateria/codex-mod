impl ChatWidget<'_> {
    /// Check if a mouse-down at (x, y) lands on the history scrollbar thumb.
    /// If so, begin a drag and return `true`.
    pub(in super::super) fn try_begin_scrollbar_drag(&mut self, x: u16, y: u16) -> bool {
        let sb_area = self.scrollbar_hit_area();
        if sb_area.width == 0 || sb_area.height < 2 {
            return false;
        }

        // Scrollbar column is the rightmost column of the history area.
        let scrollbar_x = sb_area.x.saturating_add(sb_area.width.saturating_sub(1));
        if x != scrollbar_x {
            return false;
        }
        if y < sb_area.y || y >= sb_area.y + sb_area.height {
            return false;
        }

        let max_scroll = self.layout.last_max_scroll.get();
        if max_scroll == 0 {
            return false;
        }

        let Some((thumb_start, thumb_len)) = self.scrollbar_thumb_geometry(sb_area, max_scroll) else {
            return false;
        };

        let rel_y = (y.saturating_sub(sb_area.y)) as usize;

        if rel_y >= thumb_start && rel_y < thumb_start + thumb_len {
            // Clicked on the thumb — begin drag.
            let offset = rel_y.saturating_sub(thumb_start);
            self.scrollbar_drag_offset.set(Some(offset));
            self.mouse_drag_exceeded.set(true);
            layout_scroll::flash_scrollbar(self);
            self.request_redraw();
            return true;
        }

        // Clicked on track above or below thumb — page jump.
        if rel_y < thumb_start {
            layout_scroll::page_up(self);
        } else {
            layout_scroll::page_down(self);
        }
        layout_scroll::flash_scrollbar(self);

        // Also begin a drag so continued movement scrolls smoothly.
        let (_, new_thumb_len) =
            self.scrollbar_thumb_geometry(sb_area, max_scroll).unwrap_or((0, 1));
        let offset = new_thumb_len / 2;
        self.scrollbar_drag_offset.set(Some(offset));
        self.mouse_drag_exceeded.set(true);
        true
    }

    /// Update scroll position during an active scrollbar drag.
    pub(in super::super) fn handle_scrollbar_drag(&mut self, y: u16) {
        let Some(offset_in_thumb) = self.scrollbar_drag_offset.get() else {
            return;
        };

        let sb_area = self.scrollbar_hit_area();
        let max_scroll = self.layout.last_max_scroll.get();
        if max_scroll == 0 || sb_area.height < 2 {
            return;
        }

        let track_len = sb_area.height as usize;
        let viewport_h = self.layout.last_history_viewport_height.get() as usize;
        let content_len = (max_scroll as usize).saturating_add(1);

        // Compute thumb length (same formula as scrollbar_thumb_geometry).
        let thumb_len = if viewport_h == 0 || content_len == 0 {
            1
        } else {
            let total = content_len + viewport_h;
            let raw = (viewport_h as f64 * track_len as f64 / total as f64).round() as usize;
            raw.max(1).min(track_len)
        };

        let rel_y = y.saturating_sub(sb_area.y) as usize;
        let clamped_y = rel_y.min(track_len.saturating_sub(1));
        let desired_thumb_start = clamped_y
            .saturating_sub(offset_in_thumb)
            .min(track_len.saturating_sub(thumb_len));

        // Convert thumb position to scroll-from-top.
        let max_thumb_start = track_len.saturating_sub(thumb_len);
        let pos_from_top = if max_thumb_start == 0 {
            0
        } else {
            let frac = desired_thumb_start as f64 / max_thumb_start as f64;
            (frac * f64::from(max_scroll)).round() as u16
        };

        layout_scroll::set_from_scrollbar(self, pos_from_top);
    }

    /// The area used for scrollbar hit-testing — matches what `post_paint` renders.
    fn scrollbar_hit_area(&self) -> Rect {
        let history_area = self.layout.last_history_area.get();
        Rect {
            x: history_area.x,
            y: history_area.y,
            width: history_area.width,
            // post_paint subtracts 1 row from the bottom.
            height: history_area.height.saturating_sub(1),
        }
    }

    /// Compute the thumb start offset (rows from top of track) and thumb length.
    /// Uses the same math as ratatui's no-arrows Scrollbar.
    fn scrollbar_thumb_geometry(&self, sb_area: Rect, max_scroll: u16) -> Option<(usize, usize)> {
        let track_len = sb_area.height as usize;
        if track_len == 0 || max_scroll == 0 {
            return None;
        }

        let viewport_h = self.layout.last_history_viewport_height.get() as usize;
        let content_len = (max_scroll as usize).saturating_add(1);
        let scroll_offset = self.layout.scroll_offset.get();
        let pos_from_top = max_scroll.saturating_sub(scroll_offset) as usize;

        // Thumb length: proportional to viewport / total.
        let total = content_len + viewport_h;
        let thumb_len = if total == 0 {
            1
        } else {
            let raw = (viewport_h as f64 * track_len as f64 / total as f64).round() as usize;
            raw.max(1).min(track_len)
        };

        // Thumb start: proportional to position / total.
        let start_f = pos_from_top as f64 * track_len as f64 / total as f64;
        let thumb_start = start_f.round().clamp(0.0, (track_len - 1) as f64) as usize;

        Some((thumb_start, thumb_len))
    }
}
