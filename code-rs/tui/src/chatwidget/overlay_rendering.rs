use super::*;

mod agents_terminal_overlay;
mod browser_overlay;
mod settings_overlay;
mod widget_render;

impl WidgetRef for &ChatWidget<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        self.render_widget_ref(area, buf);
    }
}
