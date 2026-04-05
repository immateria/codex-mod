use ratatui::text::Line;
use std::fmt::Write as _;

// Local helper: coalesce "<file> (lines A to B)" entries when contiguous.
pub(crate) fn coalesce_read_ranges_in_lines_local(lines: &mut Vec<Line<'static>>) {
    use ratatui::style::Modifier;
    use ratatui::style::Style;
    use ratatui::text::Span;
    // Nothing to do for empty/single line vectors
    if lines.len() <= 1 {
        return;
    }

    // Parse a content line of the form
    //   "└ <file> (lines A to B)" or "  <file> (lines A to B)"
    // into (filename, start, end, prefix, original_index).
    fn parse_read_line_with_index(
        idx: usize,
        line: &Line<'_>,
    ) -> Option<(String, u32, u32, String, usize)> {
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
        if let Some(i) = rest.rfind(" (lines ") {
            let fname = rest[..i].to_string();
            let tail = &rest[i + 1..];
            if let Some(inner) = tail.strip_prefix("(lines ").and_then(|s| s.strip_suffix(")")) {
                if let Some((s1, s2)) = inner.split_once(" to ")
                    && let (Ok(a), Ok(b)) = (s1.trim().parse::<u32>(), s2.trim().parse::<u32>())
                {
                    return Some((fname, a, b, prefix, idx));
                }
            }
        }
        None
    }

    // Collect read ranges grouped by filename, preserving first-seen order.
    // Also track the earliest prefix to reuse when emitting a single line per file.
    #[derive(Default)]
    struct FileRanges {
        prefix: String,
        first_index: usize,
        ranges: Vec<(u32, u32)>,
    }

    let mut files: Vec<(String, FileRanges)> = Vec::new();
    let mut non_read_lines: Vec<Line<'static>> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        if let Some((fname, a, b, prefix, orig_idx)) = parse_read_line_with_index(idx, line) {
            // Insert or update entry for this file, preserving encounter order
            if let Some((_name, fr)) = files.iter_mut().find(|(n, _)| n == &fname) {
                fr.ranges.push((a.min(b), a.max(b)));
                // Keep earliest index as stable ordering anchor
                if orig_idx < fr.first_index {
                    fr.first_index = orig_idx;
                }
            } else {
                files.push((
                    fname,
                    FileRanges {
                        prefix,
                        first_index: orig_idx,
                        ranges: vec![(a.min(b), a.max(b))],
                    },
                ));
            }
        } else {
            non_read_lines.push(line.clone());
        }
    }

    if files.is_empty() {
        return;
    }

    // For each file: merge overlapping/touching ranges; then sort ascending and emit one line.
    fn merge_and_sort(mut v: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
        if v.len() <= 1 {
            return v;
        }
        v.sort_by_key(|(s, _)| *s);
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(v.len());
        let mut cur = v[0];
        for &(s, e) in v.iter().skip(1) {
            if s <= cur.1.saturating_add(1) {
                // touching or overlap
                cur.1 = cur.1.max(e);
            } else {
                out.push(cur);
                cur = (s, e);
            }
        }
        out.push(cur);
        out
    }

    // Rebuild the lines vector: keep header (if present) and any non-read lines,
    // then append one consolidated line per file in first-seen order by index.
    let mut rebuilt: Vec<Line<'static>> = Vec::with_capacity(lines.len());

    // Heuristic: preserve an initial header line that does not start with a connector.
    if !lines.is_empty()
        && lines[0]
            .spans
            .first()
            .map(|s| s.content.as_ref() != "└ " && s.content.as_ref() != "  ")
            .unwrap_or(false)
    {
        rebuilt.push(lines[0].clone());
    }

    // Sort files by their first appearance index to keep stable ordering with other files.
    files.sort_by_key(|(_n, fr)| fr.first_index);

    for (name, mut fr) in files.into_iter() {
        fr.ranges = merge_and_sort(fr.ranges);
        // Build range annotation: " (lines S1 to E1, S2 to E2, ...)"
        let mut ann = String::new();
        ann.push_str(" (");
        ann.push_str("lines ");
        for (i, (s, e)) in fr.ranges.iter().enumerate() {
            if i > 0 {
                ann.push_str(", ");
            }
            let _ = write!(ann, "{s} to {e}");
        }
        ann.push(')');

        let spans: Vec<Span<'static>> = vec![
            Span::styled(fr.prefix, Style::default().add_modifier(Modifier::DIM)),
            Span::styled(name, Style::default().fg(crate::colors::text())),
            Span::styled(ann, Style::default().fg(crate::colors::text_dim())),
        ];
        rebuilt.push(Line::from(spans));
    }

    // Append any other non-read lines (rare for Read sections, but safe)
    // Note: keep their original order after consolidated entries
    rebuilt.extend(non_read_lines);

    *lines = rebuilt;
}

pub(crate) fn parse_read_line_annotation_with_range(
    cmd: &str,
) -> (Option<String>, Option<(u32, u32)>) {
    let lower = cmd.to_lowercase();
    // Try sed -n '<start>,<end>p'
    if lower.contains("sed") && lower.contains("-n") {
        // Look for a token like 123,456p possibly quoted
        for raw in cmd.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
            let token = raw.trim();
            if token.ends_with('p') {
                let core = &token[..token.len().saturating_sub(1)];
                if let Some((a, b)) = core.split_once(',')
                    && let (Ok(start), Ok(end)) =
                        (a.trim().parse::<u32>(), b.trim().parse::<u32>())
                {
                    return (
                        Some(format!("(lines {start} to {end})")),
                        Some((start, end)),
                    );
                }
            }
        }
    }
    // head -n N => lines 1..N
    if lower.contains("head") && lower.contains("-n") {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        // Find the position of "head" command first
        let head_pos = parts.iter().position(|p| {
            let lower = p.to_lowercase();
            lower == "head" || lower.ends_with("/head")
        });

        if let Some(head_idx) = head_pos {
            // Only look for -n after the head command position
            for i in head_idx..parts.len() {
                if parts[i] == "-n"
                    && i + 1 < parts.len()
                    && let Ok(n) = parts[i + 1]
                        .trim_matches('"')
                        .trim_matches('\'')
                        .parse::<u32>()
                {
                    return (Some(format!("(lines 1 to {n})")), Some((1, n)));
                }
            }
        }
    }
    // bare `head` => default 10 lines
    if lower.contains("head")
        && !lower.contains("-n")
        && cmd.split_whitespace().any(|part| part == "head")
    {
        return (Some("(lines 1 to 10)".to_string()), Some((1, 10)));
    }
    // tail -n +K => from K to end; tail -n N => last N lines
    if lower.contains("tail") && lower.contains("-n") {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        // Find the position of "tail" command first
        let tail_pos = parts.iter().position(|p| {
            let lower = p.to_lowercase();
            lower == "tail" || lower.ends_with("/tail")
        });

        if let Some(tail_idx) = tail_pos {
            // Only look for -n after the tail command position
            for i in tail_idx..parts.len() {
                if parts[i] == "-n" && i + 1 < parts.len() {
                    let val = parts[i + 1].trim_matches('"').trim_matches('\'');
                    if let Some(rest) = val.strip_prefix('+') {
                        if let Ok(k) = rest.parse::<u32>() {
                            return (
                                Some(format!("(from {k} to end)")),
                                Some((k, u32::MAX)),
                            );
                        }
                    } else if let Ok(n) = val.parse::<u32>() {
                        return (Some(format!("(last {n} lines)")), None);
                    }
                }
            }
        }
    }
    // bare `tail` => default 10 lines
    if lower.contains("tail")
        && !lower.contains("-n")
        && cmd.split_whitespace().any(|part| part == "tail")
    {
        return (Some("(last 10 lines)".to_string()), None);
    }
    (None, None)
}

pub(crate) fn parse_read_line_annotation(cmd: &str) -> Option<String> {
    parse_read_line_annotation_with_range(cmd).0
}

