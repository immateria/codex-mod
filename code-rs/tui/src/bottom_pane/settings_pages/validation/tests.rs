use super::*;

use std::sync::mpsc;

use ratatui::layout::Rect;

use crate::app_event::AppEvent;
use crate::bottom_pane::settings_ui::line_runs::selection_id_at;

fn make_view_with_tools(groups_enabled: bool, tool_names: &[&'static str]) -> ValidationSettingsView {
    let (tx, _rx) = mpsc::channel::<AppEvent>();
    let groups = vec![(
        GroupStatus {
            group: ValidationGroup::Functional,
            name: "Functional",
        },
        groups_enabled,
    )];
    let tools = tool_names
        .iter()
        .map(|name| ToolRow {
            status: ToolStatus {
                name: *name,
                description: "Run tool",
                installed: true,
                install_hint: String::new(),
                category: ValidationCategory::Functional,
            },
            enabled: true,
            group_enabled: groups_enabled,
        })
        .collect();
    ValidationSettingsView::new(groups, tools, AppEventSender::new(tx))
}

fn make_view(groups_enabled: bool) -> ValidationSettingsView {
    make_view_with_tools(groups_enabled, &["cargo-check"])
}

#[test]
fn toggling_group_can_drop_tool_selections_and_clamps_selected_idx() {
    let mut view = make_view(true);
    view.state.selected_idx = Some(1);
    view.toggle_group(0);

    let model = view.build_model();
    assert_eq!(model.selection_kinds.len(), 1);
    view.state.clamp_selection(model.selection_kinds.len());
    assert_eq!(view.state.selected_idx, Some(0));
}

#[test]
fn selection_id_at_matches_selectable_runs() {
    let view = make_view(true);
    let runs = view.build_runs(0);
    let area = Rect::new(0, 0, 30, 3);

    assert_eq!(selection_id_at(area, 1, 0, 0, &runs), Some(0));
    assert_eq!(selection_id_at(area, 1, 1, 0, &runs), Some(1));

    let view_disabled = make_view(false);
    let runs_disabled = view_disabled.build_runs(0);
    assert_eq!(selection_id_at(area, 1, 1, 0, &runs_disabled), None);
}

#[test]
fn ensure_selected_visible_clamps_scroll_top_within_section() {
    let mut view = make_view_with_tools(true, &["a", "b", "c", "d", "e"]);
    // Select the last tool row: group header=0, tool rows=1..=5.
    view.state.selected_idx = Some(5);
    view.state.scroll_top = 0;
    let model = view.build_model();
    view.ensure_selected_visible(&model, 3);

    // Section has 6 lines (header + 5 tools), so max scroll top is 3 for a 3-row viewport.
    assert_eq!(view.state.scroll_top, 3);
}

