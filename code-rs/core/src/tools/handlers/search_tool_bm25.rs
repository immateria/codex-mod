use crate::codex::Session;
use crate::mcp::ids::McpServerId;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::execute_custom_tool;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use bm25::Document;
use bm25::Language;
use bm25::SearchEngineBuilder;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde::Deserialize;
use serde_json::json;

pub(crate) struct SearchToolBm25Handler;

const DEFAULT_LIMIT: usize = 8;

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

#[derive(Debug, Deserialize)]
struct SearchToolBm25Args {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Clone)]
struct ToolEntry {
    qualified_name: String,
    server_name: String,
    tool_name: String,
    title: Option<String>,
    description: Option<String>,
    input_keys: Vec<String>,
    access_decision: Option<&'static str>,
    search_text: String,
}

impl ToolEntry {
    fn new(
        qualified_name: String,
        server_name: String,
        tool: mcp_types::Tool,
        access_decision: Option<&'static str>,
    ) -> Self {
        let input_keys = tool
            .input_schema
            .properties
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .map(|map| map.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let search_text = build_search_text(
            qualified_name.as_str(),
            server_name.as_str(),
            tool.name.as_str(),
            tool.title.as_deref(),
            tool.description.as_deref(),
            &input_keys,
        );
        Self {
            qualified_name,
            server_name,
            tool_name: tool.name,
            title: tool.title,
            description: tool.description,
            input_keys,
            access_decision,
            search_text,
        }
    }
}

fn build_search_text(
    qualified_name: &str,
    server_name: &str,
    tool_name: &str,
    title: Option<&str>,
    description: Option<&str>,
    input_keys: &[String],
) -> String {
    let mut parts = vec![
        qualified_name.to_string(),
        tool_name.to_string(),
        server_name.to_string(),
    ];

    if let Some(title) = title
        && !title.trim().is_empty()
    {
        parts.push(title.to_string());
    }

    if let Some(description) = description
        && !description.trim().is_empty()
    {
        parts.push(description.to_string());
    }

    if !input_keys.is_empty() {
        parts.extend(input_keys.iter().cloned());
    }

    parts.join(" ")
}

#[async_trait]
impl ToolHandler for SearchToolBm25Handler {
    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Function { arguments } = &inv.payload else {
            return ResponseInputItem::FunctionCallOutput {
                call_id: inv.ctx.call_id,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        "search_tool_bm25 expects function-call arguments".to_string(),
                    ),
                    success: Some(false),
                },
            };
        };

        let params_for_event = serde_json::from_str::<serde_json::Value>(arguments).ok();
        let arguments = arguments.clone();
        let ctx = inv.ctx.clone();
        let call_id = ctx.call_id.clone();
        let sub_id = ctx.sub_id.clone();

        execute_custom_tool(
            sess,
            &ctx,
            crate::openai_tools::SEARCH_TOOL_BM25_TOOL_NAME.to_string(),
            params_for_event,
            move || async move {
                let args: SearchToolBm25Args = match serde_json::from_str(&arguments) {
                    Ok(args) => args,
                    Err(err) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "invalid search_tool_bm25 arguments: {err}"
                                )),
                                success: Some(false),
                            },
                        };
                    }
                };

                let query = args.query.trim();
                if query.is_empty() {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text("query must not be empty".to_string()),
                            success: Some(false),
                        },
                    };
                }

                if args.limit == 0 {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "limit must be greater than zero".to_string(),
                            ),
                            success: Some(false),
                        },
                    };
                }
                let limit = args.limit;

                let mcp_access = sess.mcp_access_snapshot();
                let session_deny: std::collections::HashSet<String> = mcp_access
                    .session_deny_servers
                    .iter()
                    .map(|name| name.to_ascii_lowercase())
                    .collect();

                let mut entries: Vec<ToolEntry> = Vec::new();
                for (qualified_name, server_name, tool) in
                    sess.mcp_connection_manager().list_all_tools_with_server_names()
                {
                    if session_deny.contains(&server_name.to_ascii_lowercase()) {
                        continue;
                    }

                    let access_decision = McpServerId::parse(server_name.as_str())
                        .map(|server_id| {
                            crate::mcp::policy::server_access_for_turn(
                                &mcp_access,
                                sub_id.as_str(),
                                &server_id,
                            )
                        })
                        .map(|decision| match decision {
                            crate::mcp::policy::McpServerAccessDecision::Allowed => "allowed",
                            crate::mcp::policy::McpServerAccessDecision::DeniedSession => {
                                "blocked_session"
                            }
                            crate::mcp::policy::McpServerAccessDecision::DeniedStyleExclude => {
                                "blocked_style_exclude"
                            }
                            crate::mcp::policy::McpServerAccessDecision::DeniedStyleIncludeOnly => {
                                "blocked_style_include_only"
                            }
                        });

                    entries.push(ToolEntry::new(
                        qualified_name,
                        server_name,
                        tool,
                        access_decision,
                    ));
                }

                entries.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));

                if entries.is_empty() {
                    let active_selected_tools = sess
                        .mcp_tool_selection_snapshot()
                        .unwrap_or_default();
                    let content = json!({
                        "query": query,
                        "total_tools": 0,
                        "active_selected_tools": active_selected_tools,
                        "tools": [],
                    })
                    .to_string();
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(content),
                            success: Some(true),
                        },
                    };
                }

                let documents: Vec<Document<usize>> = entries
                    .iter()
                    .enumerate()
                    .map(|(idx, entry)| Document::new(idx, entry.search_text.clone()))
                    .collect();
                let engine =
                    SearchEngineBuilder::<usize>::with_documents(Language::English, documents)
                        .build();
                let results = engine.search(query, limit);

                let mut selected_tools = Vec::new();
                let mut tool_payloads = Vec::new();
                for result in results {
                    let Some(entry) = entries.get(result.document.id) else {
                        continue;
                    };
                    selected_tools.push(entry.qualified_name.clone());
                    tool_payloads.push(json!({
                        "name": entry.qualified_name,
                        "server": entry.server_name,
                        "tool_name": entry.tool_name,
                        "title": entry.title,
                        "description": entry.description,
                        "input_keys": entry.input_keys,
                        "access_decision": entry.access_decision,
                        "score": result.score,
                    }));
                }

                let active_selected_tools = sess.merge_mcp_tool_selection(selected_tools);
                let content = json!({
                    "query": query,
                    "total_tools": entries.len(),
                    "active_selected_tools": active_selected_tools,
                    "tools": tool_payloads,
                })
                .to_string();

                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(content),
                        success: Some(true),
                    },
                }
            },
        )
        .await
    }
}
