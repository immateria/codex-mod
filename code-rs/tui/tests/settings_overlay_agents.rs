use code_tui::test_helpers::{render_chat_widget_to_vt100, ChatWidgetHarness};

#[test]
fn agents_overlay_editor_stays_in_overlay() {
    let mut harness = ChatWidgetHarness::new();

    harness.open_agents_settings_overlay();
    let _initial_frame = render_chat_widget_to_vt100(&mut harness, 100, 28);

    assert!(
        harness.is_settings_overlay_visible(),
        "Settings overlay should be visible after opening",
    );
    assert!(harness.settings_overlay_is_agents_active());

    harness.show_agent_editor("code");
    let overlay_frame = render_chat_widget_to_vt100(&mut harness, 100, 28);

    assert!(
        harness.is_settings_overlay_visible(),
        "Settings overlay should remain visible after showing agent editor:\n{overlay_frame}"
    );
    assert!(harness.settings_overlay_is_agents_active());
    assert!(
        harness.agents_settings_is_agent_editor_active(),
        "Agent editor should be active inside the overlay:\n{overlay_frame}"
    );
    assert!(
        !harness.is_bottom_pane_active(),
        "Bottom pane should not activate while the agent editor is embedded in the overlay:\n{overlay_frame}"
    );
}
