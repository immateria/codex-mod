use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use crate::ui_interaction::{
    RelativeHitRegion,
    redraw_if,
    route_selectable_regions_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
    wrap_next,
    wrap_prev,
};
use super::BottomPane;
// TODO - This is currently unlinked here, on the official CODEX side, etc. figure out what to do later.
/// Interactive UI for GitHub workflow monitoring settings.
/// Shows token status and allows toggling the watcher on/off.
pub(crate) struct GithubSettingsView {
    watcher_enabled: bool,
    token_status: String,
    token_ready: bool,
    app_event_tx: AppEventSender,
    is_complete: bool,
    /// Selection index: 0 = toggle, 1 = close
    selected_row: usize,
}

impl GithubSettingsView {
    const TOGGLE_ROW: usize = 0;
    const CLOSE_ROW: usize = 1;
    const ROW_COUNT: usize = 2;

    pub fn new(watcher_enabled: bool, token_status: String, ready: bool, app_event_tx: AppEventSender) -> Self {
        Self {
            watcher_enabled,
            token_status,
            token_ready: ready,
            app_event_tx,
            is_complete: false,
            selected_row: 0,
        }
    }

    fn toggle(&mut self) {
        self.watcher_enabled = !self.watcher_enabled;
        self.app_event_tx
            .send(AppEvent::UpdateGithubWatcher(self.watcher_enabled));
    }

    fn activate_selected_row(&mut self) {
        if self.selected_row == Self::TOGGLE_ROW {
            self.toggle();
        } else {
            self.is_complete = true;
        }
    }

    fn content_area(area: Rect) -> Rect {
        let inner = Block::default().borders(Borders::ALL).inner(area);
        Rect {
            x: inner.x.saturating_add(1),
            y: inner.y,
            width: inner.width.saturating_sub(2),
            height: inner.height,
        }
    }

    fn selectable_regions() -> [RelativeHitRegion; 2] {
        [
            // See render() line layout: status, blank, toggle, blank, close, ...
            RelativeHitRegion::new(Self::TOGGLE_ROW, 2, 1),
            RelativeHitRegion::new(Self::CLOSE_ROW, 4, 1),
        ]
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                self.selected_row = wrap_prev(self.selected_row, Self::ROW_COUNT);
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                self.selected_row = wrap_next(self.selected_row, Self::ROW_COUNT);
                true
            }
            KeyEvent { code: KeyCode::Left | KeyCode::Right, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == Self::TOGGLE_ROW {
                    self.toggle();
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                self.activate_selected_row();
                true
            }
            KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == Self::TOGGLE_ROW {
                    self.toggle();
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }

    pub fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let content_area = Self::content_area(area);
        let mut selected = self.selected_row;
        let result = route_selectable_regions_mouse_with_config(
            mouse_event,
            &mut selected,
            Self::ROW_COUNT,
            content_area,
            &Self::selectable_regions(),
            SelectableListMouseConfig {
                scroll_behavior: ScrollSelectionBehavior::Wrap,
                ..SelectableListMouseConfig::default()
            },
        );
        self.selected_row = selected;

        if matches!(result, SelectableListMouseResult::Activated) {
            self.activate_selected_row();
        }

        result.handled()
    }

    pub fn is_view_complete(&self) -> bool {
        self.is_complete
    }
}

impl<'a> BottomPaneView<'a> for GithubSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_direct(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_direct(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_mouse_event_direct(mouse_event, area))
    }

    fn is_complete(&self) -> bool { self.is_complete }

    fn desired_height(&self, _width: u16) -> u16 { 9 }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" GitHub Settings ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let status_line = if self.token_ready {
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Ready", Style::default().fg(crate::colors::success()).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(&self.token_status, Style::default().fg(crate::colors::dim())),
            ])
        } else {
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("No token", Style::default().fg(crate::colors::warning()).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(
                    "Set GH_TOKEN/GITHUB_TOKEN or run: 'gh auth login'",
                    Style::default().fg(crate::colors::dim()),
                ),
            ])
        };

        let toggle_label = if self.watcher_enabled { "Enabled" } else { "Disabled" };
        let mut toggle_style = Style::default().fg(crate::colors::text());
        if self.selected_row == 0 { toggle_style = toggle_style.bg(crate::colors::selection()).add_modifier(Modifier::BOLD); }

        let lines = vec![
            status_line,
            Line::from(""),
            Line::from(vec![
                Span::styled("Workflow Monitoring: ", Style::default().fg(crate::colors::text_dim())),
                Span::styled(toggle_label, toggle_style),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(if self.selected_row == 1 { "› " } else { "  " }, Style::default()),
                Span::styled("Close", if self.selected_row == 1 { Style::default().bg(crate::colors::selection()).add_modifier(Modifier::BOLD) } else { Style::default() }),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("↑↓", Style::default().fg(crate::colors::light_blue())),
                Span::raw(" Navigate  "),
                Span::styled("←→/Space", Style::default().fg(crate::colors::success())),
                Span::raw(" Toggle  "),
                Span::styled("Enter", Style::default().fg(crate::colors::success())),
                Span::raw(" Toggle/Close  "),
                Span::styled("Esc", Style::default().fg(crate::colors::error())),
                Span::raw(" Cancel"),
            ]),
        ];

        let paragraph = Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));
        paragraph.render(Rect { x: inner.x.saturating_add(1), y: inner.y, width: inner.width.saturating_sub(2), height: inner.height }, buf);
    }
}
