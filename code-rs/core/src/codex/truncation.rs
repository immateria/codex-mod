/// Marker inserted when tool output is truncated.
pub(super) const TRUNCATION_MARKER: &str = "…truncated…\n";

pub(super) fn truncate_middle_bytes(
    s: &str,
    max_bytes: usize,
) -> (String, bool, usize, usize) {
    if s.len() <= max_bytes {
        return (s.to_string(), false, s.len(), s.len());
    }
    if max_bytes == 0 {
        return (TRUNCATION_MARKER.trim_end().to_string(), true, 0, s.len());
    }

    // Try to keep some head/tail, favoring newline boundaries when possible.
    let keep = max_bytes.saturating_sub("…truncated…\n".len());
    let left_budget = keep / 2;
    let right_budget = keep - left_budget;

    // Safe prefix end on a char boundary, prefer last newline within budget.
    let prefix_end = {
        let mut end = left_budget.min(s.len());
        if let Some(head) = s.get(..end)
            && let Some(i) = head.rfind('\n')
        {
            end = i + 1;
        }
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        end
    };

    // Safe suffix start on a char boundary, prefer first newline within budget.
    let suffix_start = {
        let mut start = s.len().saturating_sub(right_budget);
        if let Some(tail) = s.get(start..)
            && let Some(i) = tail.find('\n')
        {
            start += i + 1;
        }
        while start < s.len() && !s.is_char_boundary(start) {
            start += 1;
        }
        start
    };

    let mut out = String::with_capacity(max_bytes);
    out.push_str(&s[..prefix_end]);
    out.push_str(TRUNCATION_MARKER);
    out.push_str(&s[suffix_start..]);
    (out, true, prefix_end, suffix_start)
}

