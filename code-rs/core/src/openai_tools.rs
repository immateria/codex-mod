pub use crate::tools::spec::ConfigShellToolType;
pub use crate::tools::spec::ToolsConfig;
pub use crate::tools::spec::ToolsConfigParams;

mod builtin_tools;
mod browser_tool;
mod conversions;
mod json_schema;
mod misc_tools;
mod registry;
mod tools_json;
mod types;

#[cfg(test)]
mod tests;

const SEARCH_TOOL_DESCRIPTION_TEMPLATE: &str =
    include_str!("../templates/search_tool/tool_description.md");
pub(crate) const SEARCH_TOOL_BM25_TOOL_NAME: &str = "search_tool_bm25";
pub(crate) const READ_FILE_TOOL_NAME: &str = "read_file";
pub(crate) const LIST_DIR_TOOL_NAME: &str = "list_dir";
pub(crate) const GREP_FILES_TOOL_NAME: &str = "grep_files";
pub(crate) const REPL_TOOL_NAME: &str = "repl";
pub(crate) const REPL_RESET_TOOL_NAME: &str = "repl_reset";

/// Returns the per-runtime tool name, e.g. `"repl_python"`.
pub(crate) fn repl_tool_name_for_runtime(kind: crate::config::ReplRuntimeKindToml) -> String {
    format!("repl_{}", kind.label())
}

/// Returns the per-runtime reset tool name, e.g. `"repl_reset_python"`.
pub(crate) fn repl_reset_tool_name_for_runtime(kind: crate::config::ReplRuntimeKindToml) -> String {
    format!("repl_reset_{}", kind.label())
}

/// Returns the runtime kind for a per-runtime tool name, or `None` for the
/// generic `"repl"` / `"repl_reset"` names.
pub(crate) fn runtime_from_repl_tool_name(name: &str) -> Option<crate::config::ReplRuntimeKindToml> {
    let suffix = name.strip_prefix("repl_")?;
    crate::config::ReplRuntimeKindToml::ALL
        .iter()
        .find(|k| k.label() == suffix)
        .copied()
}

/// Returns the runtime kind for a per-runtime reset tool name like
/// `"repl_reset_python"`, or `None` for the generic `"repl_reset"`.
pub(crate) fn runtime_from_repl_reset_tool_name(name: &str) -> Option<crate::config::ReplRuntimeKindToml> {
    let suffix = name.strip_prefix("repl_reset_")?;
    crate::config::ReplRuntimeKindToml::ALL
        .iter()
        .find(|k| k.label() == suffix)
        .copied()
}

pub use registry::get_openai_tools;
pub use tools_json::create_tools_json_for_responses_api;
pub(crate) use tools_json::create_tools_json_for_chat_completions_api;
pub(crate) use json_schema::JsonSchema;
pub use types::{
    FreeformTool,
    FreeformToolFormat,
    OpenAiTool,
    ResponsesApiTool,
};

pub(crate) fn create_additional_permissions_schema() -> JsonSchema {
    JsonSchema::Object {
        properties: std::collections::BTreeMap::from([
            (
                "network".to_owned(),
                JsonSchema::Boolean {
                    description: Some(
                        "Whether this command needs sandboxed network access.".to_owned(),
                    ),
                },
            ),
            (
                "file_system".to_owned(),
                JsonSchema::Object {
                    properties: std::collections::BTreeMap::from([
                        (
                            "read".to_owned(),
                            JsonSchema::Array {
                                items: Box::new(JsonSchema::String {
                                    description: None,
                                    allowed_values: None,
                                }),
                                description: Some(
                                    "Additional filesystem paths to grant read access for this command.".to_owned(),
                                ),
                            },
                        ),
                        (
                            "write".to_owned(),
                            JsonSchema::Array {
                                items: Box::new(JsonSchema::String {
                                    description: None,
                                    allowed_values: None,
                                }),
                                description: Some(
                                    "Additional filesystem paths to grant write access for this command.".to_owned(),
                                ),
                            },
                        ),
                    ]),
                    required: None,
                    additional_properties: Some(false.into()),
                },
            ),
            (
                "macos".to_owned(),
                JsonSchema::Object {
                    properties: std::collections::BTreeMap::from([
                        (
                            "preferences".to_owned(),
                            JsonSchema::String {
                                description: Some(
                                    "Optional macOS preferences access mode (for example: readonly or readwrite).".to_owned(),
                                ),
                                allowed_values: None,
                            },
                        ),
                        (
                            "automations".to_owned(),
                            JsonSchema::Array {
                                items: Box::new(JsonSchema::String {
                                    description: None,
                                    allowed_values: None,
                                }),
                                description: Some(
                                    "Optional list of macOS bundle IDs that need automation access.".to_owned(),
                                ),
                            },
                        ),
                        (
                            "accessibility".to_owned(),
                            JsonSchema::Boolean {
                                description: Some(
                                    "Whether this command needs macOS Accessibility access.".to_owned(),
                                ),
                            },
                        ),
                        (
                            "calendar".to_owned(),
                            JsonSchema::Boolean {
                                description: Some(
                                    "Whether this command needs macOS Calendar access.".to_owned(),
                                ),
                            },
                        ),
                    ]),
                    required: None,
                    additional_properties: Some(false.into()),
                },
            ),
        ]),
        required: None,
        additional_properties: Some(false.into()),
    }
}
