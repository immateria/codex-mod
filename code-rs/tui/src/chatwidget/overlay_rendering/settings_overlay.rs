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

        let padding = 1u16;
        let overlay_area = Rect {
            x: history_area.x + padding,
            y: history_area.y,
            width: history_area.width.saturating_sub(padding * 2),
            height: history_area.height,
        };

        Clear.render(overlay_area, buf);

        let bg_style = Style::default().bg(crate::colors::overlay_scrim());
        fill_rect(buf, overlay_area, None, bg_style);

        overlay.render(overlay_area, buf);
    }
}
