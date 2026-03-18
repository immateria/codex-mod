use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use super::layout::AgentEditorLayout;
use super::model::{
    FIELD_COMMAND,
    FIELD_DESCRIPTION,
    FIELD_INSTRUCTIONS,
    FIELD_NAME,
    FIELD_READ_ONLY,
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

    pub(super) fn render_inner(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
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

        let layout = self.layout(content.width, Some(content.height));
        let AgentEditorLayout {
            lines,
            name_offset,
            command_offset,
            ro_offset,
            wr_offset,
            desc_offset,
            instr_offset,
            ro_height,
            wr_height,
            desc_height,
            instr_height,
            name_height,
            command_height,
        } = layout;

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(content, buf);

        // Draw name and command boxes
        let name_rect = Rect {
            x: content.x,
            y: content.y.saturating_add(name_offset),
            width: content.width,
            height: name_height,
        };
        let name_rect = name_rect.intersection(*buf.area());
        if name_rect.width > 0 && name_rect.height > 0 {
            let mut name_border = if self.field == FIELD_NAME {
                Style::default()
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::border())
            };
            if self.name_error.is_some() {
                name_border = name_border.fg(crate::colors::error());
            }
            let name_block = Block::default()
                .borders(Borders::ALL)
                .title(Line::from(" ID "))
                .border_style(name_border);
            let name_inner = name_block.inner(name_rect);
            let name_field_inner = name_inner.inner(Margin::new(1, 0));
            name_block.render(name_rect, buf);
            Self::clear_rect(buf, name_inner);
            self.name_field.render(
                name_field_inner,
                buf,
                self.field == FIELD_NAME && self.name_editable,
            );
        }

        let command_rect = Rect {
            x: content.x,
            y: content.y.saturating_add(command_offset),
            width: content.width,
            height: command_height,
        };
        let command_rect = command_rect.intersection(*buf.area());
        if command_rect.width > 0 && command_rect.height > 0 {
            let command_block = Block::default()
                .borders(Borders::ALL)
                .title(Line::from(" Command "))
                .border_style(if self.field == FIELD_COMMAND {
                    Style::default()
                        .fg(crate::colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(crate::colors::border())
                });
            let command_inner = command_block.inner(command_rect);
            let command_field_inner = command_inner.inner(Margin::new(1, 0));
            command_block.render(command_rect, buf);
            Self::clear_rect(buf, command_inner);
            self.command_field
                .render(command_field_inner, buf, self.field == FIELD_COMMAND);
        }

        // Draw input boxes at the same y offsets we reserved above
        let ro_rect = Rect {
            x: content.x,
            y: content.y.saturating_add(ro_offset),
            width: content.width,
            height: ro_height,
        };
        let ro_rect = ro_rect.intersection(*buf.area());
        let ro_block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(" Read-only Params "))
            .border_style(if self.field == FIELD_READ_ONLY {
                Style::default()
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::border())
            });
        if ro_rect.width > 0 && ro_rect.height > 0 {
            let ro_inner_rect = ro_block.inner(ro_rect);
            let ro_inner = ro_inner_rect.inner(Margin::new(1, 0));
            ro_block.render(ro_rect, buf);
            Self::clear_rect(buf, ro_inner_rect);
            self.params_ro
                .render(ro_inner, buf, self.field == FIELD_READ_ONLY);
        }

        // WR params box (3 rows)
        let wr_rect = Rect {
            x: content.x,
            y: content.y.saturating_add(wr_offset),
            width: content.width,
            height: wr_height,
        };
        let wr_rect = wr_rect.intersection(*buf.area());
        let wr_block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(" Write Params "))
            .border_style(if self.field == FIELD_WRITE {
                Style::default()
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::border())
            });
        if wr_rect.width > 0 && wr_rect.height > 0 {
            let wr_inner_rect = wr_block.inner(wr_rect);
            let wr_inner = wr_inner_rect.inner(Margin::new(1, 0));
            wr_block.render(wr_rect, buf);
            Self::clear_rect(buf, wr_inner_rect);
            self.params_wr.render(wr_inner, buf, self.field == FIELD_WRITE);
        }

        let desc_rect = Rect {
            x: content.x,
            y: content.y.saturating_add(desc_offset),
            width: content.width,
            height: desc_height,
        };
        let desc_rect = desc_rect.intersection(*buf.area());
        let mut desc_border_style = if self.field == FIELD_DESCRIPTION {
            Style::default()
                .fg(crate::colors::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(crate::colors::border())
        };
        if self.description_error.is_some() {
            desc_border_style = desc_border_style.fg(crate::colors::error());
        }
        let desc_block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(" What is this agent good at? "))
            .border_style(desc_border_style);
        if desc_rect.width > 0 && desc_rect.height > 0 {
            let desc_inner_rect = desc_block.inner(desc_rect);
            let desc_inner = desc_inner_rect.inner(Margin::new(1, 0));
            desc_block.render(desc_rect, buf);
            Self::clear_rect(buf, desc_inner_rect);
            self.description_field
                .render(desc_inner, buf, self.field == FIELD_DESCRIPTION);
        }

        // Instructions (multi-line; height consistent with reserved space above)
        let instr_rect = Rect {
            x: content.x,
            y: content.y.saturating_add(instr_offset),
            width: content.width,
            height: instr_height,
        };
        let instr_rect = instr_rect.intersection(*buf.area());
        let instr_block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(" Instructions "))
            .border_style(if self.field == FIELD_INSTRUCTIONS {
                Style::default()
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::border())
            });
        if instr_rect.width > 0 && instr_rect.height > 0 {
            let instr_inner_rect = instr_block.inner(instr_rect);
            let instr_inner = instr_inner_rect.inner(Margin::new(1, 0));
            instr_block.render(instr_rect, buf);
            Self::clear_rect(buf, instr_inner_rect);
            self.instr
                .render(instr_inner, buf, self.field == FIELD_INSTRUCTIONS);
        }
    }
}

