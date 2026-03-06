use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use code_core::CodexConversation;
use code_core::protocol::FileChange;
use code_core::protocol::Op;
use code_core::protocol::ReviewDecision;
use mcp_types::ElicitRequest;
use mcp_types::ElicitRequestParamsRequestedSchema;
use mcp_types::JSONRPCErrorError;
use mcp_types::ModelContextProtocolRequest;
use mcp_types::RequestId;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use tracing::error;

use crate::code_tool_runner::INVALID_PARAMS_ERROR_CODE;
use crate::outgoing_message::OutgoingMessageSender;

#[derive(Debug, Serialize)]
pub struct PatchApprovalElicitRequestParams {
    pub message: String,
    #[serde(rename = "requestedSchema")]
    pub requested_schema: ElicitRequestParamsRequestedSchema,
    pub code_elicitation: String,
    pub code_mcp_tool_call_id: String,
    pub code_event_id: String,
    pub code_call_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_grant_root: Option<PathBuf>,
    pub code_changes: HashMap<PathBuf, FileChange>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PatchApprovalResponse {
    pub decision: ReviewDecision,
}

pub(crate) struct PatchApprovalRequestContext {
    pub call_id: String,
    pub reason: Option<String>,
    pub grant_root: Option<PathBuf>,
    pub changes: HashMap<PathBuf, FileChange>,
    pub outgoing: Arc<OutgoingMessageSender>,
    pub codex: Arc<CodexConversation>,
    pub request_id: RequestId,
    pub tool_call_id: String,
    pub event_id: String,
}

pub(crate) async fn handle_patch_approval_request(
    request: PatchApprovalRequestContext,
) {
    let PatchApprovalRequestContext {
        call_id,
        reason,
        grant_root,
        changes,
        outgoing,
        codex,
        request_id,
        tool_call_id,
        event_id,
    } = request;
    let mut message_lines = Vec::new();
    if let Some(r) = &reason {
        message_lines.push(r.clone());
    }
    message_lines.push("Allow Codex to apply proposed code changes?".to_string());

    let params = PatchApprovalElicitRequestParams {
        message: message_lines.join("\n"),
        requested_schema: ElicitRequestParamsRequestedSchema {
            r#type: "object".to_string(),
            properties: json!({}),
            required: None,
        },
        code_elicitation: "patch-approval".to_string(),
        code_mcp_tool_call_id: tool_call_id.clone(),
        code_event_id: event_id.clone(),
        code_call_id: call_id,
        code_reason: reason,
        code_grant_root: grant_root,
        code_changes: changes,
    };
    let params_json = match serde_json::to_value(&params) {
        Ok(value) => value,
        Err(err) => {
            let message = format!("Failed to serialize PatchApprovalElicitRequestParams: {err}");
            error!("{message}");

            outgoing
                .send_error(
                    request_id.clone(),
                    JSONRPCErrorError {
                        code: INVALID_PARAMS_ERROR_CODE,
                        message,
                        data: None,
                    },
                )
                .await;

            return;
        }
    };

    let on_response = outgoing
        .send_request(ElicitRequest::METHOD, Some(params_json))
        .await;

    // Listen for the response on a separate task so we don't block the main agent loop.
    // Important: correlate approval by call_id (unique per request) to match core's
    // pending_approvals key, not by event id. Using the event id here can lead to
    // "no pending approval found" warnings and a stuck approval UI.
    {
        let codex = codex.clone();
        let call_id_for_response = tool_call_id.clone();
        tokio::spawn(async move {
            on_patch_approval_response(call_id_for_response, on_response, codex).await;
        });
    }
}

pub(crate) async fn on_patch_approval_response(
    approval_id: String,
    receiver: tokio::sync::oneshot::Receiver<mcp_types::Result>,
    codex: Arc<CodexConversation>,
) {
    let response = receiver.await;
    let value = match response {
        Ok(value) => value,
        Err(err) => {
            error!("request failed: {err:?}");
            if let Err(submit_err) = codex
                .submit(Op::PatchApproval {
                    id: approval_id.clone(),
                    decision: ReviewDecision::Denied,
                })
                .await
            {
                error!("failed to submit denied PatchApproval after request failure: {submit_err}");
            }
            return;
        }
    };

    let response = serde_json::from_value::<PatchApprovalResponse>(value).unwrap_or_else(|err| {
        error!("failed to deserialize PatchApprovalResponse: {err}");
        PatchApprovalResponse {
            decision: ReviewDecision::Denied,
        }
    });

    if let Err(err) = codex
        .submit(Op::PatchApproval {
            id: approval_id,
            decision: response.decision,
        })
        .await
    {
        error!("failed to submit PatchApproval: {err}");
    }
}
