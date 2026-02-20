use code_protocol::mcp::Resource as ProtocolMcpResource;
use code_protocol::mcp::ResourceTemplate as ProtocolMcpResourceTemplate;
use code_protocol::mcp::Tool as ProtocolMcpTool;

pub(super) fn convert_mcp_tool(
    tool: &mcp_types::Tool,
) -> Result<ProtocolMcpTool, serde_json::Error> {
    ProtocolMcpTool::from_mcp_value(serde_json::to_value(tool)?)
}

fn convert_mcp_resource(
    resource: &mcp_types::Resource,
) -> Result<ProtocolMcpResource, serde_json::Error> {
    ProtocolMcpResource::from_mcp_value(serde_json::to_value(resource)?)
}

fn convert_mcp_resource_template(
    resource_template: &mcp_types::ResourceTemplate,
) -> Result<ProtocolMcpResourceTemplate, serde_json::Error> {
    ProtocolMcpResourceTemplate::from_mcp_value(serde_json::to_value(resource_template)?)
}

pub(super) fn convert_mcp_resources(resources: &[mcp_types::Resource]) -> Vec<ProtocolMcpResource> {
    let mut converted = resources
        .iter()
        .filter_map(|resource| match convert_mcp_resource(resource) {
            Ok(resource) => Some(resource),
            Err(err) => {
                tracing::warn!("failed to convert MCP resource in app-server status response: {err}");
                None
            }
        })
        .collect::<Vec<_>>();
    converted.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.uri.cmp(&b.uri)));
    converted
}

pub(super) fn convert_mcp_resource_templates(
    templates: &[mcp_types::ResourceTemplate],
) -> Vec<ProtocolMcpResourceTemplate> {
    let mut converted = templates
        .iter()
        .filter_map(|template| match convert_mcp_resource_template(template) {
            Ok(template) => Some(template),
            Err(err) => {
                tracing::warn!(
                    "failed to convert MCP resource template in app-server status response: {err}"
                );
                None
            }
        })
        .collect::<Vec<_>>();
    converted.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.uri_template.cmp(&b.uri_template))
    });
    converted
}
