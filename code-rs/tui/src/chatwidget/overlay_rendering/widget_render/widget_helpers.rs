// Coalesce adjacent Read entries of the same file with contiguous ranges in a rendered lines vector.
// Expects the vector to contain a header line at index 0 (e.g., "Read"). Modifies in place.
#[allow(dead_code)]
fn coalesce_read_ranges_in_lines(lines: &mut Vec<ratatui::text::Line<'static>>) {
    use ratatui::style::Modifier;
    use ratatui::style::Style;
    use ratatui::text::Line;
    use ratatui::text::Span;

    if lines.len() <= 1 {
        return;
    }

    // Helper to parse a content line into (filename, start, end, prefix)
    fn parse_read_line(line: &Line<'_>) -> Option<(String, u32, u32, String)> {
        if line.spans.is_empty() {
            return None;
        }
        let prefix = line.spans[0].content.to_string();
        if !(prefix == "└ " || prefix == "  ") {
            return None;
        }
        let rest: String = line
            .spans
            .iter()
            .skip(1)
            .map(|s| s.content.as_ref())
            .collect();
        if let Some(idx) = rest.rfind(" (lines ") {
            let fname = rest[..idx].to_string();
            let tail = &rest[idx + 1..];
            if tail.starts_with("(lines ") && tail.ends_with(")") {
                let inner = &tail[7..tail.len() - 1];
                if let Some((s1, s2)) = inner.split_once(" to ")
                    && let (Ok(start), Ok(end)) =
                        (s1.trim().parse::<u32>(), s2.trim().parse::<u32>())
                    {
                        return Some((fname, start, end, prefix));
                    }
            }
        }
        None
    }

    // Merge overlapping or touching ranges for the same file, regardless of adjacency.
    let mut i: usize = 0; // works for vectors with or without a header line
    while i < lines.len() {
        let Some((fname_a, mut a1, mut a2, prefix_a)) = parse_read_line(&lines[i]) else {
            i += 1;
            continue;
        };
        let mut k = i + 1;
        while k < lines.len() {
            if let Some((fname_b, b1, b2, _prefix_b)) = parse_read_line(&lines[k])
                && fname_b == fname_a {
                    let touch_or_overlap = b1 <= a2.saturating_add(1) && b2.saturating_add(1) >= a1;
                    if touch_or_overlap {
                        a1 = a1.min(b1);
                        a2 = a2.max(b2);
                        let new_spans: Vec<Span<'static>> = vec![
                            Span::styled(
                                prefix_a.clone(),
                                Style::default().add_modifier(Modifier::DIM),
                            ),
                            Span::styled(
                                fname_a.clone(),
                                Style::default().fg(crate::colors::text()),
                            ),
                            Span::styled(
                                format!(" (lines {a1} to {a2})"),
                                Style::default().fg(crate::colors::text_dim()),
                            ),
                        ];
                        lines[i] = Line::from(new_spans);
                        lines.remove(k);
                        continue;
                    }
                }
            k += 1;
        }
        i += 1;
    }
}
