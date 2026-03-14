use crate::bottom_pane::{BottomPaneView, ConditionalUpdate};
use super::model_selection_state::{
    reasoning_effort_label,
    EntryKind,
    ModelSelectionData,
    ModelSelectionViewParams,
    SelectionAction,
};
use crate::bottom_pane::settings_ui::line_runs::{
    selection_id_at,
    SelectableLineRun,
};
use crate::bottom_pane::settings_ui::hints::{shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::toggle;
use crate::bottom_pane::BottomPane;
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
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use std::cell::Cell;

const SUMMARY_LINE_COUNT: usize = 3;
const FOOTER_LINE_COUNT: usize = 2;
// 2 summary rows + 1 summary spacer + 1 footer spacer + 1 footer hint row.
pub(crate) struct ModelSelectionView {
    data: ModelSelectionData,
    selected_index: usize,
    app_event_tx: AppEventSender,
    is_complete: bool,
    scroll_offset: usize,
    visible_body_rows: Cell<usize>,
}

pub(crate) type ModelSelectionViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, ModelSelectionView>;
pub(crate) type ModelSelectionViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, ModelSelectionView>;
pub(crate) type ModelSelectionViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, ModelSelectionView>;

impl ModelSelectionView {
    pub fn new(params: ModelSelectionViewParams, app_event_tx: AppEventSender) -> Self {
        let data = ModelSelectionData::new(params);
        let selected_index = data.initial_selection();
        Self {
            data,
            selected_index,
            app_event_tx,
            is_complete: false,
            scroll_offset: 0,
            visible_body_rows: Cell::new(0),
        }
    }

    fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            self.data.target.panel_title(),
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            self.header_lines(),
            self.footer_lines(),
        )
    }

    pub(crate) fn framed(&self) -> ModelSelectionViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> ModelSelectionViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> ModelSelectionViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn update_presets(&mut self, presets: Vec<ModelPreset>) {
        self.selected_index = self.data.update_presets(presets, self.selected_index);
    }

    fn entry_count(&self) -> usize {
        self.data.entry_count()
    }

    fn content_line_count(&self) -> u16 {
        self.data
            .content_line_count()
            .saturating_sub((SUMMARY_LINE_COUNT + FOOTER_LINE_COUNT) as u16)
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
        self.ensure_selected_visible();
    }

    fn move_selection_down(&mut self) {
        let total = self.entry_count();
        if total == 0 {
            return;
        }
        self.selected_index = (self.selected_index + 1) % total;
        self.ensure_selected_visible();
    }

    fn ensure_selected_visible(&mut self) {
        let body_height = self.visible_body_rows.get();
        if body_height == 0 {
            return;
        }

        let selected_line = self.selected_body_line(self.selected_index);
        if selected_line < self.scroll_offset {
            self.scroll_offset = selected_line;
        }
        let visible_end = self.scroll_offset + body_height;
        if selected_line >= visible_end {
            self.scroll_offset = selected_line.saturating_sub(body_height) + 1;
        }
    }

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    fn scroll_down(&mut self) {
        let total_lines = self.content_line_count() as usize;
        let max_scroll = total_lines.saturating_sub(self.visible_body_rows.get());
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

    fn selected_body_line(&self, entry_index: usize) -> usize {
        self.data.entry_line(entry_index).saturating_sub(SUMMARY_LINE_COUNT)
    }

    fn hit_test_in_body(&self, body: Rect, x: u16, y: u16) -> Option<usize> {
        selection_id_at(body, x, y, self.scroll_offset, &self.build_render_runs())
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

    fn handle_mouse_event_shared(
        &mut self,
        mouse_event: MouseEvent,
        body: Rect,
    ) -> ConditionalUpdate {
        let mut selected = self.selected_index;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.entry_count(),
            |x, y| self.hit_test_in_body(body, x, y),
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

    fn push_fast_mode_section<'a>(&self, lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        let fast_index = 0;
        let is_selected = self.selected_index == fast_index;
        let fast_enabled = matches!(self.data.current.current_service_tier, Some(ServiceTier::Fast));
        let status = toggle::enabled_word(fast_enabled);

        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Fast mode",
            Self::section_header_style(),
        )])]));
        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Same model, but 1.5x faster responses (uses 2x credits)",
            Self::dim_style(),
        )])]));

        let label_style = {
            let base = Self::highlighted(Style::new().fg(colors::text()), is_selected);
            if fast_enabled {
                base.fg(colors::success())
            } else {
                base
            }
        };
        let arrow_style = if is_selected {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new().fg(colors::text_dim())
        };

        lines.push(SelectableLineRun::selectable(
            fast_index,
            vec![Line::from(vec![
                Span::styled(if is_selected { "› " } else { "  " }, arrow_style),
                Span::styled("Fast mode: ", label_style),
                Span::styled(
                    status.text,
                    label_style.fg(status.style.fg.unwrap_or(colors::text())),
                ),
            ])],
        ));
        // Keep fast-mode height aligned with `FAST_MODE_SECTION_HEIGHT` for scroll math.
        Self::push_blank_line(lines);
        Self::push_blank_line(lines);
    }

    fn push_context_mode_section<'a>(&self, lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        let Some(context_index) = self.data.context_mode_entry_index() else {
            return;
        };
        let is_selected = self.selected_index == context_index;
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
            let base = Self::highlighted(Style::new().fg(colors::text()), is_selected);
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
        let arrow_style = if is_selected {
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

        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Follow Chat Mode",
            Self::section_header_style(),
        )])]));
        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Use the active chat model and reasoning; stays in sync as chat changes.",
            Self::dim_style(),
        )])]));

        let label_style = Self::highlighted(Style::new().fg(colors::text()), is_selected);
        let arrow_style = if is_selected {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new().fg(colors::text_dim())
        };
        let indent_style = if is_selected {
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
            let is_current = !self.data.current.use_chat_model
                && flat_preset.model.eq_ignore_ascii_case(&self.data.current.current_model)
                && flat_preset.effort == self.data.current.current_effort;

            let mut row_text = reasoning_effort_label(flat_preset.effort).to_string();
            if is_current {
                row_text.push_str(" (current)");
            }

            let indent_style = if is_selected {
                Style::new().bg(colors::selection()).bold()
            } else {
                Style::new()
            };
            let label_style = {
                let base = Self::highlighted(Style::new().fg(colors::text()), is_selected);
                if is_current {
                    base.fg(colors::success())
                } else {
                    base
                }
            };
            let divider_style =
                Self::highlighted(Style::new().fg(colors::text_dim()), is_selected);
            let description_style =
                Self::highlighted(Style::new().fg(colors::dim()), is_selected);

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

    fn build_render_runs<'a>(&'a self) -> Vec<SelectableLineRun<'a, usize>> {
        let mut runs = Vec::new();
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
        runs
    }

    fn header_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(vec![
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
            ]),
            Line::from(vec![
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
            ]),
            Line::from(""),
        ]
    }

    fn footer_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            shortcut_line(&[
                KeyHint::new("↑↓", " Navigate")
                    .with_key_style(Style::new().fg(colors::light_blue())),
                KeyHint::new("Enter", " Select")
                    .with_key_style(Style::new().fg(colors::success())),
                KeyHint::new("Esc", " Cancel")
                    .with_key_style(Style::new().fg(colors::error())),
            ]),
        ]
    }

    fn render_in_page(
        &self,
        page: &SettingsMenuPage<'_>,
        area: Rect,
        buf: &mut Buffer,
        framed: bool,
    ) {
        let runs = self.build_render_runs();
        let layout = if framed {
            page.framed().render_runs(area, buf, self.scroll_offset, &runs)
        } else {
            page.content_only()
                .render_runs(area, buf, self.scroll_offset, &runs)
        };
        if let Some(layout) = layout {
            self.visible_body_rows.set(layout.body.height as usize);
        }
    }

    fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_page(&self.page(), area, buf, false);
    }

    fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_page(&self.page(), area, buf, true);
    }
}

impl crate::bottom_pane::chrome_view::ChromeRenderable for ModelSelectionView {
    fn render_in_framed_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_framed(area, buf);
    }

    fn render_in_content_only_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_content_only(area, buf);
    }
}

impl crate::bottom_pane::chrome_view::ChromeMouseHandler for ModelSelectionView {
    fn handle_mouse_event_direct_in_framed_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let Some(layout) = self.page().framed().layout(area) else {
            return false;
        };
        matches!(
            self.handle_mouse_event_shared(mouse_event, layout.body),
            ConditionalUpdate::NeedsRedraw
        )
    }

    fn handle_mouse_event_direct_in_content_only_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let Some(layout) = self.page().content_only().layout(area) else {
            return false;
        };
        matches!(
            self.handle_mouse_event_shared(mouse_event, layout.body),
            ConditionalUpdate::NeedsRedraw
        )
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
        area: Rect,
    ) -> ConditionalUpdate {
        let Some(layout) = self.page().framed().layout(area) else {
            return ConditionalUpdate::NoRedraw;
        };
        self.handle_mouse_event_shared(mouse_event, layout.body)
    }

    fn update_hover(&mut self, mouse_pos: (u16, u16), _area: Rect) -> bool {
        let _ = mouse_pos;
        false
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let total = self
            .content_line_count()
            .saturating_add((SUMMARY_LINE_COUNT + FOOTER_LINE_COUNT + 2) as u16);
        total.max(9)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.framed().render(area, buf);
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
        let area = Rect::new(0, 0, 60, 7);
        let mut buf = Buffer::empty(area);

        view.scroll_offset = view.selected_body_line(2);
        view.content_only().render(area, &mut buf);
        let layout = view.page().content_only().layout(area).expect("layout");

        assert_eq!(view.hit_test_in_body(layout.body, 2, 0), Some(2));
        assert_eq!(view.hit_test_in_body(layout.body, 2, 1), Some(3));
        assert_eq!(view.hit_test_in_body(layout.body, 2, 2), None);
    }

    #[test]
    fn ensure_selected_visible_uses_body_rows() {
        let mut view = make_view(
            ModelSelectionTarget::Session,
            vec![preset("gpt-5.3-codex"), preset("gpt-5.4"), preset("gpt-5.5")],
        );

        view.visible_body_rows.set(2);
        view.selected_index = 4;
        view.ensure_selected_visible();

        assert_eq!(view.scroll_offset, view.selected_body_line(4).saturating_sub(1));
    }

    #[test]
    fn render_without_frame_draws_summary_in_header_area() {
        let view = make_view(ModelSelectionTarget::Session, vec![preset("gpt-5.3-codex")]);
        let area = Rect::new(0, 0, 60, 7);
        let mut buf = Buffer::empty(area);

        view.content_only().render(area, &mut buf);

        let top_row: String = (0..area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect();
        assert!(top_row.contains("Current model:"));
    }

    #[test]
    fn content_only_hit_testing_uses_content_geometry_not_framed_geometry() {
        let view = make_view(ModelSelectionTarget::Session, vec![preset("gpt-5.3-codex")]);
        let area = Rect::new(0, 0, 40, 12);

        let content_layout = view.page().content_only().layout(area).expect("layout");
        let framed_layout = view.page().framed().layout(area).expect("layout");

        let x = content_layout.body.x;
        let y = content_layout.body.y.saturating_add(2); // Fast Mode selectable row

        assert_eq!(
            view.hit_test_in_body(content_layout.body, x, y),
            Some(0)
        );
        assert_eq!(view.hit_test_in_body(framed_layout.body, x, y), None);
    }
}
