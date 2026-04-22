use std::collections::BTreeMap;

use crate::protocol::SandboxPolicy;

use super::json_schema::JsonSchema;
use super::types::{FreeformTool, FreeformToolFormat, OpenAiTool, ResponsesApiTool};
use super::{
    create_additional_permissions_schema,
    GREP_FILES_TOOL_NAME,
    REPL_RESET_TOOL_NAME,
    REPL_TOOL_NAME,
    LIST_DIR_TOOL_NAME,
    READ_FILE_TOOL_NAME,
    SEARCH_TOOL_BM25_TOOL_NAME,
    SEARCH_TOOL_DESCRIPTION_TEMPLATE,
};

pub(super) fn create_shell_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "command".to_owned(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some("The command to execute".to_owned()),
        },
    );
    properties.insert(
        "workdir".to_owned(),
        JsonSchema::String {
            description: Some("The working directory to execute the command in".to_owned()),
            allowed_values: None,
        },
    );
    properties.insert(
        "timeout".to_owned(),
        JsonSchema::Number {
            description: Some("Optional hard timeout in milliseconds (minimum 1,800,000 / 30 minutes). By default, commands have no hard timeout; long runs are streamed and may be backgrounded by the agent.".to_owned()),
        },
    );
    properties.insert(
        "prefix_rule".to_owned(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some(
                "Suggests a command prefix to persist for future sessions".to_owned(),
            ),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "shell".to_owned(),
        description: "Runs a shell command and returns its output. Output streams live to the UI. Long-running commands may be backgrounded after an initial window. Use `wait` to await background tasks. Optional `timeout` can set a hard kill if needed.".to_owned(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["command".to_owned()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_image_view_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "path".to_owned(),
        JsonSchema::String {
            description: Some("Local filesystem path to an image file.".to_owned()),
            allowed_values: None,
        },
    );
    properties.insert(
        "alt_text".to_owned(),
        JsonSchema::String {
            description: Some("Optional label for the image.".to_owned()),
            allowed_values: None,
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "image_view".to_owned(),
        description: "Attach a local image so the model can view it.".to_owned(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["path".to_owned()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_request_user_input_tool() -> OpenAiTool {
    let mut option_props = BTreeMap::new();
    option_props.insert(
        "label".to_owned(),
        JsonSchema::String {
            description: Some("User-facing label (1-5 words).".to_owned()),
            allowed_values: None,
        },
    );
    option_props.insert(
        "description".to_owned(),
        JsonSchema::String {
            description: Some(
                "One short sentence explaining impact/tradeoff if selected.".to_owned(),
            ),
            allowed_values: None,
        },
    );

    let options_schema = JsonSchema::Array {
        description: Some(
            "Optional selectable choices. For single-select questions, keep choices mutually exclusive. For multi-select questions, make each option an independent toggle. Put recommended options first and suffix their labels with \"(Recommended)\". Do not include an \"Other\" option in this list; the client can add a free-form custom answer automatically.".to_owned(),
        ),
        items: Box::new(JsonSchema::Object {
            properties: option_props,
            required: Some(vec!["label".to_owned(), "description".to_owned()]),
            additional_properties: Some(false.into()),
        }),
    };

    let mut question_props = BTreeMap::new();
    question_props.insert(
        "id".to_owned(),
        JsonSchema::String {
            description: Some("Stable identifier for mapping answers (snake_case).".to_owned()),
            allowed_values: None,
        },
    );
    question_props.insert(
        "header".to_owned(),
        JsonSchema::String {
            description: Some("Short header label shown in the UI (12 or fewer chars).".to_owned()),
            allowed_values: None,
        },
    );
    question_props.insert(
        "question".to_owned(),
        JsonSchema::String {
            description: Some("Single-sentence prompt shown to the user.".to_owned()),
            allowed_values: None,
        },
    );
    question_props.insert(
        "allow_freeform".to_owned(),
        JsonSchema::Boolean {
            description: Some(
                "If true, allow a custom typed answer. Defaults to true; set false for options-only questions.".to_owned(),
            ),
        },
    );
    question_props.insert(
        "allow_multiple".to_owned(),
        JsonSchema::Boolean {
            description: Some(
                "If true, render checkbox-style multi-select and return all selected option labels in order. Requires `options` to be provided.".to_owned(),
            ),
        },
    );
    question_props.insert("options".to_owned(), options_schema);

    let questions_schema = JsonSchema::Array {
        description: Some("Questions to show the user. Prefer 1 and do not exceed 3".to_owned()),
        items: Box::new(JsonSchema::Object {
            properties: question_props,
            required: Some(vec!["id".to_owned(), "header".to_owned(), "question".to_owned()]),
            additional_properties: Some(false.into()),
        }),
    };

    let mut properties = BTreeMap::new();
    properties.insert("questions".to_owned(), questions_schema);

    OpenAiTool::Function(ResponsesApiTool {
        name: "request_user_input".to_owned(),
        description: "Request user input for one to three short questions and wait for the response. Supports single-choice, checkbox-style multi-select, and freeform questions.".to_owned(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["questions".to_owned()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_request_permissions_tool() -> OpenAiTool {
    let permissions_schema = JsonSchema::Object {
        properties: BTreeMap::from([
            (
                "network".to_owned(),
                JsonSchema::Object {
                    properties: BTreeMap::from([(
                        "enabled".to_owned(),
                        JsonSchema::Boolean {
                            description: Some("Whether to enable network access.".to_owned()),
                        },
                    )]),
                    required: None,
                    additional_properties: Some(false.into()),
                },
            ),
            (
                "file_system".to_owned(),
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        (
                            "read".to_owned(),
                            JsonSchema::Array {
                                items: Box::new(JsonSchema::String {
                                    description: None,
                                    allowed_values: None,
                                }),
                                description: Some(
                                    "Additional directories/files to read (absolute or relative to the session cwd).".to_owned(),
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
                                    "Additional directories/files to write (absolute or relative to the session cwd).".to_owned(),
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
    };

    let mut properties = BTreeMap::new();
    properties.insert(
        "reason".to_owned(),
        JsonSchema::String {
            description: Some("Optional explanation shown to the user.".to_owned()),
            allowed_values: None,
        },
    );
    properties.insert("permissions".to_owned(), permissions_schema);

    OpenAiTool::Function(ResponsesApiTool {
        name: "request_permissions".to_owned(),
        description: "Request additional filesystem or network permissions from the user and wait for approval.".to_owned(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["permissions".to_owned()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_search_tool_bm25_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "query".to_owned(),
        JsonSchema::String {
            description: Some("Search query for MCP tools.".to_owned()),
            allowed_values: None,
        },
    );
    properties.insert(
        "limit".to_owned(),
        JsonSchema::Number {
            description: Some(
                "Maximum number of tools to return (defaults to 8).".to_owned(),
            ),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: SEARCH_TOOL_BM25_TOOL_NAME.to_owned(),
        description: render_search_tool_description(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["query".to_owned()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_list_mcp_resources_tool() -> OpenAiTool {
    let properties = BTreeMap::from([
        (
            "server".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Optional MCP server name. When omitted, lists resources from every configured server.".to_owned(),
                ),
                allowed_values: None,
            },
        ),
        (
            "cursor".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Opaque cursor returned by a previous list_mcp_resources call for the same server.".to_owned(),
                ),
                allowed_values: None,
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: "list_mcp_resources".to_owned(),
        description: "Lists resources provided by MCP servers. Resources allow servers to share data that provides context to language models, such as files, database schemas, or application-specific information. Prefer resources over web search when possible.".to_owned(),
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
            "server".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Optional MCP server name. When omitted, lists resource templates from all configured servers.".to_owned(),
                ),
                allowed_values: None,
            },
        ),
        (
            "cursor".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Opaque cursor returned by a previous list_mcp_resource_templates call for the same server.".to_owned(),
                ),
                allowed_values: None,
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: "list_mcp_resource_templates".to_owned(),
        description: "Lists resource templates provided by MCP servers. Parameterized resource templates allow servers to share data that takes parameters and provides context to language models, such as files, database schemas, or application-specific information. Prefer resource templates over web search when possible.".to_owned(),
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
            "server".to_owned(),
            JsonSchema::String {
                description: Some(
                    "MCP server name exactly as configured. Must match the 'server' field returned by list_mcp_resources.".to_owned(),
                ),
                allowed_values: None,
            },
        ),
        (
            "uri".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Resource URI to read. Must be one of the URIs returned by list_mcp_resources.".to_owned(),
                ),
                allowed_values: None,
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: "read_mcp_resource".to_owned(),
        description:
            "Read a specific resource from an MCP server given the server name and resource URI.".to_owned(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["server".to_owned(), "uri".to_owned()]),
            additional_properties: Some(false.into()),
        },
    })
}

fn render_search_tool_description() -> String {
    SEARCH_TOOL_DESCRIPTION_TEMPLATE.to_owned()
}

pub(super) fn create_grep_files_tool() -> OpenAiTool {
    let properties = BTreeMap::from([
        (
            "pattern".to_owned(),
            JsonSchema::String {
                description: Some("Regular expression pattern to search for.".to_owned()),
                allowed_values: None,
            },
        ),
        (
            "include".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Optional glob that limits which files are searched (e.g. \"*.rs\" or \"*.{ts,tsx}\").".to_owned(),
                ),
                allowed_values: None,
            },
        ),
        (
            "path".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Directory or file path to search. Defaults to the session's working directory.".to_owned(),
                ),
                allowed_values: None,
            },
        ),
        (
            "limit".to_owned(),
            JsonSchema::Number {
                description: Some(
                    "Maximum number of file paths to return (defaults to 100).".to_owned(),
                ),
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: GREP_FILES_TOOL_NAME.to_owned(),
        description:
            "Find files whose contents match the pattern and list them by modification time.".to_owned(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["pattern".to_owned()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_read_file_tool() -> OpenAiTool {
    let indentation_properties = BTreeMap::from([
        (
            "anchor_line".to_owned(),
            JsonSchema::Number {
                description: Some(
                    "Anchor line to center the indentation lookup on (defaults to offset).".to_owned(),
                ),
            },
        ),
        (
            "max_levels".to_owned(),
            JsonSchema::Number {
                description: Some(
                    "How many parent indentation levels (smaller indents) to include.".to_owned(),
                ),
            },
        ),
        (
            "include_siblings".to_owned(),
            JsonSchema::Boolean {
                description: Some(
                    "When true, include additional blocks that share the anchor indentation.".to_owned(),
                ),
            },
        ),
        (
            "include_header".to_owned(),
            JsonSchema::Boolean {
                description: Some(
                    "Include doc comments or attributes directly above the selected block.".to_owned(),
                ),
            },
        ),
        (
            "max_lines".to_owned(),
            JsonSchema::Number {
                description: Some(
                    "Hard cap on the number of lines returned when using indentation mode.".to_owned(),
                ),
            },
        ),
    ]);

    let properties = BTreeMap::from([
        (
            "file_path".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Path to the file (absolute recommended; relative paths are resolved against the session working directory).".to_owned(),
                ),
                allowed_values: None,
            },
        ),
        (
            "offset".to_owned(),
            JsonSchema::Number {
                description: Some(
                    "The line number to start reading from. Must be 1 or greater.".to_owned(),
                ),
            },
        ),
        (
            "limit".to_owned(),
            JsonSchema::Number {
                description: Some("The maximum number of lines to return.".to_owned()),
            },
        ),
        (
            "mode".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Optional mode selector: \"slice\" for simple ranges (default) or \"indentation\" to expand around an anchor line.".to_owned(),
                ),
                allowed_values: Some(vec!["slice".to_owned(), "indentation".to_owned()]),
            },
        ),
        (
            "indentation".to_owned(),
            JsonSchema::Object {
                properties: indentation_properties,
                required: None,
                additional_properties: Some(false.into()),
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: READ_FILE_TOOL_NAME.to_owned(),
        description:
            "Read a local file with 1-indexed line numbers, supporting slice and indentation-aware block modes.".to_owned(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["file_path".to_owned()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_list_dir_tool() -> OpenAiTool {
    let properties = BTreeMap::from([
        (
            "dir_path".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Directory path to list (absolute recommended; relative paths are resolved against the session working directory).".to_owned(),
                ),
                allowed_values: None,
            },
        ),
        (
            "offset".to_owned(),
            JsonSchema::Number {
                description: Some(
                    "The entry number to start listing from. Must be 1 or greater.".to_owned(),
                ),
            },
        ),
        (
            "limit".to_owned(),
            JsonSchema::Number {
                description: Some("The maximum number of entries to return.".to_owned()),
            },
        ),
        (
            "depth".to_owned(),
            JsonSchema::Number {
                description: Some(
                    "The maximum directory depth to traverse. Must be 1 or greater.".to_owned(),
                ),
            },
        ),
    ]);

    OpenAiTool::Function(ResponsesApiTool {
        name: LIST_DIR_TOOL_NAME.to_owned(),
        description:
            "List entries in a local directory with 1-indexed entry numbers and simple type labels.".to_owned(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["dir_path".to_owned()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub(super) fn create_repl_tool() -> OpenAiTool {
    const REPL_FREEFORM_GRAMMAR: &str = r"start: /[\s\S]*/";

    OpenAiTool::Freeform(FreeformTool {
        name: REPL_TOOL_NAME.to_owned(),
        description: "Runs code in a persistent REPL with top-level await (Node >= 18 by default; configurable to use Deno or Python). This is a freeform tool: send raw source code, optionally with a first-line pragma like `// codex-repl: timeout_ms=15000 runtime=python`; do not send JSON/quotes/markdown fences.".to_owned(),
        format: FreeformToolFormat {
            r#type: "grammar".to_owned(),
            syntax: "lark".to_owned(),
            definition: REPL_FREEFORM_GRAMMAR.to_owned(),
        },
    })
}

/// Creates a per-runtime REPL tool, e.g. `repl_python`.
pub(super) fn create_repl_tool_for_runtime(kind: crate::config::ReplRuntimeKindToml) -> OpenAiTool {
    const REPL_FREEFORM_GRAMMAR: &str = r"start: /[\s\S]*/";

    let label = kind.label();
    let description = match kind {
        crate::config::ReplRuntimeKindToml::Node => {
            "Runs JavaScript/TypeScript in a persistent Node.js REPL with top-level await. Send raw source code (no JSON, quotes, or markdown fences). Supports `// codex-repl: timeout_ms=15000` pragma."
        }
        crate::config::ReplRuntimeKindToml::Deno => {
            "Runs JavaScript/TypeScript in a persistent Deno REPL with top-level await. Send raw source code (no JSON, quotes, or markdown fences). Supports `// codex-repl: timeout_ms=15000` pragma."
        }
        crate::config::ReplRuntimeKindToml::Python => {
            "Runs Python code in a persistent Python REPL. Send raw source code (no JSON, quotes, or markdown fences). Supports `# codex-repl: timeout_ms=15000` pragma."
        }
    };

    OpenAiTool::Freeform(FreeformTool {
        name: super::repl_tool_name_for_runtime(kind),
        description: format!("{description} Tool name: repl_{label}"),
        format: FreeformToolFormat {
            r#type: "grammar".to_owned(),
            syntax: "lark".to_owned(),
            definition: REPL_FREEFORM_GRAMMAR.to_owned(),
        },
    })
}

pub(super) fn create_repl_reset_tool() -> OpenAiTool {
    OpenAiTool::Function(ResponsesApiTool {
        name: REPL_RESET_TOOL_NAME.to_owned(),
        description:
            "Restarts the repl kernel process, clearing all state including top-level bindings, imported modules, and in-flight timers.".to_owned(),
        strict: false,
        parameters: JsonSchema::Object {
            properties: BTreeMap::new(),
            required: None,
            additional_properties: Some(false.into()),
        },
    })
}

/// Creates a per-runtime reset tool, e.g. `repl_reset_python`.
pub(super) fn create_repl_reset_tool_for_runtime(kind: crate::config::ReplRuntimeKindToml) -> OpenAiTool {
    let label = kind.label();
    OpenAiTool::Function(ResponsesApiTool {
        name: super::repl_reset_tool_name_for_runtime(kind),
        description: format!(
            "Restarts the {label} repl kernel process, clearing all state."
        ),
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
        "command".to_owned(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some("The command to execute".to_owned()),
        },
    );
    properties.insert(
        "prefix_rule".to_owned(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some("Suggests a command prefix to persist for future sessions".to_owned()),
        },
    );
    properties.insert(
        "workdir".to_owned(),
        JsonSchema::String {
            description: Some("The working directory to execute the command in".to_owned()),
            allowed_values: None,
        },
    );
    properties.insert(
        "timeout_ms".to_owned(),
        JsonSchema::Number {
            description: Some("Optional hard timeout in milliseconds (minimum 1,800,000 / 30 minutes). By default, commands have no hard timeout; long runs are streamed and may be backgrounded by the agent.".to_owned()),
        },
    );

    if matches!(sandbox_policy, SandboxPolicy::WorkspaceWrite { .. }) {
        properties.insert(
            "sandbox_permissions".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Sandbox permissions for the command. Use \"with_additional_permissions\" to request additional sandboxed filesystem, network, or macOS permissions (preferred), or \"require_escalated\" to request running without sandbox restrictions; defaults to \"use_default\".".to_owned(),
                ),
                allowed_values: Some(vec![
                    "use_default".to_owned(),
                    "with_additional_permissions".to_owned(),
                    "require_escalated".to_owned(),
                ]),
            },
        );
        properties.insert(
            "justification".to_owned(),
            JsonSchema::String {
                description: Some(
                    "Only set if sandbox_permissions is \"require_escalated\". 1-sentence explanation of why we want to run this command.".to_owned(),
                ),
                allowed_values: None,
            },
        );
        properties.insert(
            "additional_permissions".to_owned(),
            create_additional_permissions_schema(),
        );
    }

    let description = match sandbox_policy {
        SandboxPolicy::WorkspaceWrite {
            network_access,
            writable_roots,
            ..
        } => {
            let roots_str = if writable_roots.is_empty() {
                "    - (none)\n".to_owned()
            } else {
                writable_roots
                    .iter()
                    .map(|p| format!("    - {}", p.display()))
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n"
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
- When additional sandboxed network or macOS permissions are enough:
  - Provide additional_permissions.network and/or additional_permissions.macos with the minimal permissions needed.

Long-running commands may be backgrounded after an initial window. Use `wait` to await background tasks. Optional `timeout` can set a hard kill if needed."#,
                roots_str,
                if *network_access {
                    ""
                } else {
                    "\n    - Commands that require network access\n"
                }
            )
        }
        SandboxPolicy::DangerFullAccess
        | SandboxPolicy::ReadOnly => {
            "Runs a shell command and returns its output. Output streams live to the UI. Long-running commands may be backgrounded after an initial window. Use `wait` to await background tasks.".to_owned()
        }
    };

    OpenAiTool::Function(ResponsesApiTool {
        name: "shell".to_owned(),
        description,
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["command".to_owned()]),
            additional_properties: Some(false.into()),
        },
    })
}
