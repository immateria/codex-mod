use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::model_selection_state::{
    reasoning_effort_label,
    EntryKind,
    ModelSelectionData,
    ModelSelectionViewParams,
    SelectionAction,
};
use super::settings_ui::line_runs::{
    hit_test_selectable_runs,
    render_selectable_runs,
    SelectableLineRun,
};
use super::settings_ui::panel::{SettingsPanel, SettingsPanelStyle};
use super::BottomPane;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use code_common::model_presets::ModelPreset;
use code_core::config_types::{ContextMode, ServiceTier};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use std::cell::RefCell;

const SUMMARY_LINE_COUNT: usize = 3;
const FOOTER_LINE_COUNT: usize = 2;
// 2 summary rows + 1 summary spacer + 1 footer spacer + 1 footer hint row.
const PANEL_CHROME_HEIGHT: usize = SUMMARY_LINE_COUNT + FOOTER_LINE_COUNT;
// Before the first render, assume the minimum inner panel height implied by
// `desired_height() == 9`: 7 content rows after the outer border is removed.
const DEFAULT_INNER_PANEL_HEIGHT: u16 = 7;

pub(crate) struct ModelSelectionView {
    data: ModelSelectionData,
    selected_index: usize,
    hovered_index: Option<usize>,
    app_event_tx: AppEventSender,
    is_complete: bool,
    /// Cached (entry_index, rect) pairs from last render for mouse hit testing.
    item_rects: RefCell<Vec<(usize, Rect)>>,
    /// Scroll offset for rendering when content exceeds available height.
    scroll_offset: usize,
    /// Cached inner panel height from the last render, used by keyboard scrolling.
    last_render_height: RefCell<u16>,
}

impl ModelSelectionView {
    pub fn new(params: ModelSelectionViewParams, app_event_tx: AppEventSender) -> Self {
        let data = ModelSelectionData::new(params);
        let selected_index = data.initial_selection();
        Self {
            data,
            selected_index,
            hovered_index: None,
            app_event_tx,
            is_complete: false,
            item_rects: RefCell::new(Vec::new()),
            scroll_offset: 0,
            last_render_height: RefCell::new(DEFAULT_INNER_PANEL_HEIGHT),
        }
    }

    fn panel(&self) -> SettingsPanel<'_> {
        SettingsPanel::new(
            self.data.target.panel_title(),
            SettingsPanelStyle::bottom_pane(),
        )
    }

    pub(crate) fn update_presets(&mut self, presets: Vec<ModelPreset>) {
        self.selected_index = self.data.update_presets(presets, self.selected_index);
    }

    fn entry_count(&self) -> usize {
        self.data.entry_count()
    }

    fn content_line_count(&self) -> u16 {
        self.data.content_line_count()
    }

    fn move_selection_up(&mut self) {
        let total = self.entry_count();
        if total == 0 {
            return;
        }
        self.selected_index = if self.selected_index == 0 {
            total - 1
        } else {
            self.selected_index.saturating_sub(1)
        };
        self.hovered_index = None;
        self.ensure_selected_visible();
    }

    fn move_selection_down(&mut self) {
        let total = self.entry_count();
        if total == 0 {
            return;
        }
        self.selected_index = (self.selected_index + 1) % total;
        self.hovered_index = None;
        self.ensure_selected_visible();
    }

    fn ensure_selected_visible(&mut self) {
        let visible_height = *self.last_render_height.borrow() as usize;
        if visible_height == 0 {
            return;
        }
        let content_height = visible_height.saturating_sub(PANEL_CHROME_HEIGHT);
        if content_height == 0 {
            return;
        }

        let selected_line = self.data.entry_line(self.selected_index);
        if selected_line < self.scroll_offset {
            self.scroll_offset = selected_line;
        }
        let visible_end = self.scroll_offset + content_height;
        if selected_line >= visible_end {
            self.scroll_offset = selected_line.saturating_sub(content_height) + 1;
        }
    }

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    fn scroll_down(&mut self) {
        let total_lines = self.content_line_count() as usize;
        let visible_height = *self.last_render_height.borrow() as usize;
        let content_height = visible_height.saturating_sub(PANEL_CHROME_HEIGHT);
        let max_scroll = total_lines.saturating_sub(content_height);
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    fn select_item(&mut self, index: usize) {
        let total = self.entry_count();
        if index >= total {
            return;
        }
        self.selected_index = index;
        self.confirm_selection();
    }

    fn hit_test(&self, x: u16, y: u16) -> Option<usize> {
        hit_test_selectable_runs(&self.item_rects.borrow(), x, y)
    }

    fn confirm_selection(&mut self) {
        if let Some(entry) = self.data.entry_at(self.selected_index)
            && let Some(action) = self.data.apply_selection(entry)
        {
            self.dispatch_selection_action(action);
        }
    }

    fn dispatch_selection_action(&mut self, action: SelectionAction) {
        let closes_view = action.closes_view();
        self.data
            .target
            .dispatch_selection_action(&self.app_event_tx, &action);
        if closes_view {
            self.send_closed(true);
        }
    }

    fn handle_mouse_event_shared(&mut self, mouse_event: MouseEvent) -> ConditionalUpdate {
        let mut selected = self.selected_index;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.entry_count(),
            |x, y| self.hit_test(x, y),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.selected_index = selected;

        if matches!(result, SelectableListMouseResult::Activated) {
            self.select_item(self.selected_index);
            return ConditionalUpdate::NeedsRedraw;
        }

        match mouse_event.kind {
            MouseEventKind::Moved => {
                let new_hover = self.hit_test(mouse_event.column, mouse_event.row);
                if new_hover != self.hovered_index {
                    self.hovered_index = new_hover;
                    return ConditionalUpdate::NeedsRedraw;
                }
            }
            MouseEventKind::ScrollUp => {
                self.scroll_up();
                return ConditionalUpdate::NeedsRedraw;
            }
            MouseEventKind::ScrollDown => {
                self.scroll_down();
                return ConditionalUpdate::NeedsRedraw;
            }
            _ => {}
        }

        if result.handled() {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Up | KeyCode::Char('k'), modifiers: KeyModifiers::NONE, .. } => {
                self.move_selection_up();
                true
            }
            KeyEvent { code: KeyCode::Down | KeyCode::Char('j'), modifiers: KeyModifiers::NONE, .. } => {
                self.move_selection_down();
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                self.confirm_selection();
                true
            }
            KeyEvent { code: KeyCode::Esc, modifiers: KeyModifiers::NONE, .. } => {
                self.send_closed(false);
                true
            }
            _ => false,
        }
    }

    /// Used when embedded in settings overlay. Hit testing relies on `item_rects`
    /// cached during the last render; `area` is accepted to match sibling view APIs.
    pub(crate) fn handle_mouse_event_direct(
        &mut self,
        mouse_event: MouseEvent,
        _area: Rect,
    ) -> ConditionalUpdate {
        self.handle_mouse_event_shared(mouse_event)
    }

    fn send_closed(&mut self, accepted: bool) {
        if self.is_complete {
            return;
        }
        self.app_event_tx.send(AppEvent::ModelSelectionClosed {
            target: self.data.target.into(),
            accepted,
        });
        self.is_complete = true;
    }

    fn base_style() -> Style {
        Style::new().bg(colors::background()).fg(colors::text())
    }

    fn dim_style() -> Style {
        Style::new().fg(colors::text_dim())
    }

    fn section_header_style() -> Style {
        Style::new().fg(colors::text_bright()).bold()
    }

    fn highlighted(base: Style, is_highlighted: bool) -> Style {
        if is_highlighted {
            base.bg(colors::selection()).bold()
        } else {
            base
        }
    }

    fn push_blank_line<'a>(lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        lines.push(SelectableLineRun::plain(vec![Line::from("")]));
    }

    fn push_summary_lines<'a>(&'a self, lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        lines.push(SelectableLineRun::plain(vec![Line::from(vec![
            Span::styled(
                format!("{}: ", self.data.target.current_label()),
                Self::dim_style(),
            ),
            Span::styled(
                if self.data.target.supports_follow_chat() && self.data.current.use_chat_model {
                    "Follow Chat Mode".to_string()
                } else {
                    self.data.current_model_display_name()
                },
                Style::new().fg(colors::warning()).bold(),
            ),
        ])]));
        lines.push(SelectableLineRun::plain(vec![Line::from(vec![
            Span::styled(
                format!("{}: ", self.data.target.reasoning_label()),
                Self::dim_style(),
            ),
            Span::styled(
                if self.data.target.supports_follow_chat() && self.data.current.use_chat_model {
                    "From chat".to_string()
                } else {
                    reasoning_effort_label(self.data.current.current_effort).to_string()
                },
                Style::new().fg(colors::warning()).bold(),
            ),
        ])]));
        Self::push_blank_line(lines);
    }

    fn push_fast_mode_section<'a>(&self, lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        let fast_index = 0;
        let is_selected = self.selected_index == fast_index;
        let is_hovered = self.hovered_index == Some(fast_index);
        let is_highlighted = is_selected || is_hovered;
        let fast_enabled = matches!(self.data.current.current_service_tier, Some(ServiceTier::Fast));
        let status = if fast_enabled { "enabled" } else { "disabled" };

        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Fast mode",
            Self::section_header_style(),
        )])]));
        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Same model, but 1.5x faster responses (uses 2x credits)",
            Self::dim_style(),
        )])]));

        let label_style = {
            let base = Self::highlighted(Style::new().fg(colors::text()), is_highlighted);
            if fast_enabled {
                base.fg(colors::success())
            } else {
                base
            }
        };
        let arrow_style = if is_highlighted {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new().fg(colors::text_dim())
        };

        lines.push(SelectableLineRun::selectable(
            fast_index,
            vec![Line::from(vec![
                Span::styled(if is_selected { "› " } else { "  " }, arrow_style),
                Span::styled(format!("Fast mode: {status}"), label_style),
            ])],
        ));
        Self::push_blank_line(lines);
    }

    fn push_context_mode_section<'a>(&self, lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        let Some(context_index) = self.data.context_mode_entry_index() else {
            return;
        };
        let is_selected = self.selected_index == context_index;
        let is_hovered = self.hovered_index == Some(context_index);
        let is_highlighted = is_selected || is_hovered;
        let context_status = match self.data.current.current_context_mode {
            Some(ContextMode::OneM) => "enabled",
            Some(ContextMode::Auto) => "auto",
            Some(ContextMode::Disabled) | None => "disabled",
        };
        let context_available = self.data.supports_extended_context();

        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Mode Settings",
            Self::section_header_style(),
        )])]));
        for info_line in ModelSelectionData::context_mode_intro_lines() {
            lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
                info_line,
                Self::dim_style(),
            )])]));
        }

        let label_style = {
            let base = Self::highlighted(Style::new().fg(colors::text()), is_highlighted);
            let with_mode = if self.data.current.current_context_mode.is_some() {
                base.fg(colors::success())
            } else {
                base
            };
            if !context_available {
                with_mode.fg(colors::text_dim())
            } else {
                with_mode
            }
        };
        let arrow_style = if is_highlighted {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new().fg(colors::text_dim())
        };

        lines.push(SelectableLineRun::selectable(
            context_index,
            vec![Line::from(vec![
                Span::styled(if is_selected { "› " } else { "  " }, arrow_style),
                Span::styled(format!("1M Context: {context_status}"), label_style),
            ])],
        ));

        if !context_available {
            lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
                "Unavailable for this model. Saved settings apply automatically on supported models.",
                Self::dim_style(),
            )])]));
        }

        Self::push_blank_line(lines);
    }

    fn push_follow_chat_section<'a>(&self, lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        let Some(follow_chat_index) = self.data.follow_chat_entry_index() else {
            return;
        };
        let is_selected = self.selected_index == follow_chat_index;
        let is_hovered = self.hovered_index == Some(follow_chat_index);
        let is_highlighted = is_selected || is_hovered;

        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Follow Chat Mode",
            Self::section_header_style(),
        )])]));
        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Use the active chat model and reasoning; stays in sync as chat changes.",
            Self::dim_style(),
        )])]));

        let label_style = Self::highlighted(Style::new().fg(colors::text()), is_highlighted);
        let arrow_style = if is_highlighted {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new().fg(colors::text_dim())
        };
        let indent_style = if is_highlighted {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new()
        };

        let mut spans = vec![
            Span::styled(if is_selected { "› " } else { "  " }, arrow_style),
            Span::styled("   ", indent_style),
            Span::styled("Use chat model", label_style),
        ];
        if self.data.current.use_chat_model {
            spans.push(Span::raw("  (current)"));
        }

        lines.push(SelectableLineRun::selectable(
            follow_chat_index,
            vec![Line::from(spans)],
        ));
        Self::push_blank_line(lines);
    }

    fn push_preset_lines<'a>(&self, lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        let mut previous_model: Option<&str> = None;
        let entries = self.data.entries();
        for (entry_idx, entry) in entries.iter().enumerate() {
            if matches!(
                entry,
                EntryKind::FastMode | EntryKind::ContextMode | EntryKind::FollowChat
            ) {
                continue;
            }
            let EntryKind::Preset(preset_index) = entry else {
                continue;
            };
            let flat_preset = &self.data.flat_presets[*preset_index];
            let is_new_model = previous_model
                .map(|model| !model.eq_ignore_ascii_case(&flat_preset.model))
                .unwrap_or(true);

            if is_new_model {
                if previous_model.is_some() {
                    Self::push_blank_line(lines);
                }
                lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
                    flat_preset.display_name.clone(),
                    Self::section_header_style(),
                )])]));
                if !flat_preset.model_description.trim().is_empty() {
                    lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
                        flat_preset.model_description.clone(),
                        Self::dim_style(),
                    )])]));
                }
                previous_model = Some(&flat_preset.model);
            }

            let is_selected = entry_idx == self.selected_index;
            let is_hovered = self.hovered_index == Some(entry_idx);
            let is_highlighted = is_selected || is_hovered;
            let is_current = !self.data.current.use_chat_model
                && flat_preset.model.eq_ignore_ascii_case(&self.data.current.current_model)
                && flat_preset.effort == self.data.current.current_effort;

            let mut row_text = reasoning_effort_label(flat_preset.effort).to_string();
            if is_current {
                row_text.push_str(" (current)");
            }

            let indent_style = if is_highlighted {
                Style::new().bg(colors::selection()).bold()
            } else {
                Style::new()
            };
            let label_style = {
                let base = Self::highlighted(Style::new().fg(colors::text()), is_highlighted);
                if is_current {
                    base.fg(colors::success())
                } else {
                    base
                }
            };
            let divider_style =
                Self::highlighted(Style::new().fg(colors::text_dim()), is_highlighted);
            let description_style =
                Self::highlighted(Style::new().fg(colors::dim()), is_highlighted);

            lines.push(SelectableLineRun::selectable(
                entry_idx,
                vec![Line::from(vec![
                    Span::styled("   ", indent_style),
                    Span::styled(row_text, label_style),
                    Span::styled(" - ", divider_style),
                    Span::styled(flat_preset.description.clone(), description_style),
                ])],
            ));
        }
    }

    fn push_footer_lines<'a>(lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        Self::push_blank_line(lines);
        lines.push(SelectableLineRun::plain(vec![Line::from(vec![
            Span::styled("↑↓", Style::new().fg(colors::light_blue())),
            Span::raw(" Navigate  "),
            Span::styled("Enter", Style::new().fg(colors::success())),
            Span::raw(" Select  "),
            Span::styled("Esc", Style::new().fg(colors::error())),
            Span::raw(" Cancel"),
        ])]));
    }

    fn build_render_runs<'a>(&'a self) -> Vec<SelectableLineRun<'a, usize>> {
        let mut runs = Vec::new();
        self.push_summary_lines(&mut runs);
        if self.data.target.supports_fast_mode() {
            self.push_fast_mode_section(&mut runs);
        }
        if self.data.target.supports_context_mode() {
            self.push_context_mode_section(&mut runs);
        }
        if self.data.target.supports_follow_chat() {
            self.push_follow_chat_section(&mut runs);
        }
        self.push_preset_lines(&mut runs);
        Self::push_footer_lines(&mut runs);
        runs
    }

    fn render_panel_body(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        *self.last_render_height.borrow_mut() = area.height;

        let mut item_rects = self.item_rects.borrow_mut();
        item_rects.clear();

        let padded = Rect {
            x: area.x.saturating_add(1),
            y: area.y,
            width: area.width.saturating_sub(1),
            height: area.height,
        };

        render_selectable_runs(
            padded,
            buf,
            self.scroll_offset,
            &self.build_render_runs(),
            Self::base_style(),
            &mut item_rects,
        );
    }

    pub(crate) fn render_without_frame(&self, area: Rect, buf: &mut Buffer) {
        self.render_panel_body(area, buf);
    }
}

impl<'a> BottomPaneView<'a> for ModelSelectionView {
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
        _area: Rect,
    ) -> ConditionalUpdate {
        self.handle_mouse_event_shared(mouse_event)
    }

    fn update_hover(&mut self, mouse_pos: (u16, u16), _area: Rect) -> bool {
        let new_hover = self.hit_test(mouse_pos.0, mouse_pos.1);
        if new_hover != self.hovered_index {
            self.hovered_index = new_hover;
            true
        } else {
            false
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let content_lines = self.content_line_count();
        let total = content_lines.saturating_add(2);
        total.max(9)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let Some(layout) = self.panel().render(area, buf) else {
            return;
        };
        self.render_panel_body(layout.content, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
    use crate::bottom_pane::ModelSelectionTarget;
    use code_common::model_presets::ReasoningEffortPreset;
    use code_core::config_types::ReasoningEffort;
    use std::sync::mpsc;

    fn preset(model: &str) -> ModelPreset {
        preset_with_effort(model, ReasoningEffort::Medium)
    }

    fn preset_with_effort(model: &str, effort: ReasoningEffort) -> ModelPreset {
        ModelPreset {
            id: model.to_string(),
            model: model.to_string(),
            display_name: model.to_string(),
            description: format!("preset for {model}"),
            default_reasoning_effort: effort.into(),
            supported_reasoning_efforts: vec![ReasoningEffortPreset {
                effort: effort.into(),
                description: effort.to_string().to_ascii_lowercase(),
            }],
            supported_text_verbosity: &[],
            is_default: false,
            upgrade: None,
            pro_only: false,
            show_in_picker: true,
        }
    }

    fn make_view(target: ModelSelectionTarget, presets: Vec<ModelPreset>) -> ModelSelectionView {
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        ModelSelectionView::new(
            ModelSelectionViewParams {
                presets,
                current_model: "unknown-model".to_string(),
                current_effort: ReasoningEffort::Medium,
                current_service_tier: None,
                current_context_mode: None,
                use_chat_model: false,
                target,
            },
            AppEventSender::new(tx),
        )
    }

    #[test]
    fn session_initial_selection_prefers_first_preset_after_fast_mode() {
        let view = make_view(ModelSelectionTarget::Session, vec![preset("gpt-5.3-codex")]);
        assert_eq!(view.selected_index, 2);
    }

    #[test]
    fn session_initial_selection_with_no_presets_uses_fast_mode() {
        let view = make_view(ModelSelectionTarget::Session, Vec::new());
        assert_eq!(view.selected_index, 0);
    }

    #[test]
    fn entry_count_includes_fast_mode() {
        let view = make_view(ModelSelectionTarget::Session, vec![preset("gpt-5.3-codex")]);
        assert_eq!(view.entry_count(), 3);
    }

    #[test]
    fn get_entry_line_accounts_for_header_and_fast_block() {
        let view = make_view(ModelSelectionTarget::Session, vec![preset("gpt-5.3-codex")]);
        assert_eq!(view.data.entry_line(0), 5);
        assert_eq!(view.data.entry_line(1), 11);
        assert_eq!(view.data.entry_line(2), 17);
    }

    #[test]
    fn context_mode_intro_mentions_auto_trigger_and_billing() {
        let lines = ModelSelectionData::context_mode_intro_lines();
        assert!(lines[1].contains("pre-turn compaction checks"));
        assert!(lines[1].contains("272,000"));
        assert!(lines[1].contains("2x input"));
        assert!(lines[1].contains("1.5x output"));
    }

    #[test]
    fn vim_navigation_keys_move_selection() {
        let mut view = make_view(
            ModelSelectionTarget::Session,
            vec![preset("gpt-5.3-codex"), preset("gpt-5.4")],
        );

        assert_eq!(view.selected_index, 2);
        assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Char('j'))));
        assert_eq!(view.selected_index, 3);
        assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Char('k'))));
        assert_eq!(view.selected_index, 2);
    }

    #[test]
    fn vim_navigation_keys_require_no_modifiers() {
        let mut view = make_view(
            ModelSelectionTarget::Session,
            vec![preset("gpt-5.3-codex"), preset("gpt-5.4")],
        );

        assert_eq!(view.selected_index, 2);
        assert!(!view.handle_key_event_direct(KeyEvent::new(
            KeyCode::Char('j'),
            KeyModifiers::CONTROL,
        )));
        assert_eq!(view.selected_index, 2);
        assert!(!view.handle_key_event_direct(KeyEvent::new(
            KeyCode::Char('k'),
            KeyModifiers::CONTROL,
        )));
        assert_eq!(view.selected_index, 2);
    }

    #[test]
    fn selecting_preset_updates_local_current_model_state() {
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let mut view = ModelSelectionView::new(
            ModelSelectionViewParams {
                presets: vec![preset_with_effort("gpt-5.3-codex", ReasoningEffort::High)],
                current_model: "gpt-5.4".to_string(),
                current_effort: ReasoningEffort::Medium,
                current_service_tier: None,
                current_context_mode: None,
                use_chat_model: false,
                target: ModelSelectionTarget::Session,
            },
            AppEventSender::new(tx),
        );

        view.select_item(2);

        assert_eq!(view.data.current.current_model, "gpt-5.3-codex");
        assert_eq!(view.data.current.current_effort, ReasoningEffort::High);
        assert!(!view.data.current.use_chat_model);
    }

    #[test]
    fn selecting_follow_chat_updates_local_follow_chat_state() {
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let mut view = ModelSelectionView::new(
            ModelSelectionViewParams {
                presets: vec![preset("gpt-5.3-codex")],
                current_model: "gpt-5.4".to_string(),
                current_effort: ReasoningEffort::Medium,
                current_service_tier: None,
                current_context_mode: None,
                use_chat_model: false,
                target: ModelSelectionTarget::Review,
            },
            AppEventSender::new(tx),
        );

        view.select_item(0);

        assert!(view.data.current.use_chat_model);
    }

    #[test]
    fn selecting_context_mode_sends_session_context_mode_update() {
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let mut view = ModelSelectionView::new(
            ModelSelectionViewParams {
                presets: vec![preset("gpt-5.4")],
                current_model: "gpt-5.4".to_string(),
                current_effort: ReasoningEffort::Medium,
                current_service_tier: None,
                current_context_mode: Some(ContextMode::Disabled),
                use_chat_model: false,
                target: ModelSelectionTarget::Session,
            },
            AppEventSender::new(tx),
        );

        view.select_item(1);

        assert_eq!(view.data.current.current_context_mode, Some(ContextMode::OneM));
        match rx.recv().expect("context mode event") {
            AppEvent::UpdateSessionContextModeSelection { context_mode } => {
                assert_eq!(context_mode, Some(ContextMode::OneM));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn hit_testing_tracks_visible_scroll_slice() {
        let mut view = make_view(
            ModelSelectionTarget::Session,
            vec![preset("gpt-5.3-codex"), preset("gpt-5.4")],
        );
        let area = Rect::new(0, 0, 60, 4);
        let mut buf = Buffer::empty(area);

        view.scroll_offset = view.data.entry_line(2);
        view.render_without_frame(area, &mut buf);

        assert_eq!(view.hit_test(2, 0), Some(2));
        assert_eq!(view.hit_test(2, 1), Some(3));
        assert_eq!(view.hit_test(2, 2), None);
    }
}
