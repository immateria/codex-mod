use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use code_core::config::{NetworkModeToml, NetworkProxySettingsToml};

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
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    is_complete: bool,
    dirty: bool,
    mode: ViewMode,
    state: ScrollState,
    viewport_rows: Cell<usize>,
}

impl NetworkSettingsView {
    const DEFAULT_VISIBLE_ROWS: usize = 8;

    pub fn new(
        settings: Option<NetworkProxySettingsToml>,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        Self {
            settings: settings.unwrap_or_default(),
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
        vec![
            Line::from(Span::styled(
                "Network mediation (managed proxy).",
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Allow/deny apply first · Enter activate · Esc close",
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(""),
        ]
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

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        match &mut self.mode {
            ViewMode::EditList { field, .. } => {
                field.handle_paste(text);
                true
            }
            ViewMode::Main { .. } | ViewMode::Transition => false,
        }
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
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
                    |x, y| self.selection_index_at(area, x, y),
                    SelectableListMouseConfig {
                        hover_select: true,
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
                        let inner = Rect {
                            x: area.x.saturating_add(1),
                            y: area.y.saturating_add(1),
                            width: area.width.saturating_sub(2),
                            height: area.height.saturating_sub(2),
                        };
                        let textarea = Rect {
                            x: inner.x,
                            y: inner.y.saturating_add(2),
                            width: inner.width,
                            height: inner.height.saturating_sub(2),
                        };
                        field.handle_mouse_click(mouse_event.column, mouse_event.row, textarea)
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

    fn selection_index_at(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        let inner = Block::default().borders(Borders::ALL).inner(area);
        if inner.width == 0 || inner.height == 0 {
            return None;
        }
        if x < inner.x
            || x >= inner.x.saturating_add(inner.width)
            || y < inner.y
            || y >= inner.y.saturating_add(inner.height)
        {
            return None;
        }

        let header_lines = self.render_header_lines();
        let available_height = inner.height as usize;
        let header_height = header_lines.len().min(available_height);
        let list_height = available_height.saturating_sub(header_height);
        if list_height == 0 {
            return None;
        }

        let rel_y = y.saturating_sub(inner.y) as usize;
        if rel_y < header_height || rel_y >= header_height + list_height {
            return None;
        }
        let line_offset = rel_y - header_height;

        let scroll_top = self.state.scroll_top;
        Some(scroll_top.saturating_add(line_offset))
    }

    fn render_main(&self, area: Rect, buf: &mut Buffer, show_advanced: bool) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Network ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let row_style = |selected: bool| {
            if selected {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            }
        };

        let arrow_style = |selected: bool| {
            if selected {
                Style::default().fg(crate::colors::primary())
            } else {
                Style::default().fg(crate::colors::text_dim())
            }
        };

        let header_lines = self.render_header_lines();
        let available_height = inner.height as usize;
        let header_height = header_lines.len().min(available_height);
        let list_height = available_height.saturating_sub(header_height);
        let visible_slots = list_height.max(1);
        self.viewport_rows.set(visible_slots);

        let rows = self.build_rows(show_advanced);
        let total = rows.len();
        let selected_idx = self
            .state
            .selected_idx
            .unwrap_or(0)
            .min(total.saturating_sub(1));
        let scroll_top = self.state.scroll_top.min(total.saturating_sub(1));

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.extend(header_lines);

        let enabled_label = if self.settings.enabled { "Enabled" } else { "Disabled" };
        let enabled_color = if self.settings.enabled {
            crate::colors::success()
        } else {
            crate::colors::warning()
        };
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

        let local_label = if self.settings.allow_local_binding { "On" } else { "Off" };
        let advanced_label = if show_advanced { "shown" } else { "hidden" };

        let socks_label = if self.settings.enable_socks5 { "On" } else { "Off" };
        let socks_udp_label = if self.settings.enable_socks5_udp { "On" } else { "Off" };
        let upstream_env_label = if self.settings.allow_upstream_proxy { "On" } else { "Off" };

        let unix_count = self.settings.allow_unix_sockets.len();
        let unix_summary = if unix_count == 0 {
            "(none)".to_string()
        } else {
            format!("{unix_count} entries")
        };

        let apply_suffix = if self.dirty { " *" } else { "" };

        let mut remaining = visible_slots;
        let mut row_index = scroll_top;
        while remaining > 0 && row_index < rows.len() {
            let kind = rows[row_index];
            let selected = row_index == selected_idx;
            let arrow = if selected { "› " } else { "  " };
            let mut spans = vec![Span::styled(arrow, arrow_style(selected))];
            match kind {
                RowKind::Enabled => {
                    spans.push(Span::raw("Enabled: "));
                    spans.push(Span::styled(
                        enabled_label,
                        Style::default()
                            .fg(enabled_color)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                RowKind::Mode => {
                    spans.push(Span::raw("Mode: "));
                    spans.push(Span::styled(mode, Style::default().fg(crate::colors::info())));
                }
                RowKind::AllowedDomains => {
                    spans.push(Span::raw("Allowed domains: "));
                    spans.push(Span::styled(
                        allowed_summary.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                RowKind::DeniedDomains => {
                    spans.push(Span::raw("Denied domains: "));
                    spans.push(Span::styled(
                        denied_summary.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                RowKind::AllowLocalBinding => {
                    spans.push(Span::raw("Allow local binding: "));
                    spans.push(Span::styled(
                        local_label,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                RowKind::AdvancedToggle => {
                    spans.push(Span::raw("Advanced: "));
                    spans.push(Span::styled(
                        advanced_label,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                RowKind::Socks5Enabled => {
                    spans.push(Span::raw("SOCKS5: "));
                    spans.push(Span::styled(
                        socks_label,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                RowKind::Socks5Udp => {
                    spans.push(Span::raw("SOCKS5 UDP: "));
                    spans.push(Span::styled(
                        socks_udp_label,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                RowKind::AllowUpstreamProxyEnv => {
                    spans.push(Span::raw("Allow upstream proxy env: "));
                    spans.push(Span::styled(
                        upstream_env_label,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                RowKind::AllowUnixSockets => {
                    spans.push(Span::raw("Allow unix sockets: "));
                    spans.push(Span::styled(
                        unix_summary.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                RowKind::Apply => {
                    spans.push(Span::raw("Apply changes"));
                    spans.push(Span::styled(
                        apply_suffix,
                        Style::default().fg(crate::colors::warning()),
                    ));
                }
                RowKind::Close => {
                    spans.push(Span::raw("Close"));
                }
            }
            lines.push(Line::from(spans).style(row_style(selected)));
            remaining = remaining.saturating_sub(1);
            row_index += 1;
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }

    fn render_edit(&self, area: Rect, buf: &mut Buffer, target: EditTarget, field: &FormTextField) {
        Clear.render(area, buf);
        let title = match target {
            EditTarget::AllowedDomains => " Network: Allowed Domains ",
            EditTarget::DeniedDomains => " Network: Denied Domains ",
            EditTarget::AllowUnixSockets => " Network: Unix Sockets ",
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(title)
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let header = vec![
            Line::from(vec![Span::styled(
                "One entry per line. Ctrl+S to save. Esc to cancel.",
                Style::default().fg(crate::colors::text_dim()),
            )]),
            Line::from(""),
        ];
        Paragraph::new(header)
            .wrap(Wrap { trim: false })
            .render(inner, buf);

        let textarea = Rect {
            x: inner.x,
            y: inner.y.saturating_add(2),
            width: inner.width,
            height: inner.height.saturating_sub(2),
        };
        if textarea.width == 0 || textarea.height == 0 {
            return;
        }
        field.render(textarea, buf, true);
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
        redraw_if(self.handle_mouse_event_direct(mouse_event, area))
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
        match &self.mode {
            ViewMode::Main { show_advanced } => self.render_main(area, buf, *show_advanced),
            ViewMode::EditList { target, field, .. } => self.render_edit(area, buf, *target, field),
            ViewMode::Transition => self.render_main(area, buf, false),
        }
    }
}
