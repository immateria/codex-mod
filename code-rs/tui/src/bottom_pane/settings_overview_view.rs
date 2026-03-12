use super::bottom_pane_view::ConditionalUpdate;
use super::settings_ui::hints::{shortcut_line, KeyHint};
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::menu_rows::SettingsMenuRow as SharedSettingsMenuRow;
use super::settings_ui::panel::SettingsPanelStyle;
use super::{BottomPane, BottomPaneView, SettingsSection};
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;
use crate::colors;
use crate::ui_interaction::{
    route_selectable_list_mouse_with_config, ScrollSelectionBehavior, SelectableListMouseConfig,
    SelectableListMouseResult,
};
use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use super::settings_ui::rows::StyledText;

#[derive(Debug, Clone)]
pub(crate) struct SettingsMenuRow {
    pub(crate) section: SettingsSection,
    pub(crate) summary: Option<String>,
}

pub(crate) struct SettingsOverviewView {
    rows: Vec<SettingsMenuRow>,
    scroll: ScrollState,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

impl SettingsOverviewView {
    fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Settings",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![shortcut_line(&[
                KeyHint::new("↑↓/jk", " move")
                    .with_key_style(Style::new().fg(colors::function())),
                KeyHint::new("Enter", " open")
                    .with_key_style(Style::new().fg(colors::function())),
                KeyHint::new("Esc", " close")
                    .with_key_style(Style::new().fg(colors::function())),
            ])],
            vec![Line::from(vec![Span::styled(
                self.selected_section()
                    .map(SettingsSection::help_line)
                    .unwrap_or(""),
                Style::new().fg(colors::text_dim()),
            )])],
        )
    }

    pub(crate) fn new(
        rows: Vec<SettingsMenuRow>,
        initial_section: SettingsSection,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut scroll = ScrollState::new();
        if !rows.is_empty() {
            let selected = rows
                .iter()
                .position(|row| row.section == initial_section)
                .unwrap_or(0);
            scroll.selected_idx = Some(selected);
        }
        Self {
            rows,
            scroll,
            app_event_tx,
            is_complete: false,
        }
    }

    fn selected_index(&self) -> usize {
        self.scroll.selected_idx.unwrap_or(0)
    }

    fn selected_section(&self) -> Option<SettingsSection> {
        self.rows.get(self.selected_index()).map(|row| row.section)
    }

    fn move_up(&mut self, visible_rows: usize) {
        self.scroll.move_up_wrap_visible(self.rows.len(), visible_rows);
    }

    fn move_down(&mut self, visible_rows: usize) {
        self.scroll.move_down_wrap_visible(self.rows.len(), visible_rows);
    }

    fn open_selected(&mut self) {
        let Some(section) = self.selected_section() else {
            return;
        };
        self.app_event_tx
            .send(AppEvent::OpenSettings { section: Some(section) });
        self.is_complete = true;
    }

    fn cancel(&mut self) {
        self.is_complete = true;
    }

    fn process_key_event(&mut self, key_event: KeyEvent, visible_rows: usize) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.cancel();
                true
            }
            KeyCode::Enter => {
                self.open_selected();
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up(visible_rows);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down(visible_rows);
                true
            }
            KeyCode::Home => {
                if !self.rows.is_empty() {
                    self.scroll.selected_idx = Some(0);
                    self.scroll.scroll_top = 0;
                }
                true
            }
            KeyCode::End => {
                let len = self.rows.len();
                if len > 0 {
                    self.scroll.selected_idx = Some(len - 1);
                    self.scroll.ensure_visible(len, visible_rows.max(1));
                }
                true
            }
            _ => false,
        }
    }

    fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let page = self.page();
        let Some(layout) = page.framed().layout(area) else {
            return false;
        };

        if self.rows.is_empty() || layout.body.width == 0 || layout.body.height == 0 {
            return false;
        }

        let visible_rows = layout.body.height as usize;
        let mut selected = self.selected_index();
        let scroll_top = self.scroll.scroll_top.min(self.rows.len().saturating_sub(1));
        let rows = self.menu_rows();

        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.rows.len(),
            |x, y| {
                SettingsMenuPage::selection_menu_id_in_body(
                    layout.body,
                    x,
                    y,
                    scroll_top,
                    &rows,
                )
                    .and_then(|section| self.rows.iter().position(|row| row.section == section))
            },
            SelectableListMouseConfig {
                hover_select: false,
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );

        match result {
            SelectableListMouseResult::Ignored => false,
            SelectableListMouseResult::SelectionChanged => {
                self.scroll.selected_idx = Some(selected);
                self.scroll.ensure_visible(self.rows.len(), visible_rows.max(1));
                true
            }
            SelectableListMouseResult::Activated => {
                self.scroll.selected_idx = Some(selected);
                self.open_selected();
                true
            }
        }
    }

    fn menu_rows(&self) -> Vec<SharedSettingsMenuRow<'_, SettingsSection>> {
        self.rows
            .iter()
            .map(|row| {
                let mut item = SharedSettingsMenuRow::new(row.section, row.section.label());
                if let Some(summary) = row.summary.as_deref() {
                    item = item.with_detail(StyledText::new(summary, Style::new().fg(colors::text_dim())));
                }
                item
            })
            .collect()
    }
}

impl<'a> BottomPaneView<'a> for SettingsOverviewView {
    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        let visible_rows = 12usize;
        if self.process_key_event(key_event, visible_rows) {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        // Ignore move events when the terminal doesn't support them.
        if matches!(mouse_event.kind, MouseEventKind::Moved) && area.width == 0 {
            return ConditionalUpdate::NoRedraw;
        }
        if self.handle_mouse_event_direct(mouse_event, area) {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let visible = self.rows.len().clamp(1, 12) as u16;
        // border (2) + header (1) + visible rows + footer (1)
        2 + 1 + visible + 1
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if self.rows.is_empty() {
            let page = self.page();
            let Some(layout) = page.framed().render_shell(area, buf) else {
                return;
            };
            Paragraph::new(Line::from(vec![Span::styled(
                "No settings sections available.",
                Style::new().fg(colors::text_dim()),
            )]))
            .render(layout.body, buf);
            return;
        }
        let scroll_top = self.scroll.scroll_top.min(self.rows.len().saturating_sub(1));
        let page = self.page();
        let rows = self.menu_rows();
        let _ = page
            .framed()
            .render_menu_rows(area, buf, scroll_top, self.selected_section(), &rows);
    }
}
