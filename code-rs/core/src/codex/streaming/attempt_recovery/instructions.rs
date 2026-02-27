use super::*;

pub(in crate::codex::streaming) const HTML_SANITIZER_GUARDRAILS_MESSAGE: &str =
    "TB2 HTML/XSS guardrails:\n- Do NOT use DOTALL/full-document regex (e.g. `<script.*?>.*?</script>`); catastrophic backtracking risk.\n- Prefer linear-time scanning with quote/state tracking; if using regex, only on bounded substrings (single tags).\n- Perf smoke test: write malformed `/tmp/stress.html` and run `timeout 5s python3 /app/filter.py /tmp/stress.html` (or equivalent). If it times out, rewrite for linear-time behavior.";

pub(in crate::codex::streaming) const SEARCH_TOOL_DEVELOPER_INSTRUCTIONS: &str =
    include_str!("../../../../templates/search_tool/developer_instructions.md");

pub(in crate::codex::streaming) fn should_inject_html_sanitizer_guardrails(
    input: &[ResponseItem],
) -> bool {
    let mut user_messages_seen = 0u32;
    let mut text = String::new();
    for item in input.iter().rev() {
        if user_messages_seen >= 6 || text.len() >= 1_200 {
            break;
        }
        let ResponseItem::Message { role, content, .. } = item else {
            continue;
        };
        if role != "user" {
            continue;
        }
        user_messages_seen = user_messages_seen.saturating_add(1);
        for entry in content {
            let ContentItem::InputText { text: piece } = entry else {
                continue;
            };
            if piece.trim().is_empty() {
                continue;
            }
            text.push_str(piece);
            text.push('\n');
            if text.len() >= 1_200 {
                break;
            }
        }
    }

    if text.is_empty() {
        return false;
    }

    let lower = text.to_ascii_lowercase();
    let has_xss = lower.contains("xss");
    let has_sanitize = lower.contains("sanitize") || lower.contains("sanitiz");
    let has_filter_js_from_html =
        lower.contains("filter-js-from-html") || lower.contains("break-filter-js-from-html");
    let has_html = lower.contains("html");
    let has_script_tag =
        lower.contains("<script") || lower.contains("script tag") || lower.contains("script-tag");
    let has_filtering = lower.contains("filter") || lower.contains("strip") || lower.contains("remove");

    has_xss
        || has_sanitize
        || has_filter_js_from_html
        || (has_html && has_script_tag && has_filtering)
}

pub(in crate::codex::streaming) fn should_inject_search_tool_developer_instructions(
    tools: &[OpenAiTool],
) -> bool {
    tools.iter().any(|tool| {
        matches!(tool, OpenAiTool::Function(ResponsesApiTool { name, .. }) if name == SEARCH_TOOL_BM25_TOOL_NAME)
    })
}

#[cfg(test)]
mod search_tool_instructions_tests {
    use super::*;

    #[test]
    fn detects_search_tool_presence() {
        let tools = vec![OpenAiTool::Function(ResponsesApiTool {
            name: SEARCH_TOOL_BM25_TOOL_NAME.to_string(),
            description: "search".to_string(),
            strict: false,
            parameters: crate::openai_tools::JsonSchema::Object {
                properties: Default::default(),
                required: None,
                additional_properties: None,
            },
        })];
        assert!(should_inject_search_tool_developer_instructions(&tools));
    }

    #[test]
    fn ignores_non_search_tools() {
        let tools = vec![OpenAiTool::Function(ResponsesApiTool {
            name: "not_search_tool".to_string(),
            description: "other".to_string(),
            strict: false,
            parameters: crate::openai_tools::JsonSchema::Object {
                properties: Default::default(),
                required: None,
                additional_properties: None,
            },
        })];
        assert!(!should_inject_search_tool_developer_instructions(&tools));
    }
}
