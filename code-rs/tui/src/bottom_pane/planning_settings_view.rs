use code_core::config_types::ReasoningEffort;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crate::ui_interaction::redraw_if;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::hints::{shortcut_line, KeyHint};
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::menu_rows::{
    selection_id_at as selection_menu_id_at,
    render_menu_rows,
    SettingsMenuRow,
};
use crate::components::scroll_state::ScrollState;
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::rows::StyledText;
use super::BottomPane;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PlanningRow {
    CustomModel,
}

pub(crate) struct PlanningSettingsView {
    use_chat_model: bool,
    planning_model: String,
    planning_reasoning: ReasoningEffort,
    app_event_tx: AppEventSender,
    state: ScrollState,
    is_complete: bool,
}

pub(crate) struct PlanningSettingsViewFramed<'v> {
    view: &'v PlanningSettingsView,
}

pub(crate) struct PlanningSettingsViewContentOnly<'v> {
    view: &'v PlanningSettingsView,
}

pub(crate) struct PlanningSettingsViewFramedMut<'v> {
    view: &'v mut PlanningSettingsView,
}

pub(crate) struct PlanningSettingsViewContentOnlyMut<'v> {
    view: &'v mut PlanningSettingsView,
}

impl PlanningSettingsView {
    fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Planning Settings",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            vec![
                Line::from(Span::styled(
                    "Select the model used when you’re in Plan Mode (Read Only).",
                    Style::new().fg(colors::text_dim()),
                )),
                shortcut_line(&[
                    KeyHint::new("↑↓", " navigate")
                        .with_key_style(Style::new().fg(colors::function())),
                    KeyHint::new("Enter/Space", " toggle/open")
                        .with_key_style(Style::new().fg(colors::function())),
                    KeyHint::new("Esc", " close")
                        .with_key_style(Style::new().fg(colors::function())),
                ]),
                Line::from(""),
            ],
            vec![],
        )
    }

    pub fn new(
        use_chat_model: bool,
        planning_model: String,
        planning_reasoning: ReasoningEffort,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        Self {
            use_chat_model,
            planning_model,
            planning_reasoning,
            app_event_tx,
            state,
            is_complete: false,
        }
    }

    pub fn set_planning_model(&mut self, model: String, effort: ReasoningEffort) {
        self.planning_model = model;
        self.planning_reasoning = effort;
    }

    pub fn set_use_chat_model(&mut self, use_chat: bool) {
        self.use_chat_model = use_chat;
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.handle_key(key_event)
    }

    pub(crate) fn framed(&self) -> PlanningSettingsViewFramed<'_> {
        PlanningSettingsViewFramed { view: self }
    }

    pub(crate) fn content_only(&self) -> PlanningSettingsViewContentOnly<'_> {
        PlanningSettingsViewContentOnly { view: self }
    }

    pub(crate) fn framed_mut(&mut self) -> PlanningSettingsViewFramedMut<'_> {
        PlanningSettingsViewFramedMut { view: self }
    }

    pub(crate) fn content_only_mut(&mut self) -> PlanningSettingsViewContentOnlyMut<'_> {
        PlanningSettingsViewContentOnlyMut { view: self }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn rows(&self) -> Vec<PlanningRow> {
        vec![PlanningRow::CustomModel]
    }

    fn handle_enter(&mut self, row: PlanningRow) {
        match row {
            PlanningRow::CustomModel => {
                self.app_event_tx.send(AppEvent::ShowPlanningModelSelector);
            }
        }
    }

    fn menu_rows(&self) -> Vec<SettingsMenuRow<'static, PlanningRow>> {
        let value_text = if self.use_chat_model {
            "Follow Chat Mode".to_string()
        } else {
            format!(
                "{} ({})",
                Self::format_model_label(&self.planning_model),
                Self::reasoning_label(self.planning_reasoning)
            )
        };
        vec![
            SettingsMenuRow::new(PlanningRow::CustomModel, "Planning model")
                .with_value(StyledText::new(value_text, Style::new().fg(colors::function())))
                .with_selected_hint("Enter to change"),
        ]
    }

    fn selected_row(&self) -> Option<PlanningRow> {
        self.rows().get(self.state.selected_idx.unwrap_or(0)).copied()
    }

    fn row_at_position(&self, body: Rect, x: u16, y: u16) -> Option<PlanningRow> {
        let rows = self.menu_rows();
        selection_menu_id_at(body, x, y, 0, &rows)
    }

    fn render_rows(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.menu_rows();
        render_menu_rows(
            area,
            buf,
            0,
            self.selected_row(),
            &rows,
            Style::new().bg(colors::background()).fg(colors::text()),
        );
    }

    fn reasoning_label(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::XHigh => "XHigh",
            ReasoningEffort::High => "High",
            ReasoningEffort::Medium => "Medium",
            ReasoningEffort::Low => "Low",
            ReasoningEffort::Minimal => "Minimal",
            ReasoningEffort::None => "None",
        }
    }

    fn format_model_label(model: &str) -> String {
        let mut parts = Vec::new();
        for (idx, part) in model.split('-').enumerate() {
            if idx == 0 {
                parts.push(part.to_ascii_uppercase());
                continue;
            }
            let mut chars = part.chars();
            let formatted = match chars.next() {
                Some(first) if first.is_ascii_alphabetic() => {
                    let mut s = String::new();
                    s.push(first.to_ascii_uppercase());
                    s.push_str(chars.as_str());
                    s
                }
                Some(first) => {
                    let mut s = String::new();
                    s.push(first);
                    s.push_str(chars.as_str());
                    s
                }
                None => String::new(),
            };
            parts.push(formatted);
        }
        parts.join("-")
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        let rows = self.rows();
        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        let total = rows.len();
        self.state.ensure_visible(total, 4);

        match key.code {
            KeyCode::Up => {
                self.state.move_up_wrap(total);
                true
            }
            KeyCode::Down => {
                self.state.move_down_wrap(total);
                true
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(sel) = self.state.selected_idx
                    && let Some(row) = rows.get(sel).copied() {
                        self.handle_enter(row);
                    }
                true
            }
            KeyCode::Esc => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }

    fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let Some(layout) = self.page().content_only().layout(area) else {
            return false;
        };
        let Some(row) = self.row_at_position(layout.body, mouse_event.column, mouse_event.row) else {
            return false;
        };

        self.state.selected_idx = Some(0);
        if matches!(
            mouse_event.kind,
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
        ) {
            self.handle_enter(row);
        }
        true
    }

    fn handle_mouse_event_direct_framed(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let Some(layout) = self.page().framed().layout(area) else {
            return false;
        };
        let Some(row) = self.row_at_position(layout.body, mouse_event.column, mouse_event.row) else {
            return false;
        };

        self.state.selected_idx = Some(0);
        if matches!(
            mouse_event.kind,
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
        ) {
            self.handle_enter(row);
        }
        true
    }

    fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        let Some(layout) = self.page().content_only().render_shell(area, buf) else {
            return;
        };
        self.render_rows(layout.body, buf);
    }

    fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        let Some(layout) = self.page().framed().render_shell(area, buf) else {
            return;
        };
        self.render_rows(layout.body, buf);
    }
}

impl<'v> PlanningSettingsViewFramed<'v> {
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render_framed(area, buf);
    }
}

impl<'v> PlanningSettingsViewContentOnly<'v> {
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render_content_only(area, buf);
    }
}

impl<'v> PlanningSettingsViewFramedMut<'v> {
    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.view.handle_mouse_event_direct_framed(mouse_event, area)
    }
}

impl<'v> PlanningSettingsViewContentOnlyMut<'v> {
    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.view
            .handle_mouse_event_direct_content_only(mouse_event, area)
    }
}

impl<'a> BottomPaneView<'a> for PlanningSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if !matches!(key_event.modifiers, KeyModifiers::NONE) {
            return;
        }
        let _ = self.handle_key(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        if !matches!(key_event.modifiers, KeyModifiers::NONE) {
            return ConditionalUpdate::NoRedraw;
        }
        redraw_if(self.handle_key(key_event))
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
        6
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.framed().render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use std::sync::mpsc::channel;

    fn left_click(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn planning_mouse_hit_targets_content_row() {
        let (tx, _rx) = channel();
        let mut view = PlanningSettingsView::new(
            false,
            "gpt-5.3-codex".to_string(),
            ReasoningEffort::Medium,
            AppEventSender::new(tx),
        );
        let area = Rect::new(0, 0, 40, 8);
        let page = view.page();
        let layout = page.content_only().layout(area).expect("layout");
        assert_eq!(
            view.row_at_position(layout.body, layout.body.x, layout.body.y),
            Some(PlanningRow::CustomModel)
        );
        assert!(view
            .content_only_mut()
            .handle_mouse_event_direct(left_click(layout.body.x, layout.body.y), area));
    }
}
