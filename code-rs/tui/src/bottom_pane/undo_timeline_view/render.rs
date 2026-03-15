use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use super::{UndoTimelineEntry, UndoTimelineView};

impl UndoTimelineView {
    fn render_list(&self, area: Rect, buf: &mut Buffer) {
        let (start, end) = self.visible_range();
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (idx, entry) in self.entries[start..end].iter().enumerate() {
            let absolute_idx = start + idx;
            let selected = absolute_idx == self.selected;

            let marker = if selected { "›" } else { " " };
            let mut title_spans = vec![
                Span::styled(
                    format!("{marker} "),
                    Style::default().fg(crate::colors::primary()),
                ),
                Span::styled(
                    entry.label.clone(),
                    if selected {
                        Style::default()
                            .fg(crate::colors::text())
                            .add_modifier(Modifier::BOLD)
                            .bg(crate::colors::selection())
                    } else {
                        Style::default().fg(crate::colors::text())
                    },
                ),
            ];
            if let Some(commit) = &entry.commit_line {
                title_spans.push(Span::raw(" "));
                title_spans.push(Span::styled(
                    commit.clone(),
                    if selected {
                        Style::default()
                            .fg(crate::colors::text_dim())
                            .bg(crate::colors::selection())
                    } else {
                        Style::default().fg(crate::colors::text_dim())
                    },
                ));
            }
            lines.push(Line::from(title_spans));

            if let Some(summary) = &entry.summary {
                let style = if selected {
                    Style::default()
                        .fg(crate::colors::text_dim())
                        .bg(crate::colors::selection())
                } else {
                    Style::default().fg(crate::colors::text_dim())
                };
                lines.push(Line::from(Span::styled(format!("  {summary}"), style)));
            }

            if entry.timestamp_line.is_some() || entry.relative_time.is_some() {
                let mut parts: Vec<String> = Vec::new();
                if let Some(ts) = &entry.timestamp_line {
                    parts.push(ts.clone());
                }
                if let Some(rel) = &entry.relative_time {
                    parts.push(rel.clone());
                }
                let style = if selected {
                    Style::default()
                        .fg(crate::colors::text_dim())
                        .bg(crate::colors::selection())
                } else {
                    Style::default().fg(crate::colors::text_dim())
                };
                lines.push(Line::from(Span::styled(
                    format!("  {}", parts.join(" • ")),
                    style,
                )));
            }

            if let Some(stats) = &entry.stats_line {
                let style = if selected {
                    Style::default()
                        .fg(crate::colors::text_dim())
                        .bg(crate::colors::selection())
                } else {
                    Style::default().fg(crate::colors::text_dim())
                };
                lines.push(Line::from(Span::styled(format!("  {stats}"), style)));
            }

            if selected {
                lines.push(Line::from(Span::styled(
                    String::new(),
                    Style::default().bg(crate::colors::selection()),
                )));
            } else {
                lines.push(Line::from(String::new()));
            }
        }

        let paragraph = Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .wrap(ratatui::widgets::Wrap { trim: false });
        paragraph.render(area, buf);
    }

    fn render_preview(&self, area: Rect, buf: &mut Buffer) {
        let Some(entry) = self.selected_entry() else {
            return;
        };

        let [conversation_area, files_area, footer_area] = Layout::vertical([
            Constraint::Percentage(55),
            Constraint::Percentage(35),
            Constraint::Length(3),
        ])
        .areas(area);

        let conversation_block = Block::default()
            .borders(Borders::ALL)
            .title(" Conversation preview ")
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));
        let conversation_inner = conversation_block.inner(conversation_area);
        conversation_block.render(conversation_area, buf);
        let conversation = Paragraph::new(entry.conversation_lines.clone())
            .wrap(ratatui::widgets::Wrap { trim: true })
            .style(Style::default().bg(crate::colors::background()))
            .alignment(Alignment::Left);
        conversation.render(conversation_inner, buf);

        let files_block = Block::default()
            .borders(Borders::ALL)
            .title(" File changes ")
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));
        let files_inner = files_block.inner(files_area);
        files_block.render(files_area, buf);
        let file_lines = if entry.file_lines.is_empty() {
            vec![Line::from(Span::styled(
                "No file changes captured for this snapshot.",
                Style::default().fg(crate::colors::text_dim()),
            ))]
        } else {
            entry.file_lines.clone()
        };
        let file_summary = Paragraph::new(file_lines)
            .wrap(ratatui::widgets::Wrap { trim: true })
            .style(Style::default().bg(crate::colors::background()))
            .alignment(Alignment::Left);
        file_summary.render(files_inner, buf);

        let footer_lines = self.footer_lines(entry);
        Paragraph::new(footer_lines)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .wrap(ratatui::widgets::Wrap { trim: true })
            .render(footer_area, buf);
    }

    fn footer_lines(&self, entry: &UndoTimelineEntry) -> Vec<Line<'static>> {
        let files_status = if entry.files_available {
            if self.restore_files {
                Span::styled("[x] Files", Style::default().fg(crate::colors::success()))
            } else {
                Span::styled("[ ] Files", Style::default().fg(crate::colors::text_dim()))
            }
        } else {
            Span::styled("[ ] Files", Style::default().fg(crate::colors::text_dim()))
        };

        let convo_status = if entry.conversation_available {
            if self.restore_conversation {
                Span::styled("[x] Conversation", Style::default().fg(crate::colors::success()))
            } else {
                Span::styled("[ ] Conversation", Style::default().fg(crate::colors::text_dim()))
            }
        } else {
            Span::styled("[ ] Conversation", Style::default().fg(crate::colors::text_dim()))
        };

        vec![
            Line::from(vec![files_status, Span::raw("  "), convo_status]),
            Line::from(vec![
                Span::styled("↑↓ PgUp PgDn", Style::default().fg(crate::colors::light_blue())),
                Span::raw(" Navigate  "),
                Span::styled("Space", Style::default().fg(crate::colors::success())),
                Span::raw(" Toggle files  "),
                Span::styled("C", Style::default().fg(crate::colors::success())),
                Span::raw(" Toggle conversation  "),
                Span::styled("Enter", Style::default().fg(crate::colors::success())),
                Span::raw(" Restore  "),
                Span::styled("Esc", Style::default().fg(crate::colors::error())),
                Span::raw(" Close"),
            ]),
        ]
    }

    pub(super) fn render_direct(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Restore workspace snapshot ")
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);
        let [list_area, preview_area] = Layout::horizontal([
            Constraint::Percentage(38),
            Constraint::Fill(1),
        ])
        .areas(inner);

        let list_block = Block::default()
            .borders(Borders::ALL)
            .title(" Snapshots ")
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));
        let list_inner = list_block.inner(list_area);
        list_block.render(list_area, buf);
        self.render_list(list_inner, buf);

        self.render_preview(preview_area, buf);
    }
}

