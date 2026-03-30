use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;

use crate::history_cell;

use super::ChatWidget;

impl ChatWidget<'_> {
    pub(crate) fn handle_secrets_command(&mut self) {
        let lines = vec![
            Line::from(Span::styled(
                "/secrets",
                Style::new().fg(crate::colors::keyword()),
            )),
            Line::from(""),
            Line::from("Store API keys and other secrets locally (encrypted at rest)."),
            Line::from(""),
            Line::from(Span::styled(
                "Set OpenAI API key:",
                Style::new().fg(crate::colors::text_dim()),
            )),
            Line::from("  code secrets set OPENAI_API_KEY"),
            Line::from(Span::styled(
                "Per-repo scope:",
                Style::new().fg(crate::colors::text_dim()),
            )),
            Line::from("  code secrets set --scope env OPENAI_API_KEY"),
        ];
        let state = history_cell::plain_message_state_from_lines(
            lines,
            history_cell::HistoryCellType::Notice,
        );
        self.history_push_plain_state(state);
    }
}

