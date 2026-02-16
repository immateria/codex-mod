use crossterm::event::KeyModifiers;
use ratatui::layout::Rect;
use ratatui::widgets::{Paragraph, Wrap};
use crate::ui_interaction::next_scroll_top_with_delta;

use super::{McpSettingsView, SummaryMetrics, SUMMARY_HORIZONTAL_SCROLL_STEP, SUMMARY_PAGE_STEP};

impl McpSettingsView {
    fn normalize_summary_scroll_top(&mut self) -> usize {
        let max_scroll = self.summary_last_max_scroll.get();
        let normalized = if self.summary_scroll_top == usize::MAX {
            max_scroll
        } else {
            self.summary_scroll_top.min(max_scroll)
        };
        self.summary_scroll_top = normalized;
        normalized
    }

    pub(super) fn summary_metrics_for_viewport(&self, viewport: Rect) -> SummaryMetrics {
        if viewport.width == 0 || viewport.height == 0 {
            self.summary_last_max_scroll.set(0);
            return SummaryMetrics {
                total_lines: 0,
                max_width: 0,
                visible_lines: 0,
            };
        }

        let lines = self.summary_lines();
        let max_width = lines.iter().map(ratatui::text::Line::width).max().unwrap_or(0);
        let mut paragraph = Paragraph::new(lines);
        if self.summary_wrap {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }
        let total_lines = paragraph.line_count(viewport.width);
        let visible_lines = viewport.height as usize;
        self.summary_last_max_scroll
            .set(total_lines.saturating_sub(visible_lines));

        SummaryMetrics {
            total_lines,
            max_width,
            visible_lines,
        }
    }

    pub(super) fn scroll_summary_lines(&mut self, delta: isize) {
        let max_scroll = self.summary_last_max_scroll.get();
        let current = self.normalize_summary_scroll_top();
        self.summary_scroll_top = next_scroll_top_with_delta(current, max_scroll, delta);
    }

    pub(super) fn page_summary(&mut self, pages: isize) {
        let delta = pages * SUMMARY_PAGE_STEP as isize;
        self.scroll_summary_lines(delta);
    }

    pub(super) fn scroll_summary_with_wheel(
        &mut self,
        delta_lines: isize,
        viewport: Rect,
        modifiers: KeyModifiers,
    ) {
        let metrics = self.summary_metrics_for_viewport(viewport);
        let has_vertical_overflow = metrics.total_lines > metrics.visible_lines;
        let force_horizontal = modifiers.contains(KeyModifiers::SHIFT);
        let use_horizontal = !self.summary_wrap && (force_horizontal || !has_vertical_overflow);

        if use_horizontal {
            let unit = SUMMARY_HORIZONTAL_SCROLL_STEP * delta_lines.signum() as i32;
            self.shift_summary_hscroll(unit);
        } else {
            self.scroll_summary_lines(delta_lines);
        }
    }
}
