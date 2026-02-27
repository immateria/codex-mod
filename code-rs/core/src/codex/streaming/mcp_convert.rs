use super::*;

use std::collections::HashMap;

pub(super) fn convert_mcp_resources_by_server(
    resources_by_server: HashMap<String, Vec<mcp_types::Resource>>,
) -> HashMap<String, Vec<code_protocol::mcp::Resource>> {
    resources_by_server
        .into_iter()
        .map(|(server, resources)| {
            let converted = resources
                .into_iter()
                .filter_map(|resource| match serde_json::to_value(resource) {
                    Ok(value) => match code_protocol::mcp::Resource::from_mcp_value(value) {
                        Ok(resource) => Some(resource),
                        Err(err) => {
                            warn!("failed to convert MCP resource for server {server}: {err}");
                            None
                        }
                    },
                    Err(err) => {
                        warn!("failed to serialize MCP resource for server {server}: {err}");
                        None
                    }
                })
                .collect();
            (server, converted)
        })
        .collect()
}

pub(super) fn convert_mcp_resource_templates_by_server(
    templates_by_server: HashMap<String, Vec<mcp_types::ResourceTemplate>>,
) -> HashMap<String, Vec<code_protocol::mcp::ResourceTemplate>> {
    templates_by_server
        .into_iter()
        .map(|(server, templates)| {
            let converted = templates
                .into_iter()
                .filter_map(|template| match serde_json::to_value(template) {
                    Ok(value) => match code_protocol::mcp::ResourceTemplate::from_mcp_value(value) {
                        Ok(template) => Some(template),
                        Err(err) => {
                            warn!(
                                "failed to convert MCP resource template for server {server}: {err}"
                            );
                            None
                        }
                    },
                    Err(err) => {
                        warn!("failed to serialize MCP resource template for server {server}: {err}");
                        None
                    }
                })
                .collect();
            (server, converted)
        })
        .collect()
}

