impl ChatWidget<'_> {
    /// Returns `true` if the trimmed input is a help trigger (`?` or `:?`),
    /// pushing a formatted help card into the chat history.
    pub(crate) fn try_handle_help_query(&mut self, trimmed: &str) -> bool {
        if trimmed != "?" && trimmed != ":?" {
            return false;
        }
        self.add_help_output();
        true
    }

    fn add_help_output(&mut self) {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::{Line, Span};

        let title_style = Style::new().fg(crate::colors::info()).add_modifier(Modifier::BOLD);
        let heading_style = Style::new()
            .fg(crate::colors::function())
            .add_modifier(Modifier::BOLD);
        let cmd_style = Style::new().fg(crate::colors::success());
        let desc_style = Style::new().fg(crate::colors::text());
        let dim_style = Style::new().fg(crate::colors::text_dim());
        let key_style = Style::new().fg(crate::colors::warning());

        let mut lines: Vec<Line<'static>> = Vec::with_capacity(48);

        lines.push(Line::from(Span::styled("Code — Quick Reference", title_style)));
        lines.push(Line::from(""));

        // ── Input shortcuts ──
        lines.push(Line::from(Span::styled("Input shortcuts", heading_style)));
        let input_shortcuts: &[(&str, &str)] = &[
            ("?  or  :?", "Show this help"),
            ("/command", "Run a slash command (type / to see list)"),
            ("$ command", "Run a shell command directly"),
            ("$$ prompt", "Ask the AI to write & run a shell command"),
        ];
        for (key, desc) in input_shortcuts {
            lines.push(Line::from(vec![
                Span::styled(format!("  {key:<14}"), cmd_style),
                Span::styled(format!(" {desc}"), desc_style),
            ]));
        }
        lines.push(Line::from(""));

        // ── Keyboard shortcuts ──
        lines.push(Line::from(Span::styled("Keyboard shortcuts", heading_style)));
        let kb_shortcuts: &[(&str, &str)] = &[
            ("Esc", "Cancel / close overlay / stop generation"),
            ("Ctrl+C", "Interrupt running command"),
            ("Ctrl+L", "Clear & redraw screen"),
            ("Ctrl+B", "Toggle browser overlay"),
            ("Ctrl+A", "Toggle agents terminal"),
            ("↑ / ↓", "Scroll chat history or command recall"),
            ("Tab", "Cycle focus / autocomplete"),
        ];
        for (key, desc) in kb_shortcuts {
            lines.push(Line::from(vec![
                Span::styled(format!("  {key:<14}"), key_style),
                Span::styled(format!(" {desc}"), desc_style),
            ]));
        }
        lines.push(Line::from(""));

        // ── Common slash commands ──
        lines.push(Line::from(Span::styled("Common commands", heading_style)));
        let common_cmds: &[(&str, &str)] = &[
            ("/new", "Start a new conversation"),
            ("/settings", "Open settings panel"),
            ("/model", "Choose default model"),
            ("/mode", "Set collaboration mode"),
            ("/status", "Show session info & token usage"),
            ("/diff", "Show git diff (including untracked)"),
            ("/undo", "Restore workspace to last snapshot"),
            ("/compact", "Summarize conversation (save context)"),
            ("/review", "Review changes for issues"),
            ("/auto", "Start Auto Drive for autonomous work"),
            ("/limits", "Adjust session rate limits"),
            ("/theme", "Customize the app theme"),
        ];
        for (cmd, desc) in common_cmds {
            lines.push(Line::from(vec![
                Span::styled(format!("  {cmd:<14}"), cmd_style),
                Span::styled(format!(" {desc}"), desc_style),
            ]));
        }
        lines.push(Line::from(""));

        // ── All slash commands (collapsed listing) ──
        lines.push(Line::from(Span::styled("All slash commands", heading_style)));
        let all_cmds = crate::slash_command::built_in_slash_commands();
        // Lay them out in a compact multi-column format
        let mut row_spans: Vec<Span<'static>> = Vec::new();
        row_spans.push(Span::styled("  ", dim_style));
        let mut col = 2usize;
        for (i, (name, _cmd)) in all_cmds.iter().enumerate() {
            let entry = format!("/{name}");
            let entry_len = entry.len() + 2; // +2 for spacing
            if col + entry_len > 78 && col > 2 {
                lines.push(Line::from(std::mem::take(&mut row_spans)));
                row_spans.push(Span::styled("  ", dim_style));
                col = 2;
            }
            row_spans.push(Span::styled(entry, cmd_style));
            if i < all_cmds.len() - 1 {
                row_spans.push(Span::styled("  ", dim_style));
                col += entry_len;
            }
        }
        if !row_spans.is_empty() {
            lines.push(Line::from(row_spans));
        }
        lines.push(Line::from(""));

        lines.push(Line::from(vec![
            Span::styled("Tip: ", Style::new().fg(Color::Yellow)),
            Span::styled(
                "Type / then start typing to filter commands in the autocomplete popup.",
                dim_style,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("     ", dim_style),
            Span::styled(
                "Hold Shift and click-drag to select text for copying.",
                dim_style,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("     ", dim_style),
            Span::styled(
                "Click and drag the scrollbar to navigate long conversations.",
                dim_style,
            ),
        ]));

        self.history_push_plain_state(crate::history_cell::plain_message_state_from_lines(
            lines,
            crate::history_cell::HistoryCellType::Notice,
        ));
        self.app_event_tx.send(AppEvent::RequestRedraw);
    }
}
