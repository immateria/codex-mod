use code_protocol::models::DeveloperInstructions;
use code_protocol::models::ResponseItem;

use crate::plugins::PluginCapabilitySummary;
use crate::plugins::render::render_explicit_plugin_instructions;

pub(crate) fn build_plugin_injections(mentioned_plugins: &[PluginCapabilitySummary]) -> Vec<ResponseItem> {
    if mentioned_plugins.is_empty() {
        return Vec::new();
    }

    mentioned_plugins
        .iter()
        .filter_map(|plugin| {
            let available_apps = plugin
                .apps
                .iter()
                .map(|crate::plugins::AppConnectorId(id)| id.clone())
                .collect::<Vec<_>>();

            render_explicit_plugin_instructions(plugin, &plugin.mcp_server_names, &available_apps)
                .map(DeveloperInstructions::new)
                .map(ResponseItem::from)
        })
        .collect()
}

