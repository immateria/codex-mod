use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::app_event::AppEvent;
use crate::bottom_pane::{BottomPaneView, ConditionalUpdate};

use super::model::{AgentsPane, AgentsSettingsContent};

use super::super::SettingsContent;

impl SettingsContent for AgentsSettingsContent {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        match &self.pane {
            AgentsPane::Overview(state) => {
                self.render_overview(area, buf, state);
            }
            AgentsPane::Subagent(view) => {
                view.render(area, buf);
            }
            AgentsPane::Agent(view) => {
                view.render(area, buf);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match &mut self.pane {
            AgentsPane::Overview(state) => Self::handle_overview_key(state, key, &self.app_event_tx),
            AgentsPane::Subagent(view) => view.handle_key_event_direct(key),
            AgentsPane::Agent(view) => view.handle_key_event_direct(key),
        }
    }

    fn is_complete(&self) -> bool {
        false
    }

    fn handle_paste(&mut self, text: String) -> bool {
        match &mut self.pane {
            AgentsPane::Agent(view) => {
                matches!(view.handle_paste(text), ConditionalUpdate::NeedsRedraw)
            }
            _ => false,
        }
    }

    fn handle_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match (&mut self.pane, mouse_event.kind) {
            (AgentsPane::Overview(state), MouseEventKind::Moved) => {
                let Some(next) = Self::overview_selection_at(state, area, mouse_event) else {
                    return false;
                };
                if state.selected == next {
                    return false;
                }
                state.selected = next;
                self.app_event_tx.send(AppEvent::AgentsOverviewSelectionChanged {
                    index: state.selected,
                });
                true
            }
            (AgentsPane::Overview(state), MouseEventKind::Down(MouseButton::Left)) => {
                let Some(next) = Self::overview_selection_at(state, area, mouse_event) else {
                    return false;
                };
                state.selected = next;
                self.app_event_tx.send(AppEvent::AgentsOverviewSelectionChanged {
                    index: state.selected,
                });
                Self::handle_overview_key(
                    state,
                    KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                    &self.app_event_tx,
                )
            }
            (AgentsPane::Overview(state), MouseEventKind::ScrollUp) => Self::handle_overview_key(
                state,
                KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
                &self.app_event_tx,
            ),
            (AgentsPane::Overview(state), MouseEventKind::ScrollDown) => Self::handle_overview_key(
                state,
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                &self.app_event_tx,
            ),
            _ => false,
        }
    }
}

