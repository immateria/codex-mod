#![allow(clippy::disallowed_methods)]

use ratatui::buffer::Buffer;
use ratatui::prelude::*;
// Paragraph/Widget previously used; manual cell writes now keep static layer intact.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum IntroArtSize {
    Large,
    Medium,
    Small,
    Tiny,
}

const LARGE_MIN_WIDTH: u16 = 80;
const MEDIUM_MIN_WIDTH: u16 = 56;
const SMALL_MIN_WIDTH: u16 = 50;
const LARGE_MIN_HEIGHT: u16 = 28;
const MEDIUM_MIN_HEIGHT: u16 = 21;
const SMALL_MIN_HEIGHT: u16 = 19;
const ANIMATED_CHARS: &[char] = &['█'];
pub(crate) const DEFAULT_BRAND_TITLE: &str = "Every Code";

type CharGrid = Vec<Vec<char>>;
type BoolGrid = Vec<Vec<bool>>;
type LineMasks = (CharGrid, BoolGrid, BoolGrid, usize, usize);

struct OverlayRenderInputs<'a> {
    chars: &'a [Vec<char>],
    mask: &'a [Vec<bool>],
    border: &'a [Vec<bool>],
}

#[derive(Copy, Clone)]
struct OverlayRenderState {
    reveal_x_outline: isize,
    reveal_x_fill: isize,
    shine_x: isize,
    shine_band: isize,
    fade: f32,
    frame: u32,
    alpha: Option<f32>,
}

pub fn intro_art_size_for_width(width: u16) -> IntroArtSize {
    if width >= LARGE_MIN_WIDTH {
        IntroArtSize::Large
    } else if width >= MEDIUM_MIN_WIDTH {
        IntroArtSize::Medium
    } else if width >= SMALL_MIN_WIDTH {
        IntroArtSize::Small
    } else {
        IntroArtSize::Tiny
    }
}

pub(crate) fn intro_art_size_for_area(width: u16, height: u16) -> IntroArtSize {
    if width >= LARGE_MIN_WIDTH && height >= LARGE_MIN_HEIGHT {
        IntroArtSize::Large
    } else if width >= MEDIUM_MIN_WIDTH && height >= MEDIUM_MIN_HEIGHT {
        IntroArtSize::Medium
    } else if width >= SMALL_MIN_WIDTH && height >= SMALL_MIN_HEIGHT {
        IntroArtSize::Small
    } else {
        IntroArtSize::Tiny
    }
}

pub fn intro_art_height(size: IntroArtSize) -> u16 {
    match size {
        IntroArtSize::Large => 28,
        IntroArtSize::Medium => 21,
        IntroArtSize::Small => 19,
        IntroArtSize::Tiny => 7,
    }
}

pub(crate) fn render_intro_animation_with_size_and_alpha_offset(
    area: Rect,
    buf: &mut Buffer,
    t: f32,
    alpha: f32,
    size: IntroArtSize,
    brand_title: &str,
    version: &str,
    row_offset: u16,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let t = t.clamp(0.0, 1.0);
    let alpha = alpha.clamp(0.0, 1.0);
    let outline_p = smoothstep(0.00, 0.60, t);
    let fill_p = smoothstep(0.35, 0.95, t);
    let fade = smoothstep(0.90, 1.00, t);
    let scan_p = smoothstep(0.55, 0.85, t);
    let frame = (t * 60.0) as u32;

    let mut lines = welcome_lines(size, brand_title, version, area.width);
    let full_width = lines.iter().map(|line| line.chars().count()).max().unwrap_or(0);
    let start = row_offset as usize;
    if start >= lines.len() {
        return;
    }
    let end = (start + area.height as usize).min(lines.len());
    if start > 0 || end < lines.len() {
        lines = lines[start..end].to_vec();
        if full_width > 0 {
            for line in &mut lines {
                let len = line.chars().count();
                if len < full_width {
                    line.push_str(&" ".repeat(full_width - len));
                }
            }
        }
    }
    let (char_mask, anim_mask, shadow_mask, w, h) =
        lines_masks(&lines, |ch| ANIMATED_CHARS.contains(&ch));
    if w == 0 || h == 0 {
        return;
    }
    let border = compute_border(&anim_mask);

    let mut render_area = area;
    render_area.height = h.min(render_area.height as usize) as u16;

    let bg = crate::colors::background();
    for y in render_area.y..render_area.y.saturating_add(render_area.height) {
        for x in render_area.x..render_area.x.saturating_add(render_area.width) {
            buf[(x, y)].set_bg(bg);
        }
    }

    let reveal_x_outline = (w as f32 * outline_p).round() as isize;
    let reveal_x_fill = (w as f32 * fill_p).round() as isize;
    let reveal_x_shadow = reveal_x_outline;

    render_static_lines(
        &lines,
        &shadow_mask,
        render_area,
        buf,
        alpha,
        frame,
        reveal_x_shadow,
    );

    let shine_x = (w as f32 * scan_p).round() as isize;
    let shine_band = 3isize;

    render_overlay_lines(
        OverlayRenderInputs {
            chars: &char_mask,
            mask: &anim_mask,
            border: &border,
        },
        OverlayRenderState {
            reveal_x_outline,
            reveal_x_fill,
            shine_x,
            shine_band,
            fade,
            frame,
            alpha: (alpha < 1.0).then_some(alpha),
        },
        render_area,
        buf,
    );
}

/* ---------------- welcome art ---------------- */

pub(crate) fn resolve_brand_title(brand_title: Option<&str>) -> String {
    brand_title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(DEFAULT_BRAND_TITLE)
        .to_string()
}

fn welcome_lines(
    size: IntroArtSize,
    brand_title: &str,
    version: &str,
    available_width: u16,
) -> Vec<String> {
    let resolved_brand_title = resolve_brand_title(Some(brand_title));
    dynamic_welcome_lines(
        size,
        &resolved_brand_title,
        version,
        available_width as usize,
    )
}

const BLOCK_FONT_HEIGHT: usize = 5;
const SHADOW_X_OFFSET: usize = 2;
const SHADOW_Y_OFFSET: usize = 1;
type BlockGlyph = [&'static str; BLOCK_FONT_HEIGHT];

fn dynamic_welcome_lines(
    size: IntroArtSize,
    brand_title: &str,
    version: &str,
    available_width: usize,
) -> Vec<String> {
    let target_height = intro_art_height(size) as usize;
    let available_width = available_width.max(1);
    let (max_width_cap, max_scale, reserved_rows) = match size {
        IntroArtSize::Large => (74usize, 3usize, 3usize),
        IntroArtSize::Medium => (56usize, 2usize, 3usize),
        IntroArtSize::Small => (48usize, 1usize, 2usize),
        // Tiny mode is height-constrained; allow wider titles when space exists.
        IntroArtSize::Tiny => (74usize, 1usize, 1usize),
    };
    let max_width = available_width.min(max_width_cap);
    let normalized = normalize_brand_for_block_font(brand_title);
    let safe_text = if normalized.is_empty() {
        "CODE".to_string()
    } else {
        normalized
    };
    let max_block_rows = target_height.saturating_sub(reserved_rows).max(1);
    let (scale, wrapped) = choose_scale_and_wrapped_lines(
        &safe_text,
        max_width,
        max_scale,
        max_block_rows,
    );
    let block_capacity = max_block_line_capacity(scale, max_block_rows);
    let used_text_fallback = wrapped.len() > block_capacity;

    let mut core_lines = Vec::new();
    core_lines.push(center_line(
        &compose_meta_line(brand_title, version, max_width),
        max_width,
    ));
    if !matches!(size, IntroArtSize::Tiny) {
        core_lines.push(String::new());
    }

    // If the block-art rendering would clip wrapped lines (e.g. tiny layouts),
    // prefer an explicit text fallback over showing a partial first word only.
    if used_text_fallback {
        let text_lines = render_brand_text_lines(brand_title, max_width, max_block_rows);
        core_lines.extend(text_lines);
    } else {
        let block_lines = render_wrapped_block_lines(&wrapped, scale, max_width, max_block_rows);
        core_lines.extend(block_lines);
    }

    if matches!(size, IntroArtSize::Large)
        && core_lines.len().saturating_add(2) <= target_height
    {
        core_lines.push(String::new());
        core_lines.push(center_line("AI coding companion", max_width));
    }
    let valign = match size {
        IntroArtSize::Large => VerticalAlign::Center,
        // Medium mode often has significant vertical slack; keep the brand
        // anchored near the top so we don't waste scarce rows above the logo.
        IntroArtSize::Medium => VerticalAlign::Top,
        IntroArtSize::Small | IntroArtSize::Tiny => {
            if used_text_fallback {
                VerticalAlign::Center
            } else {
                // On tighter layouts, keep block-art anchored near the top so we
                // don't waste scarce rows above the logo.
                VerticalAlign::Top
            }
        }
    };
    pad_lines_vertically(core_lines, target_height, valign)
}

fn compose_meta_line(brand_title: &str, version: &str, max_width: usize) -> String {
    fit_line_to_width(&format!("{brand_title} · {version}"), max_width)
}

fn choose_scale_and_wrapped_lines(
    text: &str,
    max_width: usize,
    max_scale: usize,
    max_block_rows: usize,
) -> (usize, Vec<String>) {
    for scale in (1..=max_scale).rev() {
        let max_chars = max_chars_per_block_line(max_width, scale);
        let wrapped = wrap_brand_for_block(text, max_chars);
        let per_line_rows = (BLOCK_FONT_HEIGHT * scale).saturating_add(SHADOW_Y_OFFSET);
        let rendered_rows = wrapped
            .len()
            .saturating_mul(per_line_rows)
            .saturating_add(wrapped.len().saturating_sub(1));
        if rendered_rows <= max_block_rows {
            return (scale, wrapped);
        }
    }
    let max_chars = max_chars_per_block_line(max_width, 1);
    (1, wrap_brand_for_block(text, max_chars))
}

fn max_block_line_capacity(scale: usize, max_block_rows: usize) -> usize {
    let per_line_rows = (BLOCK_FONT_HEIGHT * scale).saturating_add(SHADOW_Y_OFFSET);
    if per_line_rows == 0 {
        return 1;
    }
    // total_rows = lines * per_line_rows + (lines - 1)
    ((max_block_rows.saturating_add(1)) / per_line_rows.saturating_add(1)).max(1)
}

fn max_chars_per_block_line(max_width: usize, scale: usize) -> usize {
    let usable_width = max_width.saturating_sub(SHADOW_X_OFFSET);
    let glyph_cell_width = (5usize * scale) + scale;
    ((usable_width + scale) / glyph_cell_width).max(1)
}

fn wrap_brand_for_block(text: &str, max_chars: usize) -> Vec<String> {
    if text.is_empty() {
        return vec!["CODE".to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for token in text.split_whitespace() {
        let token_len = token.chars().count();
        if token_len > max_chars {
            if !current.is_empty() {
                lines.push(current);
                current = String::new();
            }
            split_token_hard(token, max_chars, &mut lines);
            continue;
        }
        if current.is_empty() {
            current.push_str(token);
            continue;
        }
        let candidate_len = current.chars().count() + 1 + token_len;
        if candidate_len <= max_chars {
            current.push(' ');
            current.push_str(token);
        } else {
            lines.push(current);
            current = token.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        vec!["CODE".to_string()]
    } else {
        lines
    }
}

fn split_token_hard(token: &str, max_chars: usize, out: &mut Vec<String>) {
    if max_chars == 0 {
        return;
    }
    let chars: Vec<char> = token.chars().collect();
    let mut start = 0usize;
    while start < chars.len() {
        let end = (start + max_chars).min(chars.len());
        out.push(chars[start..end].iter().collect());
        start = end;
    }
}

fn render_brand_text_lines(brand_title: &str, max_width: usize, max_rows: usize) -> Vec<String> {
    let normalized = brand_title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if max_rows == 0 {
        return Vec::new();
    }
    if normalized.is_empty() {
        return vec![center_line("Code", max_width)];
    }

    let words: Vec<&str> = normalized.split_whitespace().collect();
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in words {
        if current.is_empty() {
            current.push_str(word);
            continue;
        }
        let candidate = format!("{current} {word}");
        if candidate.chars().count() <= max_width {
            current = candidate;
        } else {
            lines.push(center_line(&current, max_width));
            current = word.to_string();
            if lines.len() >= max_rows {
                break;
            }
        }
    }
    if lines.len() < max_rows && !current.is_empty() {
        lines.push(center_line(&fit_line_to_width(&current, max_width), max_width));
    }

    if lines.is_empty() {
        vec![center_line(&fit_line_to_width(&normalized, max_width), max_width)]
    } else {
        if lines.len() > max_rows {
            lines.truncate(max_rows);
        }
        lines
    }
}

fn render_wrapped_block_lines(
    wrapped: &[String],
    scale: usize,
    max_width: usize,
    max_block_rows: usize,
) -> Vec<String> {
    let mut rows = Vec::new();
    for (line_index, line) in wrapped.iter().enumerate() {
        let line_rows = render_block_text_lines(line, scale);
        let line_rows = apply_drop_shadow(&line_rows);
        for row in line_rows {
            if rows.len() >= max_block_rows {
                return rows;
            }
            rows.push(center_line(&row, max_width));
        }
        if line_index + 1 < wrapped.len() && rows.len() < max_block_rows {
            rows.push(String::new());
        }
    }
    while rows
        .last()
        .map(|line| line.trim().is_empty())
        .unwrap_or(false)
    {
        rows.pop();
    }
    rows
}

fn apply_drop_shadow(rows: &[String]) -> Vec<String> {
    let height = rows.len().saturating_add(SHADOW_Y_OFFSET);
    let width = rows
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        .saturating_add(SHADOW_X_OFFSET);
    if height == 0 || width == 0 {
        return Vec::new();
    }

    let mut canvas = vec![vec![' '; width]; height];
    let mut mask = vec![vec![false; width]; height];
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if ch != ' ' {
                mask[y][x] = true;
            }
        }
    }

    for y in 0..rows.len() {
        for x in 0..width.saturating_sub(SHADOW_X_OFFSET) {
            if !mask[y][x] {
                continue;
            }
            let sy = y.saturating_add(SHADOW_Y_OFFSET);
            let sx = x.saturating_add(SHADOW_X_OFFSET);
            if sy < height && sx < width && !mask[sy][sx] {
                canvas[sy][sx] = '░';
            }
        }
    }
    for y in 0..rows.len() {
        for x in 0..width.saturating_sub(SHADOW_X_OFFSET) {
            if mask[y][x] {
                canvas[y][x] = '█';
            }
        }
    }

    canvas
        .into_iter()
        .map(|row| row.into_iter().collect())
        .collect()
}

fn center_line(line: &str, max_width: usize) -> String {
    let width = line.chars().count();
    if width >= max_width {
        return fit_line_to_width(line, max_width);
    }
    let pad_left = (max_width - width) / 2;
    format!("{}{}", " ".repeat(pad_left), line)
}

fn fit_line_to_width(line: &str, max_width: usize) -> String {
    let line_width = line.chars().count();
    if line_width <= max_width {
        return line.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    for ch in line.chars().take(max_width - 1) {
        out.push(ch);
    }
    out.push('…');
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerticalAlign {
    Top,
    Center,
}

fn pad_lines_vertically(
    mut lines: Vec<String>,
    target_height: usize,
    valign: VerticalAlign,
) -> Vec<String> {
    if lines.len() >= target_height {
        lines.truncate(target_height);
        return lines;
    }
    let missing = target_height - lines.len();
    let (top_pad, bottom_pad) = match valign {
        VerticalAlign::Top => (0, missing),
        VerticalAlign::Center => {
            let top = missing / 2;
            (top, missing - top)
        }
    };
    let mut out = Vec::with_capacity(target_height);
    out.extend((0..top_pad).map(|_| String::new()));
    out.append(&mut lines);
    out.extend((0..bottom_pad).map(|_| String::new()));
    out
}

fn normalize_brand_for_block_font(input: &str) -> String {
    let mut normalized = String::new();
    let mut pending_space = false;
    for ch in input.chars() {
        let mapped = match ch {
            'a'..='z' => ch.to_ascii_uppercase(),
            'A'..='Z' | '0'..='9' | '-' | '_' | '/' | '+' | '.' | ':' => ch,
            _ if ch.is_whitespace() => ' ',
            _ => ' ',
        };
        if mapped == ' ' {
            if !normalized.is_empty() {
                pending_space = true;
            }
        } else {
            if pending_space {
                normalized.push(' ');
                pending_space = false;
            }
            normalized.push(mapped);
        }
    }
    normalized
}

fn render_block_text_lines(text: &str, scale: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new(); BLOCK_FONT_HEIGHT * scale];
    }
    let chars: Vec<char> = text.chars().collect();
    let mut rows = vec![String::new(); BLOCK_FONT_HEIGHT * scale];
    let spacer = " ".repeat(scale);
    for (char_index, ch) in chars.iter().enumerate() {
        let glyph = block_glyph(*ch);
        for (row_index, pattern) in glyph.iter().enumerate() {
            let expanded = expand_block_glyph_row(pattern, scale);
            for vertical in 0..scale {
                let row = &mut rows[(row_index * scale) + vertical];
                row.push_str(&expanded);
                if char_index + 1 < chars.len() {
                    row.push_str(&spacer);
                }
            }
        }
    }
    rows
}

fn expand_block_glyph_row(pattern: &str, scale: usize) -> String {
    let mut out = String::with_capacity(pattern.len() * scale);
    for bit in pattern.chars() {
        let pixel = if bit == '1' { '█' } else { ' ' };
        for _ in 0..scale {
            out.push(pixel);
        }
    }
    out
}

fn block_glyph(ch: char) -> BlockGlyph {
    match ch {
        'A' => ["01110", "10001", "11111", "10001", "10001"],
        'B' => ["11110", "10001", "11110", "10001", "11110"],
        'C' => ["01111", "10000", "10000", "10000", "01111"],
        'D' => ["11110", "10001", "10001", "10001", "11110"],
        'E' => ["11111", "10000", "11110", "10000", "11111"],
        'F' => ["11111", "10000", "11110", "10000", "10000"],
        'G' => ["01111", "10000", "10111", "10001", "01111"],
        'H' => ["10001", "10001", "11111", "10001", "10001"],
        'I' => ["11111", "00100", "00100", "00100", "11111"],
        'J' => ["00111", "00010", "00010", "10010", "01100"],
        'K' => ["10001", "10010", "11100", "10010", "10001"],
        'L' => ["10000", "10000", "10000", "10000", "11111"],
        'M' => ["10001", "11011", "10101", "10001", "10001"],
        'N' => ["10001", "11001", "10101", "10011", "10001"],
        'O' => ["01110", "10001", "10001", "10001", "01110"],
        'P' => ["11110", "10001", "11110", "10000", "10000"],
        'Q' => ["01110", "10001", "10001", "10011", "01111"],
        'R' => ["11110", "10001", "11110", "10010", "10001"],
        'S' => ["01111", "10000", "01110", "00001", "11110"],
        'T' => ["11111", "00100", "00100", "00100", "00100"],
        'U' => ["10001", "10001", "10001", "10001", "01110"],
        'V' => ["10001", "10001", "10001", "01010", "00100"],
        'W' => ["10001", "10001", "10101", "11011", "10001"],
        'X' => ["10001", "01010", "00100", "01010", "10001"],
        'Y' => ["10001", "01010", "00100", "00100", "00100"],
        'Z' => ["11111", "00010", "00100", "01000", "11111"],
        '0' => ["01110", "10011", "10101", "11001", "01110"],
        '1' => ["00100", "01100", "00100", "00100", "01110"],
        '2' => ["01110", "10001", "00010", "00100", "11111"],
        '3' => ["11110", "00001", "00110", "00001", "11110"],
        '4' => ["00010", "00110", "01010", "11111", "00010"],
        '5' => ["11111", "10000", "11110", "00001", "11110"],
        '6' => ["01110", "10000", "11110", "10001", "01110"],
        '7' => ["11111", "00010", "00100", "01000", "01000"],
        '8' => ["01110", "10001", "01110", "10001", "01110"],
        '9' => ["01110", "10001", "01111", "00001", "01110"],
        '-' => ["00000", "00000", "11111", "00000", "00000"],
        '_' => ["00000", "00000", "00000", "00000", "11111"],
        '/' => ["00001", "00010", "00100", "01000", "10000"],
        '+' => ["00000", "00100", "11111", "00100", "00000"],
        '.' => ["00000", "00000", "00000", "00110", "00110"],
        ':' => ["00000", "00110", "00000", "00110", "00000"],
        ' ' => ["00000", "00000", "00000", "00000", "00000"],
        _ => ["11111", "00010", "00100", "00000", "00100"],
    }
}

/* ---------------- outline fill renderer ---------------- */

fn lines_masks(
    lines: &[String],
    is_animated: impl Fn(char) -> bool,
) -> LineMasks {
    let height = lines.len();
    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);

    let mut char_mask = vec![vec![' '; width]; height];
    let mut anim_mask = vec![vec![false; width]; height];
    let mut shadow_mask = vec![vec![false; width]; height];

    for (y, line) in lines.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            if x >= width {
                break;
            }
            char_mask[y][x] = ch;
            if is_animated(ch) {
                anim_mask[y][x] = true;
            } else if ch != ' ' {
                shadow_mask[y][x] = true;
            }
        }
    }

    (char_mask, anim_mask, shadow_mask, width, height)
}

fn render_static_lines(
    lines: &[String],
    shadow_mask: &[Vec<bool>],
    area: Rect,
    buf: &mut Buffer,
    alpha: f32,
    _frame: u32,
    reveal_x_shadow: isize,
) {
    let static_target = Color::Rgb(230, 232, 235); // matches CODE/EVERY final color (#e6e8eb)
    let static_color_base = blend_to_background(static_target, alpha);
    for (row_idx, line) in lines.iter().enumerate() {
        let y = area.y + row_idx as u16;
        if y >= area.y + area.height {
            break;
        }
        for (col_idx, ch) in line.chars().enumerate() {
            let x = area.x + col_idx as u16;
            if x >= area.x + area.width {
                break;
            }
            if ch == ' ' || ANIMATED_CHARS.contains(&ch) {
                continue;
            }
            if !shadow_mask[row_idx][col_idx] {
                continue;
            }
            let xi = col_idx as isize;
            if xi > reveal_x_shadow {
                continue;
            }
            let mut utf8 = [0u8; 4];
            let sym = ch.encode_utf8(&mut utf8);
            let cell = &mut buf[(x, y)];
            cell.set_symbol(sym);
            cell.set_fg(static_color_base);
            cell.set_style(Style::default().add_modifier(Modifier::BOLD));
        }
    }
}
fn render_overlay_lines(
    inputs: OverlayRenderInputs<'_>,
    state: OverlayRenderState,
    area: Rect,
    buf: &mut Buffer,
) {
    let OverlayRenderInputs { chars, mask, border } = inputs;
    let OverlayRenderState {
        reveal_x_outline,
        reveal_x_fill,
        shine_x,
        shine_band,
        fade,
        frame,
        alpha,
    } = state;
    let h = mask.len();
    let w = mask[0].len();

    for y in 0..h {
        for x in 0..w {
            let xi = x as isize;
            let base_char = chars[y][x];

            let mut draw = false;
            let mut color = Color::Reset;

            if mask[y][x] && xi <= reveal_x_fill {
                let base = gradient_multi(x as f32 / (w.max(1) as f32));
                let dx = (xi - shine_x).abs();
                let shine =
                    (1.0 - (dx as f32 / (shine_band as f32 + 0.001)).clamp(0.0, 1.0)).powf(1.6);
                let bright = bump_rgb(base, shine * 0.30);
                color = mix_rgb(bright, Color::Rgb(230, 232, 235), fade);
                if let Some(alpha) = alpha {
                    color = blend_to_background(color, alpha);
                }
                draw = true;
            } else if border[y][x] && xi <= reveal_x_outline.max(reveal_x_fill) {
                let base = gradient_multi(x as f32 / (w.max(1) as f32));
                let period = 8usize;
                let on = ((x + y + (frame as usize)) % period) < (period / 2);
                let c = if on { bump_rgb(base, 0.22) } else { base };
                color = mix_rgb(c, Color::Rgb(235, 237, 240), fade * 0.8);
                if let Some(alpha) = alpha {
                    color = blend_to_background(color, alpha);
                }
                draw = true;
            }

            if draw {
                let target_x = area.x + x as u16;
                let target_y = area.y + y as u16;
                if target_x < area.x + area.width && target_y < area.y + area.height {
                    let cell = &mut buf[(target_x, target_y)];
                    let mut utf8 = [0u8; 4];
                    let sym = base_char.encode_utf8(&mut utf8);
                    cell.set_symbol(sym);
                    cell.set_fg(color);
                    cell.set_bg(crate::colors::background());
                    cell.set_style(Style::default().add_modifier(Modifier::BOLD));
                }
            }
        }
    }
}

// Helper function to blend colors towards background
pub(crate) fn blend_to_background(color: Color, alpha: f32) -> Color {
    if alpha >= 1.0 {
        return color;
    }
    if alpha <= 0.0 {
        return crate::colors::background();
    }

    let bg = crate::colors::background();

    match (color, bg) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let r = (r1 as f32 * alpha + r2 as f32 * (1.0 - alpha)) as u8;
            let g = (g1 as f32 * alpha + g2 as f32 * (1.0 - alpha)) as u8;
            let b = (b1 as f32 * alpha + b2 as f32 * (1.0 - alpha)) as u8;
            Color::Rgb(r, g, b)
        }
        _ => {
            if alpha > 0.5 { color } else { bg }
        }
    }
}

/* ---------------- border computation ---------------- */

fn compute_border(mask: &[Vec<bool>]) -> Vec<Vec<bool>> {
    let h = mask.len();
    let w = mask[0].len();
    let mut out = vec![vec![false; w]; h];
    for y in 0..h {
        for x in 0..w {
            if !mask[y][x] {
                continue;
            }
            let up = y == 0 || !mask[y - 1][x];
            let down = y + 1 >= h || !mask[y + 1][x];
            let left = x == 0 || !mask[y][x - 1];
            let right = x + 1 >= w || !mask[y][x + 1];
            if up || down || left || right {
                out[y][x] = true;
            }
        }
    }
    out
}

/* ================= helpers ================= */

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

pub(crate) fn mix_rgb(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            Color::Rgb(lerp_u8(ar, br, t), lerp_u8(ag, bg, t), lerp_u8(ab, bb, t))
        }
        _ => b,
    }
}

// vibrant cyan -> magenta -> amber across the word
pub(crate) fn gradient_multi(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (r1, g1, b1) = (0u8, 224u8, 255u8); // #00E0FF
    let (r2, g2, b2) = (255u8, 78u8, 205u8); // #FF4ECD
    let (r3, g3, b3) = (255u8, 181u8, 0u8); // #FFB500
    if t < 0.5 {
        Color::Rgb(
            lerp_u8(r1, r2, t * 2.0),
            lerp_u8(g1, g2, t * 2.0),
            lerp_u8(b1, b2, t * 2.0),
        )
    } else {
        Color::Rgb(
            lerp_u8(r2, r3, (t - 0.5) * 2.0),
            lerp_u8(g2, g3, (t - 0.5) * 2.0),
            lerp_u8(b2, b3, (t - 0.5) * 2.0),
        )
    }
}

fn bump_rgb(c: Color, amt: f32) -> Color {
    match c {
        Color::Rgb(r, g, b) => {
            let add = |x: u8| ((x as f32 + 255.0 * amt).min(255.0)) as u8;
            Color::Rgb(add(r), add(g), add(b))
        }
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::prelude::Rect;

    #[test]
    fn renders_default_brand_with_dynamic_art() {
        let version = format!("v{}", code_version::version());
        let rect = Rect::new(0, 0, 80, intro_art_height(IntroArtSize::Large));
        let mut buf = Buffer::empty(rect);

        render_intro_animation_with_size_and_alpha_offset(
            rect,
            &mut buf,
            1.0,
            1.0,
            IntroArtSize::Large,
            DEFAULT_BRAND_TITLE,
            &version,
            0,
        );

        let rendered = buffer_to_strings(&buf, rect);
        assert!(
            rendered
                .iter()
                .any(|line| line.contains(DEFAULT_BRAND_TITLE)),
        );
        assert!(rendered.iter().any(|line| line.contains(&version)));
        assert!(
            rendered.iter().any(|line| line.contains('█')),
            "expected dynamic block glyph rows",
        );
    }

    #[test]
    fn renders_custom_brand_and_wraps() {
        let version = format!("v{}", code_version::version());
        let rect = Rect::new(0, 0, 56, intro_art_height(IntroArtSize::Medium));
        let mut buf = Buffer::empty(rect);
        let custom_title = "Immateria Codex Termux Experimental Build";

        render_intro_animation_with_size_and_alpha_offset(
            rect,
            &mut buf,
            1.0,
            1.0,
            IntroArtSize::Medium,
            custom_title,
            &version,
            0,
        );

        let rendered = buffer_to_strings(&buf, rect);
        assert!(rendered.iter().any(|line| line.contains("Immateria")));
        assert!(
            rendered.iter().any(|line| line.contains('█')),
            "expected dynamic block glyph rows",
        );
        assert!(
            rendered.iter().any(|line| line.contains("Interactive")),
            "expected footer hint on non-tiny layouts",
        );
    }

    #[test]
    fn row_offset_shifts_visible_window() {
        let version = format!("v{}", code_version::version());
        let full_rect = Rect::new(0, 0, 56, intro_art_height(IntroArtSize::Medium));
        let mut full_buf = Buffer::empty(full_rect);

        render_intro_animation_with_size_and_alpha_offset(
            full_rect,
            &mut full_buf,
            1.0,
            1.0,
            IntroArtSize::Medium,
            "Offset Test",
            &version,
            0,
        );
        let full = buffer_to_strings(&full_buf, full_rect);

        let window_rect = Rect::new(0, 0, 56, 6);
        let mut window_buf = Buffer::empty(window_rect);
        let offset = 3;
        render_intro_animation_with_size_and_alpha_offset(
            window_rect,
            &mut window_buf,
            1.0,
            1.0,
            IntroArtSize::Medium,
            "Offset Test",
            &version,
            offset,
        );
        let window = buffer_to_strings(&window_buf, window_rect);
        assert_eq!(window[0], full[offset as usize]);
    }

    #[test]
    fn tiny_layout_falls_back_to_full_text_brand_when_block_wrap_would_clip() {
        let version = format!("v{}", code_version::version());
        let rect = Rect::new(0, 0, 40, intro_art_height(IntroArtSize::Tiny));
        let mut buf = Buffer::empty(rect);
        let custom_title = "Every Code";

        render_intro_animation_with_size_and_alpha_offset(
            rect,
            &mut buf,
            1.0,
            1.0,
            IntroArtSize::Tiny,
            custom_title,
            &version,
            0,
        );

        let rendered = buffer_to_strings(&buf, rect);
        assert!(
            rendered.iter().any(|line| line.contains(custom_title)),
            "expected tiny fallback to include full brand title",
        );
    }

    #[test]
    fn tiny_layout_uses_block_art_when_width_allows_full_title() {
        let version = format!("v{}", code_version::version());
        let rect = Rect::new(0, 0, 74, intro_art_height(IntroArtSize::Tiny));
        let mut buf = Buffer::empty(rect);
        let custom_title = "Every Code";

        render_intro_animation_with_size_and_alpha_offset(
            rect,
            &mut buf,
            1.0,
            1.0,
            IntroArtSize::Tiny,
            custom_title,
            &version,
            0,
        );

        let rendered = buffer_to_strings(&buf, rect);
        let text_line_matches = rendered
            .iter()
            .filter(|line| line.contains(custom_title))
            .count();
        assert_eq!(
            text_line_matches, 1,
            "expected only the metadata line to include the title when block-art fits",
        );
    }

    fn buffer_to_strings(buf: &Buffer, area: Rect) -> Vec<String> {
        let mut lines = Vec::new();
        for y in area.y..area.y + area.height {
            let mut line = String::with_capacity(area.width as usize);
            for x in area.x..area.x + area.width {
                let symbol = buf[(x, y)].symbol();
                let ch = symbol.chars().next().unwrap_or(' ');
                line.push(ch);
            }
            lines.push(line);
        }
        lines
    }
}
