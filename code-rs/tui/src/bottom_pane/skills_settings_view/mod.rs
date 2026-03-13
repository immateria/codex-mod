use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use code_core::config::{
    find_code_home,
    set_shell_style_profile_mcp_servers,
    set_shell_style_profile_paths,
    set_shell_style_profile_skill_mode,
    ShellStyleSkillMode,
};
use code_core::config_types::{CommandSafetyProfileConfig, ShellScriptStyle, ShellStyleProfileConfig};
use code_core::protocol::Op;
use code_protocol::skills::{Skill, SkillScope};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Position, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crate::components::form_text_field::{FormTextField, InputFilter};
use crate::ui_interaction::{
    clipped_vertical_rect_with_scroll,
    next_scroll_top_with_delta,
    redraw_if,
    render_vertical_scrollbar,
    route_selectable_list_mouse_with_config,
    scroll_top_to_keep_visible,
    split_pinned_footer_layout,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::{ChromeMode, LastRenderContext};
use super::BottomPane;

mod document;
mod editor;
mod list;
mod model;
mod persistence;
mod render;

use document::*;
use model::*;

pub(crate) struct SkillsSettingsView {
    skills: Vec<Skill>,
    shell_style_profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
    selected: usize,
    mode: Mode,
    status: Option<(String, Style)>,
    app_event_tx: AppEventSender,
    complete: bool,
    // SIDE EFFECT: `render` caches the most recent area so edit-mode focus
    // scrolling can stay aligned with the last rendered layout.
    last_render: LastRenderContext,
    editor: SkillEditorState,
}

impl SkillsSettingsView {
    pub fn new(
        skills: Vec<Skill>,
        shell_style_profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
        app_event_tx: AppEventSender,
    ) -> Self {
        Self {
            skills,
            shell_style_profiles,
            selected: 0,
            mode: Mode::List,
            status: None,
            app_event_tx,
            complete: false,
            last_render: LastRenderContext::new(ChromeMode::Framed),
            editor: SkillEditorState::new(),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub(crate) fn framed(&self) -> SkillsSettingsViewFramed<'_> {
        super::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> SkillsSettingsViewContentOnly<'_> {
        super::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> SkillsSettingsViewFramedMut<'_> {
        super::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> SkillsSettingsViewContentOnlyMut<'_> {
        super::chrome_view::ContentOnlyMut::new(self)
    }
}

pub(crate) type SkillsSettingsViewFramed<'v> = super::chrome_view::Framed<'v, SkillsSettingsView>;
pub(crate) type SkillsSettingsViewContentOnly<'v> =
    super::chrome_view::ContentOnly<'v, SkillsSettingsView>;
pub(crate) type SkillsSettingsViewFramedMut<'v> =
    super::chrome_view::FramedMut<'v, SkillsSettingsView>;
pub(crate) type SkillsSettingsViewContentOnlyMut<'v> =
    super::chrome_view::ContentOnlyMut<'v, SkillsSettingsView>;

impl super::chrome_view::ChromeRenderable for SkillsSettingsView {
    fn render_in_framed_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render(area, buf);
    }

    fn render_in_content_only_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_content_only(area, buf);
    }
}

impl super::chrome_view::ChromeMouseHandler for SkillsSettingsView {
    fn handle_mouse_event_direct_in_framed_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_framed(mouse_event, area)
    }

    fn handle_mouse_event_direct_in_content_only_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_content_only(mouse_event, area)
    }
}

impl<'a> BottomPaneView<'a> for SkillsSettingsView {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if matches!(
            self.handle_key_event_with_result(pane, key_event),
            ConditionalUpdate::NeedsRedraw
        ) {
            pane.request_redraw();
        }
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
        redraw_if(
            self.framed_mut()
                .handle_mouse_event_direct(mouse_event, area),
        )
    }

    fn is_complete(&self) -> bool {
        self.is_complete()
    }

    fn desired_height(&self, _width: u16) -> u16 {
        SKILLS_SETTINGS_VIEW_HEIGHT
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.framed().render(area, buf);
    }
}
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::sync::mpsc::channel;

    fn make_view(
        profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
    ) -> SkillsSettingsView {
        let (tx, _rx) = channel();
        SkillsSettingsView::new(Vec::new(), profiles, AppEventSender::new(tx))
    }

    fn mouse_left_click(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn mouse_scroll_down(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn mouse_move(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Moved,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn paste_is_ignored_in_list_mode() {
        let mut view = make_view(HashMap::new());
        assert!(!view.handle_paste_direct("zsh".to_string()));
    }

    #[test]
    fn paste_marks_style_resource_fields_dirty() {
        let mut view = make_view(HashMap::new());
        view.start_new_skill();

        assert!(!view.editor.style_references_dirty);
        assert!(!view.editor.style_skill_roots_dirty);

        view.editor.focus = Focus::StyleReferences;
        assert!(view.handle_paste_direct("docs/shell/zsh.md".to_string()));
        assert_eq!(view.editor.style_references_field.text(), "docs/shell/zsh.md");
        assert!(view.editor.style_references_dirty);

        view.editor.focus = Focus::StyleSkillRoots;
        assert!(view.handle_paste_direct("skills/zsh".to_string()));
        assert_eq!(view.editor.style_skill_roots_field.text(), "skills/zsh");
        assert!(view.editor.style_skill_roots_dirty);
    }

    #[test]
    fn style_paste_loads_profile_paths_when_not_dirty() {
        let mut profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig> = HashMap::new();
        profiles.insert(
            ShellScriptStyle::Zsh,
            ShellStyleProfileConfig {
                references: vec![PathBuf::from("docs/shell/zsh.md")],
                skill_roots: vec![PathBuf::from("skills/zsh")],
                mcp_servers: code_core::config_types::ShellStyleMcpConfig {
                    include: vec!["termux".to_string()],
                    exclude: vec!["legacy".to_string()],
                },
                ..Default::default()
            },
        );

        let mut view = make_view(profiles);
        view.start_new_skill();
        view.editor.focus = Focus::Style;

        assert!(view.handle_paste_direct("zsh".to_string()));
        assert_eq!(view.editor.style_field.text(), "zsh");
        assert_eq!(view.editor.style_references_field.text(), "docs/shell/zsh.md");
        assert_eq!(view.editor.style_skill_roots_field.text(), "skills/zsh");
        assert_eq!(view.editor.style_mcp_include_field.text(), "termux");
        assert_eq!(view.editor.style_mcp_exclude_field.text(), "legacy");
        assert!(!view.editor.style_references_dirty);
        assert!(!view.editor.style_skill_roots_dirty);
        assert!(!view.editor.style_mcp_include_dirty);
        assert!(!view.editor.style_mcp_exclude_dirty);
    }

    #[test]
    fn style_paste_does_not_override_manual_paths_when_dirty() {
        let mut profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig> = HashMap::new();
        profiles.insert(
            ShellScriptStyle::Zsh,
            ShellStyleProfileConfig {
                references: vec![PathBuf::from("docs/shell/zsh.md")],
                skill_roots: vec![PathBuf::from("skills/zsh")],
                mcp_servers: code_core::config_types::ShellStyleMcpConfig {
                    include: vec!["termux".to_string()],
                    exclude: vec!["legacy".to_string()],
                },
                ..Default::default()
            },
        );

        let mut view = make_view(profiles);
        view.start_new_skill();
        view.editor.focus = Focus::StyleMcpInclude;
        assert!(view.handle_paste_direct("custom-server".to_string()));
        assert!(view.editor.style_mcp_include_dirty);

        view.editor.focus = Focus::Style;
        assert!(view.handle_paste_direct("zsh".to_string()));
        assert_eq!(view.editor.style_field.text(), "zsh");
        assert_eq!(view.editor.style_mcp_include_field.text(), "custom-server");
        assert_eq!(view.editor.style_mcp_exclude_field.text(), "");
        assert_eq!(view.editor.style_references_field.text(), "");
        assert_eq!(view.editor.style_skill_roots_field.text(), "");
    }

    #[test]
    fn new_skill_fields_start_empty_for_visual_placeholders() {
        let mut view = make_view(HashMap::new());
        view.start_new_skill();

        assert_eq!(view.editor.description_field.text(), "");
        assert_eq!(view.editor.examples_field.text(), "");
        assert_eq!(view.editor.body_field.text(), "");
    }

    #[test]
    fn list_click_add_new_enters_edit_mode() {
        let mut view = make_view(HashMap::new());
        let area = Rect::new(0, 0, 120, 40);

        let list_area = SkillsSettingsView::list_area_framed(area);
        let click = mouse_left_click(list_area.x.saturating_add(1), list_area.y);
        assert!(view.framed_mut().handle_mouse_event_direct(click, area));
        assert!(matches!(view.mode, Mode::Edit));
        assert!(matches!(view.editor.focus, Focus::Name));
    }

    #[test]
    fn content_only_list_mouse_geometry_differs_from_framed() {
        let area = Rect::new(0, 0, 40, 10);
        let click = mouse_left_click(area.x.saturating_add(1), area.y.saturating_add(3));

        let mut framed_view = make_view(HashMap::new());
        assert!(!framed_view.framed_mut().handle_mouse_event_direct(click, area));
        assert!(matches!(framed_view.mode, Mode::List));

        let mut content_view = make_view(HashMap::new());
        assert!(content_view
            .content_only_mut()
            .handle_mouse_event_direct(click, area));
        assert!(matches!(content_view.mode, Mode::Edit));
    }

    #[test]
    fn edit_click_focuses_style_mcp_include_field() {
        let mut profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig> = HashMap::new();
        profiles.insert(
            ShellScriptStyle::Zsh,
            ShellStyleProfileConfig {
                mcp_servers: code_core::config_types::ShellStyleMcpConfig {
                    include: vec!["termux".to_string()],
                    exclude: vec!["legacy".to_string()],
                },
                ..Default::default()
            },
        );

        let mut view = make_view(profiles);
        view.start_new_skill();
        view.editor.style_field.set_text("zsh");
        view.set_style_resource_fields_from_profile(Some(ShellScriptStyle::Zsh));

        let area = Rect::new(0, 0, 140, 48);
        let layout = view.compute_form_layout_framed(area).expect("layout should exist");
        let click = mouse_left_click(
            layout.style_mcp_include_inner.x.saturating_add(1),
            layout.style_mcp_include_inner.y.saturating_add(1),
        );
        assert!(view.framed_mut().handle_mouse_event_direct(click, area));
        assert!(matches!(view.editor.focus, Focus::StyleMcpInclude));
    }

    #[test]
    fn scrolling_body_field_with_mouse_moves_cursor() {
        let mut view = make_view(HashMap::new());
        view.start_new_skill();
        let long_body = (0..60)
            .map(|idx| format!("line {idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        view.editor.body_field.set_text(&long_body);

        let area = Rect::new(0, 0, 140, 48);
        let layout = view.compute_form_layout_framed(area).expect("layout should exist");

        let click = mouse_left_click(
            layout.body_inner.x.saturating_add(1),
            layout.body_inner.y.saturating_add(1),
        );
        assert!(view.framed_mut().handle_mouse_event_direct(click, area));
        assert!(matches!(view.editor.focus, Focus::Body));

        let before = view.editor.body_field.cursor();
        let scroll_down = mouse_scroll_down(
            layout.body_inner.x.saturating_add(1),
            layout.body_inner.y.saturating_add(1),
        );
        assert!(view.framed_mut().handle_mouse_event_direct(scroll_down, area));
        let after = view.editor.body_field.cursor();
        assert!(after > before);
    }

    #[test]
    fn mouse_move_updates_button_hover_state() {
        let mut view = make_view(HashMap::new());
        view.start_new_skill();
        let area = Rect::new(0, 0, 140, 48);
        let layout = view.compute_form_layout_framed(area).expect("layout should exist");

        use unicode_width::UnicodeWidthStr;
        let save_x = layout
            .buttons_row
            .x
            .saturating_add(
                u16::try_from("Generate draft".width()).unwrap_or(u16::MAX)
                    + u16::try_from(
                        crate::bottom_pane::settings_ui::layout::DEFAULT_BUTTON_GAP.width(),
                    )
                    .unwrap_or(u16::MAX),
            )
            .saturating_add(1);
        let hover_save = mouse_move(save_x, layout.buttons_row.y);
        assert!(view.framed_mut().handle_mouse_event_direct(hover_save, area));
        assert_eq!(view.editor.hovered_button, Some(ActionButton::Save));

        let hover_body = mouse_move(
            layout.body_inner.x.saturating_add(1),
            layout.body_inner.y.saturating_add(1),
        );
        assert!(view.framed_mut().handle_mouse_event_direct(hover_body, area));
        assert_eq!(view.editor.hovered_button, None);
    }

    #[test]
    fn short_height_editor_scrolls_focus_into_view() {
        let mut view = make_view(HashMap::new());
        view.start_new_skill();
        let area = Rect::new(0, 0, 80, 14);
        view.last_render.set(area, ChromeMode::Framed);
        assert_eq!(view.editor.edit_scroll_top, 0);

        for _ in 0..9 {
            assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        }

        assert!(matches!(view.editor.focus, Focus::Body));
        assert!(view.editor.edit_scroll_top > 0);

        let layout = view.compute_form_layout_framed(area).expect("layout should exist");
        assert!(layout.body_outer.height > 0);
    }

    #[test]
    fn edit_focus_scroll_uses_last_render_chrome_mode() {
        let mut view = make_view(HashMap::new());
        view.start_new_skill();
        view.editor.focus = Focus::Body;

        let area = Rect::new(0, 0, 2, 10);
        view.last_render.set(area, ChromeMode::ContentOnly);

        assert_eq!(view.editor.edit_scroll_top, 0);
        assert!(view.handle_key_event_direct(KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::NONE
        )));
        assert!(view.editor.edit_scroll_top > 0);
    }
}
