// Turn inline markdown links and bare URLs into display-friendly spans.
// NOTE: We intentionally avoid emitting OSC 8 hyperlinks here. While OSC 8
// hyperlinks render correctly when static, some terminals exhibit artifacts
// when scrolling or re-wrapping content that contains embedded escape
// sequences. By rendering labels as plain underlined text and leaving literal
// URLs as-is (so the terminal can auto-detect them), we guarantee stable
// rendering during scroll without leaking control characters.
//
// Behavior:
// - Markdown links [label](target): render as an underlined `label` span
//   (no OSC 8). We prefer stability over clickability for labeled links.
// - Explicit http(s) URLs and bare domains: emit verbatim text so terminals
//   can auto-link them. This keeps them clickable without control sequences.
fn autolink_spans(spans: Vec<Span<'static>>) -> Vec<Span<'static>> {
    // Patterns: markdown [label](target), explicit http(s) URLs, and plain domains.
    // Keep conservative to avoid false positives.
    static EXPL_URL_RE: once_cell::sync::OnceCell<Option<Regex>> = once_cell::sync::OnceCell::new();
    static DOMAIN_RE: once_cell::sync::OnceCell<Option<Regex>> = once_cell::sync::OnceCell::new();
    // We will parse Markdown links manually to support URLs with parentheses.
    let Some(url_re) = EXPL_URL_RE
        .get_or_init(|| Regex::new(r"(?i)\bhttps?://[^\s)]+").ok())
        .as_ref()
    else {
        return spans;
    };
    let Some(dom_re) = DOMAIN_RE
        .get_or_init(|| {
        // Conservative bare-domain matcher (no scheme). Examples:
        //   apps.shopify.com
        //   foo.example.io/path?x=1
        // It intentionally over-matches a bit; we further filter below.
            Regex::new(r"\b([a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+(?:/[\w\-./?%&=#]*)?)")
                .ok()
        })
        .as_ref()
    else {
        return spans;
    };

    // Trim common trailing punctuation from a detected URL/domain, returning
    // (core, trailing). The trailing part will be emitted as normal text (not
    // hyperlinked) so that tokens like "example.com." don’t include the period.
    fn split_trailing_punct(s: &str) -> (&str, &str) {
        let bytes = s.as_bytes();
        let mut end = bytes.len();
        while end > 0 {
            let ch = bytes[end - 1] as char;
            if ")]}>'\".,!?;:".contains(ch) {
                // common trailing punctuation
                end -= 1;
                continue;
            }
            break;
        }
        (&s[..end], &s[end..])
    }

    // Additional heuristic to avoid false positives like "e.g." or
    // "filename.rs" by requiring a well-known TLD. Precision over recall.
    fn is_probable_domain(dom: &str) -> bool {
        if dom.contains('@') {
            return false;
        }
        let dot_count = dom.matches('.').count();
        if dot_count == 0 {
            return false;
        }

        // Extract the final label (candidate TLD) and normalize.
        let tld = dom
            .rsplit_once('.')
            .map(|(_, t)| t)
            .unwrap_or("")
            .trim_matches(|c: char| !c.is_ascii_alphanumeric())
            .to_ascii_lowercase();

        // Small allowlist of popular TLDs. This intentionally excludes
        // language/file extensions like `.rs`, `.ts`, `.php`, etc.
        const ALLOWED_TLDS: &[&str] = &[
            "com", "org", "net", "edu", "gov", "io", "ai", "app", "dev", "co", "us", "uk", "ca",
            "de", "fr", "jp", "cn", "in", "au",
        ];

        ALLOWED_TLDS.contains(&tld.as_str())
    }

    let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len());
    // Slight blue-tinted color for visible URLs/domains. Blend theme text toward primary (blue-ish).
    let link_fg = crate::colors::mix_toward(
        crate::colors::text(),
        crate::colors::primary(),
        0.35,
    );
    for s in spans {
        // Skip autolinking inside inline code spans (we style code with the
        // theme's function color). This avoids linking snippets like
        // `curl https://example.com` or code identifiers containing dots.
        // Also skip when span already contains OSC8 sequences to avoid corrupting
        // hyperlinks with additional parsing or wrapping.
        if s.style.fg == Some(crate::colors::function()) || s.content.contains('\u{1b}') {
            out.push(s);
            continue;
        }
        let text = s.content.clone();
        let mut cursor = 0usize;
        let mut changed = false;

        // Scan left-to-right, preferring markdown links first (explicit intent),
        // then explicit URLs, then bare domains.
        while cursor < text.len() {
            let after = &text[cursor..];

            // 1) markdown links [label](target) with balanced parentheses in target
            if let Some((start, end, label, target)) = find_markdown_link(after) {
                let abs_start = cursor + start;
                let abs_end = cursor + end;
                if cursor < abs_start {
                    let mut span = s.clone();
                    span.content = text[cursor..abs_start].to_string().into();
                    out.push(span);
                }
                // Special case: when the label is just a short preview of the target URL
                // (e.g., "front.com" for "https://front.com/integrations/…"), emit ONLY
                // the full URL so terminals can auto‑link it. Avoid underlines/parentheses
                // to keep scroll rendering stable and prevent duplicate visual tokens.
                if is_short_preview_of_url(&label, &target) {
                    let mut url_only = s.clone();
                    url_only.content = target.clone().into();
                    url_only.style = url_only.style.patch(Style::default().fg(link_fg));
                    out.push(url_only);
                } else {
                    // Default behavior: underlined label followed by dimmed URL in parens
                    let mut lbl_span = s.clone();
                    let mut st = lbl_span.style;
                    st.add_modifier.insert(Modifier::UNDERLINED);
                    lbl_span.style = st;
                    lbl_span.content = label.into();
                    out.push(lbl_span);
                    let mut open_span = s.clone();
                    open_span.content = " (".into();
                    out.push(open_span);
                    let mut url_span = s.clone();
                    url_span.style = url_span.style.patch(Style::default().fg(link_fg));
                    url_span.content = target.clone().into();
                    out.push(url_span);
                    let mut close_span = s.clone();
                    close_span.content = ")".into();
                    out.push(close_span);
                }
                cursor = abs_end;
                changed = true;
                continue;
            }

            // 2) explicit http(s) URLs (Mixed mode: do NOT wrap; let terminal detect)
            if let Some(m) = url_re.find(after) {
                let start = cursor + m.start();
                let end = cursor + m.end();
                if cursor < start {
                    let mut span = s.clone();
                    span.content = text[cursor..start].to_string().into();
                    out.push(span);
                }
                let raw = &text[start..end];
                let (core, trailing) = split_trailing_punct(raw);
                // Emit URL text verbatim; terminal will make it clickable.
                let mut core_span = s.clone();
                core_span.content = core.to_string().into();
                core_span.style = core_span.style.patch(Style::default().fg(link_fg));
                out.push(core_span);
                if !trailing.is_empty() {
                    let mut span = s.clone();
                    span.content = trailing.to_string().into();
                    out.push(span);
                }
                cursor = start + core.len() + trailing.len();
                changed = true;
                continue;
            }

            // 3) bare domain: emit as text so terminal can auto-link
            if let Some(m) = dom_re.find(after) {
                let start = cursor + m.start();
                let end = cursor + m.end();
                if cursor < start {
                    let mut span = s.clone();
                    span.content = text[cursor..start].to_string().into();
                    out.push(span);
                }
                let raw = &text[start..end];
                let (core_dom, trailing) = split_trailing_punct(raw);
                if is_probable_domain(core_dom) {
                    let mut core_span = s.clone();
                    core_span.content = core_dom.to_string().into();
                    core_span.style = core_span.style.patch(Style::default().fg(link_fg));
                    out.push(core_span);
                    if !trailing.is_empty() {
                        let mut span = s.clone();
                        span.content = trailing.to_string().into();
                        out.push(span);
                    }
                    cursor = start + core_dom.len() + trailing.len();
                    changed = true;
                    continue;
                }
                // Not a probable domain; emit raw text
                let mut span = s.clone();
                span.content = raw.to_string().into();
                out.push(span);
                cursor = end;
                changed = true;
                continue;
            }

            // No more matches
            break;
        }

        if changed {
            if cursor < text.len() {
                let mut span = s.clone();
                span.content = text[cursor..].to_string().into();
                out.push(span);
            }
        } else {
            out.push(s);
        }
    }

    out
}

// Return (start, end, label, target) for the first markdown link found in `s`.
fn find_markdown_link(s: &str) -> Option<(usize, usize, String, String)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            // find closing ']'
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b']' {
                j += 1;
            }
            if j >= bytes.len() {
                return None;
            }
            // next must be '('
            let mut k = j + 1;
            if k >= bytes.len() || bytes[k] != b'(' {
                i += 1;
                continue;
            }
            k += 1; // position after '('
            // parse target allowing balanced parentheses
            let mut depth = 1usize;
            let targ_start = k;
            while k < bytes.len() {
                match bytes[k] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            if k >= bytes.len() || depth != 0 {
                return None;
            }
            let label = &s[i + 1..j];
            let target = &s[targ_start..k];
            // Full match is from i ..= k
            return Some((i, k + 1, label.to_string(), target.to_string()));
        }
        i += 1;
    }
    None
}

// Return (consumed_chars, label, target) for the first image starting at s (which begins with '!').
fn find_markdown_image(s: &str) -> Option<(usize, String, String)> {
    let bytes = s.as_bytes();
    if !bytes.starts_with(b"![") {
        return None;
    }
    // Parse label
    let mut i = 2; // after ![
    while i < bytes.len() && bytes[i] != b']' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let label = &s[2..i];
    // Next must be '('
    let mut k = i + 1;
    if k >= bytes.len() || bytes[k] != b'(' {
        return None;
    }
    k += 1;
    // Parse target allowing balanced parentheses and optional title
    let mut depth = 1usize;
    let targ_start = k;
    while k < bytes.len() {
        match bytes[k] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        k += 1;
    }
    if k >= bytes.len() || depth != 0 {
        return None;
    }
    let mut target = s[targ_start..k].trim().to_string();
    // Strip optional quoted title at end: url "title"
    if let Some(space_idx) = target.rfind(' ') {
        let (left, right) = target.split_at(space_idx);
        let t = right.trim();
        if (t.starts_with('\"') && t.ends_with('\"')) || (t.starts_with('\'') && t.ends_with('\''))
        {
            target = left.trim().to_string();
        }
    }
    Some((k + 1, label.to_string(), target)) // consumed up to and including ')'
}

// Heuristic: determine if `label` is a short preview (typically a bare domain)
// for the full `target` URL. When true, we should display only the full URL
// (letting the terminal auto-link it) instead of "label (url)".
fn is_short_preview_of_url(label: &str, target: &str) -> bool {
    // Must be an explicit HTTP(S) URL and label must be shorter
    let lt = label.trim();
    let tt = target.trim();
    if lt.is_empty() || tt.is_empty() {
        return false;
    }
    let lower_t = tt.to_ascii_lowercase();
    if !(lower_t.starts_with("http://") || lower_t.starts_with("https://")) {
        return false;
    }
    if lt.chars().count() >= tt.chars().count() {
        return false;
    }

    // Normalize a string into a domain host if possible.
    fn extract_host(s: &str) -> Option<String> {
        let s = s.trim();
        let lower = s.to_ascii_lowercase();
        let without_scheme = if lower.starts_with("http://") {
            &s[7..]
        } else if lower.starts_with("https://") {
            &s[8..]
        } else {
            s
        };
        let mut host = without_scheme
            .split(&['/', '?', '#'][..])
            .next()
            .unwrap_or("")
            .to_string();
        if host.is_empty() {
            return None;
        }
        // Strip userinfo and port if present
        if let Some(idx) = host.rfind('@') {
            host = host[idx + 1..].to_string();
        }
        if let Some(idx) = host.find(':') {
            host = host[..idx].to_string();
        }
        // Drop leading www.
        let host = host.trim().trim_matches('.').to_ascii_lowercase();
        let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
        if host.contains('.') { Some(host) } else { None }
    }

    // Lightweight TLD guard for labels that aren't URLs
    fn looks_like_domain(s: &str) -> bool {
        let s = s.trim().trim_end_matches('/');
        if !s.contains('.') || s.contains(' ') { return false; }
        let tld = s.rsplit_once('.').map(|(_, t)| t).unwrap_or("").to_ascii_lowercase();
        const ALLOWED_TLDS: &[&str] = &[
            "com","org","net","edu","gov","io","ai","app","dev","co","us","uk","ca","de","fr","jp","cn","in","au"
        ];
        ALLOWED_TLDS.contains(&tld.as_str())
    }

    let label_host = if lower_t.starts_with("http://") || lower_t.starts_with("https://") {
        // If label itself is a URL, compare its host; otherwise, treat as domain text
        extract_host(lt).or_else(|| Some(lt.to_ascii_lowercase()))
    } else {
        Some(lt.to_ascii_lowercase())
    };
    let target_host = extract_host(tt);

    match (label_host, target_host) {
        (Some(lh_raw), Some(mut th)) => {
            let mut lh = lh_raw.trim().trim_end_matches('/').to_ascii_lowercase();
            if lh.starts_with("www.") { lh = lh.trim_start_matches("www.").to_string(); }
            if th.starts_with("www.") { th = th.trim_start_matches("www.").to_string(); }
            // Require label to look like a domain to avoid stripping arbitrary text
            if !looks_like_domain(&lh) { return false; }
            // Exact match or label equals the registrable/root portion of the host
            if lh == th { return true; }
            // Host may include subdomains; if it ends with .<label>, treat as preview
            if th.ends_with(&format!(".{lh}")) { return true; }
            false
        }
        _ => false,
    }
}

