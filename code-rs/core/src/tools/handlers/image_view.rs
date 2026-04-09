use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::protocol::EventMsg;
use crate::protocol::ViewImageToolCallEvent;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::execute_custom_tool;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::tools::handlers::{tool_error, tool_output};
use async_trait::async_trait;
use base64::Engine;
use code_protocol::models::ContentItem;
use code_protocol::models::ResponseInputItem;

pub(crate) struct ImageViewToolHandler;

#[async_trait]
impl ToolHandler for ImageViewToolHandler {
    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Function { arguments } = inv.payload else {
            return tool_error(inv.ctx.call_id, "image_view expects function-call arguments");
        };

        handle_image_view(sess, &inv.ctx, arguments).await
    }
}

async fn handle_image_view(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    use serde::Deserialize;
    use serde_json::Value;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    struct Params {
        path: String,
        #[serde(default)]
        alt_text: Option<String>,
    }

    let mut params_for_event = serde_json::from_str::<Value>(&arguments).ok();
    let parsed: Params = match serde_json::from_str(&arguments) {
        Ok(p) => p,
        Err(e) => {
            return tool_error(
                ctx.call_id.clone(),
                format!("Invalid image_view arguments: {e}"),
            );
        }
    };

    execute_custom_tool(
        sess,
        ctx,
        "image_view".to_string(),
        params_for_event.take(),
        move || async move {
            let call_id = ctx.call_id.clone();
            let path_str = parsed.path.trim();
            if path_str.is_empty() {
                return tool_error(call_id, "image_view requires a non-empty path");
            }

            let mut resolved = PathBuf::from(path_str);
            if resolved.is_relative() {
                resolved = sess.get_cwd().join(&resolved);
            }
            if let Ok(canon) = resolved.canonicalize() {
                resolved = canon;
            }
            let metadata = match std::fs::metadata(&resolved) {
                Ok(meta) => meta,
                Err(err) => {
                    return tool_error(
                        call_id,
                        format!("image_view could not read {}: {err}", resolved.display()),
                    );
                }
            };
            if !metadata.is_file() {
                return tool_error(
                    call_id,
                    format!("image_view requires a file path, got {}", resolved.display()),
                );
            }

            let bytes = match std::fs::read(&resolved) {
                Ok(bytes) => bytes,
                Err(err) => {
                    return tool_error(
                        call_id,
                        format!("image_view could not read {}: {err}", resolved.display()),
                    );
                }
            };
            let mime = mime_guess::from_path(&resolved)
                .first()
                .map(|m| m.essence_str().to_owned())
                .unwrap_or_else(|| crate::util::MIME_OCTET_STREAM.to_string());
            if !mime.starts_with("image/") {
                return tool_error(
                    call_id,
                    format!("image_view only supports image files (got {mime})"),
                );
            }
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            let filename = resolved
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("image");
            let label = parsed
                .alt_text
                .as_ref()
                .map(|text| text.trim())
                .filter(|text| !text.is_empty())
                .unwrap_or(filename);
            let marker = format!("[image: {label}]");
            let image_message = ResponseInputItem::Message {
                role: "user".to_string(),
                content: vec![
                    ContentItem::InputText { text: marker },
                    ContentItem::InputImage {
                        image_url: format!("data:{mime};base64,{encoded}"),
                    },
                ],
            };
            sess.add_pending_input(image_message);

            sess.send_ordered_from_ctx(
                ctx,
                EventMsg::ViewImageToolCall(ViewImageToolCallEvent {
                    call_id: ctx.call_id.clone(),
                    path: resolved.clone(),
                }),
            )
            .await;

            tool_output(call_id, format!("attached image: {}", resolved.display()))
        },
    )
    .await
}
