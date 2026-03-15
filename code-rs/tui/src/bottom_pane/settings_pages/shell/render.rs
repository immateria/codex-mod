use super::*;

use crate::bottom_pane::settings_ui::fields::BorderedField;
use crate::colors;
use code_core::split_command_and_args;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

impl ShellSelectionView {
    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        if self.custom_input_mode {
            self.render_custom_mode_without_frame(area, buf);
        } else {
            self.render_list_mode_without_frame(area, buf);
        }
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        if self.custom_input_mode {
            self.render_custom_mode(area, buf);
        } else {
            self.render_list_mode(area, buf);
        }
    }

    fn render_list_mode(&self, area: Rect, buf: &mut Buffer) {
        let page = self.list_page();
        let runs = self.list_runs();
        let _ = page.framed().render_runs(area, buf, 0, &runs);
    }

    fn render_list_mode_without_frame(&self, area: Rect, buf: &mut Buffer) {
        let page = self.list_page();
        let runs = self.list_runs();
        let _ = page.content_only().render_runs(area, buf, 0, &runs);
    }

    fn render_custom_mode(&self, area: Rect, buf: &mut Buffer) {
        let page = self.edit_page();
        let buttons = self.edit_buttons();
        let Some(layout) = page
            .framed()
            .render_with_standard_actions_end(area, buf, &buttons)
        else {
            return;
        };

        if layout.body.width == 0 || layout.body.height == 0 {
            return;
        }

        let field_outer = Rect::new(layout.body.x, layout.body.y, layout.body.width, 3);
        let field = BorderedField::new(
            "Shell command",
            matches!(self.edit_focus, EditFocus::Field),
        );
        field.render(field_outer, buf, &self.custom_field);

        let style_outer = Rect::new(
            layout.body.x,
            layout.body.y.saturating_add(3),
            layout.body.width,
            3,
        );
        let style_inner = BorderedField::new("Script style", false).render_block(style_outer, buf);
        let inferred = {
            let (path, _args) = split_command_and_args(self.custom_field.text());
            ShellScriptStyle::infer_from_shell_program(&path)
        };
        let (style_text, style_style) = match (self.custom_style_override, inferred) {
            (Some(style), _) => (
                format!("{style} (explicit)"),
                Style::new().fg(colors::primary()).bold(),
            ),
            (None, Some(style)) => (
                format!("auto (inferred: {style})"),
                Style::new().fg(colors::text_dim()),
            ),
            (None, None) => ("auto".to_string(), Style::new().fg(colors::text_dim())),
        };
        Paragraph::new(Line::from(Span::styled(style_text, style_style)))
            .render(style_inner, buf);
    }

    fn render_custom_mode_without_frame(&self, area: Rect, buf: &mut Buffer) {
        let page = self.edit_page();
        let buttons = self.edit_buttons();
        let Some(layout) = page
            .content_only()
            .render_with_standard_actions_end(area, buf, &buttons)
        else {
            return;
        };

        if layout.body.width == 0 || layout.body.height == 0 {
            return;
        }

        let field_outer = Rect::new(layout.body.x, layout.body.y, layout.body.width, 3);
        let field = BorderedField::new(
            "Shell command",
            matches!(self.edit_focus, EditFocus::Field),
        );
        field.render(field_outer, buf, &self.custom_field);

        let style_outer = Rect::new(
            layout.body.x,
            layout.body.y.saturating_add(3),
            layout.body.width,
            3,
        );
        let style_inner = BorderedField::new("Script style", false).render_block(style_outer, buf);
        let inferred = {
            let (path, _args) = split_command_and_args(self.custom_field.text());
            ShellScriptStyle::infer_from_shell_program(&path)
        };
        let (style_text, style_style) = match (self.custom_style_override, inferred) {
            (Some(style), _) => (
                format!("{style} (explicit)"),
                Style::new().fg(colors::primary()).bold(),
            ),
            (None, Some(style)) => (
                format!("auto (inferred: {style})"),
                Style::new().fg(colors::text_dim()),
            ),
            (None, None) => ("auto".to_string(), Style::new().fg(colors::text_dim())),
        };
        Paragraph::new(Line::from(Span::styled(style_text, style_style)))
            .render(style_inner, buf);
    }
}
