use code_core::config_types::StatusLineLane;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget};

use super::StatusLineSetupView;

impl StatusLineSetupView {
    pub(super) fn render_direct(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        Clear.render(area, buf);
        let block = crate::components::popup_frame::themed_block()
            .title(" Status Line ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let active_lane = Self::lane_label(self.active_lane);
        let primary_lane = Self::lane_label(self.primary_lane);
        let top_preview = self.preview_text_for_lane(StatusLineLane::Top);
        let bottom_preview = self.preview_text_for_lane(StatusLineLane::Bottom);

        let mut lines = Vec::new();
        lines.push({
            use crate::bottom_pane::settings_ui::hints::{hint_enter, hint_esc, hint_nav_horizontal, shortcut_line, KeyHint};
            shortcut_line(&[
                KeyHint::new(crate::icons::tab(), " lane")
                    .with_key_style(crate::colors::style_function()),
                KeyHint::new("p", " primary")
                    .with_key_style(crate::colors::style_function()),
                KeyHint::new(crate::icons::space(), " toggle")
                    .with_key_style(crate::colors::style_success()),
                hint_nav_horizontal(" reorder"),
                hint_enter(" apply"),
                hint_esc(" cancel"),
            ])
        });

        lines.push(Line::from(vec![
            Span::styled(
                "Editing lane: ",
                crate::colors::style_text_bright(),
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
                crate::colors::style_text_bright(),
            ),
            Span::styled(
                primary_lane,
                Style::default()
                    .fg(crate::colors::success())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Top preview: ", crate::colors::style_text_bright()),
            Span::styled(
                if top_preview.is_empty() {
                    "(none)".to_string()
                } else {
                    top_preview
                },
                crate::colors::style_text_dim(),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled(
                "Bottom preview: ",
                crate::colors::style_text_bright(),
            ),
            Span::styled(
                if bottom_preview.is_empty() {
                    "(none)".to_string()
                } else {
                    bottom_preview
                },
                crate::colors::style_text_dim(),
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
                Span::styled(pointer, crate::colors::style_light_blue()),
                Span::raw(" "),
                Span::styled(marker, crate::colors::style_success()),
                Span::raw(" "),
                Span::styled(choice.item.label(), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(
                    choice.item.description(),
                    crate::colors::style_text_dim(),
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
                    crate::colors::style_light_blue(),
                );
            }
            if s < max_scroll {
                let bottom_y = content.y.saturating_add(content.height.saturating_sub(1));
                buf.set_string(
                    indicator_x,
                    bottom_y,
                    crate::icons::arrow_down(),
                    crate::colors::style_light_blue(),
                );
            }
        }
    }
}
