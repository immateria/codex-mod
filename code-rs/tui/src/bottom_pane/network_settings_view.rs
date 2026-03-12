use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};

use code_common::summarize_sandbox_policy;
use code_core::config::{NetworkModeToml, NetworkProxySettingsToml};
use code_core::protocol::SandboxPolicy;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;
use crate::ui_interaction::{
    redraw_if,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
    route_selectable_list_mouse_with_config,
};
use std::cell::Cell;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::editor_page::SettingsEditorPage;
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::row_page::SettingsRowPage;
use super::settings_ui::rows::{KeyValueRow, StyledText};
use super::settings_ui::toggle;
use super::BottomPane;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditTarget {
    AllowedDomains,
    DeniedDomains,
    AllowUnixSockets,
}

#[derive(Debug)]
enum ViewMode {
    Transition,
    Main { show_advanced: bool },
    EditList {
        target: EditTarget,
        field: FormTextField,
        show_advanced: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    Enabled,
    Mode,
    AllowedDomains,
    DeniedDomains,
    AllowLocalBinding,
    AdvancedToggle,
    Socks5Enabled,
    Socks5Udp,
    AllowUpstreamProxyEnv,
    AllowUnixSockets,
    Apply,
    Close,
}

pub(crate) struct NetworkSettingsView {
    settings: NetworkProxySettingsToml,
    sandbox_policy: SandboxPolicy,
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    is_complete: bool,
    dirty: bool,
    mode: ViewMode,
    state: ScrollState,
    viewport_rows: Cell<usize>,
}

pub(crate) struct NetworkSettingsViewFramed<'v> {
    view: &'v NetworkSettingsView,
}

pub(crate) struct NetworkSettingsViewContentOnly<'v> {
    view: &'v NetworkSettingsView,
}

pub(crate) struct NetworkSettingsViewFramedMut<'v> {
    view: &'v mut NetworkSettingsView,
}

pub(crate) struct NetworkSettingsViewContentOnlyMut<'v> {
    view: &'v mut NetworkSettingsView,
}

impl NetworkSettingsView {
    const DEFAULT_VISIBLE_ROWS: usize = 8;

    pub fn new(
        settings: Option<NetworkProxySettingsToml>,
        sandbox_policy: SandboxPolicy,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        Self {
            settings: settings.unwrap_or_default(),
            sandbox_policy,
            app_event_tx,
            ticket,
            is_complete: false,
            dirty: false,
            mode: ViewMode::Main { show_advanced: false },
            state,
            viewport_rows: Cell::new(0),
        }
    }

    fn mode_label(mode: NetworkModeToml) -> &'static str {
        match mode {
            NetworkModeToml::Limited => "limited",
            NetworkModeToml::Full => "full",
        }
    }

    fn build_rows(&self, show_advanced: bool) -> Vec<RowKind> {
        let mut rows = vec![
            RowKind::Enabled,
            RowKind::Mode,
            RowKind::AllowedDomains,
            RowKind::DeniedDomains,
            RowKind::AllowLocalBinding,
            RowKind::AdvancedToggle,
        ];

        if show_advanced {
            rows.push(RowKind::Socks5Enabled);
            if self.settings.enable_socks5 {
                rows.push(RowKind::Socks5Udp);
            }
            rows.push(RowKind::AllowUpstreamProxyEnv);
            #[cfg(target_os = "macos")]
            rows.push(RowKind::AllowUnixSockets);
        }

        rows.push(RowKind::Apply);
        rows.push(RowKind::Close);
        rows
    }

    fn render_header_lines(&self) -> Vec<Line<'static>> {
        let enabled = self.settings.enabled;
        let status_tag = if enabled { "ON" } else { "OFF" };
        let status_style = if enabled {
            Style::default()
                .fg(crate::colors::success())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(crate::colors::text_dim())
                .add_modifier(Modifier::BOLD)
        };
        let status_tail = if enabled {
            let mode = Self::mode_label(self.settings.mode);
            format!(": mediated (managed proxy) · mode {mode}")
        } else {
            let sandbox = Self::sandbox_policy_compact(&self.sandbox_policy);
            format!(": direct (sandbox policy decides) · sbx {sandbox}")
        };

        let enforcement = if !enabled {
            "Enforcement: n/a (mediation off)".to_string()
        } else if cfg!(target_os = "macos") {
            "Enforcement: fail-closed for sandboxed tools (seatbelt); best-effort otherwise"
                .to_string()
        } else if cfg!(windows) {
            "Enforcement: best-effort (no OS-level egress restriction yet)".to_string()
        } else {
            "Enforcement: best-effort (processes may ignore proxy env)".to_string()
        };

        vec![
            Line::from(vec![
                Span::styled(status_tag.to_string(), status_style),
                Span::styled(
                    status_tail,
                    Style::default().fg(crate::colors::text_dim()),
                ),
            ]),
            Line::from(Span::styled(
                "When ON: prompts only for allowlist misses. Deny/local/mode blocks require edits.",
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Coverage: exec, exec_command, web_fetch (browser partial)",
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(Span::styled(
                enforcement,
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Enter activate · Ctrl+S save lists · Esc close",
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(""),
        ]
    }

    fn sandbox_policy_compact(policy: &SandboxPolicy) -> String {
        let summary = summarize_sandbox_policy(policy);
        let base = summary.split_whitespace().next().unwrap_or("unknown");
        if summary.contains("(network access enabled)") {
            format!("{base} +net")
        } else {
            base.to_string()
        }
    }

    fn visible_budget(&self, total: usize) -> usize {
        if total == 0 {
            return 1;
        }
        let hint = self.viewport_rows.get();
        let target = if hint == 0 {
            Self::DEFAULT_VISIBLE_ROWS
        } else {
            hint
        };
        target.clamp(1, total)
    }

    fn reconcile_selection_state(&mut self, show_advanced: bool) {
        let total = self.build_rows(show_advanced).len();
        if total == 0 {
            self.state.reset();
            return;
        }
        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        self.state.scroll_top = self.state.scroll_top.min(total.saturating_sub(1));
        let visible_budget = self.visible_budget(total);
        self.state.ensure_visible(total, visible_budget);
    }

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
        self.app_event_tx.send(AppEvent::SetNetworkProxySettings(
            self.settings.clone(),
        ));
        self.app_event_tx.send_background_event_with_ticket(
            &self.ticket,
            "Network mediation: applying…".to_string(),
        );
        self.dirty = false;
    }

    fn activate_row(&mut self, kind: RowKind, show_advanced: &mut bool) {
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

    fn process_key_event_edit(&mut self, key_event: KeyEvent, show_advanced: bool) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.mode = ViewMode::Main { show_advanced };
                true
            }
            KeyEvent { code: KeyCode::Char('s'), modifiers, .. }
                if modifiers.contains(KeyModifiers::CONTROL) =>
            {
                let mode = std::mem::replace(&mut self.mode, ViewMode::Main { show_advanced });
                if let ViewMode::EditList { target, field, .. } = mode {
                    self.save_list_editor(target, &field);
                }
                self.mode = ViewMode::Main { show_advanced };
                true
            }
            _ => match &mut self.mode {
                ViewMode::EditList { field, .. } => field.handle_key(key_event),
                ViewMode::Main { .. } | ViewMode::Transition => false,
            },
        }
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main { mut show_advanced } => {
                let handled = self.process_key_event_main(key_event, &mut show_advanced);
                if matches!(self.mode, ViewMode::Transition) {
                    self.mode = ViewMode::Main { show_advanced };
                }
                handled
            }
            ViewMode::EditList { target, field, show_advanced } => {
                self.mode = ViewMode::EditList {
                    target,
                    field,
                    show_advanced,
                };
                self.process_key_event_edit(key_event, show_advanced)
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main { show_advanced: false };
                false
            }
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.process_key_event(key_event)
    }

    pub(crate) fn framed(&self) -> NetworkSettingsViewFramed<'_> {
        NetworkSettingsViewFramed { view: self }
    }

    pub(crate) fn content_only(&self) -> NetworkSettingsViewContentOnly<'_> {
        NetworkSettingsViewContentOnly { view: self }
    }

    pub(crate) fn framed_mut(&mut self) -> NetworkSettingsViewFramedMut<'_> {
        NetworkSettingsViewFramedMut { view: self }
    }

    pub(crate) fn content_only_mut(&mut self) -> NetworkSettingsViewContentOnlyMut<'_> {
        NetworkSettingsViewContentOnlyMut { view: self }
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

    fn handle_mouse_event_direct_content(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main { mut show_advanced } => {
                let rows = self.build_rows(show_advanced);
                let total = rows.len();
                if total == 0 {
                    self.mode = ViewMode::Main { show_advanced };
                    return false;
                }

                if self.state.selected_idx.is_none() {
                    self.state.selected_idx = Some(0);
                }
                self.state.clamp_selection(total);

                let mut selected = self.state.selected_idx.unwrap_or(0);
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    total,
                    |x, y| self.selection_index_at_content(area, x, y, show_advanced),
                    SelectableListMouseConfig {
                        hover_select: false,
                        require_pointer_hit_for_scroll: true,
                        scroll_behavior: ScrollSelectionBehavior::Clamp,
                        ..SelectableListMouseConfig::default()
                    },
                );
                self.state.selected_idx = Some(selected);

                if matches!(result, SelectableListMouseResult::Activated)
                    && let Some(kind) = rows.get(selected).copied()
                {
                    self.activate_row(kind, &mut show_advanced);
                }

                if result.handled() {
                    self.reconcile_selection_state(show_advanced);
                }
                if matches!(self.mode, ViewMode::Transition) {
                    self.mode = ViewMode::Main { show_advanced };
                }
                result.handled()
            }
            ViewMode::EditList { target, mut field, show_advanced } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let Some(field_area) =
                            Self::edit_page(target)
                                .content_only()
                                .layout(area)
                                .map(|layout| layout.field)
                        else {
                            return false;
                        };
                        field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                    }
                    MouseEventKind::ScrollDown => field.handle_mouse_scroll(true),
                    MouseEventKind::ScrollUp => field.handle_mouse_scroll(false),
                    _ => false,
                };
                self.mode = ViewMode::EditList {
                    target,
                    field,
                    show_advanced,
                };
                handled
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main { show_advanced: false };
                false
            }
        }
    }

    fn handle_mouse_event_direct_framed(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main { mut show_advanced } => {
                let rows = self.build_rows(show_advanced);
                let total = rows.len();
                if total == 0 {
                    self.mode = ViewMode::Main { show_advanced };
                    return false;
                }

                if self.state.selected_idx.is_none() {
                    self.state.selected_idx = Some(0);
                }
                self.state.clamp_selection(total);

                let mut selected = self.state.selected_idx.unwrap_or(0);
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    total,
                    |x, y| self.selection_index_at(area, x, y, show_advanced),
                    SelectableListMouseConfig {
                        hover_select: false,
                        require_pointer_hit_for_scroll: true,
                        scroll_behavior: ScrollSelectionBehavior::Clamp,
                        ..SelectableListMouseConfig::default()
                    },
                );
                self.state.selected_idx = Some(selected);

                if matches!(result, SelectableListMouseResult::Activated)
                    && let Some(kind) = rows.get(selected).copied()
                {
                    self.activate_row(kind, &mut show_advanced);
                }

                if result.handled() {
                    self.reconcile_selection_state(show_advanced);
                }
                if matches!(self.mode, ViewMode::Transition) {
                    self.mode = ViewMode::Main { show_advanced };
                }
                result.handled()
            }
            ViewMode::EditList {
                target,
                mut field,
                show_advanced,
            } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let Some(field_area) =
                            Self::edit_page(target)
                                .framed()
                                .layout(area)
                                .map(|layout| layout.field)
                        else {
                            return false;
                        };
                        field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                    }
                    MouseEventKind::ScrollDown => field.handle_mouse_scroll(true),
                    MouseEventKind::ScrollUp => field.handle_mouse_scroll(false),
                    _ => false,
                };
                self.mode = ViewMode::EditList {
                    target,
                    field,
                    show_advanced,
                };
                handled
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main { show_advanced: false };
                false
            }
        }
    }

    fn selection_index_at(
        &self,
        area: Rect,
        x: u16,
        y: u16,
        show_advanced: bool,
    ) -> Option<usize> {
        let page = SettingsRowPage::new(" Network ", self.render_header_lines(), vec![]);
        let layout = page.framed().layout(area)?;
        SettingsRowPage::selection_index_at(
            layout.body,
            x,
            y,
            self.state.scroll_top,
            self.build_rows(show_advanced).len(),
        )
    }

    fn selection_index_at_content(
        &self,
        area: Rect,
        x: u16,
        y: u16,
        show_advanced: bool,
    ) -> Option<usize> {
        let page = SettingsRowPage::new(" Network ", self.render_header_lines(), vec![]);
        let layout = page.content_only().layout(area)?;
        SettingsRowPage::selection_index_at(
            layout.body,
            x,
            y,
            self.state.scroll_top,
            self.build_rows(show_advanced).len(),
        )
    }

    fn edit_page(target: EditTarget) -> SettingsEditorPage<'static> {
        let (title, field_title) = match target {
            EditTarget::AllowedDomains => (" Network: Allowed Domains ", "Allowed domains"),
            EditTarget::DeniedDomains => (" Network: Denied Domains ", "Denied domains"),
            EditTarget::AllowUnixSockets => (" Network: Unix Sockets ", "Unix sockets"),
        };
        SettingsEditorPage::new(
            title,
            SettingsPanelStyle::bottom_pane(),
            field_title,
            vec![
                Line::from(vec![Span::styled(
                    "One entry per line. Ctrl+S to save. Esc to cancel.",
                    Style::default().fg(crate::colors::text_dim()),
                )]),
                Line::from(""),
            ],
            vec![],
        )
        .with_wrap_lines(true)
    }

    fn render_main(&self, area: Rect, buf: &mut Buffer, show_advanced: bool) {
        let rows = self.build_rows(show_advanced);
        let total = rows.len();
        let selected_idx = self
            .state
            .selected_idx
            .unwrap_or(0)
            .min(total.saturating_sub(1));
        let scroll_top = self.state.scroll_top.min(total.saturating_sub(1));

        let mode = Self::mode_label(self.settings.mode);

        let allowed_count = self.settings.allowed_domains.len();
        let allowed_summary = if allowed_count == 0 {
            "(none)".to_string()
        } else {
            format!("{allowed_count} entries")
        };

        let denied_count = self.settings.denied_domains.len();
        let denied_summary = if denied_count == 0 {
            "(none)".to_string()
        } else {
            format!("{denied_count} entries")
        };

        let advanced_label = if show_advanced { "shown" } else { "hidden" };

        let unix_count = self.settings.allow_unix_sockets.len();
        let unix_summary = if unix_count == 0 {
            "(none)".to_string()
        } else {
            format!("{unix_count} entries")
        };

        let apply_suffix = if self.dirty { " *" } else { "" };
        let mut enabled_status = toggle::enabled_word_warning_off(self.settings.enabled);
        enabled_status.style = enabled_status.style.bold();

        let row_specs: Vec<KeyValueRow<'_>> = rows
            .iter()
            .copied()
            .map(|kind| match kind {
                RowKind::Enabled => KeyValueRow::new("Enabled").with_value(enabled_status.clone()),
                RowKind::Mode => KeyValueRow::new("Mode").with_value(StyledText::new(
                    mode,
                    Style::default().fg(crate::colors::info()),
                )),
                RowKind::AllowedDomains => {
                    KeyValueRow::new("Allowed domains").with_value(StyledText::new(
                        allowed_summary.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ))
                }
                RowKind::DeniedDomains => {
                    KeyValueRow::new("Denied domains").with_value(StyledText::new(
                        denied_summary.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ))
                }
                RowKind::AllowLocalBinding => KeyValueRow::new("Allow local binding")
                    .with_value(toggle::on_off_word(self.settings.allow_local_binding)),
                RowKind::AdvancedToggle => KeyValueRow::new("Advanced").with_value(
                    StyledText::new(
                        advanced_label,
                        Style::default().fg(crate::colors::text_dim()),
                    ),
                ),
                RowKind::Socks5Enabled => KeyValueRow::new("SOCKS5").with_value(
                    toggle::on_off_word(self.settings.enable_socks5),
                ),
                RowKind::Socks5Udp => KeyValueRow::new("SOCKS5 UDP").with_value(
                    toggle::on_off_word(self.settings.enable_socks5_udp),
                ),
                RowKind::AllowUpstreamProxyEnv => KeyValueRow::new("Allow upstream proxy env")
                    .with_value(toggle::on_off_word(self.settings.allow_upstream_proxy)),
                RowKind::AllowUnixSockets => {
                    KeyValueRow::new("Allow unix sockets").with_value(StyledText::new(
                        unix_summary.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ))
                }
                RowKind::Apply => KeyValueRow::new("Apply changes").with_value(StyledText::new(
                    apply_suffix,
                    Style::default().fg(crate::colors::warning()),
                )),
                RowKind::Close => KeyValueRow::new("Close"),
            })
            .collect();
        let Some(layout) = SettingsRowPage::new(" Network ", self.render_header_lines(), vec![])
            .framed()
            .render(
            area,
            buf,
            scroll_top,
            Some(selected_idx),
            &row_specs,
        ) else {
            return;
        };
        self.viewport_rows.set(layout.visible_rows());
    }

    fn render_main_without_frame(&self, area: Rect, buf: &mut Buffer, show_advanced: bool) {
        let rows = self.build_rows(show_advanced);
        let total = rows.len();
        let selected_idx = self
            .state
            .selected_idx
            .unwrap_or(0)
            .min(total.saturating_sub(1));
        let scroll_top = self.state.scroll_top.min(total.saturating_sub(1));

        let mode = Self::mode_label(self.settings.mode);

        let allowed_count = self.settings.allowed_domains.len();
        let allowed_summary = if allowed_count == 0 {
            "(none)".to_string()
        } else {
            format!("{allowed_count} entries")
        };

        let denied_count = self.settings.denied_domains.len();
        let denied_summary = if denied_count == 0 {
            "(none)".to_string()
        } else {
            format!("{denied_count} entries")
        };

        let advanced_label = if show_advanced { "shown" } else { "hidden" };

        let unix_count = self.settings.allow_unix_sockets.len();
        let unix_summary = if unix_count == 0 {
            "(none)".to_string()
        } else {
            format!("{unix_count} entries")
        };

        let apply_suffix = if self.dirty { " *" } else { "" };
        let mut enabled_status = toggle::enabled_word_warning_off(self.settings.enabled);
        enabled_status.style = enabled_status.style.bold();

        let row_specs: Vec<KeyValueRow<'_>> = rows
            .iter()
            .copied()
            .map(|kind| match kind {
                RowKind::Enabled => KeyValueRow::new("Enabled").with_value(enabled_status.clone()),
                RowKind::Mode => KeyValueRow::new("Mode").with_value(StyledText::new(
                    mode,
                    Style::default().fg(crate::colors::info()),
                )),
                RowKind::AllowedDomains => {
                    KeyValueRow::new("Allowed domains").with_value(StyledText::new(
                        allowed_summary.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ))
                }
                RowKind::DeniedDomains => {
                    KeyValueRow::new("Denied domains").with_value(StyledText::new(
                        denied_summary.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ))
                }
                RowKind::AllowLocalBinding => KeyValueRow::new("Allow local binding")
                    .with_value(toggle::on_off_word(self.settings.allow_local_binding)),
                RowKind::AdvancedToggle => KeyValueRow::new("Advanced").with_value(
                    StyledText::new(
                        advanced_label,
                        Style::default().fg(crate::colors::text_dim()),
                    ),
                ),
                RowKind::Socks5Enabled => KeyValueRow::new("SOCKS5").with_value(
                    toggle::on_off_word(self.settings.enable_socks5),
                ),
                RowKind::Socks5Udp => KeyValueRow::new("SOCKS5 UDP").with_value(
                    toggle::on_off_word(self.settings.enable_socks5_udp),
                ),
                RowKind::AllowUpstreamProxyEnv => KeyValueRow::new("Allow upstream proxy env")
                    .with_value(toggle::on_off_word(self.settings.allow_upstream_proxy)),
                RowKind::AllowUnixSockets => {
                    KeyValueRow::new("Allow unix sockets").with_value(StyledText::new(
                        unix_summary.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ))
                }
                RowKind::Apply => KeyValueRow::new("Apply changes").with_value(StyledText::new(
                    apply_suffix,
                    Style::default().fg(crate::colors::warning()),
                )),
                RowKind::Close => KeyValueRow::new("Close"),
            })
            .collect();
        let Some(layout) = SettingsRowPage::new(" Network ", self.render_header_lines(), vec![])
            .content_only()
            .render(area, buf, scroll_top, Some(selected_idx), &row_specs)
        else {
            return;
        };
        self.viewport_rows.set(layout.visible_rows());
    }

    fn render_edit(&self, area: Rect, buf: &mut Buffer, target: EditTarget, field: &FormTextField) {
        let _ = Self::edit_page(target).framed().render(area, buf, field);
    }

    fn render_edit_without_frame(
        &self,
        area: Rect,
        buf: &mut Buffer,
        target: EditTarget,
        field: &FormTextField,
    ) {
        let _ = Self::edit_page(target)
            .content_only()
            .render(area, buf, field);
    }

    fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main { show_advanced } => {
                self.render_main_without_frame(area, buf, *show_advanced);
            }
            ViewMode::EditList { target, field, .. } => {
                self.render_edit_without_frame(area, buf, *target, field);
            }
            ViewMode::Transition => self.render_main_without_frame(area, buf, false),
        }
    }

    fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main { show_advanced } => self.render_main(area, buf, *show_advanced),
            ViewMode::EditList { target, field, .. } => self.render_edit(area, buf, *target, field),
            ViewMode::Transition => self.render_main(area, buf, false),
        }
    }
}

impl<'v> NetworkSettingsViewFramed<'v> {
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render_framed(area, buf);
    }
}

impl<'v> NetworkSettingsViewContentOnly<'v> {
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render_content_only(area, buf);
    }
}

impl<'v> NetworkSettingsViewFramedMut<'v> {
    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.view.handle_mouse_event_direct_framed(mouse_event, area)
    }
}

impl<'v> NetworkSettingsViewContentOnlyMut<'v> {
    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.view.handle_mouse_event_direct_content(mouse_event, area)
    }
}

impl<'a> BottomPaneView<'a> for NetworkSettingsView {
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
        redraw_if(
            self.framed_mut()
                .handle_mouse_event_direct(mouse_event, area),
        )
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        redraw_if(self.handle_paste_direct(text))
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        match &self.mode {
            ViewMode::Main { show_advanced } => {
                let header = self.render_header_lines().len() as u16;
                let total_rows = self.build_rows(*show_advanced).len();
                let visible = total_rows.clamp(1, 12) as u16;
                2 + header + visible
            }
            ViewMode::EditList { .. } => 18,
            ViewMode::Transition => 2 + self.render_header_lines().len() as u16 + 8,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.framed().render(area, buf);
    }
}
