use super::*;

use std::sync::mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;
use crate::bottom_pane::chrome::ChromeMode;

fn selected_scroll_state(idx: usize) -> ScrollState {
    let mut state = ScrollState::new();
    state.selected_idx = Some(idx);
    state
}

fn buffer_lines(buf: &ratatui::buffer::Buffer, area: ratatui::layout::Rect) -> Vec<String> {
    let mut out = Vec::new();
    for y in area.y..area.y.saturating_add(area.height) {
        let mut line = String::with_capacity(area.width as usize);
        for x in area.x..area.x.saturating_add(area.width) {
            let symbol = buf[(x, y)].symbol();
            line.push(symbol.chars().next().unwrap_or(' '));
        }
        out.push(line);
    }
    out
}

#[test]
fn ctrl_s_emits_update_feature_flags_with_selected_toggle() {
    let mut features = FeaturesToml::default();
    features.entries.insert("apps".to_string(), true);

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = ExperimentalFeaturesSettingsView::new(None, features, app_event_tx);

    let apps_idx = view
        .rows
        .iter()
        .position(|row| row.key == "apps")
        .expect("apps feature row");
    view.list_state.set(selected_scroll_state(apps_idx));

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)));

    match rx.try_recv().expect("UpdateFeatureFlags") {
        AppEvent::UpdateFeatureFlags { updates } => {
            assert_eq!(updates.get("apps").copied(), Some(false));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn esc_marks_view_complete() {
    let features = FeaturesToml::default();
    let (tx, _rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = ExperimentalFeaturesSettingsView::new(None, features, app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(view.is_complete());
}

#[test]
fn detail_pane_wraps_without_splitting_words() {
    let features = FeaturesToml::default();
    let (tx, _rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let view = ExperimentalFeaturesSettingsView::new(None, features, app_event_tx);

    let guardian_idx = view
        .rows
        .iter()
        .position(|row| row.key == "guardian_approval")
        .expect("guardian_approval feature row");
    view.list_state.set(selected_scroll_state(guardian_idx));

    let page = view.overview_page();
    let area = ratatui::layout::Rect::new(0, 0, 96, 18);
    let layout = page
        .layout_in_chrome(ChromeMode::Framed, area)
        .expect("layout");
    let body_layout = page.menu_body_layout(layout.body);
    let detail = body_layout.detail.expect("detail pane enabled");

    let mut buf = ratatui::buffer::Buffer::empty(area);
    view.render_framed(area, &mut buf);

    let detail_text = buffer_lines(&buf, detail)
        .into_iter()
        .map(|line| line.trim().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !detail_text.contains("secu\nrity"),
        "expected word wrapping, got split word:\n{detail_text}"
    );
    assert!(
        !detail_text.contains("agent o\nn "),
        "expected word wrapping, got split word:\n{detail_text}"
    );
}
