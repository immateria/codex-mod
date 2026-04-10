use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use super::layout::AgentEditorLayout;
use super::model::{
    FIELD_CANCEL,
    FIELD_COMMAND,
    FIELD_DESCRIPTION,
    FIELD_INSTRUCTIONS,
    FIELD_NAME,
    FIELD_READ_ONLY,
    FIELD_SAVE,
    FIELD_TOGGLE,
    FIELD_WRITE,
};
use super::AgentEditorView;

impl AgentEditorView {
    fn clear_rect(buf: &mut Buffer, rect: Rect) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let style = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        for y in rect.y..rect.y.saturating_add(rect.height) {
            for x in rect.x..rect.x.saturating_add(rect.width) {
                let cell = &mut buf[(x, y)];
                cell.set_symbol(" ");
                cell.set_style(style);
            }
        }
    }

    /// Compute the visible portion of a field box within the scrolled viewport.
    /// Returns `None` if the field is entirely outside the visible area.
    fn scrolled_rect(content: &Rect, offset: u16, height: u16, scroll: u16) -> Option<Rect> {
        let v_top = offset as i32 - scroll as i32;
        let v_bot = v_top + height as i32;
        let c_bot = content.height as i32;

        if v_bot <= 0 || v_top >= c_bot {
            return None;
        }

        let clipped_top = v_top.max(0) as u16;
        let clipped_bot = (v_bot.min(c_bot)) as u16;
        let h = clipped_bot.saturating_sub(clipped_top);
        if h == 0 {
            return None;
        }

        Some(Rect {
            x: content.x,
            y: content.y.saturating_add(clipped_top),
            width: content.width,
            height: h,
        })
    }

    /// Ensure the currently focused field is fully visible by adjusting
    /// `scroll_offset`.
    fn ensure_field_visible(&self, layout: &AgentEditorLayout, viewport_height: u16) {
        let (field_top, field_h) = match self.field {
            FIELD_NAME => (layout.name_offset, layout.name_height),
            FIELD_COMMAND => (layout.command_offset, layout.command_height),
            FIELD_TOGGLE => (layout.toggle_offset, 2),
            FIELD_READ_ONLY => (layout.ro_offset, layout.ro_height),
            FIELD_WRITE => (layout.wr_offset, layout.wr_height),
            FIELD_DESCRIPTION => (layout.desc_offset, layout.desc_height),
            FIELD_INSTRUCTIONS => (layout.instr_offset, layout.instr_height),
            FIELD_SAVE | FIELD_CANCEL => (layout.buttons_offset, 1),
            _ => return,
        };
        let field_bottom = field_top.saturating_add(field_h);
        let total = layout.lines.len() as u16;
        let max_scroll = total.saturating_sub(viewport_height);
        let mut scroll = self.scroll_offset.get().min(max_scroll);

        // If field is above viewport, scroll up
        if field_top < scroll {
            scroll = field_top.saturating_sub(1); // 1-line margin above
        }
        // If field is below viewport, scroll down
        if field_bottom > scroll.saturating_add(viewport_height) {
            scroll = field_bottom.saturating_sub(viewport_height).saturating_add(1);
        }

        self.scroll_offset.set(scroll.min(max_scroll));
    }

    pub(super) fn render_inner(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(crate::colors::style_border())
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Configure Agent ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let content = Rect {
            x: inner.x.saturating_add(1),
            y: inner.y,
            width: inner.width.saturating_sub(2),
            height: inner.height,
        };

        let layout = self.layout(content.width);
        let AgentEditorLayout {
            lines,
            name_offset,
            command_offset,
            toggle_offset: _,
            ro_offset,
            wr_offset,
            desc_offset,
            instr_offset,
            buttons_offset: _,
            ro_height,
            wr_height,
            desc_height,
            instr_height,
            name_height,
            command_height,
        } = &layout;

        // Adjust scroll to ensure focused field is visible
        self.ensure_field_visible(&layout, content.height);
        let scroll = self.scroll_offset.get();

        // Render the text lines with scroll offset
        Paragraph::new(lines.clone())
            .alignment(Alignment::Left)
            .scroll((scroll, 0))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(content, buf);

        // Render scroll indicator if content overflows
        let total_height = layout.lines.len() as u16;
        if total_height > content.height && content.width > 3 {
            let max_scroll = total_height.saturating_sub(content.height);
            let indicator_x = content.x.saturating_add(content.width);
            if scroll > 0 {
                buf.set_string(
                    indicator_x,
                    content.y,
                    crate::icons::arrow_up(),
                    crate::colors::style_light_blue(),
                );
            }
            if scroll < max_scroll {
                let bottom_y = content.y.saturating_add(content.height.saturating_sub(1));
                buf.set_string(
                    indicator_x,
                    bottom_y,
                    crate::icons::arrow_down(),
                    crate::colors::style_light_blue(),
                );
            }
        }

        // Helper: render a field box if it's within the scrolled viewport.
        // Returns the inner rect (with margin) for the field content, or None.
        let render_field_box = |buf: &mut Buffer,
                                offset: u16,
                                height: u16,
                                title: &str,
                                focused: bool|
         -> Option<Rect> {
            let rect = Self::scrolled_rect(&content, offset, height, scroll)?;
            let border_style = if focused {
                Style::default()
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                crate::colors::style_border()
            };
            let blk = Block::default()
                .borders(Borders::ALL)
                .title(Line::from(format!(" {title} ")))
                .border_style(border_style);
            let inner_rect = blk.inner(rect);
            let field_inner = inner_rect.inner(crate::ui_consts::HORIZONTAL_PAD);
            blk.render(rect, buf);
            Self::clear_rect(buf, inner_rect);
            Some(field_inner)
        };

        // Name box
        if let Some(field_inner) = render_field_box(
            buf, *name_offset, *name_height, "ID", self.field == FIELD_NAME,
        ) {
            let mut border_style = if self.field == FIELD_NAME {
                Style::default()
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                crate::colors::style_border()
            };
            if self.name_error.is_some() {
                border_style = border_style.fg(crate::colors::error());
                // Re-render with error border
                if let Some(rect) = Self::scrolled_rect(&content, *name_offset, *name_height, scroll) {
                    let blk = Block::default()
                        .borders(Borders::ALL)
                        .title(Line::from(" ID "))
                        .border_style(border_style);
                    blk.render(rect, buf);
                }
            }
            self.name_field.render(
                field_inner,
                buf,
                self.field == FIELD_NAME && self.name_editable,
            );
        }

        // Command box
        if let Some(field_inner) = render_field_box(
            buf, *command_offset, *command_height, "Command", self.field == FIELD_COMMAND,
        ) {
            self.command_field
                .render(field_inner, buf, self.field == FIELD_COMMAND);
        }

        // Read-only params
        if let Some(field_inner) = render_field_box(
            buf, *ro_offset, *ro_height, "Read-only Params", self.field == FIELD_READ_ONLY,
        ) {
            self.params_ro
                .render(field_inner, buf, self.field == FIELD_READ_ONLY);
        }

        // Write params
        if let Some(field_inner) = render_field_box(
            buf, *wr_offset, *wr_height, "Write Params", self.field == FIELD_WRITE,
        ) {
            self.params_wr
                .render(field_inner, buf, self.field == FIELD_WRITE);
        }

        // Description
        {
            let mut desc_border_style = if self.field == FIELD_DESCRIPTION {
                Style::default()
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                crate::colors::style_border()
            };
            if self.description_error.is_some() {
                desc_border_style = desc_border_style.fg(crate::colors::error());
            }
            if let Some(rect) = Self::scrolled_rect(&content, *desc_offset, *desc_height, scroll) {
                let blk = Block::default()
                    .borders(Borders::ALL)
                    .title(Line::from(" What is this agent good at? "))
                    .border_style(desc_border_style);
                let inner_rect = blk.inner(rect);
                let field_inner = inner_rect.inner(crate::ui_consts::HORIZONTAL_PAD);
                blk.render(rect, buf);
                Self::clear_rect(buf, inner_rect);
                self.description_field
                    .render(field_inner, buf, self.field == FIELD_DESCRIPTION);
            }
        }

        // Instructions
        if let Some(field_inner) = render_field_box(
            buf, *instr_offset, *instr_height, "Instructions", self.field == FIELD_INSTRUCTIONS,
        ) {
            self.instr
                .render(field_inner, buf, self.field == FIELD_INSTRUCTIONS);
        }
    }
}

