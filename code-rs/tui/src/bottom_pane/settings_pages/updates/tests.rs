use super::*;

#[test]
fn automatic_upgrades_row_uses_shared_toggle_word() {
    let row = UpdateSettingsView::auto_upgrade_row(false);
    let value = row.value.expect("toggle value");
    assert_eq!(value.text.as_ref(), "disabled");
    assert_eq!(value.style.fg, Some(colors::text_dim()));
}

