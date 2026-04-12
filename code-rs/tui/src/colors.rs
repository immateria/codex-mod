use ratatui::style::{Color, Modifier, Style};
use crate::theme::{current_theme, palette_mode, quantize_color_for_palette, PaletteMode};

#[allow(clippy::disallowed_methods)]
const fn indexed(i: u8) -> Color {
    Color::Indexed(i)
}

#[allow(clippy::disallowed_methods)]
pub(crate) const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

// Legacy color constants - now redirect to theme
#[inline]
pub(crate) fn light_blue() -> Color {
    current_theme().primary
}

#[inline]
pub(crate) fn success_green() -> Color {
    current_theme().success
}

#[inline]
pub(crate) fn success() -> Color {
    current_theme().success
}

#[inline]
pub(crate) fn warning() -> Color {
    current_theme().warning
}

#[inline]
pub(crate) fn error() -> Color {
    current_theme().error
}

// Convenience functions for common theme colors
#[inline]
pub(crate) fn primary() -> Color {
    current_theme().primary
}

#[inline]
pub(crate) fn secondary() -> Color {
    current_theme().secondary
}

#[inline]
pub(crate) fn border() -> Color {
    current_theme().border
}

/// A slightly dimmer variant of the standard border color.
/// Blends the theme border toward the background by 30% to reduce contrast
/// while preserving the original hue relationship.
pub(crate) fn border_dim() -> Color {
    match palette_mode() {
        PaletteMode::Ansi16 => indexed(8),
        PaletteMode::Ansi256 => {
            let theme = current_theme();
            let (br, bg_g, bb) = color_to_rgb(theme.border);
            let (rr, rg, rb) = color_to_rgb(theme.background);
            let t: f32 = 0.30; // 30% toward background
            let mix = |a: u8, b: u8| -> u8 { (f32::from(a) * (1.0 - t) + f32::from(b) * t).round() as u8 };
            let r = mix(br, rr);
            let g = mix(bg_g, rg);
            let bl = mix(bb, rb);
            quantize_color_for_palette(rgb(r, g, bl))
        }
    }
}

fn is_dark_background(color: Color) -> bool {
    matches!(color, Color::Indexed(0) | Color::Black)
}

#[inline]
pub(crate) fn border_focused() -> Color {
    current_theme().border_focused
}

#[inline]
pub(crate) fn text() -> Color {
    current_theme().text
}

#[inline]
pub(crate) fn text_dim() -> Color {
    current_theme().text_dim
}

#[inline]
pub(crate) fn text_bright() -> Color {
    current_theme().text_bright
}

#[inline]
pub(crate) fn spinner() -> Color {
    current_theme().spinner
}

/// Midpoint color between `text` and `text_dim` for secondary list levels.
pub(crate) fn text_mid() -> Color {
    match palette_mode() {
        PaletteMode::Ansi16 => {
            if is_dark_background(current_theme().background) {
                indexed(7)
            } else {
                indexed(8)
            }
        }
        PaletteMode::Ansi256 => {
            let theme = current_theme();
            mix_toward(theme.text, theme.text_dim, 0.5)
        }
    }
}

#[inline]
pub(crate) fn info() -> Color {
    current_theme().info
}

// Alias for text_dim
#[inline]
pub(crate) fn dim() -> Color {
    text_dim()
}

#[inline]
pub(crate) fn background() -> Color {
    current_theme().background
}

#[inline]
pub(crate) fn selection() -> Color {
    current_theme().selection
}

// Syntax/special helpers
#[inline]
pub(crate) fn function() -> Color {
    current_theme().function
}

#[inline]
pub(crate) fn keyword() -> Color {
    current_theme().keyword
}

// Shortcut bar hint colors
#[inline]
pub(crate) fn hint_key() -> Color {
    current_theme().hint_key
}

#[inline]
pub(crate) fn hint_dismiss() -> Color {
    current_theme().hint_dismiss
}

#[inline]
pub(crate) fn hint_confirm() -> Color {
    current_theme().hint_confirm
}

#[inline]
pub(crate) fn hint_nav() -> Color {
    current_theme().hint_nav
}

// Overlay/scrim helper: a dimmed background used behind modal overlays.
// We derive it from the current theme background so it looks consistent for
// both light and dark themes.
pub(crate) fn color_to_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::White | Color::Reset => (255, 255, 255),
        Color::Gray => (192, 192, 192),
        Color::DarkGray => (128, 128, 128),
        Color::Red => (205, 49, 49),
        Color::Green => (13, 188, 121),
        Color::Yellow => (229, 229, 16),
        Color::Blue => (36, 114, 200),
        Color::Magenta => (188, 63, 188),
        Color::Cyan => (17, 168, 205),
        Color::LightRed => (255, 102, 102),
        Color::LightGreen => (102, 255, 178),
        Color::LightYellow => (255, 255, 102),
        Color::LightBlue => (102, 153, 255),
        Color::LightMagenta => (255, 102, 255),
        Color::LightCyan => (102, 255, 255),
        // Correct mapping for ANSI-256 indexes used when we quantize themes.
        // This avoids treating all Indexed colors as grayscale and fixes
        // luminance decisions (e.g., mistaking light themes for dark) on
        // terminals that don’t advertise truecolor, including some Windows setups.
        Color::Indexed(i) => ansi256_to_rgb(i),
    }
}

// Convert an ANSI-256 color index into an approximate RGB triple using the
// standard xterm 256-color palette: 0–15 ANSI, 16–231 6×6×6 cube, 232–255 grayscale.
pub(crate) fn ansi256_to_rgb(i: u8) -> (u8, u8, u8) {
    // ANSI 16 base colors
    const ANSI16: [(u8, u8, u8); 16] = [
        (0, 0, 0),       // 0 black
        (205, 0, 0),     // 1 red
        (0, 205, 0),     // 2 green
        (205, 205, 0),   // 3 yellow
        (0, 0, 205),     // 4 blue
        (205, 0, 205),   // 5 magenta
        (0, 205, 205),   // 6 cyan
        (229, 229, 229), // 7 gray
        (127, 127, 127), // 8 dark gray
        (255, 102, 102), // 9 light red
        (102, 255, 178), // 10 light green
        (255, 255, 102), // 11 light yellow
        (102, 153, 255), // 12 light blue
        (255, 102, 255), // 13 light magenta
        (102, 255, 255), // 14 light cyan
        (255, 255, 255), // 15 white
    ];

    if i < 16 {
        return ANSI16[i as usize];
    }
    if (16..=231).contains(&i) {
        // 6×6×6 color cube
        let idx = i - 16;
        let r = idx / 36;
        let g = (idx % 36) / 6;
        let b = idx % 6;
        let step = [0, 95, 135, 175, 215, 255];
        return (step[r as usize], step[g as usize], step[b as usize]);
    }
    // Grayscale ramp 232–255 maps to 8,18,28,...,238
    let level = i.saturating_sub(232);
    let v = 8 + 10 * level;
    (v, v, v)
}

pub(crate) fn blend_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    let r = (f32::from(a.0) * inv + f32::from(b.0) * t).round() as u8;
    let g = (f32::from(a.1) * inv + f32::from(b.1) * t).round() as u8;
    let bl = (f32::from(a.2) * inv + f32::from(b.2) * t).round() as u8;
    (r, g, bl)
}

/// Mix two `Color::Rgb` values by interpolation factor `t` (0.0..=1.0).
/// Returns `b` unchanged if either input is not `Color::Rgb`.
/// Unlike [`mix_toward`], this does **not** quantize for palette terminals.
pub(crate) fn mix_rgb(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let (r, g, b) = blend_rgb((ar, ag, ab), (br, bg, bb), t);
            rgb(r, g, b)
        }
        _ => b,
    }
}

/// Blend `from` toward `to` by fraction `t` (0.0..=1.0) in RGB space.
pub(crate) fn mix_toward(from: Color, to: Color, t: f32) -> Color {
    let a = color_to_rgb(from);
    let b = color_to_rgb(to);
    let (r, g, b) = blend_rgb(a, b, t);
    quantize_color_for_palette(rgb(r, g, b))
}

fn is_dark_rgb(rgb: (u8, u8, u8)) -> bool {
    relative_luminance(rgb) < 0.55
}

/// Lightly tint the terminal background toward an accent color. Matches the
/// blending used for success backgrounds in diff rendering so shared surfaces
/// stay consistent.
pub(crate) fn tint_background_toward(accent: Color) -> Color {
    let bg = color_to_rgb(background());
    let fg = color_to_rgb(accent);
    let alpha = if is_dark_rgb(bg) { 0.20 } else { 0.10 };
    let (r, g, b) = blend_rgb(bg, fg, alpha);
    rgb(r, g, b)
}

fn blend_with_black(rgb: (u8, u8, u8), alpha: f32) -> (u8, u8, u8) {
    // target = bg*(1-alpha) + black*alpha => bg*(1-alpha)
    let inv = 1.0 - alpha;
    let r = (f32::from(rgb.0) * inv).round() as u8;
    let g = (f32::from(rgb.1) * inv).round() as u8;
    let b = (f32::from(rgb.2) * inv).round() as u8;
    (r, g, b)
}

fn is_light(rgb: (u8, u8, u8)) -> bool {
    relative_luminance(rgb) >= 0.6
}

/// Rec. 709 relative luminance of an sRGB triplet, normalized to 0.0–1.0.
// Rec. 709 luma coefficients (ITU-R BT.709).
const REC709_R: f32 = 0.2126;
const REC709_G: f32 = 0.7152;
const REC709_B: f32 = 0.0722;

/// This is a simplified (gamma-unaware) version used for quick UI decisions.
/// For WCAG-compliant contrast calculations, use [`wcag_relative_luminance`].
pub(crate) fn relative_luminance(rgb: (u8, u8, u8)) -> f32 {
    (REC709_R * f32::from(rgb.0) + REC709_G * f32::from(rgb.1) + REC709_B * f32::from(rgb.2)) / 255.0
}

/// WCAG 2.x relative luminance with proper sRGB gamma linearization.
/// Used for contrast-ratio checks and accessibility-aware color decisions.
pub(crate) fn wcag_relative_luminance(r: u8, g: u8, b: u8) -> f32 {
    fn linearize(v: u8) -> f32 {
        let c = f32::from(v) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    REC709_R * linearize(r) + REC709_G * linearize(g) + REC709_B * linearize(b)
}

/// WCAG 2.x contrast ratio between two luminance values.
#[inline]
pub(crate) fn contrast_ratio_from_luminance(l1: f32, l2: f32) -> f32 {
    let (a, b) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (a + 0.05) / (b + 0.05)
}

/// Returns `true` when the current terminal background is dark (luminance < 0.5).
#[inline]
pub(crate) fn is_dark_theme() -> bool {
    relative_luminance(color_to_rgb(background())) < 0.5
}

pub(crate) fn overlay_scrim() -> Color {
    let bg = current_theme().background;
    let bg_rgb = color_to_rgb(bg);
    // For light themes, use a slightly stronger darkening; for dark themes, a gentler one.
    let alpha = if is_light(bg_rgb) { 0.18 } else { 0.10 };
    let (r, g, b) = blend_with_black(bg_rgb, alpha);
    quantize_color_for_palette(rgb(r, g, b))
}

/// Background for assistant messages: theme background moved 5% toward theme info.
pub(crate) fn assistant_bg() -> Color {
    match palette_mode() {
        PaletteMode::Ansi16 => {
            if is_dark_background(current_theme().background) {
                indexed(4)
            } else {
                indexed(7)
            }
        }
        PaletteMode::Ansi256 => {
            let theme = current_theme();
            mix_toward(theme.background, theme.info, 0.05)
        }
    }
}

/// Background for mid-turn assistant messages.
///
/// Uses a lighter tint than `assistant_bg` so progress inserts feel secondary.
pub(crate) fn assistant_mid_turn_bg() -> Color {
    match palette_mode() {
        PaletteMode::Ansi16 => assistant_bg(),
        PaletteMode::Ansi256 => {
            let theme = current_theme();
            mix_toward(theme.background, theme.info, 0.02)
        }
    }
}

/// Background for multiline code blocks rendered in assistant markdown.
///
/// New behavior: match the assistant message background so code cards feel
/// integrated with the transcript instead of appearing as stark white/black
/// panels. Borders and inner padding also use this same background.
pub(crate) fn code_block_bg() -> Color {
    assistant_bg()
}

/// Color for horizontal rules inside assistant messages.
/// Defined as halfway from the theme background toward the assistant background tint.
/// This makes the rule more pronounced than the cell background while staying subtle.
pub(crate) fn assistant_hr() -> Color {
    match palette_mode() {
        PaletteMode::Ansi16 => {
            if is_dark_background(current_theme().background) {
                indexed(8)
            } else {
                indexed(7)
            }
        }
        PaletteMode::Ansi256 => {
            let theme = current_theme();
            let cell = assistant_bg();
            let candidate = mix_toward(theme.background, theme.info, 0.15);
            let cand_l = relative_luminance(color_to_rgb(candidate));
            let cell_l = relative_luminance(color_to_rgb(cell));
            let result = if cand_l < cell_l {
                candidate
            } else {
                let (r, g, b) = blend_with_black(color_to_rgb(cell), 0.12);
                rgb(r, g, b)
            };
            quantize_color_for_palette(result)
        }
    }
}

// ── Common style helpers ─────────────────────────────────────────────
// These eliminate `Style::default().fg(crate::colors::…())` boilerplate
// that appears 300+ times across the TUI.

#[inline]
pub(crate) fn style_text() -> Style { Style::default().fg(text()) }
#[inline]
pub(crate) fn style_text_dim() -> Style { Style::default().fg(text_dim()) }
#[inline]
pub(crate) fn style_text_bright() -> Style { Style::default().fg(text_bright()) }
#[inline]
pub(crate) fn style_text_mid() -> Style { Style::default().fg(text_mid()) }
#[inline]
pub(crate) fn style_primary() -> Style { Style::default().fg(primary()) }
#[inline]
pub(crate) fn style_secondary() -> Style { Style::default().fg(secondary()) }
#[inline]
pub(crate) fn style_success() -> Style { Style::default().fg(success()) }
#[inline]
pub(crate) fn style_success_green() -> Style { Style::default().fg(success_green()) }
#[inline]
pub(crate) fn style_warning() -> Style { Style::default().fg(warning()) }
#[inline]
pub(crate) fn style_error() -> Style { Style::default().fg(error()) }
#[inline]
pub(crate) fn style_info() -> Style { Style::default().fg(info()) }
#[inline]
pub(crate) fn style_function() -> Style { Style::default().fg(function()) }
#[inline]
pub(crate) fn style_border() -> Style { Style::default().fg(border()) }
#[inline]
pub(crate) fn style_border_dim() -> Style { Style::default().fg(border_dim()) }
#[inline]
pub(crate) fn style_light_blue() -> Style { Style::default().fg(light_blue()) }

// Background-based helpers — chain .fg() for combined styles.
#[inline]
pub(crate) fn style_on_background() -> Style { Style::default().bg(background()) }
#[inline]
pub(crate) fn style_on_overlay_scrim() -> Style { Style::default().bg(overlay_scrim()) }
#[inline]
pub(crate) fn style_on_selection() -> Style { Style::default().bg(selection()) }

// Combined fg+bg helpers for the most common pairings.
#[inline]
pub(crate) fn style_text_on_bg() -> Style { Style::default().fg(text()).bg(background()) }
#[inline]
pub(crate) fn style_border_on_bg() -> Style { Style::default().fg(border()).bg(background()) }
#[inline]
pub(crate) fn style_border_dim_on_bg() -> Style { Style::default().fg(border_dim()).bg(background()) }

// Bold helpers for the most common bold+color combinations.
#[inline]
pub(crate) fn style_text_bold() -> Style { Style::default().fg(text()).add_modifier(Modifier::BOLD) }
#[inline]
pub(crate) fn style_primary_bold() -> Style { Style::default().fg(primary()).add_modifier(Modifier::BOLD) }
#[inline]
pub(crate) fn style_error_bold() -> Style { Style::default().fg(error()).add_modifier(Modifier::BOLD) }
