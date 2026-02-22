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
use crate::ui_interaction::{
    RelativeHitRegion,
    redraw_if,
    route_selectable_regions_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
    wrap_next,
    wrap_prev,
};

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::BottomPane;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditTarget {
    AllowedDomains,
    DeniedDomains,
}

#[derive(Debug)]
enum ViewMode {
    Main,
    EditList { target: EditTarget, field: FormTextField },
}

pub(crate) struct NetworkSettingsView {
    settings: NetworkProxySettingsToml,
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    selected_row: usize,
    is_complete: bool,
    dirty: bool,
    mode: ViewMode,
}

impl NetworkSettingsView {
    const SELECTABLE_ROWS: usize = 7;
    const HIT_REGIONS: [RelativeHitRegion; Self::SELECTABLE_ROWS] = [
        // Enabled
        RelativeHitRegion::new(0, 4, 1),
        // Mode
        RelativeHitRegion::new(1, 5, 1),
        // Allowed domains
        RelativeHitRegion::new(2, 6, 1),
        // Denied domains
        RelativeHitRegion::new(3, 7, 1),
        // Allow local binding
        RelativeHitRegion::new(4, 8, 1),
        // Apply
        RelativeHitRegion::new(5, 10, 1),
        // Close
        RelativeHitRegion::new(6, 11, 1),
    ];

    pub fn new(
        settings: Option<NetworkProxySettingsToml>,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) -> Self {
        Self {
            settings: settings.unwrap_or_default(),
            app_event_tx,
            ticket,
            selected_row: 0,
            is_complete: false,
            dirty: false,
            mode: ViewMode::Main,
        }
    }

    fn mode_label(mode: NetworkModeToml) -> &'static str {
        match mode {
            NetworkModeToml::Limited => "limited",
            NetworkModeToml::Full => "full",
        }
    }

    fn toggle_enabled(&mut self) {
        self.settings.enabled = !self.settings.enabled;
        self.dirty = true;
    }

    fn toggle_allow_local_binding(&mut self) {
        self.settings.allow_local_binding = !self.settings.allow_local_binding;
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

    fn open_list_editor(&mut self, target: EditTarget) {
        let mut field = FormTextField::new_multi_line();
        field.set_placeholder("One entry per line…");
        let text = match target {
            EditTarget::AllowedDomains => self.settings.allowed_domains.join("\n"),
            EditTarget::DeniedDomains => self.settings.denied_domains.join("\n"),
        };
        field.set_text(&text);
        field.move_cursor_to_start();
        self.mode = ViewMode::EditList { target, field };
    }

    fn save_list_editor(&mut self, target: EditTarget, field: &FormTextField) {
        let mut values: Vec<String> = field
            .text()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(std::string::ToString::to_string)
            .collect();
        values.sort_by_key(|value| value.to_ascii_lowercase());
        values.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

        match target {
            EditTarget::AllowedDomains => self.settings.allowed_domains = values,
            EditTarget::DeniedDomains => self.settings.denied_domains = values,
        }
        self.dirty = true;
    }

    fn apply_settings(&mut self) {
        self.app_event_tx.send(AppEvent::SetNetworkProxySettings(
            self.settings.clone(),
        ));
        self.app_event_tx.send_background_event_with_ticket(
            &self.ticket,
            "Applying network settings…".to_string(),
        );
        self.dirty = false;
    }

    fn activate_selected_row(&mut self) {
        match self.selected_row {
            0 => self.toggle_enabled(),
            1 => self.next_mode(),
            2 => self.open_list_editor(EditTarget::AllowedDomains),
            3 => self.open_list_editor(EditTarget::DeniedDomains),
            4 => self.toggle_allow_local_binding(),
            5 => self.apply_settings(),
            6 => self.is_complete = true,
            _ => {}
        }
    }

    fn process_key_event_main(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                self.selected_row = wrap_prev(self.selected_row, Self::SELECTABLE_ROWS);
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                self.selected_row = wrap_next(self.selected_row, Self::SELECTABLE_ROWS);
                true
            }
            KeyEvent { code: KeyCode::Left, modifiers: KeyModifiers::NONE, .. } => {
                match self.selected_row {
                    0 => self.toggle_enabled(),
                    1 => self.prev_mode(),
                    4 => self.toggle_allow_local_binding(),
                    _ => {}
                }
                true
            }
            KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE, .. } => {
                match self.selected_row {
                    0 => self.toggle_enabled(),
                    1 => self.next_mode(),
                    4 => self.toggle_allow_local_binding(),
                    _ => {}
                }
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
            | KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                self.activate_selected_row();
                true
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }

    fn process_key_event_edit(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.mode = ViewMode::Main;
                true
            }
            KeyEvent { code: KeyCode::Char('s'), modifiers, .. }
                if modifiers.contains(KeyModifiers::CONTROL) =>
            {
                let mode = std::mem::replace(&mut self.mode, ViewMode::Main);
                if let ViewMode::EditList { target, field } = mode {
                    self.save_list_editor(target, &field);
                }
                self.mode = ViewMode::Main;
                true
            }
            _ => match &mut self.mode {
                ViewMode::EditList { field, .. } => field.handle_key(key_event),
                ViewMode::Main => false,
            },
        }
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        match self.mode {
            ViewMode::Main => self.process_key_event_main(key_event),
            ViewMode::EditList { .. } => self.process_key_event_edit(key_event),
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
            ViewMode::Main => false,
        }
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match &mut self.mode {
            ViewMode::Main => {
                let mut selected = self.selected_row;
                let result = route_selectable_regions_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    Self::SELECTABLE_ROWS,
                    area,
                    &Self::HIT_REGIONS,
                    SelectableListMouseConfig {
                        require_pointer_hit_for_scroll: true,
                        scroll_behavior: ScrollSelectionBehavior::Clamp,
                        ..SelectableListMouseConfig::default()
                    },
                );
                self.selected_row = selected;

                if matches!(result, SelectableListMouseResult::Activated) {
                    self.activate_selected_row();
                }
                result.handled()
            }
            ViewMode::EditList { field, .. } => match mouse_event.kind {
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
            },
        }
    }

    fn render_main(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Network ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled(
                "Managed network mediation for tool execution.",
                Style::default().fg(crate::colors::text_dim()),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                "Allow/deny lists apply before interactive approvals.",
                Style::default().fg(crate::colors::text_dim()),
            ),
        ]));
        lines.push(Line::from(""));

        let row_style = |selected: bool| {
            if selected {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            }
        };

        let enabled_label = if self.settings.enabled { "Enabled" } else { "Disabled" };
        let enabled_color = if self.settings.enabled {
            crate::colors::success()
        } else {
            crate::colors::warning()
        };
        lines.push(
            Line::from(vec![
                Span::raw("  Enabled: "),
                Span::styled(
                    enabled_label,
                    Style::default().fg(enabled_color).add_modifier(Modifier::BOLD),
                ),
            ])
            .style(row_style(self.selected_row == 0)),
        );

        let mode = Self::mode_label(self.settings.mode);
        lines.push(
            Line::from(vec![Span::raw("  Mode: "), Span::styled(mode, Style::default().fg(crate::colors::info()))])
                .style(row_style(self.selected_row == 1)),
        );

        let allowed_count = self.settings.allowed_domains.len();
        let allowed_summary = if allowed_count == 0 {
            "(none)".to_string()
        } else {
            format!("{allowed_count} entries")
        };
        lines.push(
            Line::from(vec![Span::raw("  Allowed domains: "), Span::styled(allowed_summary, Style::default().fg(crate::colors::text_dim()))])
                .style(row_style(self.selected_row == 2)),
        );

        let denied_count = self.settings.denied_domains.len();
        let denied_summary = if denied_count == 0 {
            "(none)".to_string()
        } else {
            format!("{denied_count} entries")
        };
        lines.push(
            Line::from(vec![Span::raw("  Denied domains: "), Span::styled(denied_summary, Style::default().fg(crate::colors::text_dim()))])
                .style(row_style(self.selected_row == 3)),
        );

        let local_label = if self.settings.allow_local_binding { "On" } else { "Off" };
        lines.push(
            Line::from(vec![Span::raw("  Allow local binding: "), Span::styled(local_label, Style::default().fg(crate::colors::text_dim()))])
                .style(row_style(self.selected_row == 4)),
        );

        lines.push(Line::from(""));

        let apply_suffix = if self.dirty { " *" } else { "" };
        lines.push(
            Line::from(vec![
                Span::raw("  Apply changes"),
                Span::styled(apply_suffix, Style::default().fg(crate::colors::warning())),
            ])
            .style(row_style(self.selected_row == 5)),
        );
        lines.push(
            Line::from(vec![Span::raw("  Close")]).style(row_style(self.selected_row == 6)),
        );

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }

    fn render_edit(&self, area: Rect, buf: &mut Buffer, target: EditTarget, field: &FormTextField) {
        Clear.render(area, buf);
        let title = match target {
            EditTarget::AllowedDomains => " Network: Allowed Domains ",
            EditTarget::DeniedDomains => " Network: Denied Domains ",
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
        14
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main(area, buf),
            ViewMode::EditList { target, field } => self.render_edit(area, buf, *target, field),
        }
    }
}
