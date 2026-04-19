use super::*;

impl ChatWidget<'_> {
    pub(super) fn render_settings_overlay(
        &self,
        frame_area: Rect,
        history_area: Rect,
        buf: &mut Buffer,
        overlay: &SettingsOverlayView,
    ) {
        use ratatui::widgets::Clear;

        let scrim_style = Style::default()
            .bg(crate::colors::overlay_scrim())
            .fg(crate::colors::text_dim());
        fill_rect(buf, frame_area, None, scrim_style);

        // Reduce horizontal padding on very narrow screens to maximize content room.
        let padding = u16::from(frame_area.width >= 40);
        let bottom = frame_area.y.saturating_add(frame_area.height);
        let height = bottom.saturating_sub(history_area.y);
        let overlay_area = Rect {
            x: frame_area.x.saturating_add(padding),
            y: history_area.y,
            width: frame_area.width.saturating_sub(padding * 2),
            height,
        };

        Clear.render(overlay_area, buf);

        let bg_style = crate::colors::style_on_overlay_scrim();
        fill_rect(buf, overlay_area, None, bg_style);

        overlay.render(overlay_area, buf);
    }
}
