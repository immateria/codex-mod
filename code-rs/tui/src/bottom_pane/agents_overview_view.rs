use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
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
use super::settings_ui::line_runs::selection_id_at as selection_run_id_at;
use super::settings_ui::line_runs::SelectableLineRun;
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::menu_rows::SettingsMenuRow;
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::rows::StyledText;
use super::BottomPane;

#[derive(Clone, Debug)]
pub(crate) struct AgentsOverviewView {
    agents: Vec<(String, bool /*enabled*/, bool /*installed*/, String /*command*/)>,
    commands: Vec<String>,
    selected: usize,
    is_complete: bool,
    app_event_tx: AppEventSender,
}

impl AgentsOverviewView {
    pub fn new(
        agents: Vec<(String, bool, bool, String)>,
        commands: Vec<String>,
        selected_index: usize,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut view = Self {
            agents,
            commands,
            selected: 0,
            is_complete: false,
            app_event_tx,
        };
        let total = view.total_rows();
        if total > 0 {
            view.selected = selected_index.min(total.saturating_sub(1));
        }
        view
    }

    fn total_rows(&self) -> usize {
        self.agents
            .len()
            .saturating_add(self.commands.len())
            .saturating_add(1) // Add new…
    }

    fn set_selected(&mut self, selected: usize) {
        if selected == self.selected {
            return;
        }
        self.selected = selected;
        self.app_event_tx
            .send(AppEvent::AgentsOverviewSelectionChanged { index: self.selected });
    }

    fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Agents",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            Vec::new(),
            vec![shortcut_line(&[
                KeyHint::new("↑↓", " navigate").with_key_style(Style::new().fg(colors::function())),
                KeyHint::new("Enter", " activate")
                    .with_key_style(Style::new().fg(colors::success())),
                KeyHint::new("Esc", " close")
                    .with_key_style(Style::new().fg(colors::error()).bold()),
            ])],
        )
    }

    fn runs(&self, selected_id: Option<usize>) -> Vec<SelectableLineRun<'static, usize>> {
        let mut runs = Vec::new();

        runs.push(SelectableLineRun::plain(vec![Line::from(Span::styled(
            "Agents",
            Style::new().fg(colors::text_bright()).bold(),
        ))]));

        for (idx, (name, enabled, installed, _cmd)) in self.agents.iter().enumerate() {
            let (status_text, status_color) = if !*enabled {
                ("disabled", colors::error())
            } else if !*installed {
                ("not installed", colors::warning())
            } else {
                ("enabled", colors::success())
            };
            let hint = if !*installed {
                "Enter to install"
            } else {
                "Enter to configure"
            };
            runs.push(
                SettingsMenuRow::new(idx, name.clone())
                    .with_value(StyledText::new(
                        status_text,
                        Style::new().fg(status_color).bold(),
                    ))
                    .with_selected_hint(hint)
                    .into_run(selected_id),
            );
        }

        runs.push(SelectableLineRun::plain(vec![Line::from("")]));

        runs.push(SelectableLineRun::plain(vec![Line::from(Span::styled(
            "Commands",
            Style::new().fg(colors::text_bright()).bold(),
        ))]));

        for (j, cmd) in self.commands.iter().enumerate() {
            let idx = self.agents.len().saturating_add(j);
            runs.push(
                SettingsMenuRow::new(idx, format!("/{cmd}"))
                    .with_selected_hint("Enter to configure")
                    .into_run(selected_id),
            );
        }

        let add_idx = self.agents.len().saturating_add(self.commands.len());
        runs.push(
            SettingsMenuRow::new(add_idx, "Add new…")
                .with_selected_hint("Enter to add")
                .into_run(selected_id),
        );

        runs
    }

    fn activate_selected(&mut self) {
        let idx = self.selected;
        if idx < self.agents.len() {
            let (name, _en, installed, _cmd) = self.agents[idx].clone();
            if !installed {
                self.app_event_tx.send(AppEvent::RequestAgentInstall {
                    name,
                    selected_index: idx,
                });
            } else {
                self.app_event_tx.send(AppEvent::ShowAgentEditor { name });
            }
            self.is_complete = true;
            return;
        }

        let cmd_idx = idx.saturating_sub(self.agents.len());
        if cmd_idx < self.commands.len() {
            if let Some(name) = self.commands.get(cmd_idx) {
                self.app_event_tx
                    .send(AppEvent::ShowSubagentEditorForName { name: name.clone() });
                self.is_complete = true;
            }
            return;
        }

        self.app_event_tx.send(AppEvent::ShowSubagentEditorNew);
        self.is_complete = true;
    }

    fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            KeyEvent { code: KeyCode::Up, .. } => {
                let total = self.total_rows();
                if total == 0 {
                    return false;
                }
                self.set_selected(wrap_prev(self.selected, total));
                true
            }
            KeyEvent { code: KeyCode::Down, .. } => {
                let total = self.total_rows();
                if total == 0 {
                    return false;
                }
                self.set_selected(wrap_next(self.selected, total));
                true
            }
            KeyEvent { code: KeyCode::Enter, .. } => {
                self.activate_selected();
                true
            }
            _ => false,
        }
    }

    fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let total = self.total_rows();
        if total == 0 {
            return false;
        }

        let page = self.page();
        let runs = self.runs(None);
        let Some(layout) = page.framed().layout(area) else {
            return false;
        };

        let previous = self.selected;
        let mut selected = self.selected;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            total,
            |x, y| selection_run_id_at(layout.body, x, y, 0, &runs),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        if selected != previous {
            self.set_selected(selected);
        }

        if matches!(result, SelectableListMouseResult::Activated) {
            self.activate_selected();
            return true;
        }

        result.handled()
    }
}

impl<'a> BottomPaneView<'a> for AgentsOverviewView {
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
        let body_lines = self
            .runs(None)
            .iter()
            .map(|run| run.lines.len())
            .sum::<usize>();

        // body lines + 1 footer line + 2 border rows
        body_lines
            .saturating_add(3)
            .min(u16::MAX as usize) as u16
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let page = self.page();
        let runs = self.runs(Some(self.selected));
        let _ = page.framed().render_runs(area, buf, 0, &runs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_agent_shows_enabled_status() {
        let (app_event_tx_raw, _app_event_rx) = std::sync::mpsc::channel();
        let app_event_tx = AppEventSender::new(app_event_tx_raw);
        let view = AgentsOverviewView::new(
            vec![("code".to_string(), true, true, "coder".to_string())],
            Vec::new(),
            0,
            app_event_tx,
        );

        let combined = view
            .runs(None)
            .iter()
            .flat_map(|run| run.lines.iter())
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            combined.contains("enabled"),
            "expected status text to show 'enabled', got: {combined}"
        );
        assert!(!combined.contains("not installed"));
    }
}
