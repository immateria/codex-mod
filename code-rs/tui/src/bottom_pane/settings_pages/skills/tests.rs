#![allow(clippy::unwrap_used)]

use super::*;
use std::sync::mpsc::channel;

fn make_view(profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>) -> SkillsSettingsView {
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
fn list_scrolls_to_keep_selection_visible() {
    let (tx, _rx) = channel();
    let skills = (0..12)
        .map(|idx| Skill {
            name: format!("Skill {idx}"),
            description: "desc".to_string(),
            path: PathBuf::from(format!("skills/{idx}/SKILL.md")),
            scope: SkillScope::User,
            content: String::new(),
        })
        .collect::<Vec<_>>();
    let mut view = SkillsSettingsView::new(skills, HashMap::new(), AppEventSender::new(tx));
    view.list_viewport_rows.set(3);

    assert_eq!(view.list_state.selected_idx, Some(0));
    assert_eq!(view.list_state.scroll_top, 0);

    for _ in 0..3 {
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
    }

    assert_eq!(view.list_state.selected_idx, Some(3));
    assert_eq!(view.list_state.scroll_top, 1);
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
                + u16::try_from(crate::bottom_pane::settings_ui::layout::DEFAULT_BUTTON_GAP.width())
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

