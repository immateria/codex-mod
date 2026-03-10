use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::colors;

use super::layout::DEFAULT_FOOTER_GAP_LINES;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SettingsFrameLayout {
    pub(crate) header: Rect,
    pub(crate) header_height: usize,
    pub(crate) body: Rect,
    pub(crate) footer: Rect,
    pub(crate) visible_rows: usize,
}

fn settings_block<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::border()))
        .style(Style::default().bg(colors::background()).fg(colors::text()))
        .title(title)
        .title_alignment(Alignment::Center)
}

fn saturating_u16(value: usize) -> u16 {
    value.min(u16::MAX as usize) as u16
}

pub(crate) fn render_settings_block(area: Rect, buf: &mut Buffer, title: &str) -> bool {
    if area.width == 0 || area.height == 0 {
        return false;
    }

    let block = settings_block(title);
    Clear.render(area, buf);
    block.render(area, buf);
    true
}

pub(crate) fn compute_settings_frame_layout(
    area: Rect,
    title: &str,
    header_lines: usize,
    footer_lines: usize,
) -> Option<SettingsFrameLayout> {
    let inner = settings_block(title).inner(area);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }

    let available_height = inner.height as usize;
    let header_height = header_lines.min(available_height);
    let footer_reserved = if footer_lines == 0 || available_height <= header_height {
        0
    } else {
        DEFAULT_FOOTER_GAP_LINES + footer_lines
    };
    let body_height = available_height.saturating_sub(header_height + footer_reserved);
    let header_y = inner.y;
    let body_y = inner.y.saturating_add(saturating_u16(header_height));
    let footer_y = body_y.saturating_add(saturating_u16(body_height));
    let footer_content_y = footer_y.saturating_add(saturating_u16(DEFAULT_FOOTER_GAP_LINES));

    Some(SettingsFrameLayout {
        header: Rect::new(
            inner.x,
            header_y,
            inner.width,
            saturating_u16(header_height),
        ),
        header_height,
        body: Rect::new(
            inner.x,
            body_y,
            inner.width,
            saturating_u16(body_height),
        ),
        footer: Rect::new(
            inner.x,
            footer_content_y,
            inner.width,
            saturating_u16(footer_lines),
        ),
        visible_rows: body_height,
    })
}

pub(crate) fn render_settings_frame(
    area: Rect,
    buf: &mut Buffer,
    title: &str,
    header_lines: Vec<Line<'static>>,
    footer_lines: Vec<Line<'static>>,
) -> Option<SettingsFrameLayout> {
    if !render_settings_block(area, buf, title) {
        return None;
    }
    let layout = compute_settings_frame_layout(area, title, header_lines.len(), footer_lines.len())?;
    let base = Style::default().bg(colors::background()).fg(colors::text());

    if layout.header_height > 0 {
        Paragraph::new(header_lines)
            .style(base)
            .render(layout.header, buf);
    }

    if !footer_lines.is_empty() && layout.footer.height > 0 {
        Paragraph::new(footer_lines)
            .style(base)
            .render(layout.footer, buf);
    }

    Some(layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_frame_layout_splits_header_body_and_footer() {
        let area = Rect::new(0, 0, 40, 12);
        let layout = compute_settings_frame_layout(area, " Test ", 2, 2).expect("layout");

        assert_eq!(layout.header_height, 2);
        assert_eq!(layout.header.y, 1);
        assert_eq!(layout.header.height, 2);
        assert_eq!(layout.body.y, 3);
        assert_eq!(layout.footer.y, 9);
        assert_eq!(layout.footer.height, 2);
        assert_eq!(layout.visible_rows, layout.body.height as usize);
    }

    #[test]
    fn compute_frame_layout_allows_zero_visible_rows_when_body_is_exhausted() {
        let area = Rect::new(0, 0, 20, 4);
        let layout = compute_settings_frame_layout(area, " Test ", 2, 2).expect("layout");

        assert_eq!(layout.body.height, 0);
        assert_eq!(layout.visible_rows, 0);
    }
}
