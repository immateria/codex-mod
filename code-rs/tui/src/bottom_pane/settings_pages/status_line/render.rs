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
            Span::styled("Tab", Style::default().fg(crate::colors::light_blue())),
            Span::styled(" lane  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("p", Style::default().fg(crate::colors::light_blue())),
            Span::styled(" primary  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Space", Style::default().fg(crate::colors::success())),
            Span::styled(" toggle  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("←/→", Style::default().fg(crate::colors::light_blue())),
            Span::styled(" reorder  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::styled(" apply  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Esc", Style::default().fg(crate::colors::error())),
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

        for (idx, choice) in self.choices_for_active_lane().iter().enumerate() {
            let selected = idx == self.selected_index_for_active_lane();
            let marker = if choice.enabled { "[x]" } else { "[ ]" };
            let pointer = if selected { "›" } else { " " };
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

        let paragraph = Paragraph::new(lines).style(
            Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text()),
        );
        paragraph.render(
            Rect {
                x: inner.x.saturating_add(1),
                y: inner.y,
                width: inner.width.saturating_sub(2),
                height: inner.height,
            },
            buf,
        );
    }
}

