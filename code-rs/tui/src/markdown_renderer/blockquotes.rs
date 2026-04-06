fn is_numeric(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return true;
    }
    let mut has_digit = false;
    for ch in t.chars() {
        if ch.is_ascii_digit() {
            has_digit = true;
            continue;
        }
        if matches!(ch, '+' | '-' | '.' | ',') {
            continue;
        }
        return false;
    }
    has_digit
}

// Parse consecutive blockquote lines, supporting nesting with multiple '>' markers
// and callouts: [!NOTE], [!TIP], [!WARNING], [!IMPORTANT]
fn parse_blockquotes(lines: &[&str]) -> Option<(usize, Vec<Line<'static>>)> {
    if lines.is_empty() {
        return None;
    }
    // Must start with '>'
    if !lines[0].trim_start().starts_with('>') {
        return None;
    }
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut i = 0usize;
    let mut callout_kind: Option<String> = None;
    let mut callout_color = crate::colors::info();
    let mut first_content_seen = false;
    while i < lines.len() {
        let raw = lines[i];
        let t = raw.trim_start();
        if !t.starts_with('>') {
            break;
        }
        // Count nesting depth (allow spaces between >)
        let mut idx = 0usize;
        let bytes = t.as_bytes();
        let mut depth = 0usize;
        while idx < bytes.len() {
            if bytes[idx] == b'>' {
                depth += 1;
                idx += 1;
                while idx < bytes.len() && bytes[idx] == b' ' {
                    idx += 1;
                }
            } else {
                break;
            }
        }
        let content = t[idx..].to_string();
        if !first_content_seen {
            let trimmed = content.trim();
            if let Some(inner) = trimmed.strip_prefix("[!")
                && let Some(end) = inner.find(']') {
                    let kind = inner[..end].to_ascii_uppercase();
                    match kind.as_str() {
                        "NOTE" => {
                            callout_kind = Some("NOTE".into());
                            callout_color = crate::colors::info();
                        }
                        "TIP" => {
                            callout_kind = Some("TIP".into());
                            callout_color = crate::colors::success();
                        }
                        "WARNING" => {
                            callout_kind = Some("WARNING".into());
                            callout_color = crate::colors::warning();
                        }
                        "IMPORTANT" => {
                            callout_kind = Some("IMPORTANT".into());
                            callout_color = crate::colors::info();
                        }
                        _ => {}
                    }
                    if let Some(ref k) = callout_kind {
                        // Eagerly emit the label so the block never returns None
                        // even if there are no subsequent quoted lines.
                        if out.is_empty() {
                            let label = k.clone();
                            out.push(Line::from(vec![Span::styled(
                                label,
                                Style::default()
                                    .fg(callout_color)
                                    .add_modifier(Modifier::BOLD),
                            )]));
                        }
                        i += 1; // consume marker line and continue scanning quoted content
                        continue;
                    }
                }
            first_content_seen = true;
        }

        // For callouts, render a label line once
        if let Some(ref kind) = callout_kind
            && out.is_empty() {
                let label = kind.clone();
                out.push(Line::from(vec![Span::styled(
                    label,
                    Style::default()
                        .fg(callout_color)
                        .add_modifier(Modifier::BOLD),
                )]));
            }

        // Render the quote content as raw literal text without interpreting
        // Markdown syntax inside the blockquote. This preserves the exact
        // characters shown by the model (e.g., `**bold**`, lists, images)
        // rather than re‑parsing them. Each input line corresponds to a
        // single rendered line of content.
        let lines_to_render = if content.is_empty() {
            vec![Line::from("")]
        } else {
            vec![Line::from(Span::raw(content.clone()))]
        };

        let bar_style = if callout_kind.is_some() {
            Style::default().fg(callout_color)
        } else {
            Style::default().fg(crate::colors::text_dim())
        };
        let content_fg = if callout_kind.is_some() {
            crate::colors::text()
        } else {
            crate::colors::text_dim()
        };

        for inner_line in lines_to_render {
            // Prefix depth bars (│ ) once per nesting level
            let mut prefixed: Vec<Span<'static>> = Vec::new();
            for _ in 0..depth.max(1) {
                prefixed.push(Span::styled("│ ", bar_style));
            }
            // Recolor inner content spans only if they don't already have a specific FG
            let recolored: Vec<Span<'static>> = inner_line
                .spans
                .into_iter()
                .map(|s| {
                    if let Some(_fg) = s.style.fg {
                        // Preserve explicit colors (e.g., code spans) even though we
                        // no longer parse markdown inside quotes. If a span already has
                        // an FG, keep it as-is.
                        s
                    } else {
                        let mut st = s.style;
                        st.fg = Some(content_fg);
                        Span::styled(s.content, st)
                    }
                })
                .collect();
            prefixed.extend(recolored);
            out.push(Line::from(prefixed));
        }
        i += 1;
    }
    if out.is_empty() { None } else { Some((i, out)) }
}

