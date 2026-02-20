use crate::codex::Session;
use crate::mcp_tool_call::handle_mcp_tool_call;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;

pub(crate) struct McpToolHandler;

#[async_trait]
impl ToolHandler for McpToolHandler {
    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Mcp {
            server,
            tool,
            raw_arguments,
        } = inv.payload
        else {
            return ResponseInputItem::FunctionCallOutput {
                call_id: inv.ctx.call_id,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        "MCP handler received unsupported payload".to_string(),
                    ),
                    success: Some(false),
                },
            };
        };

        let Some(server_id) = crate::mcp::ids::McpServerId::parse(server.as_str()) else {
            return ResponseInputItem::FunctionCallOutput {
                call_id: inv.ctx.call_id,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!(
                        "unsupported MCP server name `{server}`"
                    )),
                    success: Some(false),
                },
            };
        };

        let mcp_access = sess.mcp_access_snapshot();
        let mut access = crate::mcp::policy::server_access_for_turn(
            &mcp_access,
            &inv.ctx.sub_id,
            &server_id,
        );
        let mut allow = access.is_allowed();

        if !allow {
            if access.is_session_denied() {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: inv.ctx.call_id,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "MCP server `{server}` is blocked for this session. Open Settings -> MCP (or update your shell style MCP filters) to allow it."
                        )),
                        success: Some(false),
                    },
                };
            }

            use code_protocol::request_user_input::RequestUserInputEvent;

            let style_label = mcp_access.style_label.as_deref().unwrap_or("none");
            let mut question_text = format!(
                "The model attempted to call MCP tool `{server}/{tool}`, but MCP server `{server}` is blocked by your current MCP filters."
            );
            if mcp_access.style.is_some() {
                question_text.push_str(&format!(" Active shell style: `{style_label}`."));
            }
            question_text.push_str("\n\nHow do you want to proceed?");

            let prompt_call_id = format!("mcp_access:{}:{}:{tool}", inv.ctx.sub_id, server_id.as_str());
            let rx_response = match sess.register_pending_user_input(inv.ctx.sub_id.clone()) {
                Ok(rx) => rx,
                Err(err) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: inv.ctx.call_id,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(err),
                            success: Some(false),
                        },
                    };
                }
            };

            sess.send_ordered_from_ctx(
                &inv.ctx,
                crate::protocol::EventMsg::RequestUserInput(RequestUserInputEvent {
                    call_id: prompt_call_id,
                    turn_id: inv.ctx.sub_id.clone(),
                    questions: vec![crate::codex::mcp_access::mcp_access_question(
                        question_text,
                        mcp_access.style.is_some(),
                    )],
                }),
            )
            .await;

            let response = match rx_response.await {
                Ok(response) => response,
                Err(_) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: inv.ctx.call_id,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "MCP access prompt was cancelled before receiving a response."
                                    .to_string(),
                            ),
                            success: Some(false),
                        },
                    };
                }
            };

            let selection = response
                .answers
                .get("mcp_access")
                .and_then(|answer| answer.answers.first())
                .map(|value| value.trim().to_string())
                .unwrap_or_default();

            crate::codex::mcp_access::apply_mcp_access_selection(
                sess,
                &inv.ctx.sub_id,
                &server_id,
                server.as_str(),
                &mcp_access,
                selection.as_str(),
            )
            .await;

            let mcp_access_after = sess.mcp_access_snapshot();
            access = crate::mcp::policy::server_access_for_turn(
                &mcp_access_after,
                &inv.ctx.sub_id,
                &server_id,
            );
            allow = access.is_allowed();
        }

        if !allow {
            return ResponseInputItem::FunctionCallOutput {
                call_id: inv.ctx.call_id,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!(
                        "MCP server `{server}` is blocked by your current MCP filters."
                    )),
                    success: Some(false),
                },
            };
        }

        handle_mcp_tool_call(sess, &inv.ctx, server, tool, raw_arguments).await
    }
}

