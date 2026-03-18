use super::*;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::settings_pages::model::ModelSelectionTarget;
use code_common::model_presets::{ModelPreset, ReasoningEffortPreset};
use code_core::config_types::{ContextMode, ReasoningEffort};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
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
    assert_eq!(view.data.entry_line(2), 16);
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
        vec![
            preset_with_effort("gpt-5.4", ReasoningEffort::Medium),
            preset_with_effort("gpt-5.4", ReasoningEffort::High),
        ],
    );
    let area = Rect::new(0, 0, 60, 7);
    let mut buf = Buffer::empty(area);

    view.scroll_offset = view.selected_body_line(2);
    view.content_only().render(area, &mut buf);
    let layout = view
        .page()
        .layout_in_chrome(crate::bottom_pane::chrome::ChromeMode::ContentOnly, area)
        .expect("layout");

    let x = layout.body.x.saturating_add(2);
    let y0 = layout.body.y;
    assert_eq!(view.hit_test_in_body(layout.body, x, y0), Some(2));
    assert_eq!(
        view.hit_test_in_body(layout.body, x, y0.saturating_add(1)),
        Some(3)
    );
    assert_eq!(view.hit_test_in_body(layout.body, x, y0.saturating_add(2)), None);
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

    let top_row: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
    assert!(top_row.contains("Current model:"));
}

#[test]
fn content_only_hit_testing_uses_content_geometry_not_framed_geometry() {
    let view = make_view(ModelSelectionTarget::Session, vec![preset("gpt-5.3-codex")]);
    let area = Rect::new(0, 0, 40, 12);

    let content_layout = view
        .page()
        .layout_in_chrome(crate::bottom_pane::chrome::ChromeMode::ContentOnly, area)
        .expect("layout");
    let framed_layout = view
        .page()
        .layout_in_chrome(crate::bottom_pane::chrome::ChromeMode::Framed, area)
        .expect("layout");

    let x = content_layout.body.x;
    let y = content_layout.body.y.saturating_add(2); // Fast Mode selectable row

    assert_eq!(view.hit_test_in_body(content_layout.body, x, y), Some(0));
    assert_eq!(view.hit_test_in_body(framed_layout.body, x, y), None);
}
