use code_core::config_types::StatusLineLane;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use super::StatusLineSetupView;

impl StatusLineSetupView {
    pub(super) fn render_direct(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Status Line ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let active_lane = Self::lane_label(self.active_lane);
        let primary_lane = Self::lane_label(self.primary_lane);
        let top_preview = self.preview_text_for_lane(StatusLineLane::Top);
        let bottom_preview = self.preview_text_for_lane(StatusLineLane::Bottom);

        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled(crate::icons::tab(), Style::default().fg(crate::colors::light_blue())),
            Span::styled(" lane  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("p", Style::default().fg(crate::colors::light_blue())),
            Span::styled(" primary  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled(crate::icons::space(), Style::default().fg(crate::colors::success())),
            Span::styled(" toggle  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("←/→", Style::default().fg(crate::colors::light_blue())),
            Span::styled(" reorder  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled(crate::icons::enter(), Style::default().fg(crate::colors::success())),
            Span::styled(" apply  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled(crate::icons::escape(), Style::default().fg(crate::colors::error())),
            Span::styled(" cancel", Style::default().fg(crate::colors::text_dim())),
        ]));

        lines.push(Line::from(vec![
            Span::styled(
                "Editing lane: ",
                Style::default().fg(crate::colors::text_bright()),
            ),
            Span::styled(
                active_lane,
                Style::default()
                    .fg(crate::colors::light_blue())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled(
                "Primary lane: ",
                Style::default().fg(crate::colors::text_bright()),
            ),
            Span::styled(
                primary_lane,
                Style::default()
                    .fg(crate::colors::success())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Top preview: ", Style::default().fg(crate::colors::text_bright())),
            Span::styled(
                if top_preview.is_empty() {
                    "(none)".to_string()
                } else {
                    top_preview
                },
                Style::default().fg(crate::colors::text_dim()),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled(
                "Bottom preview: ",
                Style::default().fg(crate::colors::text_bright()),
            ),
            Span::styled(
                if bottom_preview.is_empty() {
                    "(none)".to_string()
                } else {
                    bottom_preview
                },
                Style::default().fg(crate::colors::text_dim()),
            ),
        ]));

        let header_lines = lines.len() as u16; // 5 header lines

        for (idx, choice) in self.choices_for_active_lane().iter().enumerate() {
            let selected = idx == self.selected_index_for_active_lane();
            let marker = if choice.enabled {
                crate::icons::checkbox_on()
            } else {
                crate::icons::checkbox_off()
            };
            let pointer = if selected { crate::icons::pointer_active() } else { " " };
            let mut line = Line::from(vec![
                Span::styled(pointer, Style::default().fg(crate::colors::light_blue())),
                Span::raw(" "),
                Span::styled(marker, Style::default().fg(crate::colors::success())),
                Span::raw(" "),
                Span::styled(choice.item.label(), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(
                    choice.item.description(),
                    Style::default().fg(crate::colors::text_dim()),
                ),
            ]);
            if selected {
                line = line.style(
                    Style::default()
                        .bg(crate::colors::selection())
                        .add_modifier(Modifier::BOLD),
                );
            }
            lines.push(line);
        }

        let content = Rect {
            x: inner.x.saturating_add(1),
            y: inner.y,
            width: inner.width.saturating_sub(2),
            height: inner.height,
        };

        // Ensure selected row is visible
        let total_lines = lines.len() as u16;
        let max_scroll = total_lines.saturating_sub(content.height);
        let mut scroll = self.scroll_offset.get().min(max_scroll);
        let selected_line = header_lines + self.selected_index_for_active_lane() as u16;
        if selected_line < scroll {
            scroll = selected_line.saturating_sub(1);
        }
        if selected_line >= scroll.saturating_add(content.height) {
            scroll = selected_line.saturating_sub(content.height).saturating_add(2);
        }
        self.scroll_offset.set(scroll.min(max_scroll));

        let paragraph = Paragraph::new(lines)
            .scroll((self.scroll_offset.get(), 0))
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            );
        paragraph.render(content, buf);

        // Scroll indicators
        if total_lines > content.height && content.width > 1 {
            let indicator_x = content.x.saturating_add(content.width);
            let s = self.scroll_offset.get();
            if s > 0 {
                buf.set_string(
                    indicator_x,
                    content.y,
                    crate::icons::arrow_up(),
                    Style::default().fg(crate::colors::light_blue()),
                );
            }
            if s < max_scroll {
                let bottom_y = content.y.saturating_add(content.height.saturating_sub(1));
                buf.set_string(
                    indicator_x,
                    bottom_y,
                    crate::icons::arrow_down(),
                    Style::default().fg(crate::colors::light_blue()),
                );
            }
        }
    }
}
