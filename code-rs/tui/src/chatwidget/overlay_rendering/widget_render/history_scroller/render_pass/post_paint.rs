use super::*;

pub(super) struct PostPaintArgs {
    pub history_area: Rect,
    pub content_area: Rect,
    pub base_style: Style,
    pub total_height: u16,
    pub scroll_pos: u16,
    pub screen_y: u16,
}

impl ChatWidget<'_> {
    pub(super) fn render_history_post_paint(
        &self,
        args: PostPaintArgs,
        buf: &mut Buffer,
    ) {
        let PostPaintArgs {
            history_area,
            content_area,
            base_style,
            total_height,
            scroll_pos,
            screen_y,
        } = args;

        if screen_y < content_area.y + content_area.height {
            let _perf_hist_clear2 = if self.perf_state.enabled {
                Some(std::time::Instant::now())
            } else {
                None
            };
            let gap_height = (content_area.y + content_area.height).saturating_sub(screen_y);
            if gap_height > 0 {
                let gap_rect = Rect::new(content_area.x, screen_y, content_area.width, gap_height);
                fill_rect(buf, gap_rect, Some(' '), base_style);
            }
            if let Some(t0) = _perf_hist_clear2 {
                let dt = t0.elapsed().as_nanos();
                let mut p = self.perf_state.stats.borrow_mut();
                p.ns_history_clear = p.ns_history_clear.saturating_add(dt);
                let cells = (content_area.width as u64)
                    * ((content_area.y + content_area.height - screen_y) as u64);
                p.cells_history_clear = p.cells_history_clear.saturating_add(cells);
            }
        }

        let now = std::time::Instant::now();
        let show_scrollbar = total_height > content_area.height
            && self
                .layout
                .scrollbar_visible_until
                .get()
                .map(|t| now < t)
                .unwrap_or(false);
        if show_scrollbar {
            let mut sb_state = self.layout.vertical_scrollbar_state.borrow_mut();
            let max_scroll = total_height.saturating_sub(content_area.height);
            let scroll_positions = max_scroll.saturating_add(1).max(1) as usize;
            let pos = scroll_pos.min(max_scroll) as usize;
            *sb_state = sb_state.content_length(scroll_positions).position(pos);
            let theme = crate::theme::current_theme();
            let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .symbols(scrollbar_symbols::VERTICAL)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .track_style(
                    Style::default()
                        .fg(crate::colors::border())
                        .bg(crate::colors::background()),
                )
                .thumb_symbol("█")
                .thumb_style(
                    Style::default()
                        .fg(theme.border_focused)
                        .bg(crate::colors::background()),
                );
            let sb_area = Rect {
                x: history_area.x,
                y: history_area.y,
                width: history_area.width,
                height: history_area.height.saturating_sub(1),
            };
            StatefulWidget::render(sb, sb_area, buf, &mut sb_state);
        }
    }
}
