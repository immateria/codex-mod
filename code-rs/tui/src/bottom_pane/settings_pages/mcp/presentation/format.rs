use std::time::Duration;

use super::super::McpSettingsView;

impl McpSettingsView {
    pub(super) fn format_tool_annotations(
        annotations: &mcp_types::ToolAnnotations,
    ) -> Option<String> {
        let mut hints = Vec::new();
        if annotations.read_only_hint == Some(true) {
            hints.push("read-only");
        }
        if annotations.idempotent_hint == Some(true) {
            hints.push("idempotent");
        }
        if annotations.destructive_hint == Some(true) {
            hints.push("destructive");
        }
        if annotations.open_world_hint == Some(true) {
            hints.push("open-world");
        }
        if hints.is_empty() {
            None
        } else {
            Some(hints.join(", "))
        }
    }

    pub(super) fn schema_property_names(
        properties: Option<&serde_json::Value>,
    ) -> Vec<String> {
        let Some(properties) = properties else {
            return Vec::new();
        };
        let Some(object) = properties.as_object() else {
            return Vec::new();
        };
        let mut names: Vec<String> = object.keys().cloned().collect();
        names.sort();
        names
    }

    pub(super) fn format_duration(duration: Duration) -> String {
        let secs = duration.as_secs_f64();
        if secs.fract() == 0.0 {
            let whole = duration.as_secs();
            if whole == 1 {
                "1 second".to_string()
            } else {
                format!("{whole} seconds")
            }
        } else {
            format!("{secs:.2} seconds")
        }
    }

    pub(super) fn join_names_limited(names: &[String], max_items: usize) -> String {
        if names.is_empty() {
            return "(none)".to_string();
        }
        if names.len() <= max_items {
            return names.join(", ");
        }
        let shown = names[..max_items].join(", ");
        let remaining = names.len().saturating_sub(max_items);
        format!("{shown} (+{remaining} more)")
    }

    pub(super) fn format_resource_line(resource: &code_protocol::mcp::Resource) -> String {
        let name = resource.name.as_str();
        let uri = resource.uri.as_str();
        let mut line = if resource.uri.trim().is_empty() {
            name.to_string()
        } else {
            format!("{name} ({uri})")
        };
        if let Some(mime_type) = resource.mime_type.as_deref()
            && !mime_type.trim().is_empty()
        {
            line.push_str(&format!(" · {mime_type}"));
        }
        if let Some(size) = resource.size
            && size > 0
        {
            line.push_str(&format!(" · {size} bytes"));
        }
        line
    }

    pub(super) fn format_resource_template_line(
        template: &code_protocol::mcp::ResourceTemplate,
    ) -> String {
        let name = template.name.as_str();
        let uri_template = template.uri_template.as_str();
        let mut line = if template.uri_template.trim().is_empty() {
            name.to_string()
        } else {
            format!("{name} ({uri_template})")
        };
        if let Some(mime_type) = template.mime_type.as_deref()
            && !mime_type.trim().is_empty()
        {
            line.push_str(&format!(" · {mime_type}"));
        }
        line
    }
}

