use super::*;

impl ThemeSelectionView {
    pub(super) fn render_themes_mode(
        &self,
        body_area: Rect,
        theme: &crate::theme::Theme,
        options: &[(ThemeName, Cow<'static, str>, &'static str)],
        buf: &mut Buffer,
    ) {
        let lines = self.render_theme_option_lines_for_palette(body_area, theme, options);
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .render(body_area, buf);
    }
}
