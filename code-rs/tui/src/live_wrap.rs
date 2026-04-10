use unicode_width::UnicodeWidthChar;

/// A single visual row produced by RowBuilder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) text: String,
    /// True if this row ends with an explicit line break (as opposed to a hard wrap).
    pub(crate) explicit_break: bool,
}

fn make_row(text: String, explicit_break: bool) -> Row {
    Row {
        text,
        explicit_break,
    }
}

/// Incrementally wraps input text into visual rows of at most `width` cells.
///
/// Step 1: plain-text only. ANSI-carry and styled spans will be added later.
pub(crate) struct RowBuilder {
    target_width: usize,
    /// Buffer for the current logical line (until a '\n' is seen).
    current_line: String,
    /// Output rows built so far for the current logical line and previous ones.
    rows: Vec<Row>,
}

impl RowBuilder {
    pub(crate) fn new(target_width: usize) -> Self {
        Self {
            target_width: target_width.max(1),
            current_line: String::new(),
            rows: Vec::new(),
        }
    }

    /// Push an input fragment. May contain newlines.
    pub(crate) fn push_fragment(&mut self, fragment: &str) {
        if fragment.is_empty() {
            return;
        }
        let mut start = 0usize;
        for (i, ch) in fragment.char_indices() {
            if ch == '\n' {
                // Flush anything pending before the newline.
                if start < i {
                    self.current_line.push_str(&fragment[start..i]);
                }
                self.flush_current_line(true);
                start = i + ch.len_utf8();
            }
        }
        if start < fragment.len() {
            self.current_line.push_str(&fragment[start..]);
            self.wrap_current_line();
        }
    }

    /// Return a snapshot of produced rows (non-draining).
    pub(crate) fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// True when the builder has no content to display (no committed rows
    /// and no partial current line).
    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.current_line.is_empty()
    }

    /// Number of display rows (committed + optional partial current line).
    pub(crate) fn display_row_count(&self) -> usize {
        self.rows.len() + usize::from(!self.current_line.is_empty())
    }

    /// Returns the in-progress (not yet committed) current line as a Row,
    /// if non-empty. Avoids the full Vec clone that `display_rows()` performs.
    pub(crate) fn trailing_display_row(&self) -> Option<Row> {
        if self.current_line.is_empty() {
            None
        } else {
            Some(make_row(self.current_line.clone(), false))
        }
    }

    /// Rows suitable for display, including the current partial line if any.
    pub(crate) fn display_rows(&self) -> Vec<Row> {
        let mut out = self.rows.clone();
        if !self.current_line.is_empty() {
            out.push(make_row(self.current_line.clone(), false));
        }
        out
    }

    fn flush_current_line(&mut self, explicit_break: bool) {
        // Wrap any remaining content in the current line and then finalize with explicit_break.
        self.wrap_current_line();
        // If the current line ended exactly on a width boundary and is non-empty, represent
        // the explicit break as an empty explicit row so that fragmentation invariance holds.
        if explicit_break {
            if self.current_line.is_empty() {
                // We ended on a boundary previously; add an empty explicit row.
                self.rows.push(make_row(String::new(), true));
            } else {
                // There is leftover content that did not wrap yet; push it now with the explicit flag.
                let mut s = String::new();
                std::mem::swap(&mut s, &mut self.current_line);
                self.rows.push(make_row(s, true));
            }
        }
        // Reset current line buffer for next logical line.
        self.current_line.clear();
    }

    fn wrap_current_line(&mut self) {
        // While the current_line exceeds width, cut a prefix.
        loop {
            if self.current_line.is_empty() {
                break;
            }
            let (prefix, suffix, taken) =
                take_prefix_by_width(&self.current_line, self.target_width);
            if taken == 0 {
                // Avoid infinite loop on pathological inputs; take one scalar and continue.
                if let Some((i, ch)) = self.current_line.char_indices().next() {
                    let len = i + ch.len_utf8();
                    let p = self.current_line[..len].to_string();
                    self.rows.push(make_row(p, false));
                    self.current_line = self.current_line[len..].to_string();
                    continue;
                }
                break;
            }
            if suffix.is_empty() {
                // Fits entirely; keep in buffer (do not push yet) so we can append more later.
                break;
            }
            // Prefer wrapping at word boundaries so we don't split words
            // unless a single word exceeds the target width.
            if let Some((ws_idx, ws_ch)) = prefix
                    .char_indices()
                    .rev()
                    .find(|(_, ch)| ch.is_whitespace())
                {
                    let before = prefix[..ws_idx].trim_end();
                    if !before.is_empty() {
                        let mut rest = String::new();
                        rest.push_str(&prefix[ws_idx + ws_ch.len_utf8()..]);
                        rest.push_str(suffix);
                        self.rows.push(make_row(before.to_string(), false));
                        self.current_line = rest;
                        continue;
                    }
                }

                // If the "word" doesn't contain any whitespace in the visible prefix, try
                // to split on hyphens before falling back to a hard wrap. This keeps
                // hyphenated identifiers readable.
                //
                // Special case: ignore a leading run of '-' so we don't split `--flag`
                // between the leading dashes; prefer splitting on internal hyphens.
                let leading_dashes = prefix
                    .as_bytes()
                    .iter()
                    .take_while(|b| **b == b'-')
                    .count();
                if leading_dashes < prefix.len()
                    && let Some(rel_idx) = prefix[leading_dashes..].rfind('-')
                {
                    let split_after = leading_dashes.saturating_add(rel_idx).saturating_add(1);
                    if split_after > 0 && split_after <= prefix.len() {
                        let mut rest = String::new();
                        rest.push_str(&prefix[split_after..]);
                        rest.push_str(suffix);
                        self.rows.push(make_row(prefix[..split_after].to_string(), false));
                        self.current_line = rest;
                        continue;
                    }
                }

                // Fall back to a hard wrap (no whitespace in the visible prefix).
                self.rows.push(make_row(prefix, false));
                self.current_line = suffix.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_at_whitespace_when_possible() {
        let mut builder = RowBuilder::new(10);
        builder.push_fragment("hello world");
        let rows = builder.display_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text, "hello");
        assert_eq!(rows[1].text, "world");
    }

    #[test]
    fn hard_wraps_when_no_whitespace_fits() {
        let mut builder = RowBuilder::new(4);
        builder.push_fragment("abcdefgh");
        let rows = builder.display_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text, "abcd");
        assert_eq!(rows[1].text, "efgh");
    }

    #[test]
    fn wraps_at_internal_hyphen_before_hard_wrap() {
        // Choose a width where the internal `-` is within the visible prefix and the
        // remainder fits on the next line without further wrapping.
        let mut builder = RowBuilder::new(8);
        builder.push_fragment("--my-example");
        let rows = builder.display_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text, "--my-");
        assert_eq!(rows[1].text, "example");
    }
}

/// Take a prefix of `text` whose visible width is at most `max_cols`.
/// Returns (prefix, suffix, prefix_width).
pub(crate) fn take_prefix_by_width(text: &str, max_cols: usize) -> (String, &str, usize) {
    if max_cols == 0 || text.is_empty() {
        return (String::new(), text, 0);
    }
    let mut cols = 0usize;
    let mut end_idx = 0usize;
    for (i, ch) in text.char_indices() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if cols.saturating_add(ch_width) > max_cols {
            break;
        }
        cols += ch_width;
        end_idx = i + ch.len_utf8();
        if cols == max_cols {
            break;
        }
    }
    let prefix = text[..end_idx].to_string();
    let suffix = &text[end_idx..];
    (prefix, suffix, cols)
}
