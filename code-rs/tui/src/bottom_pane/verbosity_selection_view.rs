use code_core::config_types::TextVerbosity;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
    wrap_next,
    wrap_prev,
};

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::hints::{shortcut_line, KeyHint};
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::menu_rows::SettingsMenuRow;
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::rows::StyledText;
use super::BottomPane;

const VERBOSITY_OPTIONS: [(TextVerbosity, &str, &str); 3] = [
    (TextVerbosity::Low, "Low", "Concise responses"),
    (TextVerbosity::Medium, "Medium", "Balanced detail (default)"),
    (TextVerbosity::High, "High", "Detailed responses"),
];

/// Interactive UI for selecting text verbosity level.
pub(crate) struct VerbositySelectionView {
    current_verbosity: TextVerbosity,
    selected_verbosity: TextVerbosity,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

impl VerbositySelectionView {
    pub fn new(current_verbosity: TextVerbosity, app_event_tx: AppEventSender) -> Self {
        Self {
            current_verbosity,
            selected_verbosity: current_verbosity,
            app_event_tx,
            is_complete: false,
        }
    }

    fn selected_index(&self) -> usize {
        VERBOSITY_OPTIONS
            .iter()
            .position(|(verbosity, _, _)| *verbosity == self.selected_verbosity)
            .unwrap_or(0)
    }

    fn set_selected_index(&mut self, idx: usize) {
        let idx = idx.min(VERBOSITY_OPTIONS.len().saturating_sub(1));
        self.selected_verbosity = VERBOSITY_OPTIONS[idx].0;
    }

    fn move_selection_up(&mut self) {
        let idx = wrap_prev(self.selected_index(), VERBOSITY_OPTIONS.len());
        self.set_selected_index(idx);
    }

    fn move_selection_down(&mut self) {
        let idx = wrap_next(self.selected_index(), VERBOSITY_OPTIONS.len());
        self.set_selected_index(idx);
    }

    fn confirm_selection(&mut self) {
        self.app_event_tx
            .send(AppEvent::UpdateTextVerbosity(self.selected_verbosity));
        self.is_complete = true;
    }

    fn page(&self) -> SettingsMenuPage<'static> {
        let header_lines = vec![Line::from(vec![
            Span::styled("Current: ", Style::new().fg(colors::text_dim())),
            Span::styled(
                format!("{}", self.current_verbosity),
                Style::new().fg(colors::warning()).bold(),
            ),
        ])];
        let footer_lines = vec![shortcut_line(&[
            KeyHint::new("↑↓", " navigate").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter", " select").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " cancel").with_key_style(Style::new().fg(colors::error()).bold()),
        ])];

        SettingsMenuPage::new(
            "Text verbosity",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            header_lines,
            footer_lines,
        )
    }

    fn menu_rows(&self) -> Vec<SettingsMenuRow<'static, TextVerbosity>> {
        VERBOSITY_OPTIONS
            .iter()
            .map(|(verbosity, name, description)| {
                let mut row = SettingsMenuRow::new(*verbosity, *name).with_detail(StyledText::new(
                    *description,
                    Style::new().fg(colors::text_dim()),
                ));
                if *verbosity == self.current_verbosity {
                    row = row.with_value(StyledText::new(
                        "(current)",
                        Style::new().fg(colors::warning()).bold(),
                    ));
                }
                row
            })
            .collect()
    }

    fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
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
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.confirm_selection();
                true
            }
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }

    fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let rows = self.menu_rows();
        let Some(layout) = self.page().layout(area) else {
            return false;
        };

        let mut selected_idx = self.selected_index();
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected_idx,
            rows.len(),
            |x, y| super::settings_ui::rows::selection_index_at(layout.body, x, y, 0, rows.len()),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.set_selected_index(selected_idx);

        if matches!(result, SelectableListMouseResult::Activated) {
            self.confirm_selection();
            return true;
        }

        result.handled()
    }
}

impl<'a> BottomPaneView<'a> for VerbositySelectionView {
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

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        9
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let page = self.page();
        let rows = self.menu_rows();
        let _ = page.render_menu_rows(area, buf, 0, Some(self.selected_verbosity), &rows);
    }
}

