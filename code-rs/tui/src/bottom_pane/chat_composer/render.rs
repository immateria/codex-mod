use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

impl ChatComposer {
    fn padded_textarea_rect(input_area: Rect) -> Rect {
        Block::default()
            .borders(Borders::ALL)
            .inner(input_area)
            .inner(Margin::new(crate::layout_consts::COMPOSER_INNER_HPAD, 0))
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        if self.render_mode == ComposerRenderMode::FooterOnly {
            return self.footer_height();
        }

        let hint_height = self.footer_height();
        let input_height = self.desired_input_height(width);
        input_height + hint_height
    }

    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        if self.render_mode == ComposerRenderMode::FooterOnly {
            return None;
        }

        let (input_area, _) = self.layout_areas(area)?;
        let padded_textarea_rect = Self::padded_textarea_rect(input_area);

        let state = self.textarea_state.borrow();
        self.textarea
            .cursor_pos_with_state(padded_textarea_rect, *state)
    }

    fn desired_input_height(&self, width: u16) -> u16 {
        let content_width = width
            .saturating_sub(crate::layout_consts::effective_composer_offset(width));
        let content_lines = self.textarea.desired_height(content_width).max(1);
        (content_lines + 2).max(3)
    }

    fn layout_areas(&self, area: Rect) -> Option<(Rect, Option<Rect>)> {
        let footer_height = self.footer_height();
        let desired_input_height = self.desired_input_height(area.width);
        let available_height = area.height.saturating_sub(footer_height);
        if available_height == 0 {
            return None;
        }
        let input_height = desired_input_height.min(available_height);
        if footer_height == 0 {
            Some((
                Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: input_height,
                },
                None,
            ))
        } else {
            let [input_area, footer_area] = Layout::vertical([
                Constraint::Length(input_height),
                Constraint::Length(footer_height),
            ])
            .areas(area);
            Some((input_area, Some(footer_area)))
        }
    }

}

impl WidgetRef for ChatComposer {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        if self.render_mode == ComposerRenderMode::FooterOnly {
            let footer_height = self.footer_height();
            if footer_height == 0 {
                return;
            }
            let footer_area = Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(footer_height),
                width: area.width,
                height: footer_height,
            };
            self.render_footer(footer_area, buf);
            return;
        }

        let Some((input_area, footer_area)) = self.layout_areas(area) else {
            return;
        };

        if let Some(footer_rect) = footer_area {
            self.render_footer(footer_rect, buf);
        }
        // Draw border around input area with optional variant title when task is running
        let mut input_block = Block::default().borders(Borders::ALL);
        let mut auto_drive_border_gradient = None;
        if let Some(style) = self
            .auto_drive_style
            .as_ref()
            .filter(|_| self.auto_drive_active)
        {
            auto_drive_border_gradient = style.border_gradient;
            input_block = input_block
                .border_style(style.border_style)
                .border_type(style.border_type)
                .style(style.background_style);
        } else {
            input_block = input_block
                .border_style(Style::default().fg(crate::colors::border()))
                .border_type(BorderType::Plain)
                .style(Style::default().bg(crate::colors::background()));
        }

        if self.is_task_running && !self.embedded_mode {
            if self.auto_drive_active {
                if let Some(style) = self.auto_drive_style.as_ref()
                    && self.show_auto_drive_goal_title
                    {
                        let title_text = format!(
                            "{}Auto Drive Goal{}",
                            style.goal_title_prefix, style.goal_title_suffix
                        );
                        let title_line =
                            Line::from(Span::styled(title_text, style.title_style));
                        input_block = input_block.title(title_line);
                    }
            } else {
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                let def = crate::spinner::current_spinner();
                let spinner_str = crate::spinner::frame_at_time(def, now_ms);

                let title_line = Line::from(vec![
                    Span::raw(" "),
                    Span::styled(spinner_str, Style::default().fg(crate::colors::info())),
                    Span::styled(
                        format!(" {}... ", self.status_message),
                        Style::default().fg(crate::colors::info()),
                    ),
                ])
                .centered();
                input_block = input_block.title(title_line);
            }
        }

        input_block.render_ref(input_area, buf);
        if let Some(gradient) = auto_drive_border_gradient {
            apply_auto_drive_border_gradient(buf, input_area, gradient);
        }

        let padded_textarea_rect = Self::padded_textarea_rect(input_area);
        // Cache the textarea rect for mouse click-to-cursor positioning
        *self.last_textarea_rect.borrow_mut() = Some(padded_textarea_rect);

        let mut state = self.textarea_state.borrow_mut();
        StatefulWidgetRef::render_ref(&(&self.textarea), padded_textarea_rect, buf, &mut state);
        // Only show placeholder if there's no chat history AND no text typed
        if !self.typed_anything && self.textarea.text().is_empty() {
            let placeholder = crate::greeting::greeting_placeholder();
            Line::from(placeholder)
                .style(Style::default().dim())
                .render_ref(padded_textarea_rect, buf);
        }

        // Draw a high-contrast cursor overlay under the terminal cursor using the theme's
        // `cursor` color. This improves visibility on dark themes where the terminal's own
        // cursor color may be hard to see or user-defined.
        //
        // Implementation notes:
        // - We compute the visible cursor position using the same `state` (scroll) used to
        //   render the textarea so the overlay aligns with wrapped lines.
        // - We paint the underlying cell with bg=theme.cursor and fg=theme.background.
        //   This provides contrast regardless of light/dark theme.
        // - The hardware cursor is still positioned via `frame.set_cursor_position` at the
        //   app layer; this overlay ensures visibility independent of terminal settings.
        let state_snapshot = *state;
        drop(state); // release the borrow before computing position again
        if let Some((cx, cy)) = self
            .textarea
            .cursor_pos_with_state(padded_textarea_rect, state_snapshot)
        {
            let theme = crate::theme::current_theme();
            let cursor_bg = theme.cursor;
            if let Some(cell) = buf.cell_mut((cx, cy)) {
                // Only tint the background so the foreground glyph stays intact. Some
                // terminals (e.g. GNOME Terminal/VTE) temporarily hide the hardware
                // cursor while processing arrow keys; preserving the foreground color
                // keeps the caret location visible instead of flashing blank cells.
                cell.set_bg(cursor_bg);
                let fg_bg_ratio = contrast_ratio(theme.background, cursor_bg);
                let fg_text_ratio = contrast_ratio(theme.text_bright, cursor_bg);
                let cursor_fg = if fg_text_ratio >= fg_bg_ratio {
                    theme.text_bright
                } else {
                    theme.background
                };
                cell.set_fg(cursor_fg);
            }
        }
    }
}

fn linearize_channel(channel: u8) -> f32 {
    let srgb = channel as f32 / 255.0;
    if srgb <= 0.04045 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}

fn relative_luminance(rgb: (u8, u8, u8)) -> f32 {
    0.2126 * linearize_channel(rgb.0)
        + 0.7152 * linearize_channel(rgb.1)
        + 0.0722 * linearize_channel(rgb.2)
}

fn contrast_ratio(color_a: Color, color_b: Color) -> f32 {
    let luminance_a = relative_luminance(crate::colors::color_to_rgb(color_a));
    let luminance_b = relative_luminance(crate::colors::color_to_rgb(color_b));
    let (bright, dark) = if luminance_a >= luminance_b {
        (luminance_a, luminance_b)
    } else {
        (luminance_b, luminance_a)
    };
    (bright + 0.05) / (dark + 0.05)
}

fn apply_auto_drive_border_gradient(
    buf: &mut Buffer,
    area: Rect,
    gradient: BorderGradient,
) {
    let width = area.width as usize;
    let height = area.height as usize;
    if width == 0 || height == 0 {
        return;
    }

    if gradient.left == gradient.right {
        let color = gradient.left;
        let bottom_y = area.y + area.height.saturating_sub(1);
        for dx in 0..width {
            let x = area.x.saturating_add(dx as u16);
            let top = &mut buf[(x, area.y)];
            top.set_fg(color);

            if height > 1 {
                let bottom = &mut buf[(x, bottom_y)];
                bottom.set_fg(color);
            }
        }

        if height > 2 {
            let left_x = area.x;
            let right_x = area.x.saturating_add(area.width.saturating_sub(1));
            for dy in 1..height.saturating_sub(1) {
                let y = area.y.saturating_add(dy as u16);
                buf[(left_x, y)].set_fg(color);
                if width > 1 {
                    buf[(right_x, y)].set_fg(color);
                }
            }
        }
        return;
    }

    let horizontal_span = (width.saturating_sub(1)) as f32;
    let bottom_y = area.y + area.height.saturating_sub(1);
    for dx in 0..width {
        let ratio = if horizontal_span <= 0.0 {
            0.0
        } else {
            dx as f32 / horizontal_span
        };
        let color = lerp_gradient_color(gradient, ratio);
        let x = area.x.saturating_add(dx as u16);
        let top = &mut buf[(x, area.y)];
        top.set_fg(color);

        if height > 1 {
            let bottom = &mut buf[(x, bottom_y)];
            bottom.set_fg(color);
        }
    }

    if height <= 2 {
        return;
    }

    let left_x = area.x;
    let right_x = area.x.saturating_add(area.width.saturating_sub(1));
    for dy in 1..height.saturating_sub(1) {
        let y = area.y.saturating_add(dy as u16);
        buf[(left_x, y)].set_fg(gradient.left);
        buf[(right_x, y)].set_fg(gradient.right);
    }
}

// The gradient ratio is derived from bounded border coordinates, so clamping
// here is a narrow, intentional exception to the project-wide float-policy lint.
#[allow(clippy::disallowed_methods)]
fn lerp_gradient_color(gradient: BorderGradient, ratio: f32) -> Color {
    let clamped = ratio.clamp(0.0, 1.0);
    let (lr, lg, lb) = crate::colors::color_to_rgb(gradient.left);
    let (rr, rg, rb) = crate::colors::color_to_rgb(gradient.right);
    let mix = |a: u8, b: u8| -> u8 {
        let a = a as f32;
        let b = b as f32;
        (a + (b - a) * clamped).round().clamp(0.0, 255.0) as u8
    };
    Color::Rgb(mix(lr, rr), mix(lg, rg), mix(lb, rb))
}
