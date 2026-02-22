use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::context::ToolCall;
use crate::tools::context::ToolPayload;
use crate::tools::handlers;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolSchedulingHints;
use crate::tools::registry::ToolRegistry;
use crate::turn_diff_tracker::TurnDiffTracker;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::LocalShellAction;
use code_protocol::models::ResponseInputItem;
use code_protocol::models::ResponseItem;
use code_protocol::models::ShellToolCallParams;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolDispatchMeta<'a> {
    pub(crate) sub_id: &'a str,
    pub(crate) seq_hint: Option<u64>,
    pub(crate) output_index: Option<u32>,
    pub(crate) attempt_req: u64,
}

impl<'a> ToolDispatchMeta<'a> {
    pub(crate) fn new(
        sub_id: &'a str,
        seq_hint: Option<u64>,
        output_index: Option<u32>,
        attempt_req: u64,
    ) -> Self {
        Self {
            sub_id,
            seq_hint,
            output_index,
            attempt_req,
        }
    }
}

pub(crate) struct ToolRouter {
    registry: ToolRegistry,
    dynamic_handler: Arc<dyn ToolHandler>,
    mcp_handler: Arc<dyn ToolHandler>,
}

impl ToolRouter {
    pub(crate) fn global() -> &'static Self {
        static ROUTER: OnceLock<ToolRouter> = OnceLock::new();
        ROUTER.get_or_init(Self::new)
    }

    pub(crate) fn function_tool_scheduling_hints(
        &self,
        tool_name: &str,
    ) -> Option<ToolSchedulingHints> {
        self.registry
            .handler(tool_name)
            .map(|handler| handler.scheduling_hints())
    }

    pub(crate) fn is_parallel_safe_function_tool(&self, tool_name: &str) -> bool {
        self.function_tool_scheduling_hints(tool_name)
            .is_some_and(ToolSchedulingHints::is_parallel_safe)
    }

    fn new() -> Self {
        let shell: Arc<dyn ToolHandler> = Arc::new(handlers::shell::ShellHandler);
        let plan: Arc<dyn ToolHandler> = Arc::new(handlers::plan::PlanHandler);
        let request_user_input: Arc<dyn ToolHandler> =
            Arc::new(handlers::request_user_input::RequestUserInputHandler);
        let search_tool_bm25: Arc<dyn ToolHandler> =
            Arc::new(handlers::search_tool_bm25::SearchToolBm25Handler);
        let apply_patch: Arc<dyn ToolHandler> = Arc::new(handlers::apply_patch::ApplyPatchToolHandler);
        let exec_command: Arc<dyn ToolHandler> = Arc::new(handlers::exec_command::ExecCommandToolHandler);
        let mcp_resource: Arc<dyn ToolHandler> =
            Arc::new(handlers::mcp_resource::McpResourceToolHandler);
        let read_file: Arc<dyn ToolHandler> = Arc::new(handlers::read_file::ReadFileToolHandler);
        let list_dir: Arc<dyn ToolHandler> = Arc::new(handlers::list_dir::ListDirToolHandler);
        let grep_files: Arc<dyn ToolHandler> = Arc::new(handlers::grep_files::GrepFilesToolHandler);
        let js_repl: Arc<dyn ToolHandler> = Arc::new(handlers::js_repl::JsReplToolHandler);
        let js_repl_reset: Arc<dyn ToolHandler> = Arc::new(handlers::js_repl::JsReplResetToolHandler);
        let agent: Arc<dyn ToolHandler> = Arc::new(handlers::agent::AgentToolHandler);
        let browser: Arc<dyn ToolHandler> = Arc::new(handlers::browser::BrowserToolHandler);
        let web_fetch: Arc<dyn ToolHandler> = Arc::new(handlers::web_fetch::WebFetchToolHandler);
        let image_view: Arc<dyn ToolHandler> = Arc::new(handlers::image_view::ImageViewToolHandler);
        let wait: Arc<dyn ToolHandler> = Arc::new(handlers::wait::WaitToolHandler);
        let kill: Arc<dyn ToolHandler> = Arc::new(handlers::kill::KillToolHandler);
        let gh_run_wait: Arc<dyn ToolHandler> = Arc::new(handlers::gh_run_wait::GhRunWaitToolHandler);
        let bridge: Arc<dyn ToolHandler> = Arc::new(handlers::bridge::BridgeToolHandler);

        let dynamic_handler: Arc<dyn ToolHandler> = Arc::new(handlers::dynamic::DynamicToolHandler);
        let mcp_handler: Arc<dyn ToolHandler> = Arc::new(handlers::mcp::McpToolHandler);

        let mut handlers = HashMap::<&'static str, Arc<dyn ToolHandler>>::new();
        handlers.insert("shell", Arc::clone(&shell));
        handlers.insert("container.exec", Arc::clone(&shell));
        handlers.insert("update_plan", plan);
        handlers.insert("request_user_input", request_user_input);
        handlers.insert("search_tool_bm25", search_tool_bm25);
        handlers.insert("apply_patch", apply_patch);
        handlers.insert(crate::exec_command::EXEC_COMMAND_TOOL_NAME, Arc::clone(&exec_command));
        handlers.insert(crate::exec_command::WRITE_STDIN_TOOL_NAME, exec_command);
        handlers.insert("list_mcp_resources", Arc::clone(&mcp_resource));
        handlers.insert("list_mcp_resource_templates", Arc::clone(&mcp_resource));
        handlers.insert("read_mcp_resource", mcp_resource);
        handlers.insert(crate::openai_tools::READ_FILE_TOOL_NAME, read_file);
        handlers.insert(crate::openai_tools::LIST_DIR_TOOL_NAME, list_dir);
        handlers.insert(crate::openai_tools::GREP_FILES_TOOL_NAME, grep_files);
        handlers.insert(crate::openai_tools::JS_REPL_TOOL_NAME, js_repl);
        handlers.insert(crate::openai_tools::JS_REPL_RESET_TOOL_NAME, js_repl_reset);
        handlers.insert("agent", agent);
        handlers.insert("browser", browser);
        handlers.insert("web_fetch", web_fetch);
        handlers.insert("image_view", image_view);
        handlers.insert("wait", wait);
        handlers.insert("kill", kill);
        handlers.insert("gh_run_wait", gh_run_wait);
        handlers.insert("code_bridge", Arc::clone(&bridge));
        handlers.insert("code_bridge_subscription", bridge);

        Self {
            registry: ToolRegistry::new(handlers),
            dynamic_handler,
            mcp_handler,
        }
    }

    pub(crate) async fn dispatch_response_item(
        &self,
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        meta: ToolDispatchMeta<'_>,
        item: ResponseItem,
    ) -> Option<ResponseInputItem> {
        match item {
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } => {
                let ctx = ToolCallCtx::new(
                    meta.sub_id.to_string(),
                    call_id,
                    meta.seq_hint,
                    meta.output_index,
                );
                Some(
                    self.dispatch_function_call(
                        sess,
                        turn_diff_tracker,
                        ctx,
                        name,
                        arguments,
                        meta.attempt_req,
                    )
                        .await,
                )
            }
            ResponseItem::LocalShellCall {
                id,
                call_id,
                status: _,
                action,
            } => {
                let LocalShellAction::Exec(action) = action;
                tracing::info!("LocalShellCall: {action:?}");
                let params = ShellToolCallParams {
                    command: action.command,
                    workdir: action.working_directory,
                    timeout_ms: action.timeout_ms,
                    sandbox_permissions: None,
                    prefix_rule: None,
                    justification: None,
                };
                let effective_call_id = match (call_id, id) {
                    (Some(call_id), _) => call_id,
                    (None, Some(id)) => id,
                    (None, None) => {
                        tracing::error!("LocalShellCall without call_id or id");
                        return Some(ResponseInputItem::FunctionCallOutput {
                            call_id: String::new(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(
                                    "LocalShellCall without call_id or id".to_string(),
                                ),
                                success: None,
                            },
                        });
                    }
                };

                let ctx = ToolCallCtx::new(
                    meta.sub_id.to_string(),
                    effective_call_id,
                    meta.seq_hint,
                    meta.output_index,
                );
                Some(
                    self.dispatch_local_shell_call(
                        sess,
                        turn_diff_tracker,
                        ctx,
                        params,
                        meta.attempt_req,
                    )
                        .await,
                )
            }
            ResponseItem::CustomToolCall { call_id, name, input, .. } => {
                let ctx = ToolCallCtx::new(
                    meta.sub_id.to_string(),
                    call_id,
                    meta.seq_hint,
                    meta.output_index,
                );
                Some(
                    self.dispatch_custom_tool_call(
                        sess,
                        turn_diff_tracker,
                        ctx,
                        name,
                        input,
                        meta.attempt_req,
                    )
                        .await,
                )
            }
            _ => None,
        }
    }

    async fn dispatch_function_call(
        &self,
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        ctx: ToolCallCtx,
        name: String,
        arguments: String,
        attempt_req: u64,
    ) -> ResponseInputItem {
        let tool_name = name.clone();

        if sess.is_dynamic_tool(tool_name.as_str()) {
            let call = ToolCall {
                tool_name,
                payload: ToolPayload::Function { arguments },
            };
            let inv = crate::tools::context::ToolInvocation {
                ctx,
                tool_name: call.tool_name,
                payload: call.payload,
                attempt_req,
            };
            return self.dynamic_handler.handle(sess, turn_diff_tracker, inv).await;
        }

        if let Some((server, tool)) = sess
            .mcp_connection_manager()
            .parse_tool_name(tool_name.as_str())
        {
            if sess.search_tool_enabled() {
                let selection = sess.mcp_tool_selection_snapshot();
                let selected = selection.as_ref().is_some_and(|tools| {
                    tools
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(tool_name.as_str()))
                });
                if !selected {
                    return crate::tools::registry::unsupported_tool_call_output(
                        &ctx.call_id,
                        false,
                        format!(
                            "MCP tool `{tool_name}` is not selected. Call `search_tool_bm25` first to select MCP tools."
                        ),
                    );
                }
            }

            let call = ToolCall {
                tool_name,
                payload: ToolPayload::Mcp {
                    server,
                    tool,
                    raw_arguments: arguments,
                },
            };
            let inv = crate::tools::context::ToolInvocation {
                ctx,
                tool_name: call.tool_name,
                payload: call.payload,
                attempt_req,
            };
            return self.mcp_handler.handle(sess, turn_diff_tracker, inv).await;
        }

        let call = ToolCall {
            tool_name,
            payload: ToolPayload::Function { arguments },
        };
        self.registry
            .dispatch(sess, turn_diff_tracker, call, ctx, attempt_req)
            .await
    }

    async fn dispatch_local_shell_call(
        &self,
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        ctx: ToolCallCtx,
        params: ShellToolCallParams,
        attempt_req: u64,
    ) -> ResponseInputItem {
        let call = ToolCall {
            tool_name: "shell".to_string(),
            payload: ToolPayload::LocalShell { params },
        };
        self.registry
            .dispatch(sess, turn_diff_tracker, call, ctx, attempt_req)
            .await
    }

    async fn dispatch_custom_tool_call(
        &self,
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        ctx: ToolCallCtx,
        name: String,
        input: String,
        attempt_req: u64,
    ) -> ResponseInputItem {
        let call = ToolCall {
            tool_name: name,
            payload: ToolPayload::Custom { input },
        };
        self.registry
            .dispatch(sess, turn_diff_tracker, call, ctx, attempt_req)
            .await
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FunctionCallRouteKind {
    Registry,
    Dynamic,
    Mcp,
    Unsupported,
}

#[cfg(test)]
fn route_kind_for_function_call(
    registry: &ToolRegistry,
    tool_name: &str,
    is_dynamic_tool: bool,
    is_mcp_tool: bool,
) -> FunctionCallRouteKind {
    if is_dynamic_tool {
        return FunctionCallRouteKind::Dynamic;
    }
    if is_mcp_tool {
        return FunctionCallRouteKind::Mcp;
    }
    if registry.handler(tool_name).is_some() {
        return FunctionCallRouteKind::Registry;
    }
    FunctionCallRouteKind::Unsupported
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_family::derive_default_model_family;
    use crate::openai_tools::get_openai_tools;
    use crate::openai_tools::OpenAiTool;
    use crate::protocol::AskForApproval;
    use crate::protocol::SandboxPolicy;
    use crate::tools::registry::unsupported_tool_call_output;
    use crate::tools::spec::ToolsConfig;
    use crate::tools::spec::ToolsConfigParams;

    #[test]
    fn function_call_routes_to_registry_handler() {
        let router = ToolRouter::global();
        let kind = route_kind_for_function_call(&router.registry, "shell", false, false);
        assert_eq!(kind, FunctionCallRouteKind::Registry);
    }

    #[test]
    fn mcp_tool_name_routes_to_mcp_handler() {
        let router = ToolRouter::global();
        let kind = route_kind_for_function_call(&router.registry, "brave_web_search", false, true);
        assert_eq!(kind, FunctionCallRouteKind::Mcp);
    }

    #[test]
    fn unknown_tool_returns_failure_payload() {
        let out = unsupported_tool_call_output(
            "call_1",
            false,
            "unsupported call: nope".to_string(),
        );
        match out {
            ResponseInputItem::FunctionCallOutput { call_id, output } => {
                assert_eq!(call_id, "call_1");
                assert_eq!(output.success, Some(false));
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn registry_has_handlers_for_default_openai_function_tools() {
        let model_family = derive_default_model_family("gpt-5.3-codex");
        let default_config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: true,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: true,
        });
        let mut search_tool_config = default_config.clone();
        search_tool_config.search_tool = true;
        let apply_patch_config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: true,
            include_apply_patch_tool: true,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: true,
        });
        let streamable_shell_config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: true,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: true,
            include_view_image_tool: true,
        });
        let mut js_repl_config = default_config.clone();
        js_repl_config.js_repl = true;

        let router = ToolRouter::global();
        let cases: Vec<(&'static str, ToolsConfig)> = vec![
            ("default", default_config),
            ("search_tool_enabled", search_tool_config),
            ("apply_patch_enabled", apply_patch_config),
            ("streamable_shell_enabled", streamable_shell_config),
            ("js_repl_enabled", js_repl_config),
        ];

        for (label, config) in cases {
            let tools = get_openai_tools(&config, None, true, false, &[]);
            for tool in tools {
                match tool {
                    OpenAiTool::Function(spec) => {
                        assert!(
                            router.registry.handler(spec.name.as_str()).is_some(),
                            "[{label}] missing handler for function tool `{}`",
                            spec.name
                        );
                    }
                    OpenAiTool::Freeform(spec) => {
                        assert!(
                            router.registry.handler(spec.name.as_str()).is_some(),
                            "[{label}] missing handler for custom tool `{}`",
                            spec.name
                        );
                    }
                    OpenAiTool::WebSearch(_) | OpenAiTool::LocalShell { .. } => {}
                }
            }
        }
    }
}
