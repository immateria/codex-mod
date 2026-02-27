use std::collections::HashMap;

use code_protocol::dynamic_tools::DynamicToolSpec;

use crate::agent_tool::create_agent_tool;
use crate::plan_tool::PLAN_TOOL;
use crate::tool_apply_patch::{
    create_apply_patch_freeform_tool, create_apply_patch_json_tool, ApplyPatchToolType,
};

use super::builtin_tools;
use super::browser_tool;
use super::conversions;
use super::misc_tools;
use super::types::{OpenAiTool, WebSearchFilters, WebSearchTool};
use super::{ConfigShellToolType, ToolsConfig};

pub fn get_openai_tools(
    config: &ToolsConfig,
    mcp_tools: Option<HashMap<String, mcp_types::Tool>>,
    browser_enabled: bool,
    _agents_active: bool,
    dynamic_tools: &[DynamicToolSpec],
) -> Vec<OpenAiTool> {
    let mut tools: Vec<OpenAiTool> = Vec::new();

    match &config.shell_type {
        ConfigShellToolType::DefaultShell => {
            tools.push(builtin_tools::create_shell_tool());
        }
        ConfigShellToolType::ShellWithRequest { sandbox_policy } => {
            tools.push(builtin_tools::create_shell_tool_for_sandbox(sandbox_policy));
        }
        ConfigShellToolType::LocalShell => {
            tools.push(OpenAiTool::LocalShell {});
        }
        ConfigShellToolType::StreamableShell => {
            tools.push(OpenAiTool::Function(
                crate::exec_command::create_exec_command_tool_for_responses_api(),
            ));
            tools.push(OpenAiTool::Function(
                crate::exec_command::create_write_stdin_tool_for_responses_api(),
            ));
        }
    }

    if config.include_view_image_tool {
        tools.push(builtin_tools::create_image_view_tool());
    }

    if let Some(apply_patch_tool_type) = &config.apply_patch_tool_type {
        let apply_patch_tool = match apply_patch_tool_type {
            ApplyPatchToolType::Function => create_apply_patch_json_tool(),
            ApplyPatchToolType::Freeform => create_apply_patch_freeform_tool(),
        };
        tools.push(apply_patch_tool);
    }

    if config.plan_tool {
        tools.push(PLAN_TOOL.clone());
    }

    tools.push(builtin_tools::create_request_user_input_tool());
    tools.push(builtin_tools::create_list_mcp_resources_tool());
    tools.push(builtin_tools::create_list_mcp_resource_templates_tool());
    tools.push(builtin_tools::create_read_mcp_resource_tool());
    tools.push(builtin_tools::create_read_file_tool());
    tools.push(builtin_tools::create_list_dir_tool());
    tools.push(builtin_tools::create_grep_files_tool());
    if config.search_tool {
        tools.push(builtin_tools::create_search_tool_bm25_tool());
    }
    if config.js_repl {
        tools.push(builtin_tools::create_js_repl_tool());
        tools.push(builtin_tools::create_js_repl_reset_tool());
    }

    tools.push(browser_tool::create_browser_tool(browser_enabled));

    // Add agent management tool for launching and monitoring asynchronous agents
    tools.push(create_agent_tool(config.agent_models()));

    // Add general wait tool for background completions
    tools.push(misc_tools::create_wait_tool());
    tools.push(misc_tools::create_kill_tool());
    tools.push(misc_tools::create_gh_run_wait_tool());
    tools.push(misc_tools::create_bridge_tool());

    if config.web_search_request {
        let tool = match &config.web_search_allowed_domains {
            Some(domains) if !domains.is_empty() => OpenAiTool::WebSearch(WebSearchTool {
                external_web_access: Some(config.web_search_external),
                filters: Some(WebSearchFilters {
                    allowed_domains: Some(domains.clone()),
                }),
            }),
            _ => OpenAiTool::WebSearch(WebSearchTool {
                external_web_access: Some(config.web_search_external),
                ..WebSearchTool::default()
            }),
        };
        tools.push(tool);
    }

    if let Some(mcp_tools) = mcp_tools {
        // Ensure deterministic ordering to maximize prompt cache hits.
        // HashMap iteration order is non-deterministic, so sort by fully-qualified tool name.
        let mut entries: Vec<(String, mcp_types::Tool)> = mcp_tools.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        for (name, tool) in entries.into_iter() {
            match conversions::mcp_tool_to_openai_tool(name.clone(), tool.clone()) {
                Ok(converted_tool) => tools.push(OpenAiTool::Function(converted_tool)),
                Err(e) => {
                    tracing::error!("Failed to convert {name:?} MCP tool to OpenAI tool: {e:?}");
                }
            }
        }
    }

    for tool in dynamic_tools {
        match conversions::dynamic_tool_to_openai_tool(tool) {
            Ok(converted_tool) => tools.push(OpenAiTool::Function(converted_tool)),
            Err(e) => {
                tracing::error!(
                    "Failed to convert dynamic tool {:?} to OpenAI tool: {e:?}",
                    tool.name
                );
            }
        }
    }

    tools
}
