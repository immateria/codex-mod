use ratatui::style::Style;

use crate::colors;

use super::rows::StyledText;

pub(crate) fn checkbox_marker(checked: bool) -> StyledText<'static> {
    if checked {
        StyledText::new(crate::icons::checkbox_on(), Style::new().fg(colors::success()))
    } else {
        StyledText::new(crate::icons::checkbox_off(), Style::new().fg(colors::text_dim()))
    }
}

pub(crate) fn checkbox_label(checked: bool, label: &str) -> StyledText<'static> {
    if checked {
        StyledText::new(
            format!("{} {label}", crate::icons::checkbox_on()),
            Style::new().fg(colors::success()),
        )
    } else {
        StyledText::new(
            format!("{} {label}", crate::icons::checkbox_off()),
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

pub(crate) fn enabled_word_warning_off(enabled: bool) -> StyledText<'static> {
    if enabled {
        StyledText::new("enabled", Style::new().fg(colors::success()))
    } else {
        StyledText::new("disabled", Style::new().fg(colors::warning()))
    }
}

pub(crate) fn on_off_word(on: bool) -> StyledText<'static> {
    if on {
        StyledText::new("On", Style::new().fg(colors::success()))
    } else {
        StyledText::new("Off", Style::new().fg(colors::text_dim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkbox_marker_matches_expected_text_and_color() {
        let on = checkbox_marker(true);
        assert_eq!(on.text.as_ref(), crate::icons::checkbox_on());
        assert_eq!(on.style.fg, Some(colors::success()));

        let off = checkbox_marker(false);
        assert_eq!(off.text.as_ref(), crate::icons::checkbox_off());
        assert_eq!(off.style.fg, Some(colors::text_dim()));
    }

    #[test]
    fn checkbox_label_includes_text_and_uses_same_palette() {
        let on = checkbox_label(true, "read-only");
        assert_eq!(on.text.as_ref(), format!("{} read-only", crate::icons::checkbox_on()));
        assert_eq!(on.style.fg, Some(colors::success()));

        let off = checkbox_label(false, "read-only");
        assert_eq!(off.text.as_ref(), format!("{} read-only", crate::icons::checkbox_off()));
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

    #[test]
    fn on_off_word_helpers_use_expected_colors() {
        let on = on_off_word(true);
        assert_eq!(on.text.as_ref(), "On");
        assert_eq!(on.style.fg, Some(colors::success()));

        let off = on_off_word(false);
        assert_eq!(off.text.as_ref(), "Off");
        assert_eq!(off.style.fg, Some(colors::text_dim()));
    }
}
