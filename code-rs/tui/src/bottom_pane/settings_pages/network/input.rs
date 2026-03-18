use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use code_core::config::NetworkModeToml;

use crate::app_event::AppEvent;
use crate::components::mode_guard::ModeGuard;

impl NetworkSettingsView {
    fn toggle_enabled(&mut self) {
        self.settings.enabled = !self.settings.enabled;
        self.dirty = true;
    }

    fn toggle_allow_local_binding(&mut self) {
        self.settings.allow_local_binding = !self.settings.allow_local_binding;
        self.dirty = true;
    }

    fn toggle_socks5_enabled(&mut self) {
        self.settings.enable_socks5 = !self.settings.enable_socks5;
        if !self.settings.enable_socks5 {
            self.settings.enable_socks5_udp = false;
        }
        self.dirty = true;
    }

    fn toggle_socks5_udp(&mut self) {
        if !self.settings.enable_socks5 {
            return;
        }
        self.settings.enable_socks5_udp = !self.settings.enable_socks5_udp;
        self.dirty = true;
    }

    fn toggle_allow_upstream_proxy_env(&mut self) {
        self.settings.allow_upstream_proxy = !self.settings.allow_upstream_proxy;
        self.dirty = true;
    }

    fn next_mode(&mut self) {
        self.settings.mode = match self.settings.mode {
            NetworkModeToml::Limited => NetworkModeToml::Full,
            NetworkModeToml::Full => NetworkModeToml::Limited,
        };
        self.dirty = true;
    }

    fn prev_mode(&mut self) {
        self.next_mode();
    }

    fn open_list_editor(&mut self, target: EditTarget, show_advanced: bool) {
        let mut field = FormTextField::new_multi_line();
        field.set_placeholder("One entry per line…");
        let text = match target {
            EditTarget::AllowedDomains => self.settings.allowed_domains.join("\n"),
            EditTarget::DeniedDomains => self.settings.denied_domains.join("\n"),
            EditTarget::AllowUnixSockets => self.settings.allow_unix_sockets.join("\n"),
        };
        field.set_text(&text);
        field.move_cursor_to_start();
        self.mode = ViewMode::EditList {
            target,
            field,
            show_advanced,
        };
    }

    fn save_list_editor(&mut self, target: EditTarget, field: &FormTextField) {
        match target {
            EditTarget::AllowedDomains => {
                let mut values: Vec<String> = field
                    .text()
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect();
                values.sort_by_key(|value| value.to_ascii_lowercase());
                values.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
                self.settings.allowed_domains = values;
            }
            EditTarget::DeniedDomains => {
                let mut values: Vec<String> = field
                    .text()
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect();
                values.sort_by_key(|value| value.to_ascii_lowercase());
                values.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
                self.settings.denied_domains = values;
            }
            EditTarget::AllowUnixSockets => {
                // Preserve user order, but dedupe exact paths (do not case-fold).
                let mut seen = std::collections::HashSet::<String>::new();
                let mut values = Vec::new();
                for line in field.text().lines().map(str::trim).filter(|line| !line.is_empty()) {
                    let value = line.to_string();
                    if seen.insert(value.clone()) {
                        values.push(value);
                    }
                }
                self.settings.allow_unix_sockets = values;
            }
        }
        self.dirty = true;
    }

    fn apply_settings(&mut self) {
        self.app_event_tx
            .send(AppEvent::SetNetworkProxySettings(self.settings.clone()));
        self.app_event_tx.send_background_event_with_ticket(
            &self.ticket,
            "Network mediation: applying…".to_string(),
        );
        self.dirty = false;
    }

    pub(super) fn activate_row(&mut self, kind: RowKind, show_advanced: &mut bool) {
        match kind {
            RowKind::Enabled => self.toggle_enabled(),
            RowKind::Mode => self.next_mode(),
            RowKind::AllowedDomains => self.open_list_editor(EditTarget::AllowedDomains, *show_advanced),
            RowKind::DeniedDomains => self.open_list_editor(EditTarget::DeniedDomains, *show_advanced),
            RowKind::AllowLocalBinding => self.toggle_allow_local_binding(),
            RowKind::AdvancedToggle => {
                *show_advanced = !*show_advanced;
            }
            RowKind::Socks5Enabled => self.toggle_socks5_enabled(),
            RowKind::Socks5Udp => self.toggle_socks5_udp(),
            RowKind::AllowUpstreamProxyEnv => self.toggle_allow_upstream_proxy_env(),
            RowKind::AllowUnixSockets => self.open_list_editor(EditTarget::AllowUnixSockets, *show_advanced),
            RowKind::Apply => self.apply_settings(),
            RowKind::Close => self.is_complete = true,
        }
    }

    fn process_key_event_main(&mut self, key_event: KeyEvent, show_advanced: &mut bool) -> bool {
        let rows = self.build_rows(*show_advanced);
        let total = rows.len();
        if total == 0 {
            if matches!(key_event.code, KeyCode::Esc) {
                self.is_complete = true;
                return true;
            }
            return false;
        }

        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        self.state.scroll_top = self.state.scroll_top.min(total.saturating_sub(1));
        let visible_budget = self.visible_budget(total);
        self.state.ensure_visible(total, visible_budget);

        let selected = self.state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
        let current_row = rows.get(selected).copied();

        let handled = match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                self.state.move_up_wrap(total);
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                self.state.move_down_wrap(total);
                true
            }
            KeyEvent { code: KeyCode::Left, modifiers: KeyModifiers::NONE, .. } => {
                if let Some(kind) = current_row {
                    match kind {
                        RowKind::Enabled => self.toggle_enabled(),
                        RowKind::Mode => self.prev_mode(),
                        RowKind::AllowLocalBinding => self.toggle_allow_local_binding(),
                        RowKind::AdvancedToggle => *show_advanced = !*show_advanced,
                        RowKind::Socks5Enabled => self.toggle_socks5_enabled(),
                        RowKind::Socks5Udp => self.toggle_socks5_udp(),
                        RowKind::AllowUpstreamProxyEnv => self.toggle_allow_upstream_proxy_env(),
                        _ => {}
                    }
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE, .. } => {
                if let Some(kind) = current_row {
                    match kind {
                        RowKind::Enabled => self.toggle_enabled(),
                        RowKind::Mode => self.next_mode(),
                        RowKind::AllowLocalBinding => self.toggle_allow_local_binding(),
                        RowKind::AdvancedToggle => *show_advanced = !*show_advanced,
                        RowKind::Socks5Enabled => self.toggle_socks5_enabled(),
                        RowKind::Socks5Udp => self.toggle_socks5_udp(),
                        RowKind::AllowUpstreamProxyEnv => self.toggle_allow_upstream_proxy_env(),
                        _ => {}
                    }
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
            | KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                if let Some(kind) = current_row {
                    self.activate_row(kind, show_advanced);
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
        };

        self.reconcile_selection_state(*show_advanced);
        handled
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mut mode_guard = ModeGuard::replace(&mut self.mode, ViewMode::Transition, |mode| {
            matches!(mode, ViewMode::Transition)
        });

        match mode_guard.mode_mut() {
            ViewMode::Main { show_advanced } => self.process_key_event_main(key_event, show_advanced),
            ViewMode::EditList {
                target,
                field,
                show_advanced,
            } => match key_event {
                KeyEvent { code: KeyCode::Esc, .. } => {
                    let show_advanced = *show_advanced;
                    self.mode = ViewMode::Main { show_advanced };
                    true
                }
                KeyEvent {
                    code: KeyCode::Char('s'),
                    modifiers,
                    ..
                } if modifiers.contains(KeyModifiers::CONTROL) => {
                    let show_advanced = *show_advanced;
                    let target = *target;
                    self.save_list_editor(target, field);
                    self.mode = ViewMode::Main { show_advanced };
                    true
                }
                _ => field.handle_key(key_event),
            },
            ViewMode::Transition => {
                self.mode = ViewMode::Main {
                    show_advanced: false,
                };
                false
            }
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.process_key_event(key_event)
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        match &mut self.mode {
            ViewMode::EditList { field, .. } => {
                field.handle_paste(text);
                true
            }
            ViewMode::Main { .. } | ViewMode::Transition => false,
        }
    }
}
