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
pub(crate) const JS_REPL_TOOL_NAME: &str = "js_repl";
pub(crate) const JS_REPL_RESET_TOOL_NAME: &str = "js_repl_reset";

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
                "network".to_string(),
                JsonSchema::Boolean {
                    description: Some(
                        "Whether this command needs sandboxed network access.".to_string(),
                    ),
                },
            ),
            (
                "file_system".to_string(),
                JsonSchema::Object {
                    properties: std::collections::BTreeMap::from([
                        (
                            "read".to_string(),
                            JsonSchema::Array {
                                items: Box::new(JsonSchema::String {
                                    description: None,
                                    allowed_values: None,
                                }),
                                description: Some(
                                    "Additional filesystem paths to grant read access for this command."
                                        .to_string(),
                                ),
                            },
                        ),
                        (
                            "write".to_string(),
                            JsonSchema::Array {
                                items: Box::new(JsonSchema::String {
                                    description: None,
                                    allowed_values: None,
                                }),
                                description: Some(
                                    "Additional filesystem paths to grant write access for this command."
                                        .to_string(),
                                ),
                            },
                        ),
                    ]),
                    required: None,
                    additional_properties: Some(false.into()),
                },
            ),
            (
                "macos".to_string(),
                JsonSchema::Object {
                    properties: std::collections::BTreeMap::from([
                        (
                            "preferences".to_string(),
                            JsonSchema::String {
                                description: Some(
                                    "Optional macOS preferences access mode (for example: readonly or readwrite)."
                                        .to_string(),
                                ),
                                allowed_values: None,
                            },
                        ),
                        (
                            "automations".to_string(),
                            JsonSchema::Array {
                                items: Box::new(JsonSchema::String {
                                    description: None,
                                    allowed_values: None,
                                }),
                                description: Some(
                                    "Optional list of macOS bundle IDs that need automation access."
                                        .to_string(),
                                ),
                            },
                        ),
                        (
                            "accessibility".to_string(),
                            JsonSchema::Boolean {
                                description: Some(
                                    "Whether this command needs macOS Accessibility access."
                                        .to_string(),
                                ),
                            },
                        ),
                        (
                            "calendar".to_string(),
                            JsonSchema::Boolean {
                                description: Some(
                                    "Whether this command needs macOS Calendar access."
                                        .to_string(),
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
