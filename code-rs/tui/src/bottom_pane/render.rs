use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use crate::util::buffer::fill_rect;

use super::layout::compute_composer_rect;
use super::{ActiveViewKind, BottomPane};

impl WidgetRef for &BottomPane<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // Base clear: fill the entire bottom pane with the theme background so
        // newly exposed rows (e.g., when the composer grows on paste) do not
        // show stale pixels from history.
        let base_style = ratatui::style::Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        fill_rect(buf, area, Some(' '), base_style);

        let mut composer_rect = compute_composer_rect(area, self.top_spacer_enabled);
        let mut composer_needs_render = true;

        if let Some(view) = &self.active_view
            && !view.is_complete()
        {
            let is_auto = matches!(self.active_view_kind, ActiveViewKind::AutoCoordinator);
            if is_auto {
                if let Some(view_rect) = self.compute_active_view_rect(area, view.as_ref()) {
                    let view_bg =
                        ratatui::style::Style::default().bg(crate::colors::background());
                    fill_rect(buf, view_rect, None, view_bg);
                    view.render(view_rect, buf);
                    let remaining_height = area.height.saturating_sub(view_rect.height);
                    if remaining_height > 0 {
                        let composer_area = Rect {
                            x: area.x,
                            y: view_rect.y.saturating_add(view_rect.height),
                            width: area.width,
                            height: remaining_height,
                        };
                        composer_rect = compute_composer_rect(composer_area, false);
                    }
                } else {
                    composer_rect = compute_composer_rect(area, self.top_spacer_enabled);
                }
            } else if let Some(view_rect) = self.compute_active_view_rect(area, view.as_ref()) {
                let view_bg = ratatui::style::Style::default().bg(crate::colors::background());
                fill_rect(buf, view_rect, None, view_bg);
                view.render_with_composer(view_rect, buf, &self.composer);
                composer_needs_render = false;
            }
        }

        if composer_needs_render && composer_rect.width > 0 && composer_rect.height > 0 {
            let comp_bg = ratatui::style::Style::default().bg(crate::colors::background());
            fill_rect(buf, composer_rect, None, comp_bg);
            self.composer.render_ref(composer_rect, buf);
        }
    }
}

