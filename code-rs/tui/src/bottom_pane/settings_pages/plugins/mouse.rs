use super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::hints::KeyHint;
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test;
use crate::colors;
use crate::ui_interaction::{ScrollSelectionBehavior, SelectableListMouseConfig, SelectableListMouseResult};

impl PluginsSettingsView {
    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        if self.is_complete || area.width == 0 || area.height == 0 {
            return false;
        }
        self.handle_mouse_event_direct_in_chrome(ChromeMode::ContentOnly, mouse_event, area)
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::Framed, mouse_event, area)
    }

    fn handle_mouse_event_direct_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        match &self.mode {
            Mode::List => self.handle_list_mouse_event_in_chrome(chrome, mouse_event, area),
            Mode::Detail { key } => self.handle_detail_mouse_event_in_chrome(chrome, mouse_event, area, key.clone()),
            Mode::ConfirmUninstall { plugin_id_key, key } => self.handle_confirm_mouse_event_in_chrome(
                chrome,
                mouse_event,
                area,
                plugin_id_key.clone(),
                key.clone(),
            ),
        }
    }

    fn handle_list_mouse_event_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let snapshot = self.shared_snapshot();
        let rows = self.list_rows(&snapshot);
        let Some(layout) = self.list_page(&snapshot).layout_in_chrome(chrome, area) else {
            return false;
        };

        let visible_rows = layout.body.height.max(1) as usize;
        self.list_viewport_rows.set(visible_rows);

        let mut state = self.list_state.get();
        let outcome = route_scroll_state_mouse_with_hit_test(
            mouse_event,
            &mut state,
            rows.len(),
            visible_rows,
            |x, y, scroll_top| {
                SettingsMenuPage::selection_menu_id_in_body(
                    layout.body,
                    x,
                    y,
                    scroll_top,
                    &rows,
                )
            },
            SelectableListMouseConfig {
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );
        self.list_state.set(state);

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            let plugin_rows = Self::plugin_rows_from_snapshot(&snapshot);
            if let Some(selected) = self.selected_plugin_row(&plugin_rows) {
                let key = PluginDetailKey::new(selected.marketplace_path, selected.plugin_name);
                self.mode = Mode::Detail { key: key.clone() };
                self.focused_detail_button = DetailAction::Back;
                self.hovered_detail_button = None;
                self.request_plugin_detail(PluginReadRequest {
                    plugin_name: key.plugin_name.clone(),
                    marketplace_path: key.marketplace_path.clone(),
                });
                return true;
            }
        }

        outcome.changed
    }

    fn handle_detail_mouse_event_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
        key: PluginDetailKey,
    ) -> bool {
        let snapshot = self.shared_snapshot();
        let title = key.plugin_name.clone();
        let status = snapshot
            .action_error
            .as_ref()
            .map(|err| StyledText::new(err.clone(), Style::new().fg(colors::error())));
        let shortcuts = [
            KeyHint::new("←→", " actions").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter", " activate").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " back").with_key_style(Style::new().fg(colors::error())),
        ];

        let page = self.detail_page(&snapshot, &title, status, &shortcuts);
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return false;
        };

        let (installed, enabled) = snapshot
            .details
            .get(&key)
            .and_then(|state| match state {
                crate::chatwidget::PluginsDetailState::Ready(outcome) => {
                    Some((outcome.plugin.installed, outcome.plugin.enabled))
                }
                _ => None,
            })
            .unwrap_or((false, false));
        let buttons = self.detail_button_specs(installed, enabled);

        match mouse_event.kind {
            MouseEventKind::Moved => {
                let hovered = page.standard_action_at_end(
                    &layout,
                    mouse_event.column,
                    mouse_event.row,
                    &buttons,
                );
                if hovered == self.hovered_detail_button {
                    return false;
                }
                self.hovered_detail_button = hovered;
                true
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(action) = page.standard_action_at_end(
                    &layout,
                    mouse_event.column,
                    mouse_event.row,
                    &buttons,
                ) {
                    self.focused_detail_button = action;
                    return self.activate_detail_action(key);
                }
                false
            }
            _ => false,
        }
    }

    fn handle_confirm_mouse_event_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
        plugin_id_key: String,
        key: PluginDetailKey,
    ) -> bool {
        let snapshot = self.shared_snapshot();
        let status = snapshot
            .action_error
            .as_ref()
            .map(|err| StyledText::new(err.clone(), Style::new().fg(colors::error())));
        let shortcuts = [
            KeyHint::new("←→", " actions").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter", " activate").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " back").with_key_style(Style::new().fg(colors::error())),
        ];
        let page = self.detail_page(
            &snapshot,
            "Confirm uninstall",
            status,
            &shortcuts,
        );
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return false;
        };
        let buttons = self.confirm_button_specs();

        match mouse_event.kind {
            MouseEventKind::Moved => {
                let hovered = page.standard_action_at_end(
                    &layout,
                    mouse_event.column,
                    mouse_event.row,
                    &buttons,
                );
                if hovered == self.hovered_confirm_button {
                    return false;
                }
                self.hovered_confirm_button = hovered;
                true
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(action) = page.standard_action_at_end(
                    &layout,
                    mouse_event.column,
                    mouse_event.row,
                    &buttons,
                ) {
                    self.focused_confirm_button = action;
                    match action {
                        ConfirmAction::Cancel => {
                            self.mode = Mode::Detail { key };
                        }
                        ConfirmAction::Uninstall => {
                            self.mode = Mode::List;
                            self.request_uninstall_plugin(plugin_id_key, /*force_remote_sync*/ false);
                        }
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}
