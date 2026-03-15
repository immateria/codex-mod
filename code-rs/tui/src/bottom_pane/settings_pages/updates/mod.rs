use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
use crate::colors;
use crate::ui_interaction::{
    HeaderBodyFooterLayout,
    redraw_if,
    route_selectable_list_mouse_with_config,
    split_header_body_footer,
    SelectableListMouseConfig,
    SelectableListMouseResult,
    wrap_next,
    wrap_prev,
};
use crate::util::buffer::{fill_rect, write_line};

use crate::bottom_pane::{BottomPaneView, ConditionalUpdate};
use crate::bottom_pane::settings_ui::hints::{shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::{render_menu_rows, selection_id_at, SettingsMenuRow};
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::bottom_pane::settings_ui::toggle;
use crate::bottom_pane::BottomPane;

#[derive(Debug, Clone, Default)]
pub struct UpdateSharedState {
    pub checking: bool,
    pub latest_version: Option<String>,
    pub error: Option<String>,
}

pub(crate) struct UpdateSettingsView {
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    field: usize,
    is_complete: bool,
    auto_enabled: bool,
    shared: Arc<Mutex<UpdateSharedState>>,
    current_version: String,
    command: Option<Vec<String>>,
    command_display: Option<String>,
    manual_instructions: Option<String>,
}

pub(crate) type UpdateSettingsViewFramed<'v> = crate::bottom_pane::chrome_view::Framed<'v, UpdateSettingsView>;
pub(crate) type UpdateSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, UpdateSettingsView>;
pub(crate) type UpdateSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, UpdateSettingsView>;
pub(crate) type UpdateSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, UpdateSettingsView>;

pub(crate) struct UpdateSettingsInit {
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) ticket: BackgroundOrderTicket,
    pub(crate) current_version: String,
    pub(crate) auto_enabled: bool,
    pub(crate) command: Option<Vec<String>>,
    pub(crate) command_display: Option<String>,
    pub(crate) manual_instructions: Option<String>,
    pub(crate) shared: Arc<Mutex<UpdateSharedState>>,
}

impl UpdateSettingsView {
    const PANEL_TITLE: &'static str = "Upgrade";
    const FIELD_COUNT: usize = 3;

    pub fn new(init: UpdateSettingsInit) -> Self {
        let UpdateSettingsInit {
            app_event_tx,
            ticket,
            current_version,
            auto_enabled,
            command,
            command_display,
            manual_instructions,
            shared,
        } = init;
        Self {
            app_event_tx,
            ticket,
            field: 0,
            is_complete: false,
            auto_enabled,
            shared,
            current_version,
            command,
            command_display,
            manual_instructions,
        }
    }

    fn current_state(&self) -> UpdateSharedState {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn footer_lines() -> Vec<Line<'static>> {
        vec![shortcut_line(&[
            KeyHint::new("Up/Down", " move"),
            KeyHint::new("Enter", " activate"),
            KeyHint::new("Space", " toggle"),
            KeyHint::new("Esc", " close"),
        ])]
    }

    fn header_lines(&self) -> Vec<Line<'static>> {
        let guide_line = if self.command.is_some() {
            format!("Guided command: {}", self.guided_command_label())
        } else {
            "Run Upgrade will post manual instructions in the transcript.".to_string()
        };

        vec![
            Line::from(vec![
                Span::styled("Current: ", Style::new().fg(colors::text_dim())),
                Span::styled(
                    self.current_version.clone(),
                    Style::new().fg(colors::text()).bold(),
                ),
            ]),
            Line::from(Span::styled(guide_line, Style::new().fg(colors::text_dim()))),
        ]
    }

    fn version_summary(&self, state: &UpdateSharedState) -> String {
        if state.checking {
            "checking for updates".to_string()
        } else if let Some(err) = state.error.as_deref() {
            format!("check failed: {err}")
        } else if let Some(latest) = state.latest_version.as_deref() {
            format!("{} -> {latest}", self.current_version)
        } else {
            format!("{} (up to date)", self.current_version)
        }
    }

    fn guided_command_label(&self) -> String {
        self.command_display.clone().unwrap_or_else(|| {
            self.command
                .as_ref()
                .map(|command| command.join(" "))
                .unwrap_or_else(|| "manual instructions".to_string())
        })
    }

    fn run_upgrade_value(&self, state: &UpdateSharedState) -> StyledText<'static> {
        if state.checking {
            StyledText::new("checking", Style::new().fg(colors::warning()))
        } else if state.error.is_some() {
            StyledText::new("blocked", Style::new().fg(colors::error()))
        } else if state.latest_version.is_some() {
            StyledText::new("available", Style::new().fg(colors::success()))
        } else if self.command.is_some() {
            StyledText::new("up to date", Style::new().fg(colors::text_dim()))
        } else {
            StyledText::new("manual", Style::new().fg(colors::info()))
        }
    }

    fn auto_upgrade_row(auto_enabled: bool) -> SettingsMenuRow<'static, usize> {
        SettingsMenuRow::new(1usize, "Automatic Upgrades")
            .with_value(toggle::enabled_word(auto_enabled))
            .with_detail(StyledText::new(
                "checks on launch",
                Style::new().fg(colors::text_dim()),
            ))
            .with_selected_hint("(press Enter/Space to toggle)")
    }

    fn rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        let state = self.current_state();
        vec![
            SettingsMenuRow::new(0usize, "Run Upgrade")
                .with_value(self.run_upgrade_value(&state))
                .with_detail(StyledText::new(
                    self.version_summary(&state),
                    Style::new().fg(colors::text_dim()),
                ))
                .with_selected_hint(if self.command.is_some() {
                    "(press Enter to start)"
                } else {
                    "(press Enter for instructions)"
                }),
            Self::auto_upgrade_row(self.auto_enabled),
            SettingsMenuRow::new(2usize, "Close"),
        ]
    }

    fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            Self::PANEL_TITLE,
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            self.header_lines(),
            Self::footer_lines(),
        )
    }

    fn content_layout(&self, area: Rect) -> Option<HeaderBodyFooterLayout> {
        split_header_body_footer(area, self.header_lines().len(), Self::footer_lines().len(), 1)
    }

    fn render_body_without_frame(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let base = Style::new().bg(colors::background()).fg(colors::text());
        fill_rect(buf, area, Some(' '), base);

        let Some(layout) = self.content_layout(area) else {
            let rows = self.rows();
            render_menu_rows(area, buf, 0, Some(self.field), &rows, base);
            return;
        };

        let header_lines = self.header_lines();
        for (idx, line) in header_lines
            .iter()
            .enumerate()
            .take(layout.header.height as usize)
        {
            let y = layout.header.y.saturating_add(idx as u16);
            write_line(buf, layout.header.x, y, layout.header.width, line, base);
        }

        let rows = self.rows();
        render_menu_rows(layout.body, buf, 0, Some(self.field), &rows, base);

        let footer_lines = Self::footer_lines();
        for (idx, line) in footer_lines
            .iter()
            .enumerate()
            .take(layout.footer.height as usize)
        {
            let y = layout.footer.y.saturating_add(idx as u16);
            write_line(buf, layout.footer.x, y, layout.footer.width, line, base);
        }
    }

    fn toggle_auto(&mut self) {
        self.auto_enabled = !self.auto_enabled;
        self.app_event_tx
            .send(AppEvent::SetAutoUpgradeEnabled(self.auto_enabled));
    }

    fn invoke_run_upgrade(&mut self) {
        let state = self.current_state();

        if self.command.is_none() {
            if let Some(instructions) = &self.manual_instructions {
                self.app_event_tx
                    .send_background_event_with_ticket(&self.ticket, instructions.clone());
            }
            return;
        }

        if state.checking {
            self.app_event_tx.send_background_event_with_ticket(
                &self.ticket,
                "Still checking for updates...".to_string(),
            );
            return;
        }
        if let Some(err) = &state.error {
            self.app_event_tx.send_background_event_with_ticket(
                &self.ticket,
                format!("/update failed: {err}"),
            );
            return;
        }
        let Some(latest) = state.latest_version else {
            self.app_event_tx.send_background_event_with_ticket(
                &self.ticket,
                "Code is already up to date.".to_string(),
            );
            return;
        };

        let Some(command) = self.command.clone() else {
            return;
        };
        let display = self
            .command_display
            .clone()
            .unwrap_or_else(|| command.join(" "));

        self.app_event_tx.send_background_event_with_ticket(
            &self.ticket,
            format!(
                "Update available: {} -> {}. Opening guided upgrade with `{display}`...",
                self.current_version, latest
            ),
        );
        self.app_event_tx.send(AppEvent::RunUpdateCommand {
            command,
            display: display.clone(),
            latest_version: Some(latest.clone()),
        });
        self.app_event_tx.send_background_event_with_ticket(
            &self.ticket,
            format!(
                "Complete the guided terminal steps for `{display}` then restart Code to finish upgrading to {latest}."
            ),
        );
        self.is_complete = true;
    }

    fn activate_selected(&mut self) {
        match self.field {
            0 => self.invoke_run_upgrade(),
            1 => self.toggle_auto(),
            _ => self.is_complete = true,
        }
    }

    fn handle_mouse_event_in_body(&mut self, mouse_event: MouseEvent, body: Rect) -> bool {
        let rows = self.rows();
        let mut selected = self.field;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            rows.len(),
            |x, y| selection_id_at(body, x, y, 0, &rows),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.field = selected;

        if matches!(result, SelectableListMouseResult::Activated) {
            self.activate_selected();
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }

        result.handled()
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        let handled = match key_event.code {
            KeyCode::Esc => {
                self.is_complete = true;
                true
            }
            KeyCode::Tab | KeyCode::Down => {
                self.field = wrap_next(self.field, Self::FIELD_COUNT);
                true
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.field = wrap_prev(self.field, Self::FIELD_COUNT);
                true
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if self.field == 1 => {
                self.toggle_auto();
                true
            }
            KeyCode::Enter => {
                self.activate_selected();
                true
            }
            _ => false,
        };
        if handled {
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }
        handled
    }

    pub(crate) fn framed(&self) -> UpdateSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> UpdateSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> UpdateSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> UpdateSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    fn handle_mouse_event_direct_content_only(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let Some(layout) = self.content_layout(area) else {
            return false;
        };
        self.handle_mouse_event_in_body(mouse_event, layout.body)
    }

    fn handle_mouse_event_direct_framed(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.page()
            .framed()
            .layout(area)
            .map(|layout| self.handle_mouse_event_in_body(mouse_event, layout.body))
            .unwrap_or(false)
    }

    pub fn is_view_complete(&self) -> bool {
        self.is_complete
    }

    fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_body_without_frame(area, buf);
    }

    fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.rows();
        let _ = self
            .page()
            .framed()
            .render_menu_rows(area, buf, 0, Some(self.field), &rows);
    }
}

impl crate::bottom_pane::chrome_view::ChromeRenderable for UpdateSettingsView {
    fn render_in_framed_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_framed(area, buf);
    }

    fn render_in_content_only_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_content_only(area, buf);
    }
}

impl crate::bottom_pane::chrome_view::ChromeMouseHandler for UpdateSettingsView {
    fn handle_mouse_event_direct_in_framed_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_framed(mouse_event, area)
    }

    fn handle_mouse_event_direct_in_content_only_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_content_only(mouse_event, area)
    }
}

impl<'a> BottomPaneView<'a> for UpdateSettingsView {
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
        redraw_if(
            self.framed_mut()
                .handle_mouse_event_direct(mouse_event, area),
        )
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let rows = self
            .header_lines()
            .len()
            .saturating_add(Self::FIELD_COUNT)
            .saturating_add(Self::footer_lines().len())
            .saturating_add(2);
        u16::try_from(rows).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.framed().render(area, buf);
    }

    fn handle_paste(&mut self, _text: String) -> ConditionalUpdate {
        ConditionalUpdate::NoRedraw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_upgrades_row_uses_shared_toggle_word() {
        let row = UpdateSettingsView::auto_upgrade_row(false);
        let value = row.value.expect("toggle value");
        assert_eq!(value.text.as_ref(), "disabled");
        assert_eq!(value.style.fg, Some(colors::text_dim()));
    }
}
