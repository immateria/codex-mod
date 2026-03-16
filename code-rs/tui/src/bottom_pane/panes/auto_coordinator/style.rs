use ratatui::style::Color;

use crate::auto_drive_style::BorderGradient;
use crate::colors;

fn is_dark_theme_active() -> bool {
    let (r, g, b) = colors::color_to_rgb(colors::background());
    let luminance = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0;
    luminance < 0.5
}

#[allow(clippy::disallowed_methods)]
pub(super) fn text_gradient_colors(gradient: BorderGradient) -> (Color, Color) {
    if is_dark_theme_active() {
        (gradient.left, gradient.right)
    } else {
        (Color::Rgb(93, 187, 255), Color::Rgb(243, 173, 72))
    }
}

