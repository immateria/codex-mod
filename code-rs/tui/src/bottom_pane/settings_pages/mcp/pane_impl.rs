use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block,
    Borders,
    Clear,
    Paragraph,
    Widget,
    Wrap,
};

use crate::ui_interaction::{inset_rect_right, redraw_if, render_vertical_scrollbar};

use crate::bottom_pane::{BottomPaneView, ConditionalUpdate};
use crate::bottom_pane::BottomPane;
use crate::bottom_pane::ChromeMode;
use super::layout::{McpPaneHit, McpViewLayout};
use super::{McpSettingsFocus, McpSettingsMode, McpSettingsView};

const MCP_SETTINGS_DESIRED_HEIGHT: u16 = 16;

impl McpSettingsView {
    fn pane_border_style(&self, focused_pane: McpSettingsFocus, pane_hit: McpPaneHit) -> Style {
        if self.focus == focused_pane {
            Style::default().fg(crate::colors::primary())
        } else if self.hovered_pane == pane_hit {
            Style::default().fg(crate::colors::function())
        } else {
            Style::default().fg(crate::colors::border())
        }
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        self.last_render.set(area, ChromeMode::Framed);

        let Some(layout) = McpViewLayout::from_area_with_scroll(area, self.stacked_scroll_top) else {
            return;
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            )
            .title(" MCP Servers ")
            .title_alignment(Alignment::Center);
        block.render(area, buf);

        self.render_main_panes(layout, buf);
        if let Some(hint_area) = layout.hint_area {
            self.render_hints(hint_area, buf);
        }

        if !matches!(self.mode, McpSettingsMode::Main) {
            self.render_policy_editor_framed(area, buf);
        }
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        Clear.render(area, buf);
        self.last_render.set(area, ChromeMode::ContentOnly);
        crate::util::buffer::fill_rect(
            buf,
            area,
            Some(' '),
            Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text()),
        );

        let Some(layout) =
            McpViewLayout::from_content_area_with_scroll(area, self.stacked_scroll_top)
        else {
            return;
        };

        self.render_main_panes(layout, buf);
        if let Some(hint_area) = layout.hint_area {
            self.render_hints(hint_area, buf);
        }

        if !matches!(self.mode, McpSettingsMode::Main) {
            self.render_policy_editor_content_only(area, buf);
        }
    }

    fn render_main_panes(&self, layout: McpViewLayout, buf: &mut Buffer) {
        let list_block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.pane_border_style(McpSettingsFocus::Servers, McpPaneHit::Servers))
            .title(if self.focus == McpSettingsFocus::Servers {
                " Servers (Focused) "
            } else {
                " Servers "
            });
        list_block.render(layout.list_rect, buf);

        let list_lines = self.list_lines(layout.list_inner.width as usize);
        let list_scroll_top = self.list_scroll_top(layout.list_inner.height);
        Paragraph::new(list_lines)
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            )
            .scroll((list_scroll_top as u16, 0))
            .render(layout.list_inner, buf);

        let summary_block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.pane_border_style(McpSettingsFocus::Summary, McpPaneHit::Summary))
            .title(if self.focus == McpSettingsFocus::Summary {
                " Details (Focused) "
            } else {
                " Details "
            });
        summary_block.render(layout.summary_rect, buf);

        let summary_lines = self.summary_lines();
        let summary_metrics = self.summary_metrics_for_viewport(layout.summary_inner);
        let mut summary_paragraph = Paragraph::new(summary_lines).style(
            Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text()),
        );
        if self.summary_wrap {
            summary_paragraph = summary_paragraph.wrap(Wrap { trim: false });
        }
        let summary_total = summary_metrics.total_lines;
        let summary_scroll_top =
            if summary_metrics.visible_lines == 0 || summary_total <= summary_metrics.visible_lines {
                0
            } else {
                self.summary_scroll_top
                    .min(summary_total.saturating_sub(summary_metrics.visible_lines))
            };

        let summary_hscroll = if self.summary_wrap {
            0usize
        } else {
            let viewport = layout.summary_inner.width as usize;
            let max_hscroll = summary_metrics.max_width.saturating_sub(viewport);
            self.summary_hscroll.min(max_hscroll)
        };

        summary_paragraph
            .scroll((summary_scroll_top as u16, summary_hscroll as u16))
            .render(layout.summary_inner, buf);

        if summary_total > summary_metrics.visible_lines {
            let max_scroll = summary_total.saturating_sub(summary_metrics.visible_lines);
            render_vertical_scrollbar(
                buf,
                layout.summary_scrollbar_area(),
                summary_scroll_top,
                max_scroll,
                summary_metrics.visible_lines,
            );
        }

        let tools_block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.pane_border_style(McpSettingsFocus::Tools, McpPaneHit::Tools))
            .title(if self.focus == McpSettingsFocus::Tools {
                " Tools (*=override) (Focused) "
            } else {
                " Tools (*=override) "
            });
        tools_block.render(layout.tools_rect, buf);

        let tool_entries = self.tool_entries();
        let tool_lines =
            self.tools_lines_for_entries(layout.tools_inner.width as usize, &tool_entries);
        let tools_scroll_top = self.tools_scroll_top_for_entries_len(
            layout.tools_inner.height,
            tool_entries.len(),
        );
        Paragraph::new(tool_lines)
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            )
            .scroll((tools_scroll_top as u16, 0))
            .render(layout.tools_inner, buf);

        if tool_entries.len() > layout.tools_inner.height as usize {
            let viewport_len = layout.tools_inner.height as usize;
            let max_scroll = tool_entries.len().saturating_sub(viewport_len);
            render_vertical_scrollbar(
                buf,
                layout.tools_scrollbar_area(),
                tools_scroll_top,
                max_scroll,
                viewport_len,
            );
        }

        if layout.stacked && layout.stack_max_scroll > 0 {
            let viewport_len = layout.stack_viewport.height as usize;
            render_vertical_scrollbar(
                buf,
                inset_rect_right(layout.stack_viewport, 1),
                layout.stack_scroll_top,
                layout.stack_max_scroll,
                viewport_len,
            );
        }
    }

    fn render_hints(&self, hint_area: Rect, buf: &mut Buffer) {
        match self.mode {
            McpSettingsMode::Main => Paragraph::new(Line::from(vec![
                Span::styled("↑↓", Style::default().fg(crate::colors::function())),
                Span::styled(" move  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Space", Style::default().fg(crate::colors::success())),
                Span::styled(" toggle tool  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Enter", Style::default().fg(crate::colors::success())),
                Span::styled(" expand tool  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Tab", Style::default().fg(crate::colors::function())),
                Span::styled(" /Click", Style::default().fg(crate::colors::function())),
                Span::styled(" focus pane  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("E", Style::default().fg(crate::colors::function())),
                Span::styled(
                    " edit scheduling  ",
                    Style::default().fg(crate::colors::text_dim()),
                ),
                Span::styled("W", Style::default().fg(crate::colors::function())),
                Span::styled(" wrap  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Esc", Style::default().fg(crate::colors::error())),
                Span::styled(" close", Style::default().fg(crate::colors::text_dim())),
            ]))
            .render(hint_area, buf),
            McpSettingsMode::EditServerScheduling(_) | McpSettingsMode::EditToolScheduling(_) => {
                Paragraph::new(Line::from(vec![
                    Span::styled("↑↓", Style::default().fg(crate::colors::function())),
                    Span::styled(" move  ", Style::default().fg(crate::colors::text_dim())),
                    Span::styled("Enter", Style::default().fg(crate::colors::success())),
                    Span::styled(
                        " edit/toggle  ",
                        Style::default().fg(crate::colors::text_dim()),
                    ),
                    Span::styled("Ctrl+S", Style::default().fg(crate::colors::function())),
                    Span::styled(" save  ", Style::default().fg(crate::colors::text_dim())),
                    Span::styled("Esc", Style::default().fg(crate::colors::error())),
                    Span::styled(" cancel", Style::default().fg(crate::colors::text_dim())),
                ]))
                .render(hint_area, buf)
            }
        };
    }
}

impl<'a> BottomPaneView<'a> for McpSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.process_key_event(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.process_key_event(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(
            self.framed_mut()
                .handle_mouse_event_direct(mouse_event, area),
        )
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        // Keep MCP settings stable; the view can scroll in stacked mode when content grows.
        MCP_SETTINGS_DESIRED_HEIGHT
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.framed().render(area, buf);
    }
}
