use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use code_core::config_types::{SettingsMenuConfig, SettingsMenuOpenMode};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use crate::util::buffer::{fill_rect, write_line};
use std::cell::Cell;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_panel::{panel_content_rect, render_panel, PanelFrameStyle};
use super::BottomPane;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    OpenMode,
    OverlayMinWidth,
    Apply,
    Close,
}

#[derive(Debug)]
enum ViewMode {
    Transition,
    Main,
    EditWidth { field: FormTextField, error: Option<String> },
}

pub(crate) struct InterfaceSettingsView {
    settings: SettingsMenuConfig,
    app_event_tx: AppEventSender,
    is_complete: bool,
    dirty: bool,
    mode: ViewMode,
    state: ScrollState,
    viewport_rows: Cell<usize>,
}

impl InterfaceSettingsView {
    pub fn new(settings: SettingsMenuConfig, app_event_tx: AppEventSender) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        Self {
            settings,
            app_event_tx,
            is_complete: false,
            dirty: false,
            mode: ViewMode::Main,
            state,
            viewport_rows: Cell::new(0),
        }
    }

    fn open_mode_label(mode: SettingsMenuOpenMode) -> &'static str {
        match mode {
            SettingsMenuOpenMode::Auto => "auto",
            SettingsMenuOpenMode::Overlay => "overlay",
            SettingsMenuOpenMode::Bottom => "bottom",
        }
    }

    fn open_mode_description(mode: SettingsMenuOpenMode, width: u16) -> String {
        match mode {
            SettingsMenuOpenMode::Auto => format!("overlay when width >= {width}"),
            SettingsMenuOpenMode::Overlay => "always show overlay".to_string(),
            SettingsMenuOpenMode::Bottom => "prefer bottom pane".to_string(),
        }
    }

    fn build_rows(&self) -> [RowKind; 4] {
        [
            RowKind::OpenMode,
            RowKind::OverlayMinWidth,
            RowKind::Apply,
            RowKind::Close,
        ]
    }

    fn selected_row(&self) -> RowKind {
        let rows = self.build_rows();
        let idx = self.state.selected_idx.unwrap_or(0).min(rows.len().saturating_sub(1));
        rows[idx]
    }

    fn cycle_open_mode_next(&mut self) {
        self.settings.open_mode = match self.settings.open_mode {
            SettingsMenuOpenMode::Auto => SettingsMenuOpenMode::Overlay,
            SettingsMenuOpenMode::Overlay => SettingsMenuOpenMode::Bottom,
            SettingsMenuOpenMode::Bottom => SettingsMenuOpenMode::Auto,
        };
        self.dirty = true;
    }

    fn adjust_min_width(&mut self, delta: i16) {
        let current = self.settings.overlay_min_width as i16;
        let next = (current + delta).clamp(40, 300) as u16;
        if next != self.settings.overlay_min_width {
            self.settings.overlay_min_width = next;
            self.dirty = true;
        }
    }

    fn open_width_editor(&mut self) {
        let mut field = FormTextField::new_single_line();
        field.set_placeholder("100");
        field.set_text(&self.settings.overlay_min_width.to_string());
        self.mode = ViewMode::EditWidth { field, error: None };
    }

    fn save_width_editor(&mut self, field: &FormTextField) -> Result<(), String> {
        let raw = field.text().trim();
        let parsed: u16 = raw
            .parse()
            .map_err(|_| "Enter a number (columns), e.g. 100.".to_string())?;
        // Keep sane bounds so accidental paste doesn't make it unusable.
        let clamped = parsed.clamp(40, 300);
        self.settings.overlay_min_width = clamped;
        self.dirty = true;
        Ok(())
    }

    fn apply_settings(&mut self) {
        self.app_event_tx
            .send(AppEvent::SetTuiSettingsMenuConfig(self.settings.clone()));
        self.dirty = false;
    }

    fn activate_selected_row(&mut self) {
        match self.selected_row() {
            RowKind::OpenMode => self.cycle_open_mode_next(),
            RowKind::OverlayMinWidth => self.open_width_editor(),
            RowKind::Apply => self.apply_settings(),
            RowKind::Close => self.is_complete = true,
        }
    }

    fn content_area(area: Rect) -> Rect {
        panel_content_rect(area, PanelFrameStyle::bottom_pane().with_margin(Margin::new(1, 0)))
    }

    fn layout_main(area: Rect) -> Option<(Rect, Rect, Rect)> {
        let content = Self::content_area(area);
        if content.width == 0 || content.height < 3 {
            return None;
        }

        let header = Rect::new(content.x, content.y, content.width, 1);
        let footer = Rect::new(
            content.x,
            content.y.saturating_add(content.height.saturating_sub(1)),
            content.width,
            1,
        );
        let list = Rect::new(
            content.x,
            content.y.saturating_add(1),
            content.width,
            content.height.saturating_sub(2),
        );
        Some((header, list, footer))
    }

    fn selection_index_at(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        let (_header, list, _footer) = Self::layout_main(area)?;
        if list.width == 0 || list.height == 0 {
            return None;
        }
        if x < list.x || x >= list.x.saturating_add(list.width) {
            return None;
        }
        if y < list.y || y >= list.y.saturating_add(list.height) {
            return None;
        }
        let rel = y.saturating_sub(list.y) as usize;
        let rows = self.build_rows();
        let scroll_top = self.state.scroll_top.min(rows.len().saturating_sub(1));
        let actual = scroll_top.saturating_add(rel);
        if actual >= rows.len() {
            return None;
        }
        Some(actual)
    }

    fn handle_mouse_event_main(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let rows = self.build_rows();
        let total = rows.len();
        if total == 0 {
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

        match result {
            SelectableListMouseResult::Ignored => false,
            SelectableListMouseResult::SelectionChanged => {
                self.state.selected_idx = Some(selected);
                let visible = self.viewport_rows.get().max(1);
                self.state.ensure_visible(total, visible);
                true
            }
            SelectableListMouseResult::Activated => {
                self.state.selected_idx = Some(selected);
                let visible = self.viewport_rows.get().max(1);
                self.state.ensure_visible(total, visible);
                self.activate_selected_row();
                true
            }
        }
    }

    fn handle_mouse_event_edit(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let content = Self::content_area(area);
        if content.width == 0 || content.height < 4 {
            return false;
        }

        // Layout: label (1), field (1), hint (1), optional error line (1).
        let label_y = content.y;
        let field_y = label_y.saturating_add(1);
        let field_area = Rect::new(content.x, field_y, content.width, 1);

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let ViewMode::EditWidth { field, .. } = &mut self.mode
                    && mouse_event.row == field_area.y
                    && mouse_event.column >= field_area.x
                    && mouse_event.column < field_area.x.saturating_add(field_area.width)
                {
                    return field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area);
                }
                false
            }
            _ => false,
        }
    }

    fn process_key_event_main(&mut self, key_event: KeyEvent) -> bool {
        let rows = self.build_rows();
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
        let selected = self.state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
        let current_row = rows.get(selected).copied();

        let visible = self.viewport_rows.get().max(1);

        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                self.state.move_up_wrap_visible(total, visible);
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                self.state.move_down_wrap_visible(total, visible);
                true
            }
            KeyEvent { code: KeyCode::Left, modifiers: KeyModifiers::NONE, .. } => {
                match current_row {
                    Some(RowKind::OpenMode) => {
                        // Reverse cycle.
                        self.settings.open_mode = match self.settings.open_mode {
                            SettingsMenuOpenMode::Auto => SettingsMenuOpenMode::Bottom,
                            SettingsMenuOpenMode::Overlay => SettingsMenuOpenMode::Auto,
                            SettingsMenuOpenMode::Bottom => SettingsMenuOpenMode::Overlay,
                        };
                        self.dirty = true;
                    }
                    Some(RowKind::OverlayMinWidth) => self.adjust_min_width(-5),
                    _ => {}
                }
                true
            }
            KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE, .. } => {
                match current_row {
                    Some(RowKind::OpenMode) => self.cycle_open_mode_next(),
                    Some(RowKind::OverlayMinWidth) => self.adjust_min_width(5),
                    _ => {}
                }
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
            | KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                if current_row.is_some() {
                    self.activate_selected_row();
                    self.state.ensure_visible(total, visible);
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
        }
    }

    fn process_key_event_edit(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.mode = ViewMode::Main;
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
                if let ViewMode::EditWidth { field, .. } = mode {
                    match self.save_width_editor(&field) {
                        Ok(()) => self.mode = ViewMode::Main,
                        Err(err) => self.mode = ViewMode::EditWidth {
                            field,
                            error: Some(err),
                        },
                    }
                } else {
                    self.mode = ViewMode::Main;
                }
                true
            }
            KeyEvent { code: KeyCode::Char('s'), modifiers, .. }
                if modifiers.contains(KeyModifiers::CONTROL) =>
            {
                let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
                if let ViewMode::EditWidth { field, .. } = mode {
                    match self.save_width_editor(&field) {
                        Ok(()) => self.mode = ViewMode::Main,
                        Err(err) => self.mode = ViewMode::EditWidth {
                            field,
                            error: Some(err),
                        },
                    }
                } else {
                    self.mode = ViewMode::Main;
                }
                true
            }
            _ => match &mut self.mode {
                ViewMode::EditWidth { field, .. } => field.handle_key(key_event),
                ViewMode::Main | ViewMode::Transition => false,
            },
        }
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main => {
                self.mode = ViewMode::Main;
                self.process_key_event_main(key_event)
            }
            ViewMode::EditWidth { field, error } => {
                self.mode = ViewMode::EditWidth { field, error };
                self.process_key_event_edit(key_event)
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.process_key_event(key_event)
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        match &mut self.mode {
            ViewMode::EditWidth { field, .. } => {
                field.handle_paste(text);
                true
            }
            ViewMode::Main | ViewMode::Transition => false,
        }
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match &self.mode {
            ViewMode::Main => self.handle_mouse_event_main(mouse_event, area),
            ViewMode::EditWidth { .. } => self.handle_mouse_event_edit(mouse_event, area),
            ViewMode::Transition => false,
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn help_for(row: RowKind) -> &'static str {
        match row {
            RowKind::OpenMode => "Auto uses overlay on wide terminals; override with overlay/bottom.",
            RowKind::OverlayMinWidth => "Terminal width (columns) at which auto prefers overlay.",
            RowKind::Apply => "Persist these preferences to config.toml.",
            RowKind::Close => "Close this panel.",
        }
    }

    fn render_main(&self, area: Rect, buf: &mut Buffer) {
        let style = PanelFrameStyle::bottom_pane().with_margin(Margin::new(1, 0));
        render_panel(area, buf, "Interface", style, |_content, buf| {
            let Some((header, list, footer)) = Self::layout_main(area) else {
                return;
            };

            let header_line = Line::from(vec![
                Span::styled("Enter", Style::default().fg(crate::colors::function())),
                Span::styled(" toggle  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Esc", Style::default().fg(crate::colors::function())),
                Span::styled(" close", Style::default().fg(crate::colors::text_dim())),
            ]);
            write_line(
                buf,
                header.x,
                header.y,
                header.width,
                &header_line,
                Style::default().bg(crate::colors::background()),
            );

            let rows = self.build_rows();
            let total = rows.len();
            let selected = self.state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
            let scroll_top = self.state.scroll_top.min(total.saturating_sub(1));
            let visible_rows = list.height.max(1) as usize;
            self.viewport_rows.set(visible_rows);

            let mut abs_idx = scroll_top;
            let mut rel_idx = 0usize;
            while rel_idx < visible_rows && abs_idx < rows.len() {
                let kind = rows[abs_idx];
                let y = list.y.saturating_add(rel_idx as u16);
                let row_area = Rect::new(list.x, y, list.width, 1);
                let is_selected = abs_idx == selected;
                let base = if is_selected {
                    Style::default()
                        .bg(crate::colors::selection())
                        .fg(crate::colors::text_bright())
                } else {
                    Style::default().bg(crate::colors::background()).fg(crate::colors::text())
                };
                fill_rect(buf, row_area, Some(' '), base);

                let (label, value) = match kind {
                    RowKind::OpenMode => {
                        let mode = Self::open_mode_label(self.settings.open_mode);
                        let desc = Self::open_mode_description(
                            self.settings.open_mode,
                            self.settings.overlay_min_width,
                        );
                        ("Settings menu", format!("{mode} ({desc})"))
                    }
                    RowKind::OverlayMinWidth => (
                        "Overlay min width",
                        format!("{}", self.settings.overlay_min_width),
                    ),
                    RowKind::Apply => {
                        let suffix = if self.dirty { " *" } else { "" };
                        ("Apply", suffix.to_string())
                    }
                    RowKind::Close => ("Close", String::new()),
                };

                let mut parts = vec![
                    Span::styled(
                        format!("{} {label}: ", if is_selected { ">" } else { " " }),
                        base.add_modifier(Modifier::BOLD),
                    ),
                ];
                if !value.is_empty() {
                    let value_style = if is_selected {
                        base.add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .bg(crate::colors::background())
                            .fg(crate::colors::text_dim())
                    };
                    parts.push(Span::styled(value, value_style));
                }
                let line = Line::from(parts);
                write_line(buf, row_area.x, row_area.y, row_area.width, &line, base);

                abs_idx = abs_idx.saturating_add(1);
                rel_idx = rel_idx.saturating_add(1);
            }

            let help = Self::help_for(self.selected_row());
            let footer_line = Line::from(vec![Span::styled(
                help.to_string(),
                Style::default().fg(crate::colors::text_dim()),
            )]);
            write_line(
                buf,
                footer.x,
                footer.y,
                footer.width,
                &footer_line,
                Style::default().bg(crate::colors::background()),
            );
        });
    }

    fn render_edit_width(&self, area: Rect, buf: &mut Buffer, field: &FormTextField, error: Option<&str>) {
        let style = PanelFrameStyle::bottom_pane().with_margin(Margin::new(1, 0));
        render_panel(area, buf, "Interface", style, |content, buf| {
            if content.width == 0 || content.height == 0 {
                return;
            }
            let base = Style::default().bg(crate::colors::background()).fg(crate::colors::text());

            let label = Line::from(vec![Span::styled(
                "Overlay min width (columns):".to_string(),
                Style::default().fg(crate::colors::text()),
            )]);
            write_line(buf, content.x, content.y, content.width, &label, base);

            let field_area = Rect::new(content.x, content.y.saturating_add(1), content.width, 1);
            fill_rect(buf, field_area, Some(' '), base);
            field.render(field_area, buf, true);

            let hint_y = content.y.saturating_add(3);
            let hint_line = Line::from(vec![
                Span::styled("Enter", Style::default().fg(crate::colors::function())),
                Span::styled("/", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Ctrl+S", Style::default().fg(crate::colors::function())),
                Span::styled(" save  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Esc", Style::default().fg(crate::colors::function())),
                Span::styled(" cancel", Style::default().fg(crate::colors::text_dim())),
            ]);
            if hint_y < content.y.saturating_add(content.height) {
                write_line(buf, content.x, hint_y, content.width, &hint_line, base);
            }

            if let Some(error) = error {
                let err_y = content.y.saturating_add(2);
                if err_y < content.y.saturating_add(content.height) {
                    let err_line = Line::from(vec![Span::styled(
                        error.to_string(),
                        Style::default().fg(crate::colors::warning()),
                    )]);
                    write_line(buf, content.x, err_y, content.width, &err_line, base);
                }
            }
        });
    }
}

impl<'a> BottomPaneView<'a> for InterfaceSettingsView {
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
            ViewMode::Main => 8,
            ViewMode::EditWidth { .. } => 8,
            ViewMode::Transition => 8,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main(area, buf),
            ViewMode::EditWidth { field, error } => {
                self.render_edit_width(area, buf, field, error.as_deref())
            }
            ViewMode::Transition => self.render_main(area, buf),
        }
    }
}
