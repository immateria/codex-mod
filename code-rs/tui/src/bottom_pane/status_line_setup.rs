use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyModifiers,
    MouseButton,
    MouseEvent,
    MouseEventKind,
};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::BottomPane;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use crate::ui_interaction::{redraw_if, wrap_next, wrap_prev};
use code_core::config_types::StatusLineLane;

#[derive(EnumIter, EnumString, Display, Debug, Clone, Copy, Eq, PartialEq)]
#[strum(serialize_all = "kebab_case")]
pub(crate) enum StatusLineItem {
    ModelName,
    ModelWithReasoning,
    CurrentDir,
    ProjectRoot,
    GitBranch,
    ContextRemaining,
    ContextUsed,
    FiveHourLimit,
    WeeklyLimit,
    CodexVersion,
    ContextWindowSize,
    UsedTokens,
    TotalInputTokens,
    TotalOutputTokens,
    SessionId,
}

impl StatusLineItem {
    pub(crate) fn label(self) -> &'static str {
        match self {
            StatusLineItem::ModelName => "Model name",
            StatusLineItem::ModelWithReasoning => "Model + reasoning",
            StatusLineItem::CurrentDir => "Current directory",
            StatusLineItem::ProjectRoot => "Project root",
            StatusLineItem::GitBranch => "Git branch",
            StatusLineItem::ContextRemaining => "Context remaining",
            StatusLineItem::ContextUsed => "Context used",
            StatusLineItem::FiveHourLimit => "5-hour limit",
            StatusLineItem::WeeklyLimit => "Weekly limit",
            StatusLineItem::CodexVersion => "Version",
            StatusLineItem::ContextWindowSize => "Context window size",
            StatusLineItem::UsedTokens => "Used tokens",
            StatusLineItem::TotalInputTokens => "Total input tokens",
            StatusLineItem::TotalOutputTokens => "Total output tokens",
            StatusLineItem::SessionId => "Session id",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            StatusLineItem::ModelName => "Current model name.",
            StatusLineItem::ModelWithReasoning => "Current model with reasoning level.",
            StatusLineItem::CurrentDir => "Current working directory.",
            StatusLineItem::ProjectRoot => "Detected project root directory.",
            StatusLineItem::GitBranch => "Current git branch when available.",
            StatusLineItem::ContextRemaining => "Remaining model context percentage.",
            StatusLineItem::ContextUsed => "Used model context percentage.",
            StatusLineItem::FiveHourLimit => "Primary rate-limit window usage.",
            StatusLineItem::WeeklyLimit => "Secondary rate-limit window usage.",
            StatusLineItem::CodexVersion => "App version.",
            StatusLineItem::ContextWindowSize => "Model context window size.",
            StatusLineItem::UsedTokens => "Total tokens used in this session.",
            StatusLineItem::TotalInputTokens => "Total input tokens.",
            StatusLineItem::TotalOutputTokens => "Total output tokens.",
            StatusLineItem::SessionId => "Current session identifier.",
        }
    }

    fn sample(self) -> &'static str {
        match self {
            StatusLineItem::ModelName => "GPT-5.3-Codex",
            StatusLineItem::ModelWithReasoning => "GPT-5.3-Codex High",
            StatusLineItem::CurrentDir => "~/code-termux",
            StatusLineItem::ProjectRoot => "code-termux",
            StatusLineItem::GitBranch => "main",
            StatusLineItem::ContextRemaining => "64% left",
            StatusLineItem::ContextUsed => "36% used",
            StatusLineItem::FiveHourLimit => "5h 27%",
            StatusLineItem::WeeklyLimit => "weekly 4%",
            StatusLineItem::CodexVersion => "v0.0.0",
            StatusLineItem::ContextWindowSize => "256K window",
            StatusLineItem::UsedTokens => "12.4K used",
            StatusLineItem::TotalInputTokens => "9.3K in",
            StatusLineItem::TotalOutputTokens => "3.1K out",
            StatusLineItem::SessionId => "a18f2f0d-01d4-4dbf-b2b6-2f53",
        }
    }
}

#[derive(Clone, Copy)]
struct StatusLineChoice {
    item: StatusLineItem,
    enabled: bool,
}

pub(crate) struct StatusLineSetupView {
    app_event_tx: AppEventSender,
    top_choices: Vec<StatusLineChoice>,
    bottom_choices: Vec<StatusLineChoice>,
    top_selected_index: usize,
    bottom_selected_index: usize,
    active_lane: StatusLineLane,
    primary_lane: StatusLineLane,
    complete: bool,
}

impl StatusLineSetupView {
    pub(crate) fn new(
        top_current: Option<&[String]>,
        bottom_current: Option<&[String]>,
        primary_lane: StatusLineLane,
        initial_lane: StatusLineLane,
        app_event_tx: AppEventSender,
    ) -> Self {
        Self {
            app_event_tx,
            top_choices: Self::build_choices(top_current),
            bottom_choices: Self::build_choices(bottom_current),
            top_selected_index: 0,
            bottom_selected_index: 0,
            active_lane: initial_lane,
            primary_lane,
            complete: false,
        }
    }

    fn build_choices(current: Option<&[String]>) -> Vec<StatusLineChoice> {
        use std::collections::HashSet;

        let mut seen = HashSet::<String>::new();
        let mut choices = Vec::<StatusLineChoice>::new();

        if let Some(ids) = current {
            for id in ids {
                let Ok(item) = id.parse::<StatusLineItem>() else {
                    continue;
                };
                let key = item.to_string();
                if !seen.insert(key) {
                    continue;
                }
                choices.push(StatusLineChoice {
                    item,
                    enabled: true,
                });
            }
        }

        for item in StatusLineItem::iter() {
            let key = item.to_string();
            if seen.contains(&key) {
                continue;
            }
            choices.push(StatusLineChoice {
                item,
                enabled: false,
            });
        }

        choices
    }

    fn selected_ids_for_lane(&self, lane: StatusLineLane) -> Vec<StatusLineItem> {
        self.choices_for_lane(lane)
            .iter()
            .filter_map(|choice| choice.enabled.then_some(choice.item))
            .collect()
    }

    fn preview_text_for_lane(&self, lane: StatusLineLane) -> String {
        self.choices_for_lane(lane)
            .iter()
            .filter(|choice| choice.enabled)
            .map(|choice| choice.item.sample())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn choices_for_lane(&self, lane: StatusLineLane) -> &[StatusLineChoice] {
        match lane {
            StatusLineLane::Top => &self.top_choices,
            StatusLineLane::Bottom => &self.bottom_choices,
        }
    }

    fn choices_for_active_lane(&self) -> &[StatusLineChoice] {
        self.choices_for_lane(self.active_lane)
    }

    fn choices_for_active_lane_mut(&mut self) -> &mut Vec<StatusLineChoice> {
        match self.active_lane {
            StatusLineLane::Top => &mut self.top_choices,
            StatusLineLane::Bottom => &mut self.bottom_choices,
        }
    }

    fn selected_index_for_active_lane(&self) -> usize {
        match self.active_lane {
            StatusLineLane::Top => self.top_selected_index,
            StatusLineLane::Bottom => self.bottom_selected_index,
        }
    }

    fn set_selected_index_for_active_lane(&mut self, value: usize) {
        match self.active_lane {
            StatusLineLane::Top => self.top_selected_index = value,
            StatusLineLane::Bottom => self.bottom_selected_index = value,
        }
    }

    fn switch_active_lane(&mut self) {
        self.active_lane = match self.active_lane {
            StatusLineLane::Top => StatusLineLane::Bottom,
            StatusLineLane::Bottom => StatusLineLane::Top,
        };

        let len = self.choices_for_active_lane().len();
        if len == 0 {
            self.set_selected_index_for_active_lane(0);
            return;
        }

        let idx = self.selected_index_for_active_lane().min(len.saturating_sub(1));
        self.set_selected_index_for_active_lane(idx);
    }

    fn toggle_primary_lane(&mut self) {
        self.primary_lane = match self.primary_lane {
            StatusLineLane::Top => StatusLineLane::Bottom,
            StatusLineLane::Bottom => StatusLineLane::Top,
        };
    }

    fn move_selection_up(&mut self) {
        let len = self.choices_for_active_lane().len();
        let idx = wrap_prev(self.selected_index_for_active_lane(), len);
        self.set_selected_index_for_active_lane(idx);
    }

    fn move_selection_down(&mut self) {
        let len = self.choices_for_active_lane().len();
        let idx = wrap_next(self.selected_index_for_active_lane(), len);
        self.set_selected_index_for_active_lane(idx);
    }

    fn toggle_selected(&mut self) {
        let idx = self.selected_index_for_active_lane();
        if let Some(choice) = self.choices_for_active_lane_mut().get_mut(idx) {
            choice.enabled = !choice.enabled;
        }
    }

    fn move_selected_left(&mut self) {
        let idx = self.selected_index_for_active_lane();
        if idx == 0 {
            return;
        }
        self.choices_for_active_lane_mut().swap(idx, idx - 1);
        self.set_selected_index_for_active_lane(idx - 1);
    }

    fn move_selected_right(&mut self) {
        let idx = self.selected_index_for_active_lane();
        let len = self.choices_for_active_lane().len();
        if idx + 1 >= len {
            return;
        }
        self.choices_for_active_lane_mut().swap(idx, idx + 1);
        self.set_selected_index_for_active_lane(idx + 1);
    }

    fn confirm(&mut self) {
        self.app_event_tx.send(AppEvent::StatusLineSetup {
            top_items: self.selected_ids_for_lane(StatusLineLane::Top),
            bottom_items: self.selected_ids_for_lane(StatusLineLane::Bottom),
            primary: self.primary_lane,
        });
        self.complete = true;
    }

    fn cancel(&mut self) {
        self.app_event_tx.send(AppEvent::StatusLineSetupCancelled);
        self.complete = true;
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selection_up();
                true
            }
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selection_down();
                true
            }
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selected_left();
                true
            }
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selected_right();
                true
            }
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.toggle_selected();
                true
            }
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.switch_active_lane();
                true
            }
            KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.toggle_primary_lane();
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.confirm();
                true
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.cancel();
                true
            }
            _ => false,
        }
    }

    fn active_item_row_bounds(area: Rect, row_index: usize) -> Option<Rect> {
        let y = area.y.saturating_add(5).saturating_add(row_index as u16);
        if y >= area.y.saturating_add(area.height) {
            return None;
        }
        Some(Rect {
            x: area.x.saturating_add(2),
            y,
            width: area.width.saturating_sub(4),
            height: 1,
        })
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match mouse_event.kind {
            MouseEventKind::ScrollUp => {
                self.move_selection_up();
                true
            }
            MouseEventKind::ScrollDown => {
                self.move_selection_down();
                true
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let status_x = area.x.saturating_add(2);
                let status_width = area.width.saturating_sub(4);
                let within_status_x = mouse_event.column >= status_x
                    && mouse_event.column < status_x.saturating_add(status_width);

                let lane_row = area.y.saturating_add(2);
                if within_status_x && mouse_event.row == lane_row {
                    self.switch_active_lane();
                    return true;
                }

                let primary_row = area.y.saturating_add(3);
                if within_status_x && mouse_event.row == primary_row {
                    self.toggle_primary_lane();
                    return true;
                }

                for idx in 0..self.choices_for_active_lane().len() {
                    let Some(row) = Self::active_item_row_bounds(area, idx) else {
                        continue;
                    };
                    let within_x = mouse_event.column >= row.x
                        && mouse_event.column < row.x.saturating_add(row.width);
                    let within_y = mouse_event.row == row.y;
                    if !within_x || !within_y {
                        continue;
                    }

                    if self.selected_index_for_active_lane() == idx {
                        self.toggle_selected();
                    } else {
                        self.set_selected_index_for_active_lane(idx);
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn lane_label(lane: StatusLineLane) -> &'static str {
        match lane {
            StatusLineLane::Top => "Top",
            StatusLineLane::Bottom => "Bottom",
        }
    }
}

impl<'a> BottomPaneView<'a> for StatusLineSetupView {
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
        redraw_if(self.handle_mouse_event_direct(mouse_event, area))
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'a>) -> CancellationEvent {
        self.cancel();
        CancellationEvent::Handled
    }

    fn desired_height(&self, _width: u16) -> u16 {
        (self.choices_for_active_lane().len() as u16).saturating_add(12)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Status Line ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let active_lane = Self::lane_label(self.active_lane);
        let primary_lane = Self::lane_label(self.primary_lane);
        let top_preview = self.preview_text_for_lane(StatusLineLane::Top);
        let bottom_preview = self.preview_text_for_lane(StatusLineLane::Bottom);

        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("Tab", Style::default().fg(crate::colors::light_blue())),
            Span::styled(" lane  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("p", Style::default().fg(crate::colors::light_blue())),
            Span::styled(" primary  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Space", Style::default().fg(crate::colors::success())),
            Span::styled(" toggle  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("←/→", Style::default().fg(crate::colors::light_blue())),
            Span::styled(" reorder  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::styled(" apply  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Esc", Style::default().fg(crate::colors::error())),
            Span::styled(" cancel", Style::default().fg(crate::colors::text_dim())),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Editing lane: ", Style::default().fg(crate::colors::text_bright())),
            Span::styled(active_lane, Style::default().fg(crate::colors::light_blue()).add_modifier(Modifier::BOLD)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Primary lane: ", Style::default().fg(crate::colors::text_bright())),
            Span::styled(primary_lane, Style::default().fg(crate::colors::success()).add_modifier(Modifier::BOLD)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Top preview: ", Style::default().fg(crate::colors::text_bright())),
            Span::styled(
                if top_preview.is_empty() { "(none)".to_string() } else { top_preview },
                Style::default().fg(crate::colors::text_dim()),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Bottom preview: ", Style::default().fg(crate::colors::text_bright())),
            Span::styled(
                if bottom_preview.is_empty() {
                    "(none)".to_string()
                } else {
                    bottom_preview
                },
                Style::default().fg(crate::colors::text_dim()),
            ),
        ]));

        for (idx, choice) in self.choices_for_active_lane().iter().enumerate() {
            let selected = idx == self.selected_index_for_active_lane();
            let marker = if choice.enabled { "[x]" } else { "[ ]" };
            let pointer = if selected { "›" } else { " " };
            let mut line = Line::from(vec![
                Span::styled(pointer, Style::default().fg(crate::colors::light_blue())),
                Span::raw(" "),
                Span::styled(marker, Style::default().fg(crate::colors::success())),
                Span::raw(" "),
                Span::styled(choice.item.label(), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(choice.item.description(), Style::default().fg(crate::colors::text_dim())),
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

        let paragraph = Paragraph::new(lines).style(
            Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text()),
        );
        paragraph.render(
            Rect {
                x: inner.x.saturating_add(1),
                y: inner.y,
                width: inner.width.saturating_sub(2),
                height: inner.height,
            },
            buf,
        );
    }
}
