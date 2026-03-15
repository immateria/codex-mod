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

