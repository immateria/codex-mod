use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::model::{FIELD_CANCEL, FIELD_SAVE, FIELD_TOGGLE};
use super::AgentEditorView;

#[derive(Debug)]
pub(super) struct AgentEditorLayout {
    pub(super) lines: Vec<Line<'static>>,
    pub(super) name_offset: u16,
    pub(super) command_offset: u16,
    pub(super) toggle_offset: u16,
    pub(super) ro_offset: u16,
    pub(super) wr_offset: u16,
    pub(super) desc_offset: u16,
    pub(super) instr_offset: u16,
    pub(super) buttons_offset: u16,
    pub(super) ro_height: u16,
    pub(super) wr_height: u16,
    pub(super) desc_height: u16,
    pub(super) instr_height: u16,
    pub(super) name_height: u16,
    pub(super) command_height: u16,
}

impl AgentEditorView {
    /// Compute the full form layout. The form is always laid out at its natural
    /// height; the viewport scroll offset handles overflow instead of squishing
    /// fields.
    pub(super) fn layout(&self, content_width: u16) -> AgentEditorLayout {
        let inner_width = content_width.saturating_sub(4);
        let instr_box_h = self.instr.desired_height(inner_width).min(8).saturating_add(2);
        let ro_box_h = self.params_ro.desired_height(inner_width).min(6).saturating_add(2);
        let wr_box_h = self.params_wr.desired_height(inner_width).min(6).saturating_add(2);
        let desc_box_h = self.description_field.desired_height(inner_width).min(6).saturating_add(2);

        let title_block: u16 = 2;
        let desc_style = Style::default().fg(crate::colors::text_dim());
        let name_box_h: u16 = 3;
        let command_box_h: u16 = 3;
        let enabled_block: u16 = 2;

        let sel = |idx: usize| {
            if self.field == idx {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            }
        };

        let name_offset = title_block;
        let command_offset = name_offset + name_box_h + 1;
        let toggle_offset = command_offset + command_box_h + 1;
        let ro_offset = toggle_offset + enabled_block;
        let wr_offset = ro_offset + ro_box_h + 1;
        let desc_offset = wr_offset + wr_box_h + 1;
        let desc_hint_lines: u16 = 2;
        let instr_offset = desc_offset + desc_box_h + desc_hint_lines;
        let instr_desc_lines: u16 = 1;
        let buttons_offset = instr_offset + instr_box_h + instr_desc_lines + 1;

        let mut lines: Vec<Line<'static>> = Vec::new();

        // Title
        lines.push(Line::from(Span::styled(
            format!("Agents » Edit Agent » {}", self.name),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        if !self.installed && !self.install_hint.is_empty() {
            lines.push(Line::from(Span::styled(
                "Command not found on PATH.",
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                self.install_hint.clone(),
                Style::default().fg(crate::colors::text_dim()),
            )));
            lines.push(Line::from(""));
        }

        // Name box
        for _ in 0..name_box_h {
            lines.push(Line::from(""));
        }
        if let Some(err) = &self.name_error {
            lines.push(Line::from(Span::styled(
                err.clone(),
                Style::default().fg(crate::colors::error()),
            )));
        } else {
            lines.push(Line::from(""));
        }
        // Command box
        for _ in 0..command_box_h {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(""));

        // Enabled toggle
        let enabled_style = if self.enabled {
            Style::default()
                .fg(crate::colors::success())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(crate::colors::text_dim())
        };
        let disabled_style = if self.enabled {
            Style::default().fg(crate::colors::text_dim())
        } else {
            Style::default()
                .fg(crate::colors::error())
                .add_modifier(Modifier::BOLD)
        };
        let label_style = if self.field == FIELD_TOGGLE {
            Style::default()
                .fg(crate::colors::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(crate::colors::text())
        };
        let enabled_marker = if self.enabled {
            crate::icons::checkbox_on()
        } else {
            crate::icons::checkbox_off()
        };
        let disabled_marker = if self.enabled {
            crate::icons::checkbox_off()
        } else {
            crate::icons::checkbox_on()
        };
        lines.push(Line::from(vec![
            Span::styled("Status:", label_style),
            Span::raw("  "),
            Span::styled(format!("{enabled_marker} Enabled"), enabled_style),
            Span::raw("  "),
            Span::styled(format!("{disabled_marker} Disabled"), disabled_style),
        ]));
        lines.push(Line::from(""));

        // Read-only params box
        for _ in 0..ro_box_h {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(""));

        // Write params box
        for _ in 0..wr_box_h {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(""));

        // Description box + helper text
        for _ in 0..desc_box_h {
            lines.push(Line::from(""));
        }
        let desc_message = if let Some(err) = &self.description_error {
            Line::from(Span::styled(
                err.clone(),
                Style::default().fg(crate::colors::error()),
            ))
        } else {
            Line::from(Span::styled(
                "Required: explain what this agent is good at so Code can pick it intelligently.",
                desc_style,
            ))
        };
        lines.push(desc_message);
        lines.push(Line::from(""));

        // Instructions box
        for _ in 0..instr_box_h {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            "Optional guidance prepended to every request sent to the agent.",
            desc_style,
        )));
        lines.push(Line::from(""));

        // Buttons row
        let save_style = sel(FIELD_SAVE).fg(crate::colors::success());
        let cancel_style = sel(FIELD_CANCEL).fg(crate::colors::text());
        lines.push(Line::from(vec![
            Span::styled("[ Save ]", save_style),
            Span::raw("  "),
            Span::styled("[ Cancel ]", cancel_style),
        ]));

        while lines
            .last()
            .map(|line| line.spans.iter().all(|s| s.content.trim().is_empty()))
            .unwrap_or(false)
        {
            lines.pop();
        }

        AgentEditorLayout {
            lines,
            name_offset,
            command_offset,
            toggle_offset,
            ro_offset,
            wr_offset,
            desc_offset,
            instr_offset,
            buttons_offset,
            ro_height: ro_box_h,
            wr_height: wr_box_h,
            desc_height: desc_box_h,
            instr_height: instr_box_h,
            name_height: name_box_h,
            command_height: command_box_h,
        }
    }
}

