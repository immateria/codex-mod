use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use code_protocol::protocol::McpAuthStatus;

use crate::config::list_mcp_servers;
use crate::config_types::McpServerConfig;
use crate::config_types::McpServerTransportConfig;
use crate::mcp_connection_manager::McpConnectionManager;
use crate::protocol::McpServerFailure;
use crate::protocol::McpServerFailurePhase;

#[derive(Clone, Debug, Default)]
pub struct McpRuntimeSnapshot {
    pub tools_by_server: HashMap<String, Vec<String>>,
    pub disabled_tools_by_server: HashMap<String, Vec<String>>,
    pub auth_statuses: HashMap<String, McpAuthStatus>,
    pub failures: HashMap<String, McpServerFailure>,
}

#[derive(Clone, Debug)]
pub struct MergedMcpServer {
    pub name: String,
    pub enabled: bool,
    pub config: McpServerConfig,
    pub tools: Vec<String>,
    pub disabled_tools: Vec<String>,
    pub auth_status: McpAuthStatus,
    pub failure: Option<McpServerFailure>,
}

pub async fn collect_runtime_snapshot(manager: &McpConnectionManager) -> McpRuntimeSnapshot {
    McpRuntimeSnapshot {
        tools_by_server: manager.list_tools_by_server(),
        disabled_tools_by_server: manager.list_disabled_tools_by_server(),
        auth_statuses: manager.list_auth_statuses().await,
        failures: manager.list_server_failures(),
    }
}

pub fn merge_servers(code_home: &Path, runtime: &McpRuntimeSnapshot) -> Result<Vec<MergedMcpServer>> {
    let (enabled, disabled) = list_mcp_servers(code_home)?;
    let mut rows = Vec::with_capacity(enabled.len() + disabled.len());

    for (name, config) in enabled {
        rows.push(build_server_row(name, config, true, runtime));
    }
    for (name, config) in disabled {
        rows.push(build_server_row(name, config, false, runtime));
    }

    rows.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(rows)
}

pub fn group_tool_definitions_by_server(
    tools: impl IntoIterator<Item = (String, String, mcp_types::Tool)>,
) -> HashMap<String, HashMap<String, mcp_types::Tool>> {
    let mut grouped: HashMap<String, HashMap<String, mcp_types::Tool>> = HashMap::new();
    for (_qualified_name, server_name, tool) in tools {
        let tool_name = tool.name.clone();
        grouped
            .entry(server_name)
            .or_default()
            .insert(tool_name, tool);
    }
    grouped
}

pub fn format_transport_summary(config: &McpServerConfig) -> String {
    match &config.transport {
        McpServerTransportConfig::Stdio { command, args, .. } => {
            if args.is_empty() {
                command.clone()
            } else {
                format!("{command} {}", args.join(" "))
            }
        }
        McpServerTransportConfig::StreamableHttp { url, .. } => format!("HTTP {url}"),
    }
}

pub fn format_failure_summary(failure: &McpServerFailure) -> String {
    let message = failure.message.replace('\n', " ");
    match failure.phase {
        McpServerFailurePhase::Start => format!("Failed to start: {message}"),
        McpServerFailurePhase::ListTools => format!("Failed to list tools: {message}"),
    }
}

fn build_server_row(
    name: String,
    config: McpServerConfig,
    enabled: bool,
    runtime: &McpRuntimeSnapshot,
) -> MergedMcpServer {
    let mut tools = if enabled {
        runtime
            .tools_by_server
            .get(&name)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    tools.sort_unstable();
    tools.dedup();

    let mut disabled_tools = config.disabled_tools.clone();
    if let Some(runtime_disabled) = runtime.disabled_tools_by_server.get(&name) {
        disabled_tools.extend(runtime_disabled.clone());
    }
    disabled_tools.sort();
    disabled_tools.dedup();

    let auth_status = runtime
        .auth_statuses
        .get(&name)
        .copied()
        .unwrap_or(McpAuthStatus::Unsupported);
    let failure = runtime.failures.get(&name).cloned();

    MergedMcpServer {
        name,
        enabled,
        config,
        tools,
        disabled_tools,
        auth_status,
        failure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_summary_for_stdio_and_http() {
        let stdio = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "@x/y".to_string()],
                env: None,
            },
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            disabled_tools: Vec::new(),
        };
        assert_eq!(format_transport_summary(&stdio), "npx -y @x/y");

        let http = McpServerConfig {
            transport: McpServerTransportConfig::StreamableHttp {
                url: "https://example.test/mcp".to_string(),
                bearer_token: None,
                bearer_token_env_var: None,
                http_headers: None,
                env_http_headers: None,
            },
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            disabled_tools: Vec::new(),
        };
        assert_eq!(format_transport_summary(&http), "HTTP https://example.test/mcp");
    }

    #[test]
    fn failure_summary_flattens_lines_and_preserves_phase() {
        let start_failure = McpServerFailure {
            phase: McpServerFailurePhase::Start,
            message: "failed\nwith detail".to_string(),
        };
        assert_eq!(
            format_failure_summary(&start_failure),
            "Failed to start: failed with detail"
        );

        let list_tools_failure = McpServerFailure {
            phase: McpServerFailurePhase::ListTools,
            message: "boom".to_string(),
        };
        assert_eq!(
            format_failure_summary(&list_tools_failure),
            "Failed to list tools: boom"
        );
    }
}
