use ratatui::style::Color;

use crate::auto_drive_style::BorderGradient;
use crate::colors;

fn is_dark_theme_active() -> bool {
    colors::is_dark_theme()
}

#[allow(clippy::disallowed_methods)]
pub(super) fn text_gradient_colors(gradient: BorderGradient) -> (Color, Color) {
    if is_dark_theme_active() {
        (gradient.left, gradient.right)
    } else {
        (Color::Rgb(93, 187, 255), Color::Rgb(243, 173, 72))
    }
}

