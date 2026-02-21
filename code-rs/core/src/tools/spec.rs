use crate::model_family::ModelFamily;
use crate::protocol::AskForApproval;
use crate::protocol::SandboxPolicy;
use crate::tool_apply_patch::ApplyPatchToolType;

#[derive(Debug, Clone)]
pub enum ConfigShellToolType {
    DefaultShell,
    ShellWithRequest { sandbox_policy: SandboxPolicy },
    LocalShell,
    StreamableShell,
}

#[derive(Debug, Clone)]
pub struct ToolsConfig {
    pub shell_type: ConfigShellToolType,
    pub plan_tool: bool,
    #[allow(dead_code)]
    pub apply_patch_tool_type: Option<ApplyPatchToolType>,
    pub web_search_request: bool,
    pub web_search_external: bool,
    pub search_tool: bool,
    pub js_repl: bool,
    #[allow(dead_code)]
    pub include_view_image_tool: bool,
    pub web_search_allowed_domains: Option<Vec<String>>,
    pub agent_model_allowed_values: Vec<String>,
}

#[allow(dead_code)]
pub struct ToolsConfigParams<'a> {
    pub model_family: &'a ModelFamily,
    pub approval_policy: AskForApproval,
    pub sandbox_policy: SandboxPolicy,
    pub include_plan_tool: bool,
    pub include_apply_patch_tool: bool,
    pub include_web_search_request: bool,
    pub use_streamable_shell_tool: bool,
    pub include_view_image_tool: bool,
}

impl ToolsConfig {
    pub fn new(params: ToolsConfigParams<'_>) -> Self {
        let ToolsConfigParams {
            model_family,
            approval_policy,
            sandbox_policy,
            include_plan_tool,
            include_apply_patch_tool,
            include_web_search_request,
            use_streamable_shell_tool,
            include_view_image_tool,
        } = params;
        let mut shell_type = if use_streamable_shell_tool {
            ConfigShellToolType::StreamableShell
        } else if model_family.uses_local_shell_tool {
            ConfigShellToolType::LocalShell
        } else {
            ConfigShellToolType::DefaultShell
        };
        if matches!(approval_policy, AskForApproval::OnRequest) && !use_streamable_shell_tool {
            shell_type = ConfigShellToolType::ShellWithRequest { sandbox_policy };
        }

        let apply_patch_tool_type = if include_apply_patch_tool {
            // On Windows, grammar-based apply_patch invocations rely on heredocs
            // the shell cannot parse. Force the JSON/function variant instead.
            #[cfg(target_os = "windows")]
            {
                model_family
                    .apply_patch_tool_type
                    .clone()
                    .map(|_| ApplyPatchToolType::Function)
            }
            #[cfg(not(target_os = "windows"))]
            {
                model_family.apply_patch_tool_type.clone()
            }
        } else {
            None
        };

        Self {
            shell_type,
            plan_tool: include_plan_tool,
            apply_patch_tool_type,
            web_search_request: include_web_search_request,
            web_search_external: true,
            search_tool: false,
            js_repl: false,
            include_view_image_tool,
            web_search_allowed_domains: None,
            agent_model_allowed_values: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn new_from_params(p: &ToolsConfigParams) -> Self {
        Self::new(ToolsConfigParams {
            model_family: p.model_family,
            approval_policy: p.approval_policy,
            sandbox_policy: p.sandbox_policy.clone(),
            include_plan_tool: p.include_plan_tool,
            include_apply_patch_tool: p.include_apply_patch_tool,
            include_web_search_request: p.include_web_search_request,
            use_streamable_shell_tool: p.use_streamable_shell_tool,
            include_view_image_tool: p.include_view_image_tool,
        })
    }

    pub fn set_agent_models(&mut self, models: Vec<String>) {
        self.agent_model_allowed_values = models;
    }

    pub fn agent_models(&self) -> &[String] {
        &self.agent_model_allowed_values
    }
}
