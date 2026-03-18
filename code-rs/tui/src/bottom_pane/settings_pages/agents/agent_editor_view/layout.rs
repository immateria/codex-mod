use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::model::{FIELD_CANCEL, FIELD_SAVE, FIELD_TOGGLE};
use super::AgentEditorView;

#[derive(Debug)]
pub(super) struct AgentEditorLayout {
    pub(super) lines: Vec<Line<'static>>,
    pub(super) name_offset: u16,
    pub(super) command_offset: u16,
    pub(super) ro_offset: u16,
    pub(super) wr_offset: u16,
    pub(super) desc_offset: u16,
    pub(super) instr_offset: u16,
    pub(super) ro_height: u16,
    pub(super) wr_height: u16,
    pub(super) desc_height: u16,
    pub(super) instr_height: u16,
    pub(super) name_height: u16,
    pub(super) command_height: u16,
}

impl AgentEditorView {
    pub(super) fn layout(
        &self,
        content_width: u16,
        max_height: Option<u16>,
    ) -> AgentEditorLayout {
        let inner_width = content_width.saturating_sub(4);
        let desired_instr_inner = self.instr.desired_height(inner_width).min(8);
        let mut instr_box_h = desired_instr_inner.saturating_add(2);

        let desired_ro_inner = self.params_ro.desired_height(inner_width).min(6);
        let ro_box_h = desired_ro_inner.saturating_add(2);
        let desired_wr_inner = self.params_wr.desired_height(inner_width).min(6);
        let wr_box_h = desired_wr_inner.saturating_add(2);
        let desired_desc_inner = self.description_field.desired_height(inner_width).min(6);
        let desc_box_h = desired_desc_inner.saturating_add(2);

        let title_block: u16 = 2; // title + blank
        let desc_style = Style::default().fg(crate::colors::text_dim());
        let name_box_h: u16 = 3;
        let command_box_h: u16 = 3;
        let top_block = title_block;
        let enabled_block: u16 = 2; // toggle row + spacer
        let desc_hint_lines: u16 = 2; // guidance line + spacer
        let instr_desc_lines: u16 = 1;
        let spacer_before_buttons: u16 = 1;
        let buttons_block: u16 = 1;
        let footer_lines_default: u16 = 0;

        let base_fixed_top = top_block
            + name_box_h
            + 1
            + command_box_h
            + 1
            + enabled_block
            + ro_box_h
            + 1
            + wr_box_h
            + 1
            + desc_box_h
            + desc_hint_lines;

        let mut footer_lines = footer_lines_default;
        let mut include_gap_before_buttons = spacer_before_buttons > 0;

        if let Some(height) = max_height {
            let mut fixed_after_box =
                instr_desc_lines + spacer_before_buttons + buttons_block + footer_lines;
            if base_fixed_top
                .saturating_add(instr_box_h)
                .saturating_add(fixed_after_box)
                > height
            {
                footer_lines = 0;
            }
            fixed_after_box = instr_desc_lines + spacer_before_buttons + buttons_block + footer_lines;
            if base_fixed_top
                .saturating_add(instr_box_h)
                .saturating_add(fixed_after_box)
                > height
            {
                let min_ih: u16 = 3;
                let available_for_box = height
                    .saturating_sub(base_fixed_top)
                    .saturating_sub(fixed_after_box);
                instr_box_h = instr_box_h.min(available_for_box).max(min_ih);
            }
            fixed_after_box = instr_desc_lines + spacer_before_buttons + buttons_block + footer_lines;
            if base_fixed_top
                .saturating_add(instr_box_h)
                .saturating_add(fixed_after_box)
                > height
            {
                include_gap_before_buttons = false;
            }
        }

        let sel = |idx: usize| {
            if self.field == idx {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            }
        };

        let name_offset = top_block;
        let command_offset = name_offset + name_box_h + 1;
        let toggle_offset = command_offset + command_box_h + 1;
        let ro_offset = toggle_offset + enabled_block;
        let wr_offset = ro_offset + ro_box_h + 1;
        let desc_offset = wr_offset + wr_box_h + 1;
        let instr_offset = desc_offset + desc_box_h + desc_hint_lines;
        let mut lines: Vec<Line<'static>> = Vec::new();

        // Title, spacer
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

        // Reserve space for Name box
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
        // Reserve space for Command box
        for _ in 0..command_box_h {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(""));

        // Enabled toggle + spacer
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
        let enabled_text = format!("[{}] Enabled", if self.enabled { 'x' } else { ' ' });
        let disabled_text = format!("[{}] Disabled", if self.enabled { ' ' } else { 'x' });
        lines.push(Line::from(vec![
            Span::styled("Status:", label_style),
            Span::raw("  "),
            Span::styled(enabled_text, enabled_style),
            Span::raw("  "),
            Span::styled(disabled_text, disabled_style),
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
        if include_gap_before_buttons {
            lines.push(Line::from(""));
        }
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
            ro_offset,
            wr_offset,
            desc_offset,
            instr_offset,
            ro_height: ro_box_h,
            wr_height: wr_box_h,
            desc_height: desc_box_h,
            instr_height: instr_box_h,
            name_height: name_box_h,
            command_height: command_box_h,
        }
    }
}

