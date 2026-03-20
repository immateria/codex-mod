use std::path::Path;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Paragraph, Widget, Wrap};
use ratatui_image::{Image, Resize};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::FilterType;
use image::ImageReader;

use super::*;
use super::super::card_style::CARD_ACCENT_WIDTH;
use crate::colors;

pub(super) struct ScreenshotLayout {
    pub(super) start_row: usize,
    pub(super) height_rows: usize,
    pub(super) width_cols: usize,
    pub(super) indent_cols: usize,
}

impl BrowserSessionCell {
    pub(crate) fn ensure_picker_initialized(
        &self,
        picker: Option<Picker>,
        font_size: (u16, u16),
    ) {
        let mut slot = self.cached_picker.borrow_mut();
        if slot.is_some() {
            return;
        }
        if let Some(p) = picker {
            *slot = Some(p);
        } else {
            *slot = Some(Picker::from_fontsize(font_size));
        }
    }

    pub(super) fn compute_screenshot_layout(&self, body_width: usize) -> Option<ScreenshotLayout> {
        self.screenshot_path.as_ref()?;

        if body_width
            < SCREENSHOT_LEFT_PAD
                + SCREENSHOT_MIN_WIDTH
                + SCREENSHOT_GAP
                + MIN_TEXT_WIDTH
                + TEXT_RIGHT_PADDING
        {
            return None;
        }

        let max_screenshot = body_width.saturating_sub(
            SCREENSHOT_LEFT_PAD + MIN_TEXT_WIDTH + SCREENSHOT_GAP + TEXT_RIGHT_PADDING,
        );
        if max_screenshot < SCREENSHOT_MIN_WIDTH {
            return None;
        }

        let mut screenshot_cols = max_screenshot;
        screenshot_cols = screenshot_cols.clamp(SCREENSHOT_MIN_WIDTH, SCREENSHOT_MAX_WIDTH);

        let rows = self.compute_screenshot_rows(screenshot_cols)?;
        Some(ScreenshotLayout {
            start_row: 0,
            height_rows: rows,
            width_cols: screenshot_cols,
            indent_cols: SCREENSHOT_LEFT_PAD + screenshot_cols + SCREENSHOT_GAP,
        })
    }

    fn ensure_picker(&self) -> Picker {
        let mut picker_ref = self.cached_picker.borrow_mut();
        if picker_ref.is_none() {
            *picker_ref = Some(Picker::from_fontsize((8, 16)));
        }
        picker_ref
            .as_ref()
            .cloned()
            .unwrap_or_else(|| Picker::from_fontsize((8, 16)))
    }

    fn compute_screenshot_rows(&self, screenshot_cols: usize) -> Option<usize> {
        if screenshot_cols == 0 {
            return None;
        }
        let path = Path::new(self.screenshot_path.as_ref()?);

        let picker = self.ensure_picker();
        let (cell_w, cell_h) = picker.font_size();
        if cell_w == 0 || cell_h == 0 {
            return Some(MIN_SCREENSHOT_ROWS);
        }

        let (img_w, img_h) = match image::image_dimensions(path) {
            Ok(dim) if dim.0 > 0 && dim.1 > 0 => dim,
            _ => return Some(MIN_SCREENSHOT_ROWS),
        };

        let cols = screenshot_cols as u32;
        if cols == 0 {
            return None;
        }

        let cw = cell_w as u32;
        let ch = cell_h as u32;

        let rows_by_w = (cols * cw * img_h) as f64 / (img_w * ch) as f64;
        let rows = rows_by_w.ceil().max(1.0) as usize;
        Some(rows.clamp(MIN_SCREENSHOT_ROWS, MAX_SCREENSHOT_ROWS))
    }

    pub(super) fn render_screenshot_preview(
        &self,
        area: Rect,
        buf: &mut Buffer,
        skip_rows: u16,
        layout: &ScreenshotLayout,
        path_str: &str,
    ) {
        let accent_width = CARD_ACCENT_WIDTH.min(area.width as usize) as u16;
        if accent_width >= area.width {
            return;
        }

        let viewport_top = skip_rows as usize;
        let viewport_bottom = viewport_top + area.height as usize;
        let shot_top = layout.start_row;
        let shot_bottom = layout.start_row + layout.height_rows;

        if shot_bottom <= viewport_top || shot_top >= viewport_bottom {
            return;
        }

        let visible_top = shot_top.max(viewport_top);
        let visible_bottom = shot_bottom.min(viewport_bottom);
        if visible_bottom <= visible_top {
            return;
        }

        let body_width = area.width.saturating_sub(accent_width);
        if body_width == 0 {
            return;
        }

        let left_pad = SCREENSHOT_LEFT_PAD.min(body_width as usize) as u16;
        if body_width <= left_pad {
            return;
        }

        let usable_width = body_width.saturating_sub(left_pad);
        let screenshot_width = layout.width_cols.min(usable_width as usize) as u16;
        if screenshot_width == 0 {
            return;
        }

        let path = Path::new(path_str);
        let rows_to_copy = (visible_bottom - visible_top) as u16;
        if rows_to_copy == 0 {
            return;
        }

        let dest_x = area.x + accent_width + left_pad;
        let dest_y = area.y + (visible_top - viewport_top) as u16;
        let placeholder_area = Rect {
            x: dest_x,
            y: dest_y,
            width: screenshot_width,
            height: rows_to_copy,
        };

        if !path.exists() {
            self.render_screenshot_placeholder(path, placeholder_area, buf);
            return;
        }

        let full_height = layout.height_rows as u16;
        if full_height == 0 {
            return;
        }

        let picker = self.ensure_picker();
        let supports_partial_render = matches!(picker.protocol_type(), ProtocolType::Halfblocks);
        let is_partially_visible = visible_top != shot_top || visible_bottom != shot_bottom;
        if is_partially_visible && !supports_partial_render {
            self.render_screenshot_placeholder(path, placeholder_area, buf);
            return;
        }

        if !is_partially_visible {
            let protocol_target = Rect::new(0, 0, screenshot_width, full_height);
            if self.ensure_protocol(path, protocol_target, &picker).is_err() {
                self.render_screenshot_placeholder(path, placeholder_area, buf);
                return;
            }
            let dest_target = Rect::new(dest_x, dest_y, screenshot_width, full_height);
            if let Some((_, _, protocol)) = self.cached_image_protocol.borrow_mut().as_mut() {
                let image = Image::new(protocol);
                image.render(dest_target, buf);
            } else {
                self.render_screenshot_placeholder(path, placeholder_area, buf);
            }
            return;
        }

        if !supports_partial_render {
            self.render_screenshot_placeholder(path, placeholder_area, buf);
            return;
        }

        let offscreen = match self.render_screenshot_buffer(path, screenshot_width, full_height) {
            Ok(buffer) => buffer,
            Err(_) => {
                self.render_screenshot_placeholder(path, placeholder_area, buf);
                return;
            }
        };

        let src_start_row = (visible_top - shot_top) as u16;
        let area_bottom = area.y + area.height;
        let area_right = area.x + area.width;

        for row in 0..rows_to_copy {
            let dest_row = dest_y + row;
            if dest_row >= area_bottom {
                break;
            }
            let src_row = src_start_row + row;
            for col in 0..screenshot_width {
                let dest_col = dest_x + col;
                if dest_col >= area_right {
                    break;
                }
                let Some(src_cell) = offscreen.cell((col, src_row)) else {
                    continue;
                };
                if let Some(dest_cell) = buf.cell_mut((dest_col, dest_row)) {
                    *dest_cell = src_cell.clone();
                }
            }
        }
    }

    fn render_screenshot_placeholder(&self, path: &Path, area: Rect, buf: &mut Buffer) {
        use ratatui::style::{Modifier, Style};
        use ratatui::widgets::{Block, Borders};

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("screenshot");
        let placeholder_text = format!("Screenshot:\n{filename}");

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::info()))
            .title("Browser");
        let inner = block.inner(area);
        block.render(area, buf);
        Paragraph::new(placeholder_text)
            .style(
                Style::default()
                    .fg(colors::text_dim())
                    .add_modifier(Modifier::ITALIC),
            )
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }

    fn render_screenshot_buffer(&self, path: &Path, width: u16, height: u16) -> Result<Buffer, ()> {
        if width == 0 || height == 0 {
            return Err(());
        }

        let picker = self.ensure_picker();
        let target = Rect::new(0, 0, width, height);
        self.ensure_protocol(path, target, &picker)?;

        let mut buffer = Buffer::empty(target);
        if let Some((_, _, protocol)) = self.cached_image_protocol.borrow_mut().as_mut() {
            let image = Image::new(protocol);
            image.render(target, &mut buffer);
            Ok(buffer)
        } else {
            Err(())
        }
    }

    fn ensure_protocol(&self, path: &Path, target: Rect, picker: &Picker) -> Result<(), ()> {
        let mut cache = self.cached_image_protocol.borrow_mut();
        let needs_recreate = match cache.as_ref() {
            Some((cached_path, cached_rect, _)) => cached_path != path || *cached_rect != target,
            None => true,
        };

        if needs_recreate {
            let dyn_img = match ImageReader::open(path) {
                Ok(reader) => reader.decode().map_err(|_| ())?,
                Err(_) => return Err(()),
            };
            let protocol = picker
                .new_protocol(dyn_img, target, Resize::Fit(Some(FilterType::Lanczos3)))
                .map_err(|_| ())?;
            *cache = Some((path.to_path_buf(), target, protocol));
        }

        Ok(())
    }
}
