use std::collections::BTreeMap;

use crate::protocol::SandboxPolicy;

use super::json_schema::JsonSchema;
use super::types::{FreeformTool, FreeformToolFormat, OpenAiTool, ResponsesApiTool};
use super::{
    GREP_FILES_TOOL_NAME,
    JS_REPL_RESET_TOOL_NAME,
    JS_REPL_TOOL_NAME,
    LIST_DIR_TOOL_NAME,
    READ_FILE_TOOL_NAME,
    SEARCH_TOOL_BM25_TOOL_NAME,
    SEARCH_TOOL_DESCRIPTION_TEMPLATE,
};

pub(super) fn create_shell_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "command".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some("The command to execute".to_string()),
        },
    );
    properties.insert(
        "workdir".to_string(),
        JsonSchema::String {
            description: Some("The working directory to execute the command in".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "timeout".to_string(),
        JsonSchema::Number {
            description: Some("Optional hard timeout in milliseconds (minimum 1,800,000 / 30 minutes). By default, commands have no hard timeout; long runs are streamed and may be backgrounded by the agent.".to_string()),
        },
    );
    properties.insert(
        "prefix_rule".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some(
                "Suggests a command prefix to persist for future sessions".to_string(),
            ),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "shell".to_string(),
        description: "Runs a shell command and returns its output. Output streams live to the UI. Long-running commands may be backgrounded after an initial window. Use `wait` to await background tasks. Optional `timeout` can set a hard kill if needed.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["command".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_image_view_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "path".to_string(),
        JsonSchema::String {
            description: Some("Local filesystem path to an image file.".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "alt_text".to_string(),
        JsonSchema::String {
            description: Some("Optional label for the image.".to_string()),
            allowed_values: None,
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "image_view".to_string(),
        description: "Attach a local image so the model can view it.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["path".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_request_user_input_tool() -> OpenAiTool {
    let mut option_props = BTreeMap::new();
    option_props.insert(
        "label".to_string(),
        JsonSchema::String {
            description: Some("User-facing label (1-5 words).".to_string()),
            allowed_values: None,
        },
    );
    option_props.insert(
        "description".to_string(),
        JsonSchema::String {
            description: Some(
                "One short sentence explaining impact/tradeoff if selected.".to_string(),
            ),
            allowed_values: None,
        },
    );

    let options_schema = JsonSchema::Array {
        description: Some(
            "Provide 2-3 mutually exclusive choices. Put the recommended option first and suffix its label with \"(Recommended)\". Do not include an \"Other\" option in this list; the client will add a free-form \"Other\" option automatically.".to_string(),
        ),
        items: Box::new(JsonSchema::Object {
            properties: option_props,
            required: Some(vec!["label".to_string(), "description".to_string()]),
            additional_properties: Some(false.into()),
        }),
    };

    let mut question_props = BTreeMap::new();
    question_props.insert(
        "id".to_string(),
        JsonSchema::String {
            description: Some("Stable identifier for mapping answers (snake_case).".to_string()),
            allowed_values: None,
        },
    );
    question_props.insert(
        "header".to_string(),
        JsonSchema::String {
            description: Some("Short header label shown in the UI (12 or fewer chars).".to_string()),
            allowed_values: None,
        },
    );
    question_props.insert(
        "question".to_string(),
        JsonSchema::String {
            description: Some("Single-sentence prompt shown to the user.".to_string()),
            allowed_values: None,
        },
    );
    question_props.insert("options".to_string(), options_schema);

    let questions_schema = JsonSchema::Array {
        description: Some("Questions to show the user. Prefer 1 and do not exceed 3".to_string()),
        items: Box::new(JsonSchema::Object {
            properties: question_props,
            required: Some(vec![
                "id".to_string(),
                "header".to_string(),
                "question".to_string(),
                "options".to_string(),
            ]),
            additional_properties: Some(false.into()),
        }),
    };

    let mut properties = BTreeMap::new();
    properties.insert("questions".to_string(), questions_schema);

    OpenAiTool::Function(ResponsesApiTool {
        name: "request_user_input".to_string(),
        description: "Request user input for one to three short questions and wait for the response."
            .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["questions".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_search_tool_bm25_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "query".to_string(),
        JsonSchema::String {
            description: Some("Search query for MCP tools.".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "limit".to_string(),
        JsonSchema::Number {
            description: Some(
                "Maximum number of tools to return (defaults to 8).".to_string(),
            ),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: SEARCH_TOOL_BM25_TOOL_NAME.to_string(),
        description: render_search_tool_description(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["query".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_list_mcp_resources_tool() -> OpenAiTool {
    let properties = BTreeMap::from([
        (
            "server".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional MCP server name. When omitted, lists resources from every configured server."
                        .to_string(),
                ),
                allowed_values: None,
            },
        ),
        (
            "cursor".to_string(),
            JsonSchema::String {
                description: Some(
                    "Opaque cursor returned by a previous list_mcp_resources call for the same server."
                        .to_string(),
                ),
                allowed_values: None,
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: "list_mcp_resources".to_string(),
        description: "Lists resources provided by MCP servers. Resources allow servers to share data that provides context to language models, such as files, database schemas, or application-specific information. Prefer resources over web search when possible.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: None,
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_list_mcp_resource_templates_tool() -> OpenAiTool {
    let properties = BTreeMap::from([
        (
            "server".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional MCP server name. When omitted, lists resource templates from all configured servers."
                        .to_string(),
                ),
                allowed_values: None,
            },
        ),
        (
            "cursor".to_string(),
            JsonSchema::String {
                description: Some(
                    "Opaque cursor returned by a previous list_mcp_resource_templates call for the same server."
                        .to_string(),
                ),
                allowed_values: None,
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: "list_mcp_resource_templates".to_string(),
        description: "Lists resource templates provided by MCP servers. Parameterized resource templates allow servers to share data that takes parameters and provides context to language models, such as files, database schemas, or application-specific information. Prefer resource templates over web search when possible.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: None,
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_read_mcp_resource_tool() -> OpenAiTool {
    let properties = BTreeMap::from([
        (
            "server".to_string(),
            JsonSchema::String {
                description: Some(
                    "MCP server name exactly as configured. Must match the 'server' field returned by list_mcp_resources."
                        .to_string(),
                ),
                allowed_values: None,
            },
        ),
        (
            "uri".to_string(),
            JsonSchema::String {
                description: Some(
                    "Resource URI to read. Must be one of the URIs returned by list_mcp_resources."
                        .to_string(),
                ),
                allowed_values: None,
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: "read_mcp_resource".to_string(),
        description:
            "Read a specific resource from an MCP server given the server name and resource URI."
                .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["server".to_string(), "uri".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

fn render_search_tool_description() -> String {
    SEARCH_TOOL_DESCRIPTION_TEMPLATE.to_string()
}

pub(super) fn create_grep_files_tool() -> OpenAiTool {
    let properties = BTreeMap::from([
        (
            "pattern".to_string(),
            JsonSchema::String {
                description: Some("Regular expression pattern to search for.".to_string()),
                allowed_values: None,
            },
        ),
        (
            "include".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional glob that limits which files are searched (e.g. \"*.rs\" or \"*.{ts,tsx}\")."
                        .to_string(),
                ),
                allowed_values: None,
            },
        ),
        (
            "path".to_string(),
            JsonSchema::String {
                description: Some(
                    "Directory or file path to search. Defaults to the session's working directory."
                        .to_string(),
                ),
                allowed_values: None,
            },
        ),
        (
            "limit".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Maximum number of file paths to return (defaults to 100).".to_string(),
                ),
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: GREP_FILES_TOOL_NAME.to_string(),
        description:
            "Find files whose contents match the pattern and list them by modification time."
                .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["pattern".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_read_file_tool() -> OpenAiTool {
    let indentation_properties = BTreeMap::from([
        (
            "anchor_line".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Anchor line to center the indentation lookup on (defaults to offset)."
                        .to_string(),
                ),
            },
        ),
        (
            "max_levels".to_string(),
            JsonSchema::Number {
                description: Some(
                    "How many parent indentation levels (smaller indents) to include.".to_string(),
                ),
            },
        ),
        (
            "include_siblings".to_string(),
            JsonSchema::Boolean {
                description: Some(
                    "When true, include additional blocks that share the anchor indentation."
                        .to_string(),
                ),
            },
        ),
        (
            "include_header".to_string(),
            JsonSchema::Boolean {
                description: Some(
                    "Include doc comments or attributes directly above the selected block."
                        .to_string(),
                ),
            },
        ),
        (
            "max_lines".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Hard cap on the number of lines returned when using indentation mode."
                        .to_string(),
                ),
            },
        ),
    ]);

    let properties = BTreeMap::from([
        (
            "file_path".to_string(),
            JsonSchema::String {
                description: Some(
                    "Path to the file (absolute recommended; relative paths are resolved against the session working directory)."
                        .to_string(),
                ),
                allowed_values: None,
            },
        ),
        (
            "offset".to_string(),
            JsonSchema::Number {
                description: Some(
                    "The line number to start reading from. Must be 1 or greater.".to_string(),
                ),
            },
        ),
        (
            "limit".to_string(),
            JsonSchema::Number {
                description: Some("The maximum number of lines to return.".to_string()),
            },
        ),
        (
            "mode".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional mode selector: \"slice\" for simple ranges (default) or \"indentation\" to expand around an anchor line."
                        .to_string(),
                ),
                allowed_values: Some(vec!["slice".to_string(), "indentation".to_string()]),
            },
        ),
        (
            "indentation".to_string(),
            JsonSchema::Object {
                properties: indentation_properties,
                required: None,
                additional_properties: Some(false.into()),
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: READ_FILE_TOOL_NAME.to_string(),
        description:
            "Read a local file with 1-indexed line numbers, supporting slice and indentation-aware block modes."
                .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["file_path".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_list_dir_tool() -> OpenAiTool {
    let properties = BTreeMap::from([
        (
            "dir_path".to_string(),
            JsonSchema::String {
                description: Some(
                    "Directory path to list (absolute recommended; relative paths are resolved against the session working directory)."
                        .to_string(),
                ),
                allowed_values: None,
            },
        ),
        (
            "offset".to_string(),
            JsonSchema::Number {
                description: Some(
                    "The entry number to start listing from. Must be 1 or greater.".to_string(),
                ),
            },
        ),
        (
            "limit".to_string(),
            JsonSchema::Number {
                description: Some("The maximum number of entries to return.".to_string()),
            },
        ),
        (
            "depth".to_string(),
            JsonSchema::Number {
                description: Some(
                    "The maximum directory depth to traverse. Must be 1 or greater.".to_string(),
                ),
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: LIST_DIR_TOOL_NAME.to_string(),
        description:
            "List entries in a local directory with 1-indexed entry numbers and simple type labels."
                .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["dir_path".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_js_repl_tool() -> OpenAiTool {
    const JS_REPL_FREEFORM_GRAMMAR: &str = r#"start: /[\s\S]*/"#;

    OpenAiTool::Freeform(FreeformTool {
        name: JS_REPL_TOOL_NAME.to_string(),
        description: "Runs JavaScript in a persistent Node kernel with top-level await. This is a freeform tool: send raw JavaScript source text, optionally with a first-line pragma like `// codex-js-repl: timeout_ms=15000`; do not send JSON/quotes/markdown fences."
            .to_string(),
        format: FreeformToolFormat {
            r#type: "grammar".to_string(),
            syntax: "lark".to_string(),
            definition: JS_REPL_FREEFORM_GRAMMAR.to_string(),
        },
    })
}

pub(super) fn create_js_repl_reset_tool() -> OpenAiTool {
    OpenAiTool::Function(ResponsesApiTool {
        name: JS_REPL_RESET_TOOL_NAME.to_string(),
        description:
            "Restarts the js_repl kernel for this run and clears persisted top-level bindings."
                .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties: BTreeMap::new(),
            required: None,
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_shell_tool_for_sandbox(sandbox_policy: &SandboxPolicy) -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "command".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some("The command to execute".to_string()),
        },
    );
    properties.insert(
        "prefix_rule".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some("Suggests a command prefix to persist for future sessions".to_string()),
        },
    );
    properties.insert(
        "workdir".to_string(),
        JsonSchema::String {
            description: Some("The working directory to execute the command in".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "timeout_ms".to_string(),
        JsonSchema::Number {
            description: Some("Optional hard timeout in milliseconds (minimum 1,800,000 / 30 minutes). By default, commands have no hard timeout; long runs are streamed and may be backgrounded by the agent.".to_string()),
        },
    );

    if matches!(sandbox_policy, SandboxPolicy::WorkspaceWrite { .. }) {
        properties.insert(
            "sandbox_permissions".to_string(),
            JsonSchema::String {
                description: Some(
                    "Sandbox permissions for the command. Use \"with_additional_permissions\" to request additional sandboxed filesystem access (preferred), or \"require_escalated\" to request running without sandbox restrictions; defaults to \"use_default\"."
                        .to_string(),
                ),
                allowed_values: Some(vec![
                    "use_default".to_string(),
                    "with_additional_permissions".to_string(),
                    "require_escalated".to_string(),
                ]),
            },
        );
        properties.insert(
            "justification".to_string(),
            JsonSchema::String {
                description: Some(
                    "Only set if sandbox_permissions is \"require_escalated\". 1-sentence explanation of why we want to run this command."
                        .to_string(),
                ),
                allowed_values: None,
            },
        );
        properties.insert(
            "additional_permissions".to_string(),
            JsonSchema::Object {
                properties: BTreeMap::from([(
                    "file_system".to_string(),
                    JsonSchema::Object {
                        properties: BTreeMap::from([
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
                )]),
                required: Some(vec!["file_system".to_string()]),
                additional_properties: Some(false.into()),
            },
        );
    }

    let description = match sandbox_policy {
        SandboxPolicy::WorkspaceWrite {
            network_access,
            writable_roots,
            ..
        } => {
            let roots_str = if writable_roots.is_empty() {
                "    - (none)\n".to_string()
            } else {
                writable_roots
                    .iter()
                    .map(|p| format!("    - {}\n", p.display()))
                    .collect()
            };
            format!(
                r#"
The shell tool is used to execute shell commands.
- When invoking the shell tool, your call will be running in a sandbox, and some shell commands will require escalated privileges:
  - Types of actions that require escalated privileges:
    - Writing files other than those in the writable roots
      - writable roots:
{}{}
  - Examples of commands that require escalated privileges:
    - git commit
    - npm install or pnpm install
    - cargo build
    - cargo test
- When invoking a command that will require escalated privileges:
  - Provide the sandbox_permissions parameter with the value \"require_escalated\"
  - Include a short, 1 sentence explanation for why we need escalated permissions in the justification parameter.
- When additional sandboxed filesystem access is enough:
  - Provide the sandbox_permissions parameter with the value \"with_additional_permissions\"
  - Provide additional_permissions.file_system.read and/or additional_permissions.file_system.write with the minimal paths needed.

Long-running commands may be backgrounded after an initial window. Use `wait` to await background tasks. Optional `timeout` can set a hard kill if needed."#,
                roots_str,
                if !network_access {
                    "\n    - Commands that require network access\n"
                } else {
                    ""
                }
            )
        }
        SandboxPolicy::DangerFullAccess => {
            "Runs a shell command and returns its output. Output streams live to the UI. Long-running commands may be backgrounded after an initial window. Use `wait` to await background tasks.".to_string()
        }
        SandboxPolicy::ReadOnly => {
            "Runs a shell command and returns its output. Output streams live to the UI. Long-running commands may be backgrounded after an initial window. Use `wait` to await background tasks.".to_string()
        }
    };

    OpenAiTool::Function(ResponsesApiTool {
        name: "shell".to_string(),
        description,
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["command".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}
