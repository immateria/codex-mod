use super::*;

use crate::colors;

#[test]
fn enabled_value_uses_shared_toggle_palette() {
    let enabled = JsReplSettingsView::enabled_value(true);
    assert_eq!(enabled.text.as_ref(), "enabled");
    assert_eq!(enabled.style.fg, Some(colors::success()));

    let disabled = JsReplSettingsView::enabled_value(false);
    assert_eq!(disabled.text.as_ref(), "disabled");
    assert_eq!(disabled.style.fg, Some(colors::warning()));
}

#[test]
fn build_rows_omits_picker_actions_when_caps_are_off() {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            crate::platform_caps::set_test_force_no_picker(false);
        }
    }

    crate::platform_caps::set_test_force_no_picker(true);
    let _guard = Guard;

    let settings = JsReplSettingsToml::default();
    let (tx, _rx) = std::sync::mpsc::channel();
    let sender = crate::app_event_sender::AppEventSender::new(tx);
    let ticket = crate::chatwidget::BackgroundOrderTicket::test_ticket(1);
    let view = JsReplSettingsView::new(settings, false, sender, ticket);

    let rows = view.build_rows();
    assert!(!rows.contains(&RowKind::PickRuntimePath));
    assert!(!rows.contains(&RowKind::AddNodeModuleDir));
}
