impl HistoryCell for AgentRunCell {
    impl_as_any!();

    fn gutter_symbol(&self) -> Option<&'static str> {
        None
    }

    fn kind(&self) -> HistoryCellType {
        let status = if self.completed {
            if self.status_label == "Failed" {
                ToolCellStatus::Failed
            } else {
                ToolCellStatus::Success
            }
        } else {
            ToolCellStatus::Running
        };
        HistoryCellType::Tool { status }
    }

    fn call_id(&self) -> Option<&str> {
        self.cell_key.as_deref()
    }

    fn parent_call_id(&self) -> Option<&str> {
        self.parent_call_id.as_deref()
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        self
            .build_plain_summary()
            .into_iter()
            .map(Line::from)
            .collect()
    }

    fn desired_height(&self, width: u16) -> u16 {
        let style = agent_card_style(self.write_enabled);
        let trimmed_width = width.saturating_sub(2);
        if trimmed_width == 0 {
            return 0;
        }
        let rows = self.build_card_rows(trimmed_width, &style);
        rows.len().max(1) as u16
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        if area.width <= 2 || area.height == 0 {
            return;
        }

        let style = agent_card_style(self.write_enabled);
        let draw_width = area.width - 2;
        let render_area = Rect {
            width: draw_width,
            ..area
        };

        fill_card_background(buf, render_area, &style);
        let rows = self.build_card_rows(render_area.width, &style);
        let lines = rows_to_lines(&rows, &style, render_area.width);
        let text = Text::from(lines);

        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((skip_rows, 0))
            .render(render_area, buf);

        let clear_start = area.x + draw_width;
        let clear_end = area.x + area.width;
        for x in clear_start..clear_end {
            for row in 0..area.height {
                let cell = &mut buf[(x, area.y + row)];
                cell.set_symbol(" ");
                cell.set_bg(crate::colors::background());
            }
        }
    }
}

