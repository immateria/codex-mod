// Apply bold + text_bright to the first sentence in a span list (first line only),
// preserving any existing bold spans and other inline styles.
fn apply_first_sentence_style(spans: &mut Vec<Span<'static>>) -> bool {
    use ratatui::style::Modifier;
    // Concatenate text to find terminator
    let full: String = spans.iter().map(|s| s.content.as_ref()).collect();
    let trimmed = full.trim_start();
    // Skip if line begins with a markdown bullet glyph
    if trimmed.starts_with('-')
        || trimmed.starts_with('•')
        || trimmed.starts_with('◦')
        || trimmed.starts_with('·')
        || trimmed.starts_with('∘')
        || trimmed.starts_with('⋅')
    {
        return false;
    }
    // Find a sensible terminator index with simple heuristics
    let chars: Vec<char> = full.chars().collect();
    let mut term: Option<usize> = None;
    for i in 0..chars.len() {
        let ch = chars[i];
        if ch == '.' || ch == '!' || ch == '?' || ch == ':' {
            let next = chars.get(i + 1).copied();
            // Skip filename-like or abbreviation endings
            if matches!(next, Some(c) if c.is_ascii_alphanumeric()) {
                continue;
            }
            if i >= 3 {
                let tail: String = chars[i - 3..=i].iter().collect::<String>().to_lowercase();
                if tail == "e.g." || tail == "i.e." {
                    continue;
                }
            }
            // Accept if eol/space or quote then space/eol
            let ok = match next {
                None => true,
                Some(c) if c.is_whitespace() => true,
                Some('"') | Some('\'') => {
                    let n2 = chars.get(i + 2).copied();
                    n2.is_none() || n2.map(char::is_whitespace).unwrap_or(false)
                }
                _ => false,
            };
            if ok {
                term = Some(i + 1);
                break;
            }
        }
    }
    let Some(limit) = term else { return false };
    // If no non-space content after limit, consider single-sentence → no bold
    if !chars.iter().skip(limit).any(|c| !c.is_whitespace()) {
        return false;
    }

    // Walk spans and apply style up to limit (build a new vec to avoid borrow conflicts)
    let original = std::mem::take(spans);
    let mut out: Vec<Span<'static>> = Vec::with_capacity(original.len() + 2);
    let mut consumed = 0usize; // chars consumed across spans
    for sp in original.into_iter() {
        if consumed >= limit {
            out.push(sp);
            continue;
        }
        let text = sp.content.into_owned();
        let len = text.chars().count();
        let end_here = (limit - consumed).min(len);
        if end_here == len {
            // Entire span within bold range
            let mut st = sp.style;
            if !st.add_modifier.contains(Modifier::BOLD) {
                st.add_modifier.insert(Modifier::BOLD);
                st.fg = Some(crate::colors::text_bright());
            }
            out.push(Span::styled(text, st));
        } else if end_here == 0 {
            out.push(Span::styled(text, sp.style));
        } else {
            // Split span
            let mut iter = text.chars();
            let left: String = iter.by_ref().take(end_here).collect();
            let right: String = iter.collect();
            let mut left_style = sp.style;
            if !left_style.add_modifier.contains(Modifier::BOLD) {
                left_style.add_modifier.insert(Modifier::BOLD);
                left_style.fg = Some(crate::colors::text_bright());
            }
            out.push(Span::styled(left, left_style));
            out.push(Span::styled(right, sp.style));
        }
        consumed += end_here;
    }
    *spans = out;
    true
}

