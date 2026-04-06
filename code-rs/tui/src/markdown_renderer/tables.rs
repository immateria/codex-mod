// Parse a markdown pipe table starting at `lines[0]`.
// Returns (consumed_line_count, rendered_lines) on success.
// We keep it simple and robust for TUI: left-align columns and pad with spaces.
fn parse_markdown_table(lines: &[&str]) -> Option<(usize, Vec<Line<'static>>)> {
    if lines.len() < 2 {
        return None;
    }
    let header_line = lines[0].trim();
    let sep_line = lines[1].trim();
    if !header_line.contains('|') {
        return None;
    }

    // Split a row by '|' and trim spaces; drop empty edge cells from leading/trailing '|'
    fn split_row(s: &str) -> Vec<String> {
        let mut parts: Vec<String> = s.split('|').map(|x| x.trim().to_string()).collect();
        // Trim empty edge cells introduced by leading/trailing '|'
        if parts.first().is_some_and(std::string::String::is_empty) {
            parts.remove(0);
        }
        if parts.last().is_some_and(std::string::String::is_empty) {
            parts.pop();
        }
        parts
    }

    let header_cells = split_row(header_line);
    if header_cells.is_empty() {
        return None;
    }

    // Validate separator: must have at least the same number of segments and each segment is --- with optional : for alignment
    // Parse separator: either pipe-based or dashed segments separated by 2+ spaces
    let (sep_segments, has_pipe_sep) = if sep_line.contains('|') {
        (split_row(sep_line), true)
    } else {
        // Split on runs of 2+ spaces
        let mut segs: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut space_run = 0;
        for ch in sep_line.chars() {
            if ch == ' ' {
                space_run += 1;
            } else {
                space_run = 0;
            }
            if space_run >= 2 {
                if !cur.trim().is_empty() {
                    segs.push(cur.trim().to_string());
                }
                cur.clear();
                space_run = 0;
            } else {
                cur.push(ch);
            }
        }
        if !cur.trim().is_empty() {
            segs.push(cur.trim().to_string());
        }
        (segs, false)
    };
    if sep_segments.len() < header_cells.len() {
        return None;
    }
    let valid_sep = sep_segments.iter().take(header_cells.len()).all(|c| {
        let core = c.replace(':', "");
        !core.is_empty() && core.chars().all(|ch| ch == '-')
    });
    if !valid_sep {
        return None;
    }

    // Collect body rows until a non-table line
    let mut body: Vec<Vec<String>> = Vec::new();
    let mut idx = 2usize;
    while idx < lines.len() {
        let raw = lines[idx];
        if !raw.contains('|') {
            break;
        }
        let row = split_row(raw);
        if row.is_empty() {
            break;
        }
        body.push(row);
        idx += 1;
    }

    let cols = header_cells
        .len()
        .max(body.iter().map(std::vec::Vec::len).max().unwrap_or(0));
    // Column alignment: from pipe separators with colons if present; otherwise
    // infer right alignment for numeric-only columns, left otherwise.
    #[derive(Copy, Clone)]
    enum Align {
        Left,
        Right,
    }
    let mut aligns = vec![Align::Left; cols];
    if has_pipe_sep {
        for (i, align) in aligns.iter_mut().enumerate().take(cols) {
            let seg = sep_segments
                .get(i)
                .map(std::string::String::as_str)
                .unwrap_or("");
            let left_colon = seg.starts_with(':');
            let right_colon = seg.ends_with(':');
            *align = if right_colon && !left_colon {
                Align::Right
            } else {
                Align::Left
            };
        }
    }
    // Compute widths per column
    let mut widths = vec![0usize; cols];
    for (i, cell) in header_cells.iter().enumerate() {
        widths[i] = widths[i].max(cell.chars().count());
    }
    for row in &body {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    // Infer alignment for numeric columns if not specified by pipes
    if !has_pipe_sep {
        for (i, align) in aligns.iter_mut().enumerate().take(cols) {
            let numeric = body
                .iter()
                .all(|r| r.get(i).is_none_or(|c| is_numeric(c)));
            if numeric {
                *align = Align::Right;
            }
        }
    }

    fn pad_cell(s: &str, w: usize, align: Align) -> String {
        let len = s.chars().count();
        if len >= w {
            return s.to_string();
        }
        let pad = w - len;
        match align {
            Align::Left => format!("{}{}", s, " ".repeat(pad)),
            Align::Right => format!("{}{}", " ".repeat(pad), s),
        }
    }

    let mut out: Vec<Line<'static>> = Vec::new();
    // Header (bold)
    {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for i in 0..cols {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            let text = pad_cell(
                header_cells.get(i).map(String::as_str).unwrap_or(""),
                widths[i],
                aligns[i],
            );
            spans.push(Span::styled(
                text,
                Style::default().add_modifier(Modifier::BOLD),
            ));
        }
        out.push(Line::from(spans));
    }
    // Separator row using box-drawing to avoid being mistaken for a horizontal rule
    {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for (i, width) in widths.iter().copied().enumerate().take(cols) {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::raw("─".repeat(width).clone()));
        }
        out.push(Line::from(spans));
    }
    // Body
    for row in body {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for i in 0..cols {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            let text = pad_cell(
                row.get(i).map(String::as_str).unwrap_or(""),
                widths[i],
                aligns[i],
            );
            spans.push(Span::raw(text));
        }
        out.push(Line::from(spans));
    }

    Some((idx, out))
}

