use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

impl PromptsSettingsView {
    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        self.render_body_without_frame(area, buf);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        self.render_body(area, buf);
    }

    fn render_body(&self, area: Rect, buf: &mut Buffer) {
        match self.mode {
            Mode::List => self.render_list(area, buf),
            Mode::Edit => self.render_form(area, buf),
        }
    }

    fn render_body_without_frame(&self, area: Rect, buf: &mut Buffer) {
        match self.mode {
            Mode::List => self.render_list_without_frame(area, buf),
            Mode::Edit => self.render_form_without_frame(area, buf),
        }
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.list_rows();
        let Some(_layout) = self
            .list_page()
            .framed()
            .render_menu_rows(area, buf, 0, Some(self.selected), &rows)
        else {
            return;
        };
    }

    fn render_list_without_frame(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.list_rows();
        let _ = self
            .list_page()
            .content_only()
            .render_menu_rows(area, buf, 0, Some(self.selected), &rows);
    }

    fn render_form(&self, area: Rect, buf: &mut Buffer) {
        let page = self.edit_form_page();
        let buttons = self.edit_button_specs();
        let Some(_layout) = page.framed().render_with_standard_actions_end(
            area,
            buf,
            &[&self.name_field, &self.body_field],
            &buttons,
        )
        else {
            return;
        };
    }

    fn render_form_without_frame(&self, area: Rect, buf: &mut Buffer) {
        let page = self.edit_form_page();
        let buttons = self.edit_button_specs();
        let _ = page.content_only().render_with_standard_actions_end(
            area,
            buf,
            &[&self.name_field, &self.body_field],
            &buttons,
        );
    }
}

