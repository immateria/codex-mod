use super::*;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::settings_ui::line_runs::selection_id_at;
use ratatui::layout::Rect;
use std::sync::mpsc;

fn make_view() -> ReviewSettingsView {
    let (tx, _rx) = mpsc::channel::<AppEvent>();
    ReviewSettingsView::new(ReviewSettingsInit {
        review_use_chat_model: false,
        review_model: "gpt-5.4".to_string(),
        review_reasoning: ReasoningEffort::Medium,
        review_resolve_use_chat_model: false,
        review_resolve_model: "gpt-5.4".to_string(),
        review_resolve_reasoning: ReasoningEffort::Medium,
        review_auto_resolve_enabled: true,
        review_followups: AutoResolveAttemptLimit::DEFAULT,
        auto_review_enabled: true,
        auto_review_use_chat_model: false,
        auto_review_model: "gpt-5.4".to_string(),
        auto_review_reasoning: ReasoningEffort::Medium,
        auto_review_resolve_use_chat_model: false,
        auto_review_resolve_model: "gpt-5.4".to_string(),
        auto_review_resolve_reasoning: ReasoningEffort::Medium,
        auto_review_followups: AutoResolveAttemptLimit::DEFAULT,
        app_event_tx: AppEventSender::new(tx),
    })
}

#[test]
fn selection_index_to_kind_order_is_stable() {
    let view = make_view();
    let model = view.build_model();
    assert_eq!(
        model.selection_kinds,
        vec![
            SelectionKind::ReviewEnabled,
            SelectionKind::ReviewModel,
            SelectionKind::ReviewResolveModel,
            SelectionKind::ReviewAttempts,
            SelectionKind::AutoReviewEnabled,
            SelectionKind::AutoReviewModel,
            SelectionKind::AutoReviewResolveModel,
            SelectionKind::AutoReviewAttempts,
        ]
    );
}

#[test]
fn ensure_selected_visible_clamps_scroll_within_section() {
    let mut view = make_view();
    view.state.selected_idx = Some(3);
    view.state.scroll_top = 0;
    let model = view.build_model();
    view.ensure_selected_visible(&model, 3);
    assert_eq!(view.state.scroll_top, 2);
}

#[test]
fn selection_id_at_matches_run_geometry_with_scroll() {
    let view = make_view();
    let runs = view.build_runs(usize::MAX);
    let area = Rect::new(2, 4, 20, 10);

    assert_eq!(selection_id_at(area, 3, 4, 0, &runs), None);
    assert_eq!(selection_id_at(area, 3, 5, 0, &runs), Some(0));
    assert_eq!(selection_id_at(area, 3, 6, 0, &runs), Some(1));
    assert_eq!(selection_id_at(area, 3, 4, 2, &runs), Some(1));
}
