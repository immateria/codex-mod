use ratatui::style::Style;

use crate::colors;

use super::rows::StyledText;

pub(crate) fn checkbox_marker(checked: bool) -> StyledText<'static> {
    if checked {
        StyledText::new("[x]", Style::new().fg(colors::success()))
    } else {
        StyledText::new("[ ]", Style::new().fg(colors::text_dim()))
    }
}

pub(crate) fn checkbox_label(checked: bool, label: &str) -> StyledText<'static> {
    if checked {
        StyledText::new(
            format!("[x] {label}"),
            Style::new().fg(colors::success()),
        )
    } else {
        StyledText::new(
            format!("[ ] {label}"),
            Style::new().fg(colors::text_dim()),
        )
    }
}

pub(crate) fn enabled_word(enabled: bool) -> StyledText<'static> {
    if enabled {
        StyledText::new("enabled", Style::new().fg(colors::success()))
    } else {
        StyledText::new("disabled", Style::new().fg(colors::text_dim()))
    }
}

#[allow(dead_code)]
pub(crate) fn enabled_word_warning_off(enabled: bool) -> StyledText<'static> {
    if enabled {
        StyledText::new("enabled", Style::new().fg(colors::success()))
    } else {
        StyledText::new("disabled", Style::new().fg(colors::warning()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkbox_marker_matches_expected_text_and_color() {
        let on = checkbox_marker(true);
        assert_eq!(on.text.as_ref(), "[x]");
        assert_eq!(on.style.fg, Some(colors::success()));

        let off = checkbox_marker(false);
        assert_eq!(off.text.as_ref(), "[ ]");
        assert_eq!(off.style.fg, Some(colors::text_dim()));
    }

    #[test]
    fn checkbox_label_includes_text_and_uses_same_palette() {
        let on = checkbox_label(true, "read-only");
        assert_eq!(on.text.as_ref(), "[x] read-only");
        assert_eq!(on.style.fg, Some(colors::success()));

        let off = checkbox_label(false, "read-only");
        assert_eq!(off.text.as_ref(), "[ ] read-only");
        assert_eq!(off.style.fg, Some(colors::text_dim()));
    }

    #[test]
    fn enabled_word_helpers_use_expected_colors() {
        let on = enabled_word(true);
        assert_eq!(on.text.as_ref(), "enabled");
        assert_eq!(on.style.fg, Some(colors::success()));

        let off = enabled_word(false);
        assert_eq!(off.text.as_ref(), "disabled");
        assert_eq!(off.style.fg, Some(colors::text_dim()));

        let warn_off = enabled_word_warning_off(false);
        assert_eq!(warn_off.text.as_ref(), "disabled");
        assert_eq!(warn_off.style.fg, Some(colors::warning()));
    }
}
