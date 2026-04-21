use super::*;

use crate::colors;

#[test]
fn enabled_value_uses_shared_toggle_palette() {
    let enabled = ReplSettingsView::enabled_value(true);
    assert_eq!(enabled.text.as_ref(), "enabled");
    assert_eq!(enabled.style.fg, Some(colors::success()));

    let disabled = ReplSettingsView::enabled_value(false);
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

    let settings = ReplSettingsToml::default();
    let (tx, _rx) = std::sync::mpsc::channel();
    let sender = crate::app_event_sender::AppEventSender::new(tx);
    let ticket = crate::chatwidget::BackgroundOrderTicket::test_ticket(1);
    let view = ReplSettingsView::new(settings, false, sender, ticket);

    let rows = view.build_rows();
    assert!(!rows.contains(&RowKind::PickRuntimePath));
    assert!(!rows.contains(&RowKind::AddModuleDir));
}

#[test]
fn cycle_runtime_preserves_per_runtime_specs() {
    let mut runtimes = std::collections::BTreeMap::new();
    runtimes.insert(
        ReplRuntimeKindToml::Node,
        code_core::config::ReplRuntimeSpec {
            path: Some(std::path::PathBuf::from("/custom/node")),
            args: vec!["--inspect".to_string()],
            module_dirs: vec![std::path::PathBuf::from("/app/node_modules")],
        },
    );
    runtimes.insert(
        ReplRuntimeKindToml::Deno,
        code_core::config::ReplRuntimeSpec {
            path: Some(std::path::PathBuf::from("/custom/deno")),
            args: vec!["run".to_string()],
            module_dirs: Vec::new(),
        },
    );
    let settings = ReplSettingsToml {
        enabled: true,
        node_enabled: true,
        deno_enabled: true,
        python_enabled: true,
        runtime: ReplRuntimeKindToml::Node,
        runtimes,
        deno_permissions: Default::default(),
    };
    let (tx, _rx) = std::sync::mpsc::channel();
    let sender = crate::app_event_sender::AppEventSender::new(tx);
    let ticket = crate::chatwidget::BackgroundOrderTicket::test_ticket(2);
    let mut view = ReplSettingsView::new(settings, false, sender, ticket);

    view.activate_row(RowKind::RuntimeKind);

    assert_eq!(view.settings.runtime, ReplRuntimeKindToml::Deno);
    assert_eq!(
        view.current_runtime_spec().path,
        Some(std::path::PathBuf::from("/custom/deno"))
    );
    assert_eq!(
        view.settings
            .runtimes
            .get(&ReplRuntimeKindToml::Node)
            .and_then(|spec| spec.path.clone()),
        Some(std::path::PathBuf::from("/custom/node"))
    );
}
