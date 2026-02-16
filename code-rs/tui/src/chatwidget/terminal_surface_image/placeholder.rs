use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;
use std::path::Path;

pub(super) fn render_image_placeholder(path: &Path, area: Rect, buf: &mut Buffer, title: &str) {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
    let placeholder_text = format!("[{title}]\n{filename}");
    let widget = Paragraph::new(placeholder_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(crate::colors::info()))
                .title(title),
        )
        .style(
            Style::default()
                .fg(crate::colors::text_dim())
                .add_modifier(Modifier::ITALIC),
        )
        .wrap(Wrap { trim: true });
    Widget::render(widget, area, buf);
}
