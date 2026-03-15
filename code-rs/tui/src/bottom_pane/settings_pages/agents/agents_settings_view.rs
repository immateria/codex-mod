use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;

use crate::bottom_pane::{BottomPaneView, ConditionalUpdate};
use crate::bottom_pane::BottomPane;
use crate::components::form_text_field::FormTextField;
use crate::ui_interaction::redraw_if;

use crate::bottom_pane::settings_ui::action_page::SettingsActionPage;
use crate::bottom_pane::settings_ui::buttons::{
    standard_button_specs,
    SettingsButtonKind,
    StandardButtonSpec,
    TextButtonAlign,
};
use crate::bottom_pane::settings_ui::fields::BorderedField;
use crate::bottom_pane::settings_ui::hints::{status_and_shortcuts_split, title_line, KeyHint};
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::bottom_pane::settings_ui::toggle;
use crate::bottom_pane::settings_ui::wrap::wrap_spans;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Name,
    Mode,
    Agents,
    Instructions,
    Save,
    Delete,
    Cancel,
}

#[derive(Debug)]
pub struct SubagentEditorView {
    name_field: FormTextField,
    read_only: bool,
    selected_agent_indices: Vec<usize>,
    agent_cursor: usize,
    orch_field: FormTextField,
    available_agents: Vec<String>,
    is_new: bool,
    focus: Focus,
    is_complete: bool,
    app_event_tx: AppEventSender,
    confirm_delete: bool,
}

impl SubagentEditorView {
    fn panel_style() -> SettingsPanelStyle {
        SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0))
    }

    fn build_with(
        available_agents: Vec<String>,
        existing: Vec<code_core::config_types::SubagentCommandConfig>,
        app_event_tx: AppEventSender,
        name: &str,
    ) -> Self {
        let mut me = Self {
            name_field: FormTextField::new_single_line(),
            read_only: if name.is_empty() { false } else { code_core::slash_commands::default_read_only_for(name) },
            selected_agent_indices: Vec::new(),
            agent_cursor: 0,
            orch_field: FormTextField::new_multi_line(),
            available_agents,
            is_new: name.is_empty(),
            focus: Focus::Name,
            is_complete: false,
            app_event_tx,
            confirm_delete: false,
        };
        // Always seed the name field with the provided name
        if !name.is_empty() { me.name_field.set_text(name); }
        // Restrict ID field to [A-Za-z0-9_-]
        me.name_field.set_filter(crate::components::form_text_field::InputFilter::Id);
        // Seed from existing config if present
        if let Some(cfg) = existing.iter().find(|c| c.name.eq_ignore_ascii_case(name)) {
            me.name_field.set_text(&cfg.name);
            me.read_only = cfg.read_only;
            me.orch_field.set_text(&cfg.orchestrator_instructions.clone().unwrap_or_default());
            let set: std::collections::HashSet<String> = cfg.agents.iter().cloned().collect();
            for (idx, a) in me.available_agents.iter().enumerate() {
                if set.contains(a) { me.selected_agent_indices.push(idx); }
            }
        } else {
            // No user config yet; provide sensible defaults from core for built-ins
            if !name.is_empty() {
                me.read_only = code_core::slash_commands::default_read_only_for(name);
                if let Some(instr) = code_core::slash_commands::default_instructions_for(name) {
                    me.orch_field.set_text(&instr);
                    // Start cursor at the top so the first lines are visible.
                    me.orch_field.move_cursor_to_start();
                }
            }
            // Default selection: when no explicit config exists, preselect all available agents.
            if me.selected_agent_indices.is_empty() {
                me.selected_agent_indices = (0..me.available_agents.len()).collect();
            }
        }
        me
    }

    pub fn new_with_data(
        name: String,
        available_agents: Vec<String>,
        existing: Vec<code_core::config_types::SubagentCommandConfig>,
        is_new: bool,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut s = Self::build_with(available_agents, existing, app_event_tx, &name);
        s.is_new = is_new;
        s
    }

    fn toggle_agent_at(&mut self, idx: usize) {
        if let Some(pos) = self.selected_agent_indices.iter().position(|i| *i == idx) {
            self.selected_agent_indices.remove(pos);
        } else {
            self.selected_agent_indices.push(idx);
        }
    }

    fn save(&mut self) {
        let agents: Vec<String> = if self.selected_agent_indices.is_empty() {
            Vec::new()
        } else {
            self.selected_agent_indices.iter().filter_map(|i| self.available_agents.get(*i).cloned()).collect()
        };
        let cfg = code_core::config_types::SubagentCommandConfig {
            name: self.name_field.text().to_string(),
            read_only: self.read_only,
            agents,
            orchestrator_instructions: {
                let t = self.orch_field.text().trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            },
            agent_instructions: None,
        };
        // Persist to disk asynchronously to avoid blocking the TUI runtime
        if let Ok(home) = code_core::config::find_code_home() {
            let cfg_clone = cfg.clone();
            tokio::spawn(async move {
                let _ = code_core::config_edit::upsert_subagent_command(&home, &cfg_clone).await;
            });
        }
        // Update in-memory config
        self.app_event_tx.send(AppEvent::UpdateSubagentCommand(cfg));
    }

    fn show_delete(&self) -> bool {
        if self.is_new {
            return false;
        }
        let name = self.name_field.text();
        !name.trim().is_empty()
            && !["plan", "solve", "code"]
                .iter()
                .any(|reserved| name.eq_ignore_ascii_case(reserved))
    }

    fn focus_chain(&self) -> Vec<Focus> {
        let mut chain = vec![Focus::Name, Focus::Mode, Focus::Agents, Focus::Instructions];
        chain.extend(self.action_items().into_iter().map(|(id, _)| id));
        chain
    }

    fn focus_prev(&mut self) {
        let chain = self.focus_chain();
        let Some(idx) = chain.iter().position(|f| *f == self.focus) else {
            self.focus = Focus::Name;
            return;
        };
        if idx > 0 {
            self.focus = chain[idx - 1];
        }
    }

    fn focus_next(&mut self) {
        let chain = self.focus_chain();
        let Some(idx) = chain.iter().position(|f| *f == self.focus) else {
            self.focus = Focus::Name;
            return;
        };
        if idx + 1 < chain.len() {
            self.focus = chain[idx + 1];
        }
    }

    fn action_items(&self) -> Vec<(Focus, SettingsButtonKind)> {
        if self.confirm_delete {
            vec![
                (Focus::Delete, SettingsButtonKind::Delete),
                (Focus::Cancel, SettingsButtonKind::Cancel),
            ]
        } else if self.show_delete() {
            vec![
                (Focus::Save, SettingsButtonKind::Save),
                (Focus::Delete, SettingsButtonKind::Delete),
                (Focus::Cancel, SettingsButtonKind::Cancel),
            ]
        } else {
            vec![
                (Focus::Save, SettingsButtonKind::Save),
                (Focus::Cancel, SettingsButtonKind::Cancel),
            ]
        }
    }

    fn action_button_specs(&self) -> Vec<StandardButtonSpec<Focus>> {
        let focused = matches!(self.focus, Focus::Save | Focus::Delete | Focus::Cancel)
            .then_some(self.focus);
        standard_button_specs(&self.action_items(), focused, None)
    }

    fn move_action_left(&mut self) {
        let actions = self.action_items();
        let Some(idx) = actions.iter().position(|(id, _)| *id == self.focus) else {
            return;
        };
        if idx > 0 {
            self.focus = actions[idx - 1].0;
        }
    }

    fn move_action_right(&mut self) {
        let actions = self.action_items();
        let Some(idx) = actions.iter().position(|(id, _)| *id == self.focus) else {
            return;
        };
        if idx + 1 < actions.len() {
            self.focus = actions[idx + 1].0;
        }
    }

    fn enter_confirm_delete(&mut self) {
        self.confirm_delete = true;
        self.focus = Focus::Delete;
    }

    fn exit_confirm_delete(&mut self) {
        self.confirm_delete = false;
        if self.show_delete() {
            self.focus = Focus::Delete;
        } else {
            self.focus = Focus::Save;
        }
    }

    fn delete_current(&mut self) {
        let id = self.name_field.text().to_string();
        if id.trim().is_empty() {
            self.exit_confirm_delete();
            return;
        }

        if let Ok(home) = code_core::config::find_code_home() {
            let idc = id.clone();
            tokio::spawn(async move {
                let _ = code_core::config_edit::delete_subagent_command(&home, &idc).await;
            });
        }
        self.app_event_tx.send(AppEvent::DeleteSubagentCommand(id));
        self.is_complete = true;
        self.app_event_tx.send(AppEvent::ShowAgentsOverview);
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
            KeyHint::new("Esc", " back").with_key_style(Style::new().fg(colors::error())),
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
        let _ = BorderedField::new("ID", self.focus == Focus::Name)
            .render(id_rect, buf, &self.name_field);
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

    fn handle_key_event_internal(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                self.confirm_delete = false;
                self.app_event_tx.send(AppEvent::ShowAgentsOverview);
                true
            }
            KeyEvent { code: KeyCode::Tab, .. } => {
                self.focus_next();
                true
            }
            KeyEvent { code: KeyCode::BackTab, .. } => {
                self.focus_prev();
                true
            }
            KeyEvent { code: KeyCode::Up, modifiers, .. } => {
                if self.focus == Focus::Instructions {
                    let at_start = self.orch_field.cursor_is_at_start();
                    let _ = self
                        .orch_field
                        .handle_key(KeyEvent { code: KeyCode::Up, modifiers, ..key_event });
                    if at_start {
                        self.focus_prev();
                    }
                } else {
                    self.focus_prev();
                }
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers, .. } => {
                if self.focus == Focus::Instructions {
                    let at_end = self.orch_field.cursor_is_at_end();
                    let _ = self
                        .orch_field
                        .handle_key(KeyEvent { code: KeyCode::Down, modifiers, ..key_event });
                    if at_end {
                        self.focus_next();
                    }
                } else {
                    self.focus_next();
                }
                true
            }
            KeyEvent { code: KeyCode::Left, modifiers: KeyModifiers::NONE, .. }
                if matches!(self.focus, Focus::Save | Focus::Delete | Focus::Cancel) =>
            {
                self.move_action_left();
                true
            }
            KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE, .. }
                if matches!(self.focus, Focus::Save | Focus::Delete | Focus::Cancel) =>
            {
                self.move_action_right();
                true
            }
            KeyEvent { code: KeyCode::Left | KeyCode::Right | KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
                if self.focus == Focus::Mode =>
            {
                self.read_only = !self.read_only;
                true
            }
            KeyEvent { code: KeyCode::Left, modifiers: KeyModifiers::NONE, .. }
                if self.focus == Focus::Agents =>
            {
                if self.agent_cursor > 0 {
                    self.agent_cursor -= 1;
                }
                true
            }
            KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE, .. }
                if self.focus == Focus::Agents =>
            {
                if self.agent_cursor + 1 < self.available_agents.len() {
                    self.agent_cursor += 1;
                }
                true
            }
            KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. }
                if self.focus == Focus::Agents =>
            {
                let idx = self
                    .agent_cursor
                    .min(self.available_agents.len().saturating_sub(1));
                self.toggle_agent_at(idx);
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
                if self.focus == Focus::Agents =>
            {
                let idx = self
                    .agent_cursor
                    .min(self.available_agents.len().saturating_sub(1));
                self.toggle_agent_at(idx);
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
                if self.focus == Focus::Save && !self.confirm_delete =>
            {
                self.save();
                self.is_complete = true;
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
                if self.focus == Focus::Delete && !self.confirm_delete =>
            {
                self.enter_confirm_delete();
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
                if self.focus == Focus::Delete && self.confirm_delete =>
            {
                self.delete_current();
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
                if self.focus == Focus::Cancel =>
            {
                if self.confirm_delete {
                    self.exit_confirm_delete();
                } else {
                    self.is_complete = true;
                    self.app_event_tx.send(AppEvent::ShowAgentsOverview);
                }
                true
            }
            KeyEvent { code: KeyCode::Char('s'), modifiers, .. }
                if modifiers.contains(KeyModifiers::CONTROL) && !self.confirm_delete =>
            {
                self.save();
                self.is_complete = true;
                true
            }
            ev @ KeyEvent { .. } if self.focus == Focus::Name => {
                let _ = self.name_field.handle_key(ev);
                true
            }
            ev @ KeyEvent { .. } if self.focus == Focus::Instructions => {
                let _ = self.orch_field.handle_key(ev);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.handle_key_event_internal(key_event)
    }
}

impl<'a> BottomPaneView<'a> for SubagentEditorView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_internal(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_internal(key_event))
    }

    fn is_complete(&self) -> bool { self.is_complete }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        match self.focus {
            Focus::Name => self.name_field.handle_paste(text),
            Focus::Instructions => self.orch_field.handle_paste(text),
            _ => {}
        }
        ConditionalUpdate::NeedsRedraw
    }

    fn desired_height(&self, width: u16) -> u16 {
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

        let body_rows = id_box_h
            + gap_h
            + mode_box_h
            + gap_h
            + agent_box_h
            + gap_h
            + orch_box_h;
        let total_rows = header_rows + body_rows + status_rows + action_rows + footer_rows;
        total_rows.saturating_add(2).clamp(10, 50)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let page = self.page();
        let buttons = self.action_button_specs();
        let Some(layout) = page.framed().render_shell(area, buf) else {
            return;
        };

        self.render_body(layout.body, buf);
        page.render_standard_actions(&layout, buf, &buttons, TextButtonAlign::End);
    }
}
