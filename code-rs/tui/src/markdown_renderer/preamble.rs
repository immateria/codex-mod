use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use regex_lite::Regex;

/// Custom markdown renderer with full control over spacing and styling
pub struct MarkdownRenderer {
    lines: Vec<Line<'static>>,
    current_line: Vec<Span<'static>>,
    in_code_block: bool,
    code_block_lang: Option<String>,
    code_block_buf: String,
    bold_first_sentence: bool,
    first_sentence_done: bool,
    // When set, inline code spans (created from single-backticks) are tinted
    // 30% toward this target text color. This lets inline code harmonize with
    // the surrounding context (e.g., different list nesting levels) while
    // still reading as code.
    inline_code_tint_target: Option<Color>,
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            current_line: Vec::new(),
            in_code_block: false,
            code_block_lang: None,
            code_block_buf: String::new(),
            bold_first_sentence: false,
            first_sentence_done: false,
            inline_code_tint_target: None,
        }
    }

    pub fn render(text: &str) -> Vec<Line<'static>> {
        let mut renderer = Self::new();
        // Top-level assistant text uses the theme's primary text color as the
        // base for tinting inline code spans.
        renderer.inline_code_tint_target = Some(crate::colors::text());
        renderer.process_text(text);
        renderer.finish();
        renderer.lines
    }

    pub fn render_with_bold_first_sentence(text: &str) -> Vec<Line<'static>> {
        let mut renderer = Self::new();
        renderer.bold_first_sentence = true;
        renderer.inline_code_tint_target = Some(crate::colors::text());
        renderer.process_text(text);
        renderer.finish();
        renderer.lines
    }

    fn process_text(&mut self, text: &str) {
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            // Handle code blocks
            if line.trim_start().starts_with("```") {
                self.handle_code_fence(line);
                i += 1;
                continue;
            }

            // Handle tables EARLY to avoid printing the pipe header as plain text
            if let Some((consumed, table_lines)) = parse_markdown_table(&lines[i..]) {
                self.flush_current_line();
                self.lines.extend(table_lines);
                i += consumed;
                continue;
            }

            if self.in_code_block {
                self.add_code_line(line);
                i += 1;
                continue;
            }

            // Handle headings
            if let Some(heading) = self.parse_heading(line) {
                self.flush_current_line();
                // Do not auto-insert spacing before headings; preserve exactly what the
                // assistant returned. Only explicit blank lines in the source should render.
                self.lines.push(heading);
                i += 1;
                continue;
            }

            // Blockquotes / callouts (supports nesting and [!NOTE]/[!TIP]/[!WARNING]/[!IMPORTANT])
            if let Some((consumed, quote_lines)) = parse_blockquotes(&lines[i..]) {
                self.flush_current_line();
                self.lines.extend(quote_lines);
                i += consumed;
                continue;
            }

            // Handle lists
            if let Some(list_item) = self.parse_list_item(line) {
                self.flush_current_line();
                self.lines.push(list_item);
                i += 1;
                continue;
            }

            // Handle blank lines
            if line.trim().is_empty() {
                self.flush_current_line();
                // Don't add multiple consecutive blank lines
                if !self.is_last_line_blank() {
                    self.lines.push(Line::from(""));
                }
                i += 1;
                continue;
            }

            // Regular text with inline formatting
            self.process_inline_text(line);
            i += 1;
        }
    }

    fn handle_code_fence(&mut self, line: &str) {
        let trimmed = line.trim_start();
        if self.in_code_block {
            // Closing fence
            // Render accumulated buffer with syntax highlighting
            let lang = self.code_block_lang.as_deref();
            let code_bg = crate::colors::code_block_bg();
            let crate::syntax_highlight::HighlightedCodeBlock {
                lines: mut highlighted_lines,
                line_widths,
                max_width,
            } = crate::syntax_highlight::highlight_code_block_with_metrics(&self.code_block_buf, lang);
            use ratatui::style::Style;
            use ratatui::text::Span;
            let target_w = max_width; // no extra horizontal padding
            // Emit hidden sentinel with language for border/title downstream
            let label = self
                .code_block_lang
                .clone()
                .unwrap_or_else(|| "text".to_string());
            self.lines.push(Line::from(Span::styled(
                format!("⟦LANG:{label}⟧"),
                Style::default().fg(code_bg).bg(code_bg),
            )));

            for (idx, l) in highlighted_lines.iter_mut().enumerate() {
                // Paint background on each span, not the whole line, so width
                // matches our explicit padding rectangle.
                for sp in l.spans.iter_mut() {
                    sp.style = sp.style.bg(code_bg);
                }
                let w = line_widths.get(idx).copied().unwrap_or(0);
                if target_w > w {
                    let pad = " ".repeat(target_w - w);
                    l.spans
                        .push(Span::styled(pad, Style::default().bg(code_bg)));
                } else if w == 0 {
                    // Ensure at least one painted cell so the background shows
                    l.spans
                        .push(Span::styled(" ", Style::default().bg(code_bg)));
                }
            }
            self.lines.extend(highlighted_lines);
            self.code_block_buf.clear();
            self.in_code_block = false;
            self.code_block_lang = None;
        } else {
            // Opening fence
            self.flush_current_line();
            self.in_code_block = true;
            // Extract language if present
            let lang = trimmed.trim_start_matches("```").trim();
            self.code_block_lang = if lang.is_empty() {
                None
            } else {
                Some(lang.to_string())
            };
            self.code_block_buf.clear();
        }
    }

    fn add_code_line(&mut self, line: &str) {
        // Accumulate; add a newline that was lost by `lines()` iteration
        self.code_block_buf.push_str(line);
        self.code_block_buf.push('\n');
    }

    fn parse_heading(&self, line: &str) -> Option<Line<'static>> {
        let trimmed = line.trim_start();

        // Count heading level
        let mut level = 0;
        for ch in trimmed.chars() {
            if ch == '#' {
                level += 1;
            } else {
                break;
            }
        }

        if level == 0 || level > 6 {
            return None;
        }

        // Must have space after #
        if trimmed.chars().nth(level) != Some(' ') {
            return None;
        }

        let heading_text = trimmed[level..].trim();

        // Headings: strip the leading #'s and render in bold (no special color).
        let style = Style::default().add_modifier(Modifier::BOLD);
        Some(Line::from(Span::styled(heading_text.to_string(), style)))
    }

    fn parse_list_item(&mut self, line: &str) -> Option<Line<'static>> {
        let trimmed = line.trim_start();

        // Check for unordered list markers
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            let indent = line.len() - trimmed.len();
            let mut content = &trimmed[2..];
            // Collapse any extra spaces after the list marker so we render
            // a single space after the bullet ("-  item" -> "- item").
            content = content.trim_start();

            // Task list checkbox support: - [ ] / - [x]
            let mut checkbox_spans: Vec<Span<'static>> = Vec::new();
            if let Some(rest) = content.strip_prefix("[ ] ") {
                content = rest.trim_start();
                checkbox_spans.push(Span::raw("☐ "));
            } else if let Some(rest) = content
                .strip_prefix("[x] ")
                .or_else(|| content.strip_prefix("[X] "))
            {
                content = rest.trim_start();
                checkbox_spans.push(Span::styled(
                    "✓ ",
                    Style::default().fg(crate::colors::success()),
                ));
            }

            let mut styled_content = self.process_inline_spans(content);
            // Run autolink on list content so links in bullets convert properly.
            styled_content = autolink_spans(styled_content);
            let has_checkbox = !checkbox_spans.is_empty();
            if has_checkbox {
                // Prepend checkbox to content; do NOT render a bullet for task list items.
                let mut tmp = Vec::with_capacity(checkbox_spans.len() + styled_content.len());
                tmp.extend(checkbox_spans);
                tmp.append(&mut styled_content);
                styled_content = tmp;
            }
            // Determine nesting level from indent (2 spaces per level approximation)
            let level = (indent / 2) + 1;
            let bullet = match level {
                1 => "-",
                2 => "·",
                3 => "-",
                _ => "⋅",
            };
            // Color by nesting level:
            // 1 → text, 2 → midpoint between text and text_dim, 3+ → text_dim
            let content_fg = match level {
                1 => crate::colors::text(),
                2 => crate::colors::text_mid(),
                _ => crate::colors::text_dim(),
            };

            let mut spans = vec![Span::raw(" ".repeat(indent))];
            if !has_checkbox {
                // Render bullet to match content color
                spans.push(Span::styled(bullet, Style::default().fg(content_fg)));
                spans.push(Span::raw(" "));
            }
            // Recolor content to desired level color while preserving modifiers and any
            // spans that already carry a specific foreground color (e.g., inline code, checkmarks).
            // For inline code, blend its base color 30% toward the bullet's text color.
            let recolored: Vec<Span<'static>> = styled_content
                .into_iter()
                .map(|s| {
                    if let Some(fg) = s.style.fg {
                        if fg == crate::colors::function() {
                            let mut st = s.style;
                            st.fg = Some(crate::colors::mix_toward(fg, content_fg, 0.30));
                            return Span::styled(s.content, st);
                        }
                        s
                    } else {
                        let mut st = s.style;
                        st.fg = Some(content_fg);
                        Span::styled(s.content, st)
                    }
                })
                .collect();
            spans.extend(recolored);

            return Some(Line::from(spans));
        }

        // Check for ordered list markers (1. 2. etc)
        if let Some(dot_pos) = trimmed.find(". ") {
            let number_part = &trimmed[..dot_pos];
            if number_part.chars().all(|c| c.is_ascii_digit()) && !number_part.is_empty() {
                let indent = line.len() - trimmed.len();
                let content = trimmed[dot_pos + 2..].trim_start();
                let styled_content = self.process_inline_spans(content);
                let depth = indent / 2 + 1;
                let content_fg = match depth {
                    1 => crate::colors::text(),
                    2 => crate::colors::text_mid(),
                    _ => crate::colors::text_dim(),
                };

                let mut spans = vec![
                    Span::raw(" ".repeat(indent)),
                    // Make the number bold (no primary color)
                    Span::styled(
                        format!("{number_part}."),
                        Style::default().add_modifier(Modifier::BOLD).fg(content_fg),
                    ),
                    Span::raw(" "),
                ];
                // Recolor content preserving modifiers and pre-set foreground colors.
                // For inline code, blend base code color 30% toward the list text color.
                let recolored: Vec<Span<'static>> = styled_content
                    .into_iter()
                    .map(|s| {
                        if let Some(fg) = s.style.fg {
                            if fg == crate::colors::function() {
                                let mut st = s.style;
                                st.fg = Some(crate::colors::mix_toward(fg, content_fg, 0.30));
                                return Span::styled(s.content, st);
                            }
                            s
                        } else {
                            let mut st = s.style;
                            st.fg = Some(content_fg);
                            Span::styled(s.content, st)
                        }
                    })
                    .collect();
                spans.extend(recolored);

                return Some(Line::from(spans));
            }
        }

        None
    }

    fn process_inline_text(&mut self, text: &str) {
        let spans = self.process_inline_spans(text);
        self.current_line.extend(spans);
        self.flush_current_line();
    }

    fn process_inline_spans(&mut self, text: &str) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut current_text = String::new();

        while i < chars.len() {
            // Markdown image ![alt](url "title")
            if chars[i] == '!' {
                let rest: String = chars[i..].iter().collect();
                if let Some((consumed, label, target)) = find_markdown_image(&rest) {
                    if !current_text.is_empty() {
                        spans.push(Span::raw(current_text.clone()));
                        current_text.clear();
                    }
                    // Render label and make the target URL visible next to it.
                    let lbl = if label.is_empty() {
                        "Image".to_string()
                    } else {
                        label
                    };
                    // Underlined label
                    let mut st = Style::default();
                    st.add_modifier.insert(Modifier::UNDERLINED);
                    spans.push(Span::styled(lbl, st));
                    // Append visible URL in parens (dimmed)
                    spans.push(Span::raw(" ("));
                    spans.push(Span::styled(target.clone(), Style::default().fg(crate::colors::text_dim())));
                    spans.push(Span::raw(")"));
                    i += consumed;
                    continue;
                }
            }
            // Bold+Italic ***text*** or ___text___
            if i + 2 < chars.len()
                && ((chars[i] == '*' && chars[i + 1] == '*' && chars[i + 2] == '*')
                    || (chars[i] == '_' && chars[i + 1] == '_' && chars[i + 2] == '_'))
            {
                // Flush current text
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }
                let marker = chars[i];
                // Find closing triple marker
                let mut j = i + 3;
                let mut content = String::new();
                while j + 2 < chars.len() {
                    if chars[j] == marker && chars[j + 1] == marker && chars[j + 2] == marker {
                        let st = Style::default()
                            .add_modifier(Modifier::BOLD | Modifier::ITALIC)
                            .fg(crate::colors::text_bright());
                        spans.push(Span::styled(content, st));
                        i = j + 3;
                        break;
                    }
                    content.push(chars[j]);
                    j += 1;
                }
                if j + 2 >= chars.len() {
                    // No closing marker; treat as literal
                    current_text.push(marker);
                    current_text.push(marker);
                    current_text.push(marker);
                    i += 3;
                }
                continue;
            }

            // Strikethrough ~~text~~
            if i + 1 < chars.len() && chars[i] == '~' && chars[i + 1] == '~' {
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }
                let mut j = i + 2;
                let mut content = String::new();
                while j + 1 < chars.len() {
                    if chars[j] == '~' && chars[j + 1] == '~' {
                        let st = Style::default().add_modifier(Modifier::CROSSED_OUT);
                        spans.push(Span::styled(content, st));
                        i = j + 2;
                        break;
                    }
                    content.push(chars[j]);
                    j += 1;
                }
                if j + 1 >= chars.len() {
                    current_text.push('~');
                    current_text.push('~');
                    i += 2;
                }
                continue;
            }

            // Simple HTML underline <u>text</u>
            if chars[i] == '<' {
                // Try <u>, <sub>, <sup>
                let rest: String = chars[i..].iter().collect();
                if let Some(inner) = rest.strip_prefix("<u>") {
                    if let Some(end) = inner.find("</u>") {
                        if !current_text.is_empty() {
                            spans.push(Span::raw(current_text.clone()));
                            current_text.clear();
                        }
                        let content = inner[..end].to_string();
                        spans.push(Span::styled(
                            content,
                            Style::default().add_modifier(Modifier::UNDERLINED),
                        ));
                        i += 3 + end + 4; // len("<u>") + content + len("</u>")
                        continue;
                    }
                } else if let Some(inner) = rest.strip_prefix("<sub>") {
                    if let Some(end) = inner.find("</sub>") {
                        if !current_text.is_empty() {
                            spans.push(Span::raw(current_text.clone()));
                            current_text.clear();
                        }
                        let content = inner[..end].to_string();
                        spans.push(Span::raw(to_subscript(&content)));
                        i += 5 + end + 6; // <sub> + content + </sub>
                        continue;
                    }
                } else if let Some(inner) = rest.strip_prefix("<sup>")
                    && let Some(end) = inner.find("</sup>") {
                        if !current_text.is_empty() {
                            spans.push(Span::raw(current_text.clone()));
                            current_text.clear();
                        }
                        let content = inner[..end].to_string();
                        spans.push(Span::raw(to_superscript(&content)));
                        i += 5 + end + 6; // <sup> + content + </sup>
                        continue;
                    }
            }

            // Check for inline code
            if chars[i] == '`' {
                // Flush current text
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }

                // Find closing backtick
                let mut j = i + 1;
                let mut code_content = String::new();
                while j < chars.len() && chars[j] != '`' {
                    code_content.push(chars[j]);
                    j += 1;
                }

                if j < chars.len() {
                    // Found closing backtick — render code without surrounding backticks
                    // Use the base code color here; context-specific tinting is
                    // applied at line flush or by list/blockquote handlers.
                    spans.push(Span::styled(
                        code_content,
                        Style::default().fg(crate::colors::function()),
                    ));
                    i = j + 1;
                } else {
                    // No closing backtick, treat as regular text
                    current_text.push('`');
                    i += 1;
                }
                continue;
            }

            // Check for bold (**text** or __text__)
            if i + 1 < chars.len()
                && ((chars[i] == '*' && chars[i + 1] == '*')
                    || (chars[i] == '_' && chars[i + 1] == '_'))
            {
                // Flush current text
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }

                let marker = chars[i];
                // Find closing markers
                let mut j = i + 2;
                let mut bold_content = String::new();
                while j + 1 < chars.len() {
                    if chars[j] == marker && chars[j + 1] == marker {
                        // Found closing markers
                        // Bold text uses bright color consistently
                        let bold_color = crate::colors::text_bright();
                        spans.push(Span::styled(
                            bold_content,
                            Style::default().fg(bold_color).add_modifier(Modifier::BOLD),
                        ));
                        i = j + 2;
                        break;
                    }
                    bold_content.push(chars[j]);
                    j += 1;
                }

                if j + 1 >= chars.len() {
                    // No closing markers, treat as regular text
                    current_text.push(marker);
                    current_text.push(marker);
                    i += 2;
                }
                continue;
            }

            // Check for italic (*text* or _text_)
            if (chars[i] == '*' || chars[i] == '_')
                && (i == 0 || !chars[i - 1].is_alphanumeric())
                && (i + 1 < chars.len() && chars[i + 1] != ' ' && chars[i + 1] != chars[i])
            {
                // Flush current text
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }

                let marker = chars[i];
                // Find closing marker
                let mut j = i + 1;
                let mut italic_content = String::new();
                while j < chars.len() {
                    if chars[j] == marker
                        && (j + 1 >= chars.len() || !chars[j + 1].is_alphanumeric())
                    {
                        // Found closing marker
                        spans.push(Span::styled(
                            italic_content,
                            Style::default().add_modifier(Modifier::ITALIC),
                        ));
                        i = j + 1;
                        break;
                    }
                    italic_content.push(chars[j]);
                    j += 1;
                }

                if j >= chars.len() {
                    // No closing marker, treat as regular text
                    current_text.push(marker);
                    i += 1;
                }
                continue;
            }

            // Regular character
            current_text.push(chars[i]);
            i += 1;
        }

        // Flush any remaining plain text as-is; first-sentence bolding is handled elsewhere
        if !current_text.is_empty() {
            spans.push(Span::raw(current_text));
        }

        spans
    }

    fn flush_current_line(&mut self) {
        if !self.current_line.is_empty() {
            // Autolink URLs and markdown links inside the accumulated spans.
            let mut linked = autolink_spans(std::mem::take(&mut self.current_line));
            // Apply first-sentence styling to the first rendered line.
            if self.bold_first_sentence && !self.first_sentence_done
                && apply_first_sentence_style(&mut linked) {
                    self.first_sentence_done = true;
                }
            // If requested, gently tint inline code spans toward the provided
            // context text color so they blend better with the surrounding text.
            if let Some(target) = self.inline_code_tint_target {
                let base = crate::colors::function();
                let tint = crate::colors::mix_toward(base, target, 0.30);
                for sp in &mut linked {
                    if sp.style.fg == Some(base) {
                        let mut st = sp.style;
                        st.fg = Some(tint);
                        *sp = Span::styled(sp.content.clone(), st);
                    }
                }
            }
            self.lines.push(Line::from(linked));
            self.current_line.clear();
        }
    }

    fn is_last_line_blank(&self) -> bool {
        if let Some(last) = self.lines.last() {
            last.spans.is_empty() || last.spans.iter().all(|s| s.content.trim().is_empty())
        } else {
            false
        }
    }

    fn finish(&mut self) {
        self.flush_current_line();
        // If an unterminated fence was left open, render its buffer.
        if self.in_code_block {
            let lang = self.code_block_lang.as_deref();
            let code_bg = crate::colors::code_block_bg();
            let mut highlighted =
                crate::syntax_highlight::highlight_code_block(&self.code_block_buf, lang);
            use ratatui::style::Style;
            use ratatui::text::Span;
            use unicode_width::UnicodeWidthStr;
            let max_w: usize = highlighted
                .iter()
                .map(|l| {
                    l.spans
                        .iter()
                        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                        .sum::<usize>()
                })
                .max()
                .unwrap_or(0);
            let target_w = max_w; // no extra horizontal padding
            // Emit hidden sentinel with language for border/title downstream
            let label = self
                .code_block_lang
                .clone()
                .unwrap_or_else(|| "text".to_string());
            self.lines.push(Line::from(Span::styled(
                format!("⟦LANG:{label}⟧"),
                Style::default().fg(code_bg).bg(code_bg),
            )));

            for l in highlighted.iter_mut() {
                for sp in l.spans.iter_mut() {
                    sp.style = sp.style.bg(code_bg);
                }
                let w: usize = l
                    .spans
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                    .sum();
                if target_w > w {
                    let pad = " ".repeat(target_w - w);
                    l.spans
                        .push(Span::styled(pad, Style::default().bg(code_bg)));
                } else if w == 0 {
                    l.spans
                        .push(Span::styled(" ", Style::default().bg(code_bg)));
                }
            }
            self.lines.extend(highlighted);
            self.code_block_buf.clear();
            self.in_code_block = false;
            self.code_block_lang = None;
        }
    }
}

