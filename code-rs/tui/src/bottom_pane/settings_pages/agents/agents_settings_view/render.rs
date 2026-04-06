use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::action_page::SettingsActionPage;
use crate::bottom_pane::settings_ui::buttons::{TextButtonAlign, StandardButtonSpec};
use crate::bottom_pane::settings_ui::fields::BorderedField;
use crate::bottom_pane::settings_ui::hints::{hint_esc, status_and_shortcuts_split, title_line, KeyHint};
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::bottom_pane::settings_ui::toggle;
use crate::bottom_pane::settings_ui::wrap::wrap_spans;
use crate::colors;

use super::model::{Focus, SubagentEditorView};

impl SubagentEditorView {
    fn panel_style() -> SettingsPanelStyle {
        SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0))
    }

    fn header_lines(&self) -> Vec<Line<'static>> {
        let title = if self.is_new {
            "New agent command".to_string()
        } else {
            let id = self.name_field.text();
            if id.trim().is_empty() {
                "Edit agent command".to_string()
            } else {
                format!("Edit agent command: {id}")
            }
        };
        vec![title_line(title)]
    }

    fn action_status_text(&self) -> Option<StyledText<'static>> {
        self.confirm_delete.then_some(StyledText::new(
            "Confirm delete: this removes the command from config.".to_string(),
            Style::new().fg(colors::error()).bold(),
        ))
    }

    fn page(&self) -> SettingsActionPage<'static> {
        let hints = [
            KeyHint::new("Tab", " next"),
            KeyHint::new("Shift+Tab", " prev"),
            KeyHint::new("Space", " toggle").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Ctrl+S", " save").with_key_style(Style::new().fg(colors::success())),
            hint_esc(" back"),
        ];
        let (status_lines, footer_lines) =
            status_and_shortcuts_split(self.action_status_text(), &hints);
        SettingsActionPage::new(
            "Configure Agent Command",
            Self::panel_style(),
            self.header_lines(),
            footer_lines,
        )
        .with_status_lines(status_lines)
        .with_wrap_lines(true)
    }

    fn agent_lines(&self, max_width: u16) -> Vec<Line<'static>> {
        let max_width = max_width.max(1) as usize;
        let mut spans = Vec::new();

        for (idx, agent) in self.available_agents.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::raw("  "));
            }

            let checked = if self.selected_agent_indices.contains(&idx) {
                "[x]"
            } else {
                "[ ]"
            };
            let mut style = if self.selected_agent_indices.contains(&idx) {
                Style::new().fg(colors::success()).bold()
            } else {
                Style::new().fg(colors::text_dim())
            };

            if self.focus == Focus::Agents && idx == self.agent_cursor {
                style = style.bg(colors::selection()).bold();
            }

            spans.push(Span::styled(format!("{checked} {agent}"), style));
        }

        wrap_spans(spans, max_width)
    }

    fn render_body(&self, body: Rect, buf: &mut Buffer) {
        if body.width == 0 || body.height == 0 {
            return;
        }

        let gap_h = 1u16;
        let id_box_h = 3u16;
        let mode_box_h = 3u16;
        let agent_inner_w = body.width.saturating_sub(4).max(1);
        let agent_inner_lines =
            u16::try_from(self.agent_lines(agent_inner_w).len()).unwrap_or(u16::MAX);
        let agent_box_h = agent_inner_lines.saturating_add(2).max(3);

        let orch_inner_w = body.width.saturating_sub(2).max(1);
        let desired_orch_inner = self.orch_field.desired_height(orch_inner_w).max(1);
        let orch_box_h = desired_orch_inner.min(8).saturating_add(2).max(3);

        let mut y = body.y;
        let mut remaining = body.height;

        let base_style = Style::new().bg(colors::background()).fg(colors::text());

        let id_h = id_box_h.min(remaining);
        let id_rect = Rect::new(body.x, y, body.width, id_h);
        let _ = BorderedField::new("ID", self.focus == Focus::Name).render(id_rect, buf, &self.name_field);
        y = y.saturating_add(id_h);
        remaining = remaining.saturating_sub(id_h);

        if remaining == 0 {
            return;
        }
        let spacer = gap_h.min(remaining);
        y = y.saturating_add(spacer);
        remaining = remaining.saturating_sub(spacer);

        if remaining == 0 {
            return;
        }
        let mode_h = mode_box_h.min(remaining);
        let mode_rect = Rect::new(body.x, y, body.width, mode_h);
        let mode_inner = BorderedField::new("Mode", self.focus == Focus::Mode)
            .render_block(mode_rect, buf)
            .inner(Margin::new(1, 0));
        let ro = toggle::checkbox_label(self.read_only, "read-only");
        let ro_style = if self.read_only { ro.style.bold() } else { ro.style };
        let wr = toggle::checkbox_label(!self.read_only, "write");
        let wr_style = if self.read_only { wr.style } else { wr.style.bold() };
        let mode_line = Line::from(vec![
            Span::styled(ro.text, ro_style),
            Span::raw("  "),
            Span::styled(wr.text, wr_style),
        ]);
        Paragraph::new(vec![mode_line])
            .style(base_style)
            .render(mode_inner, buf);
        y = y.saturating_add(mode_h);
        remaining = remaining.saturating_sub(mode_h);

        if remaining == 0 {
            return;
        }
        let spacer = gap_h.min(remaining);
        y = y.saturating_add(spacer);
        remaining = remaining.saturating_sub(spacer);

        if remaining == 0 {
            return;
        }
        let agents_h = agent_box_h.min(remaining);
        let agents_rect = Rect::new(body.x, y, body.width, agents_h);
        let agents_inner = BorderedField::new("Agents", self.focus == Focus::Agents)
            .render_block(agents_rect, buf)
            .inner(Margin::new(1, 0));
        let agent_lines = self.agent_lines(agents_inner.width);
        Paragraph::new(agent_lines).style(base_style).render(agents_inner, buf);
        y = y.saturating_add(agents_h);
        remaining = remaining.saturating_sub(agents_h);

        if remaining == 0 {
            return;
        }
        let spacer = gap_h.min(remaining);
        y = y.saturating_add(spacer);
        remaining = remaining.saturating_sub(spacer);

        if remaining == 0 {
            return;
        }
        let orch_h = orch_box_h.min(remaining);
        let orch_rect = Rect::new(body.x, y, body.width, orch_h);
        let _ = BorderedField::new("Instructions", self.focus == Focus::Instructions)
            .render(orch_rect, buf, &self.orch_field);
    }

    pub(super) fn desired_height_inner(&self, width: u16) -> u16 {
        let content_w = width
            .saturating_sub(2)
            .saturating_sub(Self::panel_style().content_margin.horizontal * 2)
            .max(10);

        let header_rows = u16::try_from(self.header_lines().len()).unwrap_or(u16::MAX);
        let status_rows = u16::from(self.confirm_delete);
        let footer_rows = 1u16;
        let action_rows = 1u16;

        let gap_h = 1u16;
        let id_box_h = 3u16;
        let mode_box_h = 3u16;
        let agent_inner_w = content_w.saturating_sub(4).max(1);
        let agent_inner_lines =
            u16::try_from(self.agent_lines(agent_inner_w).len()).unwrap_or(u16::MAX);
        let agent_box_h = agent_inner_lines.saturating_add(2).max(3);

        let orch_inner_w = content_w.saturating_sub(2).max(1);
        let desired_orch_inner = self.orch_field.desired_height(orch_inner_w).max(1);
        let orch_box_h = desired_orch_inner.min(8).saturating_add(2).max(3);

        let body_rows = id_box_h + gap_h + mode_box_h + gap_h + agent_box_h + gap_h + orch_box_h;
        let total_rows = header_rows + body_rows + status_rows + action_rows + footer_rows;
        total_rows.saturating_add(2).clamp(10, 50)
    }

    pub(super) fn render_inner(&self, area: Rect, buf: &mut Buffer) {
        let page = self.page();
        let buttons: Vec<StandardButtonSpec<Focus>> = self.action_button_specs();
        let Some(layout) = page.render_shell_in_chrome(ChromeMode::Framed, area, buf) else {
            return;
        };

        self.render_body(layout.body, buf);
        page.render_standard_actions(&layout, buf, &buttons, TextButtonAlign::End);
    }
}

