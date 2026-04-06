use crate::markdown_renderer::MarkdownRenderer;
use ratatui::text::Line;
use ratatui::text::Text;

/// Render markdown into a `ratatui::Text` using our custom chat renderer.
///
/// This intentionally avoids pulldown-cmark so the TUI has a single markdown
/// rendering engine to improve over time.
pub fn render_markdown_text(input: &str) -> Text<'static> {
    let lines: Vec<Line<'static>> = MarkdownRenderer::render(input)
        .into_iter()
        // The chat history layout uses this sentinel to build code cards.
        // For standalone `Text` rendering, drop it to avoid leaking internal markers.
        .filter(|line| !is_code_lang_sentinel_line(line))
        .collect();
    Text::from(lines)
}

fn is_code_lang_sentinel_line(line: &Line<'_>) -> bool {
    let flat: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    flat.strip_prefix("⟦LANG:").is_some_and(|tail| tail.contains('⟧'))
}
