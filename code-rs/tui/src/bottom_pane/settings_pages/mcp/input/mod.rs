mod hover;
mod mouse;
mod scrollbar;
mod stack_scroll;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders};

use super::{McpSettingsFocus, McpSettingsMode, McpSettingsView, SUMMARY_HORIZONTAL_SCROLL_STEP};
use crate::bottom_pane::ChromeMode;

impl McpSettingsView {
    pub(super) fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        if !matches!(self.mode, McpSettingsMode::Main) {
            return self.handle_policy_editor_key(key_event);
        }

        self.clear_server_row_click_arm();
        self.clear_list_hover();
        let handled = match key_event {
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.cycle_focus(false);
                true
            }
            KeyEvent {
                code: KeyCode::BackTab,
                ..
            } => {
                self.cycle_focus(true);
                true
            }
            KeyEvent { code: KeyCode::Up, .. } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.move_selection_up(),
                    McpSettingsFocus::Summary => self.scroll_summary_lines(-1),
                    McpSettingsFocus::Tools => {
                        let len = self.tool_entries().len();
                        if len > 0 {
                            if self.tools_selected == 0 {
                                self.tools_selected = len - 1;
                            } else {
                                self.tools_selected -= 1;
                            }
                        }
                    }
                }
                true
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.move_selection_down(),
                    McpSettingsFocus::Summary => self.scroll_summary_lines(1),
                    McpSettingsFocus::Tools => {
                        let len = self.tool_entries().len();
                        if len > 0 {
                            self.tools_selected = (self.tools_selected + 1) % len;
                        }
                    }
                }
                true
            }
            KeyEvent {
                code: KeyCode::Left,
                ..
            } => {
                if self.focus == McpSettingsFocus::Servers {
                    self.on_toggle_server();
                } else if self.focus == McpSettingsFocus::Tools {
                    self.set_expanded_tool_for_selected_server(None);
                } else if self.focus == McpSettingsFocus::Summary && !self.summary_wrap {
                    self.shift_summary_hscroll(-SUMMARY_HORIZONTAL_SCROLL_STEP);
                }
                true
            }
            KeyEvent {
                code: KeyCode::Right,
                ..
            } => {
                if self.focus == McpSettingsFocus::Servers {
                    self.on_toggle_server();
                } else if self.focus == McpSettingsFocus::Tools {
                    self.toggle_selected_tool_expansion();
                } else if self.focus == McpSettingsFocus::Summary && !self.summary_wrap {
                    self.shift_summary_hscroll(SUMMARY_HORIZONTAL_SCROLL_STEP);
                }
                true
            }
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.on_toggle_server(),
                    McpSettingsFocus::Tools => self.toggle_selected_tool(),
                    McpSettingsFocus::Summary => {}
                }
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.on_enter_server_selection(),
                    McpSettingsFocus::Tools => self.toggle_selected_tool_expansion(),
                    McpSettingsFocus::Summary => {}
                }
                true
            }
            KeyEvent {
                code: KeyCode::PageUp,
                ..
            } => {
                self.page_summary(-1);
                true
            }
            KeyEvent {
                code: KeyCode::PageDown,
                ..
            } => {
                self.page_summary(1);
                true
            }
            KeyEvent {
                code: KeyCode::Home,
                ..
            } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.set_selected(0),
                    McpSettingsFocus::Tools => self.tools_selected = 0,
                    McpSettingsFocus::Summary => self.summary_scroll_top = 0,
                }
                true
            }
            KeyEvent {
                code: KeyCode::End, ..
            } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.set_selected(self.len().saturating_sub(1)),
                    McpSettingsFocus::Tools => {
                        let len = self.tool_entries().len();
                        self.tools_selected = len.saturating_sub(1);
                    }
                    McpSettingsFocus::Summary => self.summary_scroll_top = usize::MAX,
                }
                true
            }
            KeyEvent {
                code: KeyCode::Char('r' | 'R'),
                modifiers,
                ..
            } if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                self.request_refresh();
                true
            }
            KeyEvent {
                code: KeyCode::Char('s' | 'S'),
                modifiers,
                ..
            } if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                self.queue_status_report();
                true
            }
            KeyEvent {
                code: KeyCode::Char('w' | 'W'),
                modifiers,
                ..
            } if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                self.toggle_summary_wrap_mode();
                true
            }
            KeyEvent {
                code: KeyCode::Char('e' | 'E'),
                modifiers,
                ..
            } if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                self.open_scheduling_editor_from_focus()
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.is_complete = true;
                true
            }
            _ => false,
        };

        if handled {
            self.ensure_stacked_focus_visible_from_last_render();
        }

        handled
    }

    pub(super) fn content_rect(area: Rect) -> Rect {
        let inner = Block::default().borders(Borders::ALL).inner(area);
        Rect {
            x: inner.x.saturating_add(1),
            y: inner.y,
            width: inner.width.saturating_sub(2),
            height: inner.height,
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.process_key_event(key_event)
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_impl(mouse_event, area, ChromeMode::Framed)
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_impl(mouse_event, area, ChromeMode::ContentOnly)
    }
}

