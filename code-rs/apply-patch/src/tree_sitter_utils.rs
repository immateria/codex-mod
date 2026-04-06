use crate::{EmbeddedApplyPatch};

/// Locate an embedded `apply_patch <<EOF ... EOF` heredoc in a shell script and
/// return its patch body, optional `cd` path (when present as `cd <path> &&` on
/// the same line), and the byte range to remove from the script.
///
/// This uses a lightweight textual scan that works reliably for the common
/// forms we emit in tool calls. It intentionally avoids failing on unexpected
/// syntax; if anything is off, we simply return Ok(None).
pub(crate) fn find_embedded_apply_patch(script: &str) -> Result<Option<EmbeddedApplyPatch>, ()> {
    let _bytes = script.as_bytes();
    let mut i = 0usize;
    // Support both command spellings accepted by apply_patch tooling.
    // We scan for the next occurrence of either token and treat the first one
    // we find (closest to the current cursor) as the candidate.
    const CMD1: &str = "apply_patch";
    const CMD2: &str = "applypatch";
    while i < script.len() {
        // Find next match of either token from i
        let p1 = script[i..].find(CMD1).map(|p| (p, CMD1.len()));
        let p2 = script[i..].find(CMD2).map(|p| (p, CMD2.len()));
        let (rel, cmd_len) = match (p1, p2) {
            (Some(a), Some(b)) => if a.0 <= b.0 { a } else { b },
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => break,
        };
        let start = i + rel;
        // Ensure token boundary (start or whitespace/punct before, and space or '<' after)
        let ok_before = start == 0
            || script[..start]
                .chars()
                .rev()
                .next()
                .map(|c| c.is_whitespace() || ";|&".contains(c))
                .unwrap_or(true);
        let after = script[start..].chars().skip(cmd_len).next();
        let ok_after = after.is_some_and(|c| c.is_whitespace() || c == '<');
        if !ok_before || !ok_after {
            i = start + cmd_len;
            continue;
        }

        // Find heredoc op: << (allow spaces between)
        let rest = &script[start + cmd_len..];
        let mut j = 0usize;
        // Skip spaces
        while j < rest.len() && rest.as_bytes()[j].is_ascii_whitespace() { j += 1; }
        if j + 1 >= rest.len() || rest[j..].get(..2).unwrap_or("") != "<<" {
            i = start + cmd_len;
            continue;
        }
        j += 2; // past <<
        // Skip spaces
        while j < rest.len() && rest.as_bytes()[j].is_ascii_whitespace() { j += 1; }
        if j >= rest.len() { break; }
        // Parse delimiter token: EOF, 'EOF', or "EOF"
        let (delim, after_delim_idx) = match rest.as_bytes()[j] {
            b'\'' => {
                // 'EOF'
                let k = rest[j+1..].find('\'').map(|p| j + 1 + p);
                if let Some(endq) = k { (&rest[j+1..endq], endq + 1) } else { i = start + 2; continue; }
            }
            b'"' => {
                let k = rest[j+1..].find('"').map(|p| j + 1 + p);
                if let Some(endq) = k { (&rest[j+1..endq], endq + 1) } else { i = start + 2; continue; }
            }
            _ => {
                // Bare word up to whitespace
                let mut k = j;
                while k < rest.len() && !rest.as_bytes()[k].is_ascii_whitespace() { k += 1; }
                (&rest[j..k], k)
            }
        };
        let delim = delim.trim();
        if delim.is_empty() { i = start + 2; continue; }

        // Find end of header line (newline)
        let header_slice = &rest[after_delim_idx..];
        let Some(nl_rel) = header_slice.find('\n') else { break };
        let header_end = start + cmd_len + after_delim_idx + nl_rel + 1; // pos after newline

        // Search for terminator line equal to delim
        let mut scan = header_end;
        let mut found_end: Option<usize> = None;
        while scan < script.len() {
            let _line_start = scan;
            if let Some(nl) = script[scan..].find('\n') {
                let line = &script[scan..scan+nl];
                if line == delim { found_end = Some(scan + nl + 1); break; }
                scan += nl + 1;
            } else {
                // Last line without newline
                let line = &script[scan..];
                if line == delim { found_end = Some(script.len()); }
                break;
            }
        }
        let Some(body_end) = found_end else { i = start + 2; continue; };

        // Determine line start for this statement and optional preceding `cd <path> &&`
        let line_start = script[..start].rfind('\n').map_or(0, |p| p + 1);
        let before_apply = &script[line_start..start];
        let mut cd_path: Option<String> = None;
        let mut stmt_begin = start;
        {
            let prefix = before_apply.trim_end();
            // Try to match "cd <path> &&" directly before apply_patch (allow whitespace)
            if let Some(and_and_pos) = prefix.rfind("&&") {
                let left = prefix[..and_and_pos].trim_end();
                // Ensure no other tokens after && besides whitespace
                if prefix[and_and_pos+2..].trim().is_empty() {
                    if let Some(rest) = left.strip_suffix(|c: char| c.is_whitespace()) {
                        let left_trim = rest.trim_end();
                        if let Some(arg) = left_trim.strip_prefix("cd ") {
                            let path = arg.trim();
                            // Take first token or a single-quoted/quoted string
                            let path_str = if (path.starts_with('\'') && path.ends_with('\'')) || (path.starts_with('"') && path.ends_with('"')) {
                                path[1..path.len().saturating_sub(1)].to_string()
                            } else {
                                // up to next whitespace
                                let tok_end = path.find(char::is_whitespace).unwrap_or(path.len());
                                path[..tok_end].to_string()
                            };
                            cd_path = Some(path_str);
                            // Include the cd... && in the removal range
                            stmt_begin = line_start + left.find("cd ").unwrap_or(0);
                        }
                    }
                }
            }
        }

        let patch_body = script[header_end..body_end].trim_end_matches('\n').to_string();
        return Ok(Some(EmbeddedApplyPatch { patch_body, cd_path, stmt_byte_range: (stmt_begin, body_end) }));
    }
    Ok(None)
}
