use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::execute_custom_tool;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use code_browser::BrowserConfig as CodexBrowserConfig;
use code_browser::BrowserManager;
use std::time::Duration;

pub(crate) struct WebFetchToolHandler;

#[async_trait]
impl ToolHandler for WebFetchToolHandler {
    fn is_parallel_safe(&self) -> bool {
        true
    }

    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Function { arguments } = inv.payload else {
            return ResponseInputItem::FunctionCallOutput {
                call_id: inv.ctx.call_id,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        "web_fetch expects function-call arguments".to_string(),
                    ),
                    success: Some(false),
                },
            };
        };

        handle_web_fetch(sess, &inv.ctx, arguments).await
    }
}

pub(crate) async fn handle_web_fetch(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    // Include raw params in begin event for observability
    let mut params_for_event = serde_json::from_str::<serde_json::Value>(&arguments).ok();
    // If call_id is provided, include a friendly "for" string with the command we are waiting on
    if let Some(serde_json::Value::Object(map)) = params_for_event.as_mut()
        && let Some(serde_json::Value::String(cid)) = map.get("call_id")
        && let Some(display) = sess.background_exec_cmd_display(cid)
    {
        map.insert("for".to_string(), serde_json::Value::String(display));
    }
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "web_fetch".to_string(),
        params_for_event,
        || async move {
            #[derive(serde::Deserialize)]
            struct WebFetchParams {
                url: String,
                #[serde(default)]
                timeout_ms: Option<u64>,
                #[serde(default)]
                mode: Option<String>, // "auto" (default), "browser", or "http"
            }

            let parsed: Result<WebFetchParams, _> = serde_json::from_str(&arguments_clone);
            let params = match parsed {
                Ok(p) => p,
                Err(e) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Invalid web_fetch arguments: {e}")),
                            success: Some(false),
                        },
                    };
                }
            };

            struct BrowserFetchOutcome {
                html: String,
                final_url: Option<String>,
                headless: bool,
            }

            async fn fetch_html_via_headless_browser(
                url: &str,
                timeout: Duration,
            ) -> Result<BrowserFetchOutcome, String> {
                let config = CodexBrowserConfig {
                    enabled: true,
                    headless: true,
                    fullpage: false,
                    segments_max: 2,
                    persist_profile: false,
                    idle_timeout_ms: 10_000,
                    ..CodexBrowserConfig::default()
                };

                let manager = BrowserManager::new(config);
                manager.set_enabled_sync(true);

                const CHECK_JS: &str = r#"(function(){
  const discuss = document.querySelectorAll('[data-test-selector=\"issue-comment-body\"]');
  const timeline = document.querySelectorAll('.js-timeline-item');
  const article = document.querySelectorAll('article, main');
  return (discuss.length + timeline.length + article.length);
})()"#;
                const HTML_JS: &str =
                    "(function(){ return { html: document.documentElement.outerHTML, title: document.title||'' }; })()";

                let goto_result = match tokio::time::timeout(timeout, manager.goto(url)).await {
                    Ok(Ok(res)) => res,
                    Ok(Err(e)) => {
                        let _ = manager.stop().await;
                        return Err(format!("Headless goto failed: {e}"));
                    }
                    Err(_) => {
                        let _ = manager.stop().await;
                        return Err("Headless goto timed out".to_string());
                    }
                };

                for _ in 0..6 {
                    match tokio::time::timeout(Duration::from_millis(1500), manager.execute_javascript(CHECK_JS)).await {
                        Ok(Ok(val)) => {
                            let count = val
                                .get("value")
                                .and_then(serde_json::Value::as_i64)
                                .unwrap_or(0);
                            if count > 0 {
                                break;
                            }
                        }
                        Ok(Err(e)) => {
                            tracing::debug!("Headless readiness check failed: {}", e);
                            break;
                        }
                        Err(_) => {
                            tracing::debug!("Headless readiness check timed out");
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(800)).await;
                }

                let html_value = match tokio::time::timeout(timeout, manager.execute_javascript(HTML_JS)).await {
                    Ok(Ok(val)) => val,
                    Ok(Err(e)) => {
                        let _ = manager.stop().await;
                        return Err(format!("Headless HTML extraction failed: {e}"));
                    }
                    Err(_) => {
                        let _ = manager.stop().await;
                        return Err("Headless HTML extraction timed out".to_string());
                    }
                };

                let html = html_value
                    .get("value")
                    .and_then(|v| v.get("html"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if html.trim().is_empty() {
                    let _ = manager.stop().await;
                    return Err("Headless browser returned empty HTML".to_string());
                }

                let final_url = Some(goto_result.url.clone());
                let _ = manager.stop().await;

                Ok(BrowserFetchOutcome {
                    html,
                    final_url,
                    headless: true,
                })
            }

            async fn fetch_html_via_browser(
                url: &str,
                timeout: Duration,
                prefer_global: bool,
            ) -> Option<BrowserFetchOutcome> {
                const HTML_JS: &str =
                    "(function(){ return { html: document.documentElement.outerHTML, title: document.title||'' }; })()";
                const CHECK_JS: &str = r#"(function(){
  const discuss = document.querySelectorAll('[data-test-selector=\"issue-comment-body\"]');
  const timeline = document.querySelectorAll('.js-timeline-item');
  const article = document.querySelectorAll('article, main');
  return (discuss.length + timeline.length + article.length);
})()"#;

                if prefer_global
                    && let Some(manager) = code_browser::global::get_browser_manager().await {
                        if manager.is_enabled_sync() {
                            match tokio::time::timeout(timeout, manager.goto(url)).await {
                                Ok(Ok(res)) => {
                                    for _ in 0..6 {
                                        match tokio::time::timeout(Duration::from_millis(1500), manager.execute_javascript(CHECK_JS)).await {
                                            Ok(Ok(val)) => {
                                                let count = val
                                                    .get("value")
                                                    .and_then(serde_json::Value::as_i64)
                                                    .unwrap_or(0);
                                                if count > 0 {
                                                    break;
                                                }
                                            }
                                            Ok(Err(e)) => {
                                                tracing::debug!("Global browser readiness check failed: {}", e);
                                                break;
                                            }
                                            Err(_) => {
                                                tracing::debug!("Global browser readiness timed out");
                                                break;
                                            }
                                        }
                                        tokio::time::sleep(Duration::from_millis(800)).await;
                                    }

                                    match tokio::time::timeout(timeout, manager.execute_javascript(HTML_JS)).await {
                                        Ok(Ok(val)) => {
                                            if let Some(html) = val
                                                .get("value")
                                                .and_then(|v| v.get("html"))
                                                .and_then(|v| v.as_str())
                                                && !html.trim().is_empty() {
                                                    return Some(BrowserFetchOutcome {
                                                        html: html.to_string(),
                                                        final_url: Some(res.url.clone()),
                                                        headless: false,
                                                    });
                                                }
                                        }
                                        Ok(Err(e)) => {
                                            tracing::debug!("Global browser HTML extraction failed: {}", e);
                                        }
                                        Err(_) => {
                                            tracing::debug!("Global browser HTML extraction timed out");
                                        }
                                    }
                                }
                                Ok(Err(e)) => {
                                    tracing::warn!("Global browser navigation failed: {}", e);
                                }
                                Err(_) => {
                                    tracing::warn!("Global browser navigation timed out");
                                }
                            }
                        } else {
                            tracing::debug!("Global browser manager disabled; skipping UI fetch");
                        }
                    }

                match fetch_html_via_headless_browser(url, timeout).await {
                    Ok(outcome) => Some(outcome),
                    Err(err) => {
                        tracing::warn!("Headless browser fallback failed for {}: {}", url, err);
                        None
                    }
                }
            }

            // Helper: build a client with a specific UA and common headers.
            async fn do_request(
                url: &str,
                ua: &str,
                timeout: Duration,
                extra_headers: Option<&[(reqwest::header::HeaderName, &'static str)]>,
            ) -> Result<reqwest::Response, reqwest::Error> {
                let client = reqwest::Client::builder()
                    .timeout(timeout)
                    .user_agent(ua)
                    .build()?;
                let mut req = client.get(url)
                    // Add a few browser-like headers to reduce blocks
                    .header(reqwest::header::ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
                    .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9");
                if let Some(pairs) = extra_headers {
                    for (k, v) in pairs.iter() {
                        req = req.header(k, *v);
                    }
                }
                req.send().await
            }

            // Helper: remove obvious noisy blocks before markdown conversion.
            // This uses a lightweight ASCII-insensitive scan to drop whole
            // elements whose contents should never be surfaced to the model
            // (scripts, styles, templates, headers/footers/navigation, etc.).
            fn strip_noisy_tags(mut html: String) -> String {
                // Remove <script>, <style>, and <noscript> blocks with a simple
                // ASCII case-insensitive scan that preserves UTF-8 boundaries.
                // This avoids allocating lowercase copies and accidentally using
                // indices from a different string representation.
                fn eq_ascii_ci(a: u8, b: u8) -> bool {
                    a.eq_ignore_ascii_case(&b)
                }
                fn starts_with_tag_ci(bytes: &[u8], tag: &[u8]) -> bool {
                    if bytes.len() < tag.len() { return false; }
                    for i in 0..tag.len() {
                        if !eq_ascii_ci(bytes[i], tag[i]) { return false; }
                    }
                    true
                }
                // Find the next opening tag like "<script" (allowing whitespace after '<').
                fn find_open_tag_ci(s: &str, tag: &str, from: usize) -> Option<usize> {
                    let bytes = s.as_bytes();
                    let tag_bytes = tag.as_bytes();
                    let mut i = from;
                    while i + 1 < bytes.len() {
                        if bytes[i] == b'<' {
                            let mut j = i + 1;
                            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r') {
                                j += 1;
                            }
                            if j < bytes.len() && starts_with_tag_ci(&bytes[j..], tag_bytes) {
                                return Some(i);
                            }
                        }
                        i += 1;
                    }
                    None
                }
                // Find the corresponding closing tag like "</script>" starting at or after `from`.
                // Returns the byte index just after the closing '>' if found.
                fn find_close_after_ci(s: &str, tag: &str, from: usize) -> Option<usize> {
                    let bytes = s.as_bytes();
                    let tag_bytes = tag.as_bytes();
                    let mut i = from;
                    while i + 2 < bytes.len() { // need at least '<' '/' and one tag byte
                        if bytes[i] == b'<' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                            let mut j = i + 2;
                            // Optional whitespace before tag name
                            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r') {
                                j += 1;
                            }
                            if starts_with_tag_ci(&bytes[j..], tag_bytes) {
                                // Advance past tag name
                                j += tag_bytes.len();
                                // Skip optional whitespace until '>'
                                while j < bytes.len() && bytes[j] != b'>' {
                                    j += 1;
                                }
                                if j < bytes.len() && bytes[j] == b'>' {
                                    return Some(j + 1);
                                }
                                return None; // No closing '>'
                            }
                        }
                        i += 1;
                    }
                    None
                }

                // Keep this conservative to avoid dropping content.
                let tags = ["script", "style", "noscript"];
                for tag in tags.iter() {
                    let mut guard = 0;
                    loop {
                        if guard > 64 { break; }
                        let Some(start) = find_open_tag_ci(&html, tag, 0) else { break; };
                        let search_from = start + 1; // after '<'
                        if let Some(end) = find_close_after_ci(&html, tag, search_from) {
                            // Safe because both start and end are on ASCII boundaries ('<' and '>')
                            html.replace_range(start..end, "");
                        } else {
                            // No close tag found; drop from the opening tag to end
                            html.truncate(start);
                            break;
                        }
                        guard += 1;
                    }
                }
                html
            }

            // Try to keep only <main> content if present; drastically reduces
            // boilerplate from navigation and login banners on many sites.
            fn extract_main(html: &str) -> Option<String> {
                // Find opening <main ...>
                let bytes = html.as_bytes();
                let open = {
                    let mut i = 0usize;
                    let tag = b"main";
                    let mut found = None;
                    while i + 5 < bytes.len() { // < m a i n > (min)
                        let candidate = if bytes[i] == b'<' {
                            // skip '<' and whitespace
                            let mut j = i + 1;
                            while j < bytes.len() && bytes[j].is_ascii_whitespace() { j += 1; }
                            if j + tag.len() <= bytes.len() && bytes[j..j+tag.len()].eq_ignore_ascii_case(tag) {
                                // Found '<main'; now find '>'
                                while j < bytes.len() && bytes[j] != b'>' { j += 1; }
                                if j < bytes.len() { Some((i, j + 1)) } else { None }
                            } else { None }
                        } else {
                            None
                        };
                        if let Some(candidate) = candidate {
                            found = Some(candidate);
                            break;
                        }
                        i += 1;
                    }
                    found
                };
                let (start, after_open) = open?;
                // Find closing </main>
                let mut i = after_open;
                let tag_close = b"</main";
                while i + tag_close.len() + 1 < bytes.len() {
                    if bytes[i] == b'<' && bytes[i+1] == b'/'
                        && bytes[i..].len() >= tag_close.len() && bytes[i..i+tag_close.len()].eq_ignore_ascii_case(tag_close) {
                            // Find closing '>'
                            let mut j = i + tag_close.len();
                            while j < bytes.len() && bytes[j] != b'>' { j += 1; }
                            if j < bytes.len() {
                                return Some(html[start..j+1].to_string());
                            } else {
                                return Some(html[start..].to_string());
                            }
                        }
                    i += 1;
                }
                Some(html[start..].to_string())
            }

            // Inside fenced code blocks, collapse massively-escaped Windows paths like
            // `C:\\Users\\...` to `C:\Users\...`. Only applies to drive-rooted paths.
            fn unescape_windows_paths(line: &str) -> String {
                let bytes = line.as_bytes();
                let mut out = String::with_capacity(line.len());
                let mut i = 0usize;
                while i < bytes.len() {
                    // Pattern: [A-Za-z] : \\+
                    if i + 3 < bytes.len()
                        && bytes[i].is_ascii_alphabetic()
                        && bytes[i+1] == b':'
                        && bytes[i+2] == b'\\'
                        && bytes[i+3] == b'\\'
                    {
                        // Emit drive and a single backslash
                        out.push(bytes[i] as char);
                        out.push(':');
                        out.push('\\');
                        // Skip all following backslashes in this run
                        i += 4;
                        while i < bytes.len() && bytes[i] == b'\\' { i += 1; }
                        continue;
                    }
                    out.push(bytes[i] as char);
                    i += 1;
                }
                out
            }

            // Lightweight cleanup on the resulting markdown to remove leaked
            // JSON blobs and obvious client boot payloads that sometimes escape
            // the <script> filter on complex sites. Avoids touching fenced code.
            fn postprocess_markdown(md: &str) -> String {
                let mut out: Vec<String> = Vec::with_capacity(md.len() / 64 + 1);
                let mut in_fence = false;
                let mut empty_run = 0usize;
                for line in md.lines() {
                    // Track fenced code blocks
                    if let Some(rest) = line.trim_start().strip_prefix("```") {
                        in_fence = !in_fence;
                        let _lang = if in_fence { Some(rest.trim()) } else { None };
                        out.push(line.to_string());
                        empty_run = 0;
                        continue;
                    }
                    if in_fence {
                        // Only normalize Windows path over-escaping; do not alter other content.
                        let normalized = unescape_windows_paths(line);
                        out.push(normalized);
                        continue;
                    }

                    let trimmed = line.trim();
                    // Drop extremely long single lines only if they're likely SPA boot payloads
                    if trimmed.len() > 8000 { continue; }
                    // Common SPA boot keys that shouldn't appear in human output.
                    // Keep this list tight to avoid dropping legitimate examples.
                    if trimmed.contains("\"payload\"") || trimmed.contains("\"props\"") || trimmed.contains("\"preloaded_records\"") || trimmed.contains("\"appPayload\"") || trimmed.contains("\"preloadedQueries\"") {
                        continue;
                    }

                    if trimmed.is_empty() {
                        // Collapse multiple empty lines to max 1
                        if empty_run == 0 {
                            out.push(String::new());
                        }
                        empty_run += 1;
                    } else {
                        out.push(line.to_string());
                        empty_run = 0;
                    }
                }
                // Trim leading/trailing blank lines
                let mut s = out.join("\n");
                while s.starts_with('\n') { s.remove(0); }
                while s.ends_with('\n') { s.pop(); }
                s
            }

            // Domain-specific: extract rich content from GitHub issue/PR pages
            // without requiring a JS-capable browser. We parse JSON-LD and the
            // inlined GraphQL payload (preloadedQueries) to reconstruct the
            // issue body and comments into readable markdown.
            fn try_extract_github_issue_markdown(html: &str) -> Option<String> {
                // Helper: extract the first <script type="application/ld+json"> block
                fn extract_ld_json(html: &str) -> Option<serde_json::Value> {
                    let mut s = html;
                    loop {
                        let start = s.find("<script")?;
                        let rest = &s[start + 7..];
                        if rest.to_lowercase().contains("type=\"application/ld+json\"") {
                            // Find end of script open tag
                            let open_end_rel = rest.find('>')?;
                            let open_end = start + 7 + open_end_rel + 1;
                            let after_open = &s[open_end..];
                            // Find closing </script>
                            if let Some(close_rel) = after_open.to_lowercase().find("</script>") {
                                let json_str = &after_open[..close_rel];
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                                    return Some(v);
                                }
                                // Some pages JSON-encode the JSON-LD; try to unescape once
                                if let Ok(un) = serde_json::from_str::<String>(json_str)
                                    && let Ok(v2) = serde_json::from_str::<serde_json::Value>(&un) {
                                        return Some(v2);
                                    }
                                // Advance after this script to search for next
                                s = &after_open[close_rel + 9..];
                                continue;
                            }
                        }
                        // Advance and continue search
                        s = &rest[1..];
                    }
                }

                // Helper: extract substring for the JSON array that follows key
                fn extract_json_array_after(html: &str, key: &str) -> Option<String> {
                    let idx = html.find(key)?;
                    let bytes = html.as_bytes();
                    // Find the first '[' after key
                    let mut i = idx + key.len();
                    while i < bytes.len() && bytes[i] != b'[' { i += 1; }
                    if i >= bytes.len() { return None; }
                    let start = i;
                    // Scan to matching ']' accounting for strings and escapes
                    let mut depth: i32 = 0;
                    let mut in_str = false;
                    let mut escape = false;
                    while i < bytes.len() {
                        let c = bytes[i] as char;
                        if in_str {
                            if escape { escape = false; }
                            else if c == '\\' { escape = true; }
                            else if c == '"' { in_str = false; }
                            i += 1; continue;
                        }
                        match c {
                            '"' => { in_str = true; },
                            '[' => { depth += 1; },
                            ']' => { depth -= 1; if depth == 0 { let end = i + 1; return Some(html[start..end].to_string()); } },
                            _ => {}
                        }
                        i += 1;
                    }
                    None
                }

                // Parse JSON-LD for headline, articleBody, author, date
                let mut title: Option<String> = None;
                let mut issue_body_md: Option<String> = None;
                let mut opened_by: Option<String> = None;
                let mut opened_at: Option<String> = None;
                if let Some(ld) = extract_ld_json(html)
                    && ld.get("@type").and_then(|v| v.as_str()) == Some("DiscussionForumPosting") {
                        title = ld.get("headline").and_then(|v| v.as_str()).map(std::string::ToString::to_string);
                        issue_body_md = ld.get("articleBody").and_then(|v| v.as_str()).map(std::string::ToString::to_string);
                        opened_by = ld.get("author").and_then(|a| a.get("name")).and_then(|v| v.as_str()).map(std::string::ToString::to_string);
                        opened_at = ld.get("datePublished").and_then(|v| v.as_str()).map(std::string::ToString::to_string);
                    }

                // Parse GraphQL payload for comments and state
                let arr_str = extract_json_array_after(html, "\"preloadedQueries\"")?;
                let arr: serde_json::Value = serde_json::from_str(&arr_str).ok()?;
                let mut comments: Vec<(String, String, String)> = Vec::new();
                let mut state: Option<String> = None;
                let mut state_reason: Option<String> = None;
                if let Some(items) = arr.as_array() {
                    for item in items {
                        let repo = item.get("result").and_then(|v| v.get("data")).and_then(|v| v.get("repository"));
                        let issue = repo.and_then(|r| r.get("issue"));
                        if let Some(issue) = issue {
                            if state.is_none() {
                                state = issue.get("state").and_then(|v| v.as_str()).map(std::string::ToString::to_string);
                                state_reason = issue.get("stateReason").and_then(|v| v.as_str()).map(std::string::ToString::to_string);
                            }
                            if let Some(edges) = issue.get("frontTimelineItems").and_then(|v| v.get("edges")).and_then(|v| v.as_array()) {
                                for e in edges {
                                    let node = e.get("node");
                                    let typename = node.and_then(|n| n.get("__typename")).and_then(|v| v.as_str()).unwrap_or("");
                                    if typename == "IssueComment" {
                                        let author = node.and_then(|n| n.get("author")).and_then(|a| a.get("login")).and_then(|v| v.as_str()).unwrap_or("");
                                        let created = node.and_then(|n| n.get("createdAt")).and_then(|v| v.as_str()).unwrap_or("");
                                        let body = node.and_then(|n| n.get("body")).and_then(|v| v.as_str()).unwrap_or("");
                                        if !body.is_empty() {
                                            comments.push((author.to_string(), created.to_string(), body.to_string()));
                                        } else {
                                            let body_html = node.and_then(|n| n.get("bodyHTML")).and_then(|v| v.as_str()).unwrap_or("");
                                            if !body_html.is_empty() {
                                                // Minimal HTML→MD for comments if body missing
                                                let options = htmd::options::Options { heading_style: htmd::options::HeadingStyle::Atx, code_block_style: htmd::options::CodeBlockStyle::Fenced, link_style: htmd::options::LinkStyle::Inlined, ..Default::default() };
                                                let conv = htmd::HtmlToMarkdown::builder().options(options).build();
                                                if let Ok(md) = conv.convert(body_html) {
                                                    comments.push((author.to_string(), created.to_string(), md));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // If nothing meaningful extracted, bail out.
                if title.is_none() && comments.is_empty() && issue_body_md.is_none() {
                    return None;
                }

                // Compose readable markdown
                let mut out = String::new();
                if let Some(t) = title { out.push_str(&format!("# {t}\n\n")); }
                if let (Some(by), Some(at)) = (opened_by, opened_at) { out.push_str(&format!("Opened by {by} on {at}\n\n")); }
                if let (Some(s), _) = (state, state_reason) { out.push_str(&format!("State: {s}\n\n")); }
                if let Some(body) = issue_body_md { out.push_str(&format!("{body}\n\n")); }
                if !comments.is_empty() {
                    out.push_str("## Comments\n\n");
                    for (author, created, body) in comments {
                        out.push_str(&format!("- {author} — {created}\n\n{body}\n\n"));
                    }
                }
                Some(out)
            }

            // Helper: convert HTML to markdown and truncate if too large.
            fn convert_html_to_markdown_trimmed(html: String, max_chars: usize) -> crate::error::Result<(String, bool)> {
                let options = htmd::options::Options {
                    heading_style: htmd::options::HeadingStyle::Atx,
                    code_block_style: htmd::options::CodeBlockStyle::Fenced,
                    link_style: htmd::options::LinkStyle::Inlined,
                    ..Default::default()
                };
                let converter = htmd::HtmlToMarkdown::builder().options(options).build();
                let reduced = extract_main(&html).unwrap_or(html);
                let sanitized = strip_noisy_tags(reduced);
                let markdown = converter.convert(&sanitized)?;
                let markdown = postprocess_markdown(&markdown);
                let mut truncated = false;
                let rendered = {
                    let char_count = markdown.chars().count();
                    if char_count > max_chars {
                        truncated = true;
                        let mut s: String = markdown.chars().take(max_chars).collect();
                        s.push_str("\n\n… (truncated)\n");
                        s
                    } else {
                        markdown
                    }
                };
                Ok((rendered, truncated))
            }

            // Helper: detect WAF/challenge pages to avoid dumping challenge content.
            fn detect_block_vendor(_status: reqwest::StatusCode, body: &str) -> Option<&'static str> {
                // Identify common bot-challenge pages regardless of HTTP status.
                // Cloudflare often returns 200 with a challenge that requires JS/cookies.
                let lower = body.to_lowercase();
                if lower.contains("cloudflare")
                    || lower.contains("cf-ray")
                    || lower.contains("_cf_chl_opt")
                    || lower.contains("challenge-platform")
                    || lower.contains("checking if the site connection is secure")
                    || lower.contains("waiting for")
                    || lower.contains("just a moment")
                {
                    return Some("cloudflare");
                }
                None
            }

            fn headers_indicate_block(headers: &reqwest::header::HeaderMap) -> bool {
                let h = headers;
                let has_cf_ray = h.get("cf-ray").is_some();
                let has_cf_mitigated = h.get("cf-mitigated").is_some();
                let has_cf_bm = h.get("set-cookie").and_then(|v| v.to_str().ok()).map(|s| s.contains("__cf_bm=")).unwrap_or(false);
                let has_chlray = h.get("server-timing").and_then(|v| v.to_str().ok()).map(|s| s.to_lowercase().contains("chlray")).unwrap_or(false);
                has_cf_ray || has_cf_mitigated || has_cf_bm || has_chlray
            }

            fn looks_like_challenge_markdown(md: &str) -> bool {
                let l = md.to_lowercase();
                l.contains("just a moment") || l.contains("enable javascript and cookies") || l.contains("waiting for ")
            }

            let timeout = Duration::from_millis(params.timeout_ms.unwrap_or(15000));
            let code_ua = crate::default_client::get_code_user_agent(Some("web_fetch"));

            if matches!(params.mode.as_deref(), Some("browser"))
                && let Some(browser_fetch) = fetch_html_via_browser(&params.url, timeout, true).await {
                    let (markdown, truncated) = match convert_html_to_markdown_trimmed(browser_fetch.html, 120_000) {
                        Ok(t) => t,
                        Err(e) => {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(format!("Markdown conversion failed: {e}")), success: Some(false) },
                            };
                        }
                    };

                    let body = serde_json::json!({
                        "url": params.url,
                        "status": 200,
                        "final_url": browser_fetch.final_url.unwrap_or_else(|| params.url.clone()),
                        "content_type": "text/html",
                        "used_browser_ua": true,
                        "via_browser": true,
                        "headless": browser_fetch.headless,
                        "truncated": truncated,
                        "markdown": markdown,
                    });
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(body.to_string()), success: Some(true) },
                    };
                }
            // Attempt 1: Codex UA + polite headers
            let resp = match do_request(&params.url, &code_ua, timeout, None).await {
                Ok(r) => r,
                Err(e) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(format!("Request failed: {e}")), success: Some(false) },
                    };
                }
            };

            // Capture metadata before consuming the response body.
            let mut status = resp.status();
            let mut final_url = resp.url().to_string();
            let mut headers = resp.headers().clone();
            // Read body
            let mut body_text = match resp.text().await {
                Ok(t) => t,
                Err(e) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(format!("Failed to read response body: {e}")), success: Some(false) },
                    };
                }
            };
            let mut used_browser_ua = false;
            let browser_ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
            if !matches!(params.mode.as_deref(), Some("http")) && (detect_block_vendor(status, &body_text).is_some() || headers_indicate_block(&headers)) {
                // Simple retry with a browser UA and extra headers
                let extra = [
                    (reqwest::header::HeaderName::from_static("upgrade-insecure-requests"), "1"),
                ];
                if let Ok(r2) = do_request(&params.url, browser_ua, timeout, Some(&extra)).await {
                    let status2 = r2.status();
                    let final_url2 = r2.url().to_string();
                    let headers2 = r2.headers().clone();
                    if let Ok(t2) = r2.text().await {
                        used_browser_ua = true;
                        status = status2;
                        final_url = final_url2;
                        headers = headers2;
                        body_text = t2;
                    }
                }
            }

            // Response metadata
            let content_type = headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            // Provide structured diagnostics if blocked by WAF (even if HTTP 200)
            if !matches!(params.mode.as_deref(), Some("http")) && (detect_block_vendor(status, &body_text).is_some() || headers_indicate_block(&headers)) {
                let vendor = "cloudflare";
                let retry_after = headers
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .map(std::string::ToString::to_string);
                let cf_ray = headers
                    .get("cf-ray")
                    .and_then(|v| v.to_str().ok())
                    .map(std::string::ToString::to_string);

                let mut diag = serde_json::json!({
                    "final_url": final_url,
                    "content_type": content_type,
                    "used_browser_ua": used_browser_ua,
                    "blocked_by_waf": true,
                    "vendor": vendor,
                });
                if let Some(ra) = retry_after { diag["retry_after"] = serde_json::json!(ra); }
                if let Some(ray) = cf_ray { diag["cf_ray"] = serde_json::json!(ray); }

                if let Some(browser_fetch) = fetch_html_via_browser(&params.url, timeout, false).await {
                    let (markdown, truncated) = match convert_html_to_markdown_trimmed(browser_fetch.html, 120_000) {
                        Ok(t) => t,
                        Err(e) => {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(format!("Markdown conversion failed: {e}")), success: Some(false) },
                            };
                        }
                    };

                    diag["via_browser"] = serde_json::json!(true);
                    if browser_fetch.headless {
                        diag["headless"] = serde_json::json!(true);
                    }

                    let body = serde_json::json!({
                        "url": params.url,
                        "status": 200,
                        "final_url": browser_fetch.final_url.unwrap_or_else(|| final_url.clone()),
                        "content_type": content_type,
                        "used_browser_ua": true,
                        "via_browser": true,
                        "headless": browser_fetch.headless,
                        "truncated": truncated,
                        "markdown": markdown,
                    });
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(body.to_string()), success: Some(true) },
                    };
                }

                let (md_preview, _trunc) = match convert_html_to_markdown_trimmed(body_text, 2000) {
                    Ok(t) => t,
                    Err(_) => ("".to_string(), false),
                };

                let body = serde_json::json!({
                    "url": params.url,
                    "status": status.as_u16(),
                    "error": "Blocked by site challenge",
                    "diagnostics": diag,
                    "markdown": md_preview,
                });

                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(body.to_string()), success: Some(false) },
                };
            }

            // If not success, provide structured, minimal diagnostics without dumping content.
            if !status.is_success() {
                let waf_vendor = detect_block_vendor(status, &body_text);
                let retry_after = headers
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .map(std::string::ToString::to_string);
                let cf_ray = headers
                    .get("cf-ray")
                    .and_then(|v| v.to_str().ok())
                    .map(std::string::ToString::to_string);

                let mut diag = serde_json::json!({
                    "final_url": final_url,
                    "content_type": content_type,
                    "used_browser_ua": used_browser_ua,
                });
                if let Some(vendor) = waf_vendor { diag["blocked_by_waf"] = serde_json::json!(true); diag["vendor"] = serde_json::json!(vendor); }
                if let Some(ra) = retry_after { diag["retry_after"] = serde_json::json!(ra); }
                if let Some(ray) = cf_ray { diag["cf_ray"] = serde_json::json!(ray); }

                // Provide a tiny, safe preview of visible text only (converted and truncated).
                let (md_preview, _trunc) = match convert_html_to_markdown_trimmed(body_text, 2000) {
                    Ok(t) => t,
                    Err(_) => ("".to_string(), false),
                };

                let body = serde_json::json!({
                    "url": params.url,
                    "status": status.as_u16(),
                    "error": format!("HTTP {} {}", status.as_u16(), status.canonical_reason().unwrap_or("")),
                    "diagnostics": diag,
                    // Keep a short, human-friendly preview; avoid dumping raw HTML or long JS.
                    "markdown": md_preview,
                });

                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(body.to_string()), success: Some(false) },
                };
            }

            // Domain-specific extraction first (e.g., GitHub issues)
            if params.url.contains("github.com/") && params.url.contains("/issues/")
                && let Some(md) = try_extract_github_issue_markdown(&body_text) {
                    let body = serde_json::json!({
                        "url": params.url,
                        "status": status.as_u16(),
                        "final_url": final_url,
                        "content_type": content_type,
                        "used_browser_ua": used_browser_ua,
                        "truncated": false,
                        "markdown": md,
                    });
                    return ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(body.to_string()), success: Some(true) } };
                }

            // Success: convert to markdown (sanitized and size-limited)
            let (markdown, truncated) = match convert_html_to_markdown_trimmed(body_text, 120_000) {
                Ok(t) => t,
                Err(e) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(format!("Markdown conversion failed: {e}")), success: Some(false) },
                    };
                }
            };

            // If the rendered markdown still looks like a challenge page, attempt browser fallback (unless http-only).
            if !matches!(params.mode.as_deref(), Some("http")) && looks_like_challenge_markdown(&markdown) {
                if let Some(browser_fetch) = fetch_html_via_browser(&params.url, timeout, false).await {
                    let (md2, truncated2) = match convert_html_to_markdown_trimmed(browser_fetch.html, 120_000) {
                        Ok(t) => t,
                        Err(e) => {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(format!("Markdown conversion failed: {e}")), success: Some(false) },
                            };
                        }
                    };

                    let body = serde_json::json!({
                        "url": params.url,
                        "status": 200,
                        "final_url": browser_fetch.final_url.unwrap_or_else(|| final_url.clone()),
                        "content_type": content_type,
                        "used_browser_ua": true,
                        "via_browser": true,
                        "headless": browser_fetch.headless,
                        "truncated": truncated2,
                        "markdown": md2,
                    });
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(body.to_string()), success: Some(true) },
                    };
                }

                // If fallback not possible, return structured error rather than a useless challenge page
                let body = serde_json::json!({
                    "url": params.url,
                    "status": 200,
                    "error": "Blocked by site challenge",
                    "diagnostics": { "final_url": final_url, "content_type": content_type, "used_browser_ua": used_browser_ua, "blocked_by_waf": true, "vendor": "cloudflare", "detected_via": "markdown" },
                    "markdown": markdown.chars().take(2000).collect::<String>(),
                });
                return ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(body.to_string()), success: Some(false) } };
            }

            let body = serde_json::json!({
                "url": params.url,
                "status": status.as_u16(),
                "final_url": final_url,
                "content_type": content_type,
                "used_browser_ua": used_browser_ua,
                "truncated": truncated,
                "markdown": markdown,
            });

            ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(body.to_string()), success: Some(true) } }
        },
    ).await
}
