use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::protocol::EventMsg;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;

pub(crate) struct RequestPermissionsHandler;

#[async_trait]
impl ToolHandler for RequestPermissionsHandler {
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
                        "request_permissions expects function-call arguments".to_string(),
                    ),
                    success: Some(false),
                },
            };
        };

        handle_request_permissions(sess, &inv.ctx, arguments).await
    }
}

async fn handle_request_permissions(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    use code_protocol::request_permissions::PermissionGrantScope;
    use code_protocol::request_permissions::RequestPermissionProfile;
    use code_protocol::request_permissions::RequestPermissionsArgs;
    use code_protocol::request_permissions::RequestPermissionsEvent;
    use code_protocol::request_permissions::RequestPermissionsResponse;

    // Parse and resolve relative filesystem permission paths against the session cwd.
    let mut raw: serde_json::Value = match serde_json::from_str(&arguments) {
        Ok(value) => value,
        Err(err) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!(
                        "invalid request_permissions arguments: {err}"
                    )),
                    success: Some(false),
                },
            };
        }
    };

    for key in ["read", "write"] {
        let Some(list) = raw
            .get_mut("permissions")
            .and_then(|p| p.get_mut("file_system"))
            .and_then(|fs| fs.get_mut(key))
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };

        for item in list.iter_mut() {
            let Some(path) = item.as_str() else {
                continue;
            };
            let resolved = sess.get_cwd().join(path);
            *item = serde_json::Value::String(resolved.to_string_lossy().to_string());
        }
    }

    let args: RequestPermissionsArgs = match serde_json::from_value(raw) {
        Ok(args) => args,
        Err(err) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!(
                        "invalid request_permissions arguments: {err}"
                    )),
                    success: Some(false),
                },
            };
        }
    };

    if args.permissions.is_empty() {
        return ResponseInputItem::FunctionCallOutput {
            call_id: ctx.call_id.clone(),
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(
                    "request_permissions requires at least one permission".to_string(),
                ),
                success: Some(false),
            },
        };
    }

    match sess.get_approval_policy() {
        crate::protocol::AskForApproval::Never => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        serde_json::to_string(&RequestPermissionsResponse {
                            permissions: RequestPermissionProfile::default(),
                            scope: PermissionGrantScope::Turn,
                        })
                        .unwrap_or_else(|_| "{}".to_string()),
                    ),
                    success: Some(true),
                },
            };
        }
        crate::protocol::AskForApproval::Reject(config) if config.rejects_request_permissions() => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        serde_json::to_string(&RequestPermissionsResponse {
                            permissions: RequestPermissionProfile::default(),
                            scope: PermissionGrantScope::Turn,
                        })
                        .unwrap_or_else(|_| "{}".to_string()),
                    ),
                    success: Some(true),
                },
            };
        }
        crate::protocol::AskForApproval::UnlessTrusted
        | crate::protocol::AskForApproval::OnFailure
        | crate::protocol::AskForApproval::OnRequest
        | crate::protocol::AskForApproval::Reject(_) => {}
    }

    let rx_response = match sess.register_pending_request_permissions(ctx.sub_id.clone(), ctx.call_id.clone()) {
        Ok(rx) => rx,
        Err(err) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(err),
                    success: Some(false),
                },
            };
        }
    };

    sess.send_ordered_from_ctx(
        ctx,
        EventMsg::RequestPermissions(RequestPermissionsEvent {
            call_id: ctx.call_id.clone(),
            turn_id: ctx.sub_id.clone(),
            reason: args.reason,
            permissions: args.permissions,
        }),
    )
    .await;

    let response = match rx_response.await {
        Ok(response) => response,
        Err(_) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        "request_permissions was cancelled before receiving a response".to_string(),
                    ),
                    success: Some(false),
                },
            };
        }
    };

    let content = match serde_json::to_string(&response) {
        Ok(content) => content,
        Err(err) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!(
                        "failed to serialize request_permissions response: {err}"
                    )),
                    success: Some(false),
                },
            };
        }
    };

    ResponseInputItem::FunctionCallOutput {
        call_id: ctx.call_id.clone(),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(content),
            success: Some(true),
        },
    }
}
