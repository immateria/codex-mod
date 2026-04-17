use serde_json::json;

use super::types::OpenAiTool;

/// Returns JSON values that are compatible with Function Calling in the
/// Responses API:
/// <https://platform.openai.com/docs/guides/function-calling?api-mode=responses>
pub fn create_tools_json_for_responses_api(
    tools: &[OpenAiTool],
) -> crate::error::Result<Vec<serde_json::Value>> {
    let mut tools_json = Vec::with_capacity(tools.len());

    for tool in tools {
        let json = serde_json::to_value(tool)?;
        tools_json.push(json);
    }

    Ok(tools_json)
}

/// Returns JSON values that are compatible with Function Calling in the
/// Chat Completions API:
/// <https://platform.openai.com/docs/guides/function-calling?api-mode=chat>
pub(crate) fn create_tools_json_for_chat_completions_api(
    tools: &[OpenAiTool],
) -> crate::error::Result<Vec<serde_json::Value>> {
    // We start with the JSON for the Responses API and then rewrite it to match
    // the chat completions tool call format.
    let responses_api_tools_json = create_tools_json_for_responses_api(tools)?;
    let tools_json = responses_api_tools_json
        .into_iter()
        .filter_map(|mut tool| {
            let tool_type = tool.get("type").and_then(serde_json::Value::as_str);
            match tool_type {
                Some("function") => {
                    if let Some(map) = tool.as_object_mut() {
                        // Remove "type" field as it is not needed in chat completions.
                        map.remove("type");
                        Some(json!({
                            "type": "function",
                            "function": map,
                        }))
                    } else {
                        None
                    }
                }
                // Convert freeform/custom tools to function tools with a
                // single `input` string parameter so non-OpenAI Chat
                // Completions providers can still invoke them.
                Some("custom") => {
                    let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("").to_owned();
                    let description = tool.get("description").and_then(|v| v.as_str()).unwrap_or("").to_owned();
                    Some(json!({
                        "type": "function",
                        "function": {
                            "name": name,
                            "description": description,
                            "strict": false,
                            "parameters": {
                                "type": "object",
                                "properties": {
                                    "input": {
                                        "type": "string",
                                        "description": "The raw input text for this tool."
                                    }
                                },
                                "required": ["input"],
                                "additionalProperties": false
                            }
                        }
                    }))
                }
                _ => None,
            }
        })
        .collect::<Vec<serde_json::Value>>();
    Ok(tools_json)
}
