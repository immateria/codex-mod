use crate::codex::Session;
use crate::mcp::ids::McpServerId;
use crate::protocol::EventMsg;
use crate::protocol::McpInvocation;
use crate::protocol::McpToolCallBeginEvent;
use crate::protocol::McpToolCallEndEvent;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use mcp_types::CallToolResult;
use mcp_types::ContentBlock;
use mcp_types::TextContent;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::time::Instant;

pub(crate) struct McpResourceToolHandler;

#[derive(Debug, Deserialize, Default)]
struct ListResourcesArgs {
    /// Lists all resources from all servers if not specified.
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ListResourceTemplatesArgs {
    /// Lists all resource templates from all servers if not specified.
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReadResourceArgs {
    server: String,
    uri: String,
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_required_string(name: &str, value: String) -> std::result::Result<String, String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        Err(format!("{name} must not be empty"))
    } else {
        Ok(trimmed)
    }
}

#[derive(Debug, Serialize)]
struct ResourceWithServer {
    server: String,
    #[serde(flatten)]
    resource: mcp_types::Resource,
}

impl ResourceWithServer {
    fn new(server: String, resource: mcp_types::Resource) -> Self {
        Self { server, resource }
    }
}

#[derive(Debug, Serialize)]
struct ResourceTemplateWithServer {
    server: String,
    #[serde(flatten)]
    template: mcp_types::ResourceTemplate,
}

impl ResourceTemplateWithServer {
    fn new(server: String, template: mcp_types::ResourceTemplate) -> Self {
        Self { server, template }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListResourcesPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<String>,
    resources: Vec<ResourceWithServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

impl ListResourcesPayload {
    fn from_single_server(server: String, result: mcp_types::ListResourcesResult) -> Self {
        let resources = result
            .resources
            .into_iter()
            .map(|resource| ResourceWithServer::new(server.clone(), resource))
            .collect();
        Self {
            server: Some(server),
            resources,
            next_cursor: result.next_cursor,
        }
    }

    fn from_all_servers(resources_by_server: HashMap<String, Vec<mcp_types::Resource>>) -> Self {
        let mut entries: Vec<(String, Vec<mcp_types::Resource>)> =
            resources_by_server.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut resources = Vec::new();
        for (server, server_resources) in entries {
            for resource in server_resources {
                resources.push(ResourceWithServer::new(server.clone(), resource));
            }
        }

        Self {
            server: None,
            resources,
            next_cursor: None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListResourceTemplatesPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<String>,
    resource_templates: Vec<ResourceTemplateWithServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

impl ListResourceTemplatesPayload {
    fn from_single_server(server: String, result: mcp_types::ListResourceTemplatesResult) -> Self {
        let resource_templates = result
            .resource_templates
            .into_iter()
            .map(|template| ResourceTemplateWithServer::new(server.clone(), template))
            .collect();
        Self {
            server: Some(server),
            resource_templates,
            next_cursor: result.next_cursor,
        }
    }

    fn from_all_servers(
        templates_by_server: HashMap<String, Vec<mcp_types::ResourceTemplate>>,
    ) -> Self {
        let mut entries: Vec<(String, Vec<mcp_types::ResourceTemplate>)> =
            templates_by_server.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut resource_templates = Vec::new();
        for (server, server_templates) in entries {
            for template in server_templates {
                resource_templates.push(ResourceTemplateWithServer::new(server.clone(), template));
            }
        }

        Self {
            server: None,
            resource_templates,
            next_cursor: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct ReadResourcePayload {
    server: String,
    uri: String,
    #[serde(flatten)]
    result: mcp_types::ReadResourceResult,
}

fn call_tool_result_from_content(content: &str, is_error: bool) -> CallToolResult {
    let content = ContentBlock::TextContent(TextContent {
        annotations: None,
        text: content.to_string(),
        r#type: "text".to_string(),
    });
    CallToolResult {
        content: vec![content],
        structured_content: None,
        is_error: Some(is_error),
    }
}

async fn emit_tool_call_begin(
    sess: &Session,
    inv: &crate::codex::ToolCallCtx,
    invocation: McpInvocation,
) {
    sess.send_ordered_from_ctx(
        inv,
        EventMsg::McpToolCallBegin(McpToolCallBeginEvent {
            call_id: inv.call_id.clone(),
            invocation,
        }),
    )
    .await;
}

async fn emit_tool_call_end(
    sess: &Session,
    inv: &crate::codex::ToolCallCtx,
    invocation: McpInvocation,
    duration: std::time::Duration,
    result: std::result::Result<CallToolResult, String>,
) {
    sess.send_ordered_from_ctx(
        inv,
        EventMsg::McpToolCallEnd(McpToolCallEndEvent {
            call_id: inv.call_id.clone(),
            invocation,
            duration,
            result,
        }),
    )
    .await;
}

#[async_trait]
impl ToolHandler for McpResourceToolHandler {
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
                        "MCP resource tools expect function-call arguments".to_string(),
                    ),
                    success: Some(false),
                },
            };
        };

        let arguments_str = arguments.clone();
        let args_json: Option<JsonValue> = if arguments_str.trim().is_empty() {
            None
        } else {
            match serde_json::from_str(&arguments_str) {
                Ok(value) => Some(value),
                Err(err) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: inv.ctx.call_id,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!(
                                "invalid {tool} arguments: {err}",
                                tool = inv.tool_name
                            )),
                            success: Some(false),
                        },
                    };
                }
            }
        };

        let server_for_event = args_json
            .as_ref()
            .and_then(|value| value.get("server"))
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("mcp")
            .to_string();

        let invocation = McpInvocation {
            server: server_for_event,
            tool: inv.tool_name.clone(),
            arguments: args_json.clone(),
        };

        emit_tool_call_begin(sess, &inv.ctx, invocation.clone()).await;
        let start = Instant::now();

        let result = match inv.tool_name.as_str() {
            "list_mcp_resources" => {
                self.handle_list_resources(sess, &inv.ctx, &arguments_str)
                    .await
            }
            "list_mcp_resource_templates" => {
                self.handle_list_resource_templates(sess, &inv.ctx, &arguments_str)
                    .await
            }
            "read_mcp_resource" => self.handle_read_resource(sess, &inv.ctx, &arguments_str).await,
            other => Err(format!("unsupported MCP resource tool: {other}")),
        };

        let duration = start.elapsed();
        match result {
            Ok(output) => {
                emit_tool_call_end(
                    sess,
                    &inv.ctx,
                    invocation,
                    duration,
                    Ok(call_tool_result_from_content(output.as_str(), false)),
                )
                .await;
                ResponseInputItem::FunctionCallOutput {
                    call_id: inv.ctx.call_id,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(output),
                        success: Some(true),
                    },
                }
            }
            Err(message) => {
                emit_tool_call_end(sess, &inv.ctx, invocation, duration, Err(message.clone()))
                    .await;
                ResponseInputItem::FunctionCallOutput {
                    call_id: inv.ctx.call_id,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(message),
                        success: Some(false),
                    },
                }
            }
        }
    }
}

impl McpResourceToolHandler {
    async fn handle_list_resources(
        &self,
        sess: &Session,
        ctx: &crate::codex::ToolCallCtx,
        arguments: &str,
    ) -> std::result::Result<String, String> {
        let args: ListResourcesArgs = if arguments.trim().is_empty() {
            ListResourcesArgs::default()
        } else {
            serde_json::from_str(arguments)
                .map_err(|err| format!("invalid list_mcp_resources arguments: {err}"))?
        };

        let server = normalize_optional_string(args.server);
        let cursor = normalize_optional_string(args.cursor);

        if let Some(server_name) = server {
            let server_id = McpServerId::parse(server_name.as_str())
                .ok_or_else(|| format!("unsupported MCP server name `{server_name}`"))?;
            crate::codex::mcp_access::ensure_mcp_server_access_for_turn(
                sess,
                ctx,
                &server_id,
                server_name.as_str(),
                "MCP resource tool `list_mcp_resources`",
            )
            .await?;

            let params = cursor
                .as_ref()
                .map(|value| mcp_types::ListResourcesRequestParams {
                    cursor: Some(value.clone()),
                });
            let result = sess
                .list_resources(server_name.as_str(), params, None)
                .await
                .map_err(|err| format!("resources/list failed: {err:#}"))?;
            let payload = ListResourcesPayload::from_single_server(server_name, result);
            serde_json::to_string(&payload).map_err(|err| format!("failed to encode output: {err}"))
        } else {
            if cursor.is_some() {
                return Err("cursor can only be used when a server is specified".to_string());
            }

            let mcp_access = sess.mcp_access_snapshot();
            let server_names = sess.mcp_connection_manager().list_server_names();

            let mut resources_by_server = HashMap::new();
            for server_name in server_names {
                let Some(server_id) = McpServerId::parse(server_name.as_str()) else {
                    continue;
                };
                let access = crate::mcp::policy::server_access_for_turn(
                    &mcp_access,
                    ctx.sub_id.as_str(),
                    &server_id,
                );
                if !access.is_allowed() {
                    continue;
                }

                match list_all_resources_for_server(sess, server_name.as_str()).await {
                    Ok(resources) => {
                        resources_by_server.insert(server_name.clone(), resources);
                    }
                    Err(err) => {
                        tracing::warn!("Failed to list resources for MCP server '{server_name}': {err:#}");
                    }
                };
            }

            let payload = ListResourcesPayload::from_all_servers(resources_by_server);
            serde_json::to_string(&payload).map_err(|err| format!("failed to encode output: {err}"))
        }
    }

    async fn handle_list_resource_templates(
        &self,
        sess: &Session,
        ctx: &crate::codex::ToolCallCtx,
        arguments: &str,
    ) -> std::result::Result<String, String> {
        let args: ListResourceTemplatesArgs = if arguments.trim().is_empty() {
            ListResourceTemplatesArgs::default()
        } else {
            serde_json::from_str(arguments).map_err(|err| {
                format!("invalid list_mcp_resource_templates arguments: {err}")
            })?
        };

        let server = normalize_optional_string(args.server);
        let cursor = normalize_optional_string(args.cursor);

        if let Some(server_name) = server {
            let server_id = McpServerId::parse(server_name.as_str())
                .ok_or_else(|| format!("unsupported MCP server name `{server_name}`"))?;
            crate::codex::mcp_access::ensure_mcp_server_access_for_turn(
                sess,
                ctx,
                &server_id,
                server_name.as_str(),
                "MCP resource tool `list_mcp_resource_templates`",
            )
            .await?;

            let params = cursor
                .as_ref()
                .map(|value| mcp_types::ListResourceTemplatesRequestParams {
                    cursor: Some(value.clone()),
                });
            let result = sess
                .list_resource_templates(server_name.as_str(), params, None)
                .await
                .map_err(|err| format!("resources/templates/list failed: {err:#}"))?;
            let payload = ListResourceTemplatesPayload::from_single_server(server_name, result);
            serde_json::to_string(&payload).map_err(|err| format!("failed to encode output: {err}"))
        } else {
            if cursor.is_some() {
                return Err("cursor can only be used when a server is specified".to_string());
            }

            let mcp_access = sess.mcp_access_snapshot();
            let server_names = sess.mcp_connection_manager().list_server_names();

            let mut templates_by_server = HashMap::new();
            for server_name in server_names {
                let Some(server_id) = McpServerId::parse(server_name.as_str()) else {
                    continue;
                };
                let access = crate::mcp::policy::server_access_for_turn(
                    &mcp_access,
                    ctx.sub_id.as_str(),
                    &server_id,
                );
                if !access.is_allowed() {
                    continue;
                }

                match list_all_resource_templates_for_server(sess, server_name.as_str()).await {
                    Ok(templates) => {
                        templates_by_server.insert(server_name.clone(), templates);
                    }
                    Err(err) => {
                        tracing::warn!("Failed to list resource templates for MCP server '{server_name}': {err:#}");
                    }
                };
            }

            let payload = ListResourceTemplatesPayload::from_all_servers(templates_by_server);
            serde_json::to_string(&payload).map_err(|err| format!("failed to encode output: {err}"))
        }
    }

    async fn handle_read_resource(
        &self,
        sess: &Session,
        ctx: &crate::codex::ToolCallCtx,
        arguments: &str,
    ) -> std::result::Result<String, String> {
        let args: ReadResourceArgs = serde_json::from_str(arguments)
            .map_err(|err| format!("invalid read_mcp_resource arguments: {err}"))?;
        let server = normalize_required_string("server", args.server)?;
        let uri = normalize_required_string("uri", args.uri)?;

        let server_id =
            McpServerId::parse(server.as_str()).ok_or_else(|| format!("unsupported MCP server name `{server}`"))?;
        crate::codex::mcp_access::ensure_mcp_server_access_for_turn(
            sess,
            ctx,
            &server_id,
            server.as_str(),
            "MCP resource tool `read_mcp_resource`",
        )
        .await?;

        let result = sess
            .read_resource(
                server.as_str(),
                mcp_types::ReadResourceRequestParams { uri: uri.clone() },
                None,
            )
            .await
            .map_err(|err| format!("resources/read failed: {err:#}"))?;

        let payload = ReadResourcePayload { server, uri, result };
        serde_json::to_string(&payload).map_err(|err| format!("failed to encode output: {err}"))
    }
}

async fn list_all_resources_for_server(
    sess: &Session,
    server: &str,
) -> anyhow::Result<Vec<mcp_types::Resource>> {
    let mut resources = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen_cursors = std::collections::HashSet::<String>::new();

    loop {
        let params = cursor
            .as_ref()
            .map(|next| mcp_types::ListResourcesRequestParams {
                cursor: Some(next.clone()),
            });
        let result = sess.list_resources(server, params, None).await?;
        resources.extend(result.resources);

        match result.next_cursor {
            Some(next) => {
                if !seen_cursors.insert(next.clone()) {
                    anyhow::bail!("resources/list returned repeated cursor");
                }
                cursor = Some(next);
            }
            None => return Ok(resources),
        }
    }
}

async fn list_all_resource_templates_for_server(
    sess: &Session,
    server: &str,
) -> anyhow::Result<Vec<mcp_types::ResourceTemplate>> {
    let mut templates = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen_cursors = std::collections::HashSet::<String>::new();

    loop {
        let params = cursor
            .as_ref()
            .map(|next| mcp_types::ListResourceTemplatesRequestParams {
                cursor: Some(next.clone()),
            });
        let result = sess.list_resource_templates(server, params, None).await?;
        templates.extend(result.resource_templates);

        match result.next_cursor {
            Some(next) => {
                if !seen_cursors.insert(next.clone()) {
                    anyhow::bail!("resources/templates/list returned repeated cursor");
                }
                cursor = Some(next);
            }
            None => return Ok(templates),
        }
    }
}
