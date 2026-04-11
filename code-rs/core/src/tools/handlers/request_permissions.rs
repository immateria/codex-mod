use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::protocol::EventMsg;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::tools::handlers::{tool_error, tool_output};
use async_trait::async_trait;
use code_protocol::models::ResponseInputItem;
use code_utils_absolute_path::AbsolutePathBuf;
use code_utils_absolute_path::AbsolutePathBufGuard;
use std::path::Path;

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
            return tool_error(inv.ctx.call_id, "request_permissions expects function-call arguments");
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

    let mut args: RequestPermissionsArgs = match parse_request_permissions_args(sess.get_cwd(), &arguments) {
        Ok(args) => args,
        Err(err) => {
            return tool_error(
                ctx.call_id.clone(),
                format!("invalid request_permissions arguments: {err}"),
            );
        }
    };

    normalize_request_permission_profile(&mut args.permissions);

    if args.permissions.is_empty() {
        return tool_error(
            ctx.call_id.clone(),
            "request_permissions requires at least one permission",
        );
    }

    match sess.get_approval_policy() {
        crate::protocol::AskForApproval::Never => {
            return tool_output(
                ctx.call_id.clone(),
                serde_json::to_string(&RequestPermissionsResponse {
                    permissions: RequestPermissionProfile::default(),
                    scope: PermissionGrantScope::Turn,
                })
                .unwrap_or_else(|_| "{}".to_owned()),
            );
        }
        crate::protocol::AskForApproval::Reject(config) if config.rejects_request_permissions() => {
            return tool_output(
                ctx.call_id.clone(),
                serde_json::to_string(&RequestPermissionsResponse {
                    permissions: RequestPermissionProfile::default(),
                    scope: PermissionGrantScope::Turn,
                })
                .unwrap_or_else(|_| "{}".to_owned()),
            );
        }
        crate::protocol::AskForApproval::UnlessTrusted
        | crate::protocol::AskForApproval::OnFailure
        | crate::protocol::AskForApproval::OnRequest
        | crate::protocol::AskForApproval::Reject(_) => {}
    }

    let rx_response = match sess.register_pending_request_permissions(ctx.sub_id.clone(), ctx.call_id.clone()) {
        Ok(rx) => rx,
        Err(err) => {
            return tool_error(ctx.call_id.clone(), err);
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

    let Ok(response) = rx_response.await else {
        return tool_error(
            ctx.call_id.clone(),
            "request_permissions was cancelled before receiving a response",
        );
    };

    let content = match serde_json::to_string(&response) {
        Ok(content) => content,
        Err(err) => {
            return tool_error(
                ctx.call_id.clone(),
                format!("failed to serialize request_permissions response: {err}"),
            );
        }
    };

    tool_output(ctx.call_id.clone(), content)
}

fn parse_request_permissions_args(
    cwd: &Path,
    arguments: &str,
) -> Result<code_protocol::request_permissions::RequestPermissionsArgs, serde_json::Error> {
    let _guard = AbsolutePathBufGuard::new(cwd);
    serde_json::from_str(arguments)
}

fn normalize_request_permission_profile(profile: &mut code_protocol::request_permissions::RequestPermissionProfile) {
    fn normalize_paths(paths: &mut Vec<AbsolutePathBuf>) {
        paths.sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));
        paths.dedup();
    }

    let Some(fs) = profile.file_system.as_mut() else {
        return;
    };
    if let Some(read) = fs.read.as_mut() {
        normalize_paths(read);
    }
    if let Some(write) = fs.write.as_mut() {
        normalize_paths(write);
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_request_permission_profile, parse_request_permissions_args};
    use code_protocol::request_permissions::RequestPermissionProfile;
    use code_protocol::request_permissions::RequestPermissionsArgs;
    use tempfile::tempdir;

    #[test]
    fn parses_relative_filesystem_paths_against_session_cwd() {
        let dir = tempdir().expect("temp dir");
        let cwd = dir.path();
        let json = r#"{
  "permissions": {
    "file_system": {
      "read": ["foo.txt"],
      "write": ["subdir/bar.txt"]
    }
  }
}"#;

        let args: RequestPermissionsArgs =
            parse_request_permissions_args(cwd, json).expect("parse args");
        let fs = args.permissions.file_system.expect("filesystem permissions");
        assert_eq!(
            fs.read
                .expect("read paths")
                .iter()
                .map(|p| p.as_path().strip_prefix(cwd).unwrap().to_path_buf())
                .collect::<Vec<_>>(),
            vec![std::path::PathBuf::from("foo.txt")]
        );
        assert_eq!(
            fs.write
                .expect("write paths")
                .iter()
                .map(|p| p.as_path().strip_prefix(cwd).unwrap().to_path_buf())
                .collect::<Vec<_>>(),
            vec![std::path::PathBuf::from("subdir/bar.txt")]
        );
    }

    #[test]
    fn normalize_dedups_filesystem_paths() {
        let dir = tempdir().expect("temp dir");
        let cwd = dir.path();
        let json = r#"{
  "permissions": {
    "file_system": {
      "read": ["foo.txt", "foo.txt", "bar.txt"],
      "write": ["bar.txt", "bar.txt"]
    }
  }
}"#;

        let mut args: RequestPermissionsArgs =
            parse_request_permissions_args(cwd, json).expect("parse args");
        normalize_request_permission_profile(&mut args.permissions);

        let fs = args.permissions.file_system.expect("filesystem permissions");
        assert_eq!(fs.read.expect("read paths").len(), 2);
        assert_eq!(fs.write.expect("write paths").len(), 1);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn parses_home_directory_shorthand_paths() {
        let Some(home) = dirs::home_dir() else {
            return;
        };
        let dir = tempdir().expect("temp dir");
        let cwd = dir.path();
        let json = r#"{
  "permissions": {
    "file_system": {
      "read": ["~/code"],
      "write": ["~"]
    }
  }
}"#;

        let args: RequestPermissionsArgs =
            parse_request_permissions_args(cwd, json).expect("parse args");
        let fs = args.permissions.file_system.expect("filesystem permissions");
        assert_eq!(fs.read.expect("read paths")[0].as_path(), home.join("code").as_path());
        assert_eq!(fs.write.expect("write paths")[0].as_path(), home.as_path());
    }

    #[test]
    fn normalize_is_noop_when_filesystem_permissions_absent() {
        let mut profile = RequestPermissionProfile::default();
        normalize_request_permission_profile(&mut profile);
        assert!(profile.is_empty());
    }
}
