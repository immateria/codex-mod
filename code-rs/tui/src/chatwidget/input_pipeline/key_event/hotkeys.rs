fn matches_tui_hotkey(hk: code_core::config_types::TuiHotkey, ev: &KeyEvent) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mods = ev.modifiers.difference(KeyModifiers::SHIFT);
    match hk {
        code_core::config_types::TuiHotkey::Legacy => false,
        code_core::config_types::TuiHotkey::Function(fk) => {
            let Some(n) = fk.as_u8() else {
                return false;
            };
            matches!(ev.code, KeyCode::F(code_n) if code_n == n)
                && !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        }
        code_core::config_types::TuiHotkey::Chord(chord) => {
            let required = match (chord.ctrl, chord.alt) {
                (true, true) => KeyModifiers::CONTROL | KeyModifiers::ALT,
                (true, false) => KeyModifiers::CONTROL,
                (false, true) => KeyModifiers::ALT,
                (false, false) => KeyModifiers::NONE,
            };
            matches!(ev.code, KeyCode::Char(c) if c.to_ascii_lowercase() == chord.key) && mods == required
        }
    }
}
