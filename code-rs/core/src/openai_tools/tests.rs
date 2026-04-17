    use crate::model_family::find_family_for_model;
    use crate::protocol::{AskForApproval, SandboxPolicy};
    use mcp_types::ToolInputSchema;
    use pretty_assertions::assert_eq;
    use std::collections::{BTreeMap, HashMap};

    use super::*;

    use crate::agent_defaults::enabled_agent_model_specs;

    fn test_agent_models() -> Vec<String> {
        enabled_agent_model_specs()
            .into_iter()
            .map(|spec| spec.slug.to_string())
            .collect()
    }

    fn apply_default_agent_models(config: &mut ToolsConfig) {
        config.set_agent_models(test_agent_models());
    }

    fn model_family_or_panic(slug: &str) -> crate::model_family::ModelFamily {
        match find_family_for_model(slug) {
            Some(model_family) => model_family,
            None => panic!("{slug} should be a valid model family"),
        }
    }

    fn web_search_tool_or_panic(tools: &[OpenAiTool]) -> &super::types::WebSearchTool {
        match tools.iter().find_map(|tool| match tool {
            OpenAiTool::WebSearch(web_search_tool) => Some(web_search_tool),
            _ => None,
        }) {
            Some(web_search_tool) => web_search_tool,
            None => panic!("web_search tool should be present"),
        }
    }

    fn function_tool_or_panic<'a>(
        tools: &'a [OpenAiTool],
        expected_name: &str,
    ) -> &'a OpenAiTool {
        match tools.iter().find(|tool| {
            matches!(tool, OpenAiTool::Function(ResponsesApiTool { name, .. }) if name == expected_name)
        }) {
            Some(tool) => tool,
            None => panic!("{expected_name} tool present"),
        }
    }

    fn assert_eq_tool_names(tools: &[OpenAiTool], expected_names: &[&str]) {
        let tool_names = tools
            .iter()
            .map(|tool| match tool {
                OpenAiTool::Function(ResponsesApiTool { name, .. }) => name,
                OpenAiTool::LocalShell {} => "local_shell",
                OpenAiTool::WebSearch(_) => "web_search",
                OpenAiTool::Freeform(FreeformTool { name, .. }) => name,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            tool_names.len(),
            expected_names.len(),
            "tool_name mismatch, {tool_names:?}, {expected_names:?}",
        );
        for (name, expected_name) in tool_names.iter().zip(expected_names.iter()) {
            assert_eq!(
                name, expected_name,
                "tool_name mismatch, {name:?}, {expected_name:?}"
            );
        }
    }

    #[test]
    fn test_get_openai_tools() {
        let model_family = model_family_or_panic("codex-mini-latest");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: true,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
        apply_default_agent_models(&mut config);
        let tools = get_openai_tools(&config, Some(HashMap::new()), false, false, &[]);

        assert_eq_tool_names(
            &tools,
            &[
                "local_shell",
                "update_plan",
                "request_user_input",
                "request_permissions",
                "list_mcp_resources",
                "list_mcp_resource_templates",
                "read_mcp_resource",
                "read_file",
                "list_dir",
                "grep_files",
                "browser",
                "agent",
                "wait",
                "kill",
                "gh_run_wait",
                "code_bridge",
                "web_search",
            ],
        );
    }

    #[test]
    fn test_get_openai_tools_streamable_shell() {
        let model_family = model_family_or_panic("codex-mini-latest");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: true,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: true,
            include_view_image_tool: false,
        });
        apply_default_agent_models(&mut config);
        let tools = get_openai_tools(&config, Some(HashMap::new()), false, false, &[]);

        assert_eq_tool_names(
            &tools,
            &[
                "exec_command",
                "write_stdin",
                "update_plan",
                "request_user_input",
                "request_permissions",
                "list_mcp_resources",
                "list_mcp_resource_templates",
                "read_mcp_resource",
                "read_file",
                "list_dir",
                "grep_files",
                "browser",
                "agent",
                "wait",
                "kill",
                "gh_run_wait",
                "code_bridge",
                "web_search",
            ],
        );
    }

    #[test]
    fn test_search_tool_description_template_is_rendered() {
        let tool = super::builtin_tools::create_search_tool_bm25_tool();
        let OpenAiTool::Function(tool_spec) = tool else {
            panic!("search tool should be a function tool");
        };
        assert_eq!(tool_spec.name, SEARCH_TOOL_BM25_TOOL_NAME);
        assert!(tool_spec.description.contains("MCP tool discovery"));
        assert!(tool_spec
            .description
            .contains("MCP tools are hidden until you search for them"));
    }

    #[test]
    fn test_web_search_defaults_to_external_access_enabled() {
        let model_family = model_family_or_panic("o3");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
        apply_default_agent_models(&mut config);

        let tools = get_openai_tools(&config, Some(HashMap::new()), false, false, &[]);
        let web_search_tool = web_search_tool_or_panic(&tools);

        assert_eq!(web_search_tool.external_web_access, Some(true));
    }

    #[test]
    fn test_web_search_external_access_can_be_disabled() {
        let model_family = model_family_or_panic("o3");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
        config.web_search_external = false;
        config.web_search_allowed_domains = Some(vec!["openai.com".to_string()]);
        apply_default_agent_models(&mut config);

        let tools = get_openai_tools(&config, Some(HashMap::new()), false, false, &[]);
        let web_search_tool = web_search_tool_or_panic(&tools);

        assert_eq!(web_search_tool.external_web_access, Some(false));
        assert_eq!(
            web_search_tool
                .filters
                .as_ref()
                .and_then(|filters| filters.allowed_domains.as_ref())
                .cloned(),
            Some(vec!["openai.com".to_string()])
        );
    }

    #[test]
    fn test_get_openai_tools_with_active_agents() {
        let model_family = model_family_or_panic("codex-mini-latest");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: true,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
        apply_default_agent_models(&mut config);
        let tools = get_openai_tools(&config, Some(HashMap::new()), false, true, &[]);

        assert_eq_tool_names(
            &tools,
            &[
                "local_shell",
                "update_plan",
                "request_user_input",
                "request_permissions",
                "list_mcp_resources",
                "list_mcp_resource_templates",
                "read_mcp_resource",
                "read_file",
                "list_dir",
                "grep_files",
                "browser",
                "agent",
                "wait",
                "kill",
                "gh_run_wait",
                "code_bridge",
                "web_search",
            ],
        );
    }

    #[test]
    fn test_get_openai_tools_default_shell() {
        let model_family = model_family_or_panic("o3");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: true,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
        apply_default_agent_models(&mut config);
        let tools = get_openai_tools(&config, Some(HashMap::new()), false, false, &[]);

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "update_plan",
                "request_user_input",
                "request_permissions",
                "list_mcp_resources",
                "list_mcp_resource_templates",
                "read_mcp_resource",
                "read_file",
                "list_dir",
                "grep_files",
                "browser",
                "agent",
                "wait",
                "kill",
                "gh_run_wait",
                "code_bridge",
                "web_search",
            ],
        );
    }

    #[test]
    fn test_get_openai_tools_mcp_tools() {
        let model_family = model_family_or_panic("o3");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
        apply_default_agent_models(&mut config);
        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "test_server/do_something_cool".to_string(),
                mcp_types::Tool {
                    name: "do_something_cool".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "string_argument": {
                                "type": "string",
                            },
                            "number_argument": {
                                "type": "number",
                            },
                            "object_argument": {
                                "type": "object",
                                "properties": {
                                    "string_property": { "type": "string" },
                                    "number_property": { "type": "number" },
                                },
                                "required": [
                                    "string_property",
                                    "number_property",
                                ],
                                "additionalProperties": Some(false),
                            },
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("Do something cool".to_string()),
                },
            )])),
            false,
            true,
            &[],
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "request_user_input",
                "request_permissions",
                "list_mcp_resources",
                "list_mcp_resource_templates",
                "read_mcp_resource",
                "read_file",
                "list_dir",
                "grep_files",
                "browser",
                "agent",
                "wait",
                "kill",
                "gh_run_wait",
                "code_bridge",
                "web_search",
                "test_server/do_something_cool",
            ],
        );

        assert_eq!(
            tools[16],
            OpenAiTool::Function(ResponsesApiTool {
                name: "test_server/do_something_cool".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([
                        (
                            "string_argument".to_string(),
                            JsonSchema::String { description: None, allowed_values: None }
                        ),
                        (
                            "number_argument".to_string(),
                            JsonSchema::Number { description: None }
                        ),
                        (
                            "object_argument".to_string(),
                            JsonSchema::Object {
                                properties: BTreeMap::from([
                                    (
                                        "string_property".to_string(),
                                        JsonSchema::String { description: None, allowed_values: None }
                                    ),
                                    (
                                        "number_property".to_string(),
                                        JsonSchema::Number { description: None }
                                    ),
                                ]),
                                required: Some(vec![
                                    "string_property".to_string(),
                                    "number_property".to_string(),
                                ]),
                                additional_properties: Some(false.into()),
                            },
                        ),
                    ]),
                    required: None,
                    additional_properties: None,
                },
                description: "Do something cool".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_get_openai_tools_mcp_tools_with_additional_properties_schema() {
        let model_family = model_family_or_panic("o3");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: true,
        });
        apply_default_agent_models(&mut config);
        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "test_server/do_something_cool".to_string(),
                mcp_types::Tool {
                    name: "do_something_cool".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "string_argument": {
                                "type": "string",
                            },
                            "number_argument": {
                                "type": "number",
                            },
                            "object_argument": {
                                "type": "object",
                                "properties": {
                                    "string_property": { "type": "string" },
                                    "number_property": { "type": "number" },
                                },
                                "required": [
                                    "string_property",
                                    "number_property",
                                ],
                                "additionalProperties": {
                                    "type": "object",
                                    "properties": {
                                        "addtl_prop": { "type": "string" },
                                    },
                                    "required": [
                                        "addtl_prop",
                                    ],
                                    "additionalProperties": false,
                                },
                            },
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("Do something cool".to_string()),
                },
            )])),
            false,
            true,
            &[],
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "image_view",
                "request_user_input",
                "request_permissions",
                "list_mcp_resources",
                "list_mcp_resource_templates",
                "read_mcp_resource",
                "read_file",
                "list_dir",
                "grep_files",
                "browser",
                "agent",
                "wait",
                "kill",
                "gh_run_wait",
                "code_bridge",
                "web_search",
                "test_server/do_something_cool",
            ],
        );

        assert_eq!(
            tools[17],
            OpenAiTool::Function(ResponsesApiTool {
                name: "test_server/do_something_cool".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([
                        (
                            "string_argument".to_string(),
                            JsonSchema::String { description: None, allowed_values: None }
                        ),
                        (
                            "number_argument".to_string(),
                            JsonSchema::Number { description: None }
                        ),
                        (
                            "object_argument".to_string(),
                            JsonSchema::Object {
                                properties: BTreeMap::from([
                                    (
                                        "string_property".to_string(),
                                        JsonSchema::String { description: None, allowed_values: None }
                                    ),
                                    (
                                        "number_property".to_string(),
                                        JsonSchema::Number { description: None }
                                    ),
                                ]),
                                required: Some(vec![
                                    "string_property".to_string(),
                                    "number_property".to_string(),
                                ]),
                                additional_properties: Some(
                                    JsonSchema::Object {
                                        properties: BTreeMap::from([(
                                            "addtl_prop".to_string(),
                                            JsonSchema::String { description: None, allowed_values: None }
                                        ),]),
                                        required: Some(vec!["addtl_prop".to_string(),]),
                                        additional_properties: Some(false.into()),
                                    }
                                    .into()
                                ),
                            },
                        ),
                    ]),
                    required: None,
                    additional_properties: None,
                },
                description: "Do something cool".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_get_openai_tools_mcp_tools_sorted_by_name() {
        let model_family = model_family_or_panic("o3");
        let _config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
    }

    #[test]
    fn test_mcp_tool_property_missing_type_defaults_to_string() {
        let model_family = model_family_or_panic("o3");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: true,
        });
        apply_default_agent_models(&mut config);

        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "dash/search".to_string(),
                mcp_types::Tool {
                    name: "search".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "query": {
                                "description": "search query"
                            }
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("Search docs".to_string()),
                },
            )])),
            false,
            true,
            &[],
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "image_view",
                "request_user_input",
                "request_permissions",
                "list_mcp_resources",
                "list_mcp_resource_templates",
                "read_mcp_resource",
                "read_file",
                "list_dir",
                "grep_files",
                "browser",
                "agent",
                "wait",
                "kill",
                "gh_run_wait",
                "code_bridge",
                "web_search",
                "dash/search",
            ],
        );

        assert_eq!(
            tools[17],
            OpenAiTool::Function(ResponsesApiTool {
                name: "dash/search".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([(
                        "query".to_string(),
                        JsonSchema::String {
                            description: Some("search query".to_string()),
                            allowed_values: None,
                        }
                    )]),
                    required: None,
                    additional_properties: None,
                },
                description: "Search docs".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_mcp_tool_integer_normalized_to_number() {
        let model_family = model_family_or_panic("o3");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
        apply_default_agent_models(&mut config);

        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "dash/paginate".to_string(),
                mcp_types::Tool {
                    name: "paginate".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "page": { "type": "integer" }
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("Pagination".to_string()),
                },
            )])),
            false,
            true,
            &[],
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "request_user_input",
                "request_permissions",
                "list_mcp_resources",
                "list_mcp_resource_templates",
                "read_mcp_resource",
                "read_file",
                "list_dir",
                "grep_files",
                "browser",
                "agent",
                "wait",
                "kill",
                "gh_run_wait",
                "code_bridge",
                "web_search",
                "dash/paginate",
            ],
        );
        let paginate_tool = function_tool_or_panic(&tools, "dash/paginate");

        assert_eq!(
            paginate_tool,
            &OpenAiTool::Function(ResponsesApiTool {
                name: "dash/paginate".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([(
                        "page".to_string(),
                        JsonSchema::Number { description: None }
                    )]),
                    required: None,
                    additional_properties: None,
                },
                description: "Pagination".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_mcp_tool_array_without_items_gets_default_string_items() {
        let model_family = model_family_or_panic("o3");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
        apply_default_agent_models(&mut config);

        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "dash/tags".to_string(),
                mcp_types::Tool {
                    name: "tags".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "tags": { "type": "array" }
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("Tags".to_string()),
                },
            )])),
            false,
            true,
            &[],
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "request_user_input",
                "request_permissions",
                "list_mcp_resources",
                "list_mcp_resource_templates",
                "read_mcp_resource",
                "read_file",
                "list_dir",
                "grep_files",
                "browser",
                "agent",
                "wait",
                "kill",
                "gh_run_wait",
                "code_bridge",
                "web_search",
                "dash/tags",
            ],
        );
        assert_eq!(
            tools[16],
            OpenAiTool::Function(ResponsesApiTool {
                name: "dash/tags".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([(
                        "tags".to_string(),
                        JsonSchema::Array {
                            items: Box::new(JsonSchema::String { description: None, allowed_values: None }),
                            description: None
                        }
                    )]),
                    required: None,
                    additional_properties: None,
                },
                description: "Tags".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_mcp_tool_anyof_defaults_to_string() {
        let model_family = model_family_or_panic("o3");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
        apply_default_agent_models(&mut config);

        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "dash/value".to_string(),
                mcp_types::Tool {
                    name: "value".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "value": { "anyOf": [ { "type": "string" }, { "type": "number" } ] }
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("AnyOf Value".to_string()),
                },
            )])),
            false,
            true,
            &[],
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "request_user_input",
                "request_permissions",
                "list_mcp_resources",
                "list_mcp_resource_templates",
                "read_mcp_resource",
                "read_file",
                "list_dir",
                "grep_files",
                "browser",
                "agent",
                "wait",
                "kill",
                "gh_run_wait",
                "code_bridge",
                "web_search",
                "dash/value",
            ],
        );
        assert_eq!(
            tools[16],
            OpenAiTool::Function(ResponsesApiTool {
                name: "dash/value".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([(
                        "value".to_string(),
                        JsonSchema::String { description: None, allowed_values: None }
                    )]),
                    required: None,
                    additional_properties: None,
                },
                description: "AnyOf Value".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_shell_tool_for_sandbox_workspace_write() {
        let sandbox_policy = SandboxPolicy::WorkspaceWrite {
            writable_roots: vec!["workspace".into()],
            network_access: false,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
            allow_git_writes: true,
        };
        let tool = super::builtin_tools::create_shell_tool_for_sandbox(&sandbox_policy);
        let OpenAiTool::Function(ResponsesApiTool {
            description, name, ..
        }) = &tool
        else {
            panic!("expected function tool");
        };
        assert_eq!(name, "shell");
        assert!(
            description.contains("The shell tool is used to execute shell commands."),
            "description should explain shell usage"
        );
        assert!(
            description.contains("writable roots:"),
            "description should list writable roots"
        );
        assert!(
            description.contains("- workspace"),
            "description should mention workspace root"
        );
        assert!(
            description.contains("Commands that require network access"),
            "description should mention network access requirements"
        );
        assert!(
            description.contains("Long-running commands may be backgrounded"),
            "description should mention backgrounded commands"
        );
    }

    #[test]
    fn test_shell_tool_for_sandbox_readonly() {
        let tool = super::builtin_tools::create_shell_tool_for_sandbox(&SandboxPolicy::ReadOnly);
        let OpenAiTool::Function(ResponsesApiTool {
            description, name, ..
        }) = &tool
        else {
            panic!("expected function tool");
        };
        assert_eq!(name, "shell");
        assert!(description.starts_with("Runs a shell command and returns its output."));
        assert!(description.contains("Long-running commands may be backgrounded"));
    }

    #[test]
    fn test_shell_tool_for_sandbox_danger_full_access() {
        let tool =
            super::builtin_tools::create_shell_tool_for_sandbox(&SandboxPolicy::DangerFullAccess);
        let OpenAiTool::Function(ResponsesApiTool {
            description, name, ..
        }) = &tool
        else {
            panic!("expected function tool");
        };
        assert_eq!(name, "shell");
        assert!(description.starts_with("Runs a shell command and returns its output."));
        assert!(description.contains("Long-running commands may be backgrounded"));
    }

    #[test]
    fn test_per_runtime_tool_names() {
        use crate::config::ReplRuntimeKindToml;

        assert_eq!(super::repl_tool_name_for_runtime(ReplRuntimeKindToml::Python), "repl_python");
        assert_eq!(super::repl_tool_name_for_runtime(ReplRuntimeKindToml::Node), "repl_node");
        assert_eq!(super::repl_tool_name_for_runtime(ReplRuntimeKindToml::Deno), "repl_deno");

        assert_eq!(super::repl_reset_tool_name_for_runtime(ReplRuntimeKindToml::Python), "repl_reset_python");
        assert_eq!(super::repl_reset_tool_name_for_runtime(ReplRuntimeKindToml::Node), "repl_reset_node");
    }

    #[test]
    fn test_runtime_from_tool_name() {
        use crate::config::ReplRuntimeKindToml;

        // Per-runtime exec names.
        assert_eq!(super::runtime_from_repl_tool_name("repl_python"), Some(ReplRuntimeKindToml::Python));
        assert_eq!(super::runtime_from_repl_tool_name("repl_node"), Some(ReplRuntimeKindToml::Node));
        assert_eq!(super::runtime_from_repl_tool_name("repl_deno"), Some(ReplRuntimeKindToml::Deno));

        // Case-insensitive.
        assert_eq!(super::runtime_from_repl_tool_name("REPL_PYTHON"), Some(ReplRuntimeKindToml::Python));
        assert_eq!(super::runtime_from_repl_tool_name("Repl_Node"), Some(ReplRuntimeKindToml::Node));

        // Generic name returns None.
        assert_eq!(super::runtime_from_repl_tool_name("repl"), None);

        // Reset names must NOT match the exec helper.
        assert_eq!(super::runtime_from_repl_tool_name("repl_reset_python"), None);

        // Per-runtime reset names.
        assert_eq!(super::runtime_from_repl_reset_tool_name("repl_reset_python"), Some(ReplRuntimeKindToml::Python));
        assert_eq!(super::runtime_from_repl_reset_tool_name("REPL_RESET_DENO"), Some(ReplRuntimeKindToml::Deno));

        // Generic reset returns None.
        assert_eq!(super::runtime_from_repl_reset_tool_name("repl_reset"), None);
    }

    #[test]
    fn test_per_runtime_tools_registered_when_available() {
        use crate::config::ReplRuntimeKindToml;

        let model_family = model_family_or_panic("codex-mini-latest");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: false,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
        config.repl = true;
        config.repl_available_runtimes = vec![ReplRuntimeKindToml::Python, ReplRuntimeKindToml::Node];

        let tools = super::get_openai_tools(&config, None, true, false, &[]);
        let tool_names: Vec<&str> = tools.iter().map(|t| match t {
            OpenAiTool::Function(f) => f.name.as_str(),
            OpenAiTool::Freeform(f) => f.name.as_str(),
            _ => "",
        }).collect();

        // Generic tools present.
        assert!(tool_names.contains(&"repl"), "missing generic repl: {tool_names:?}");
        assert!(tool_names.contains(&"repl_reset"), "missing generic repl_reset: {tool_names:?}");

        // Per-runtime tools present.
        assert!(tool_names.contains(&"repl_python"), "missing repl_python: {tool_names:?}");
        assert!(tool_names.contains(&"repl_node"), "missing repl_node: {tool_names:?}");
        assert!(tool_names.contains(&"repl_reset_python"), "missing repl_reset_python: {tool_names:?}");
        assert!(tool_names.contains(&"repl_reset_node"), "missing repl_reset_node: {tool_names:?}");

        // Deno NOT available, so not registered.
        assert!(!tool_names.contains(&"repl_deno"), "unexpected repl_deno: {tool_names:?}");
    }

    #[test]
    fn test_chat_completions_converts_freeform_tools() {
        use crate::config::ReplRuntimeKindToml;

        let model_family = model_family_or_panic("codex-mini-latest");
        let mut config = ToolsConfig::new(ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: false,
            use_streamable_shell_tool: false,
            include_view_image_tool: false,
        });
        config.repl = true;
        config.repl_available_runtimes = vec![ReplRuntimeKindToml::Python];

        let tools = super::get_openai_tools(&config, None, true, false, &[]);
        let chat_json = super::create_tools_json_for_chat_completions_api(&tools)
            .expect("serialize for chat completions");

        // All tools should survive (none silently dropped).
        let names: Vec<&str> = chat_json.iter().filter_map(|t| {
            t.get("function")?.get("name")?.as_str()
        }).collect();

        assert!(names.contains(&"repl"), "generic repl missing from chat completions: {names:?}");
        assert!(names.contains(&"repl_python"), "repl_python missing from chat completions: {names:?}");
        assert!(names.contains(&"repl_reset"), "repl_reset missing from chat completions: {names:?}");
        assert!(names.contains(&"repl_reset_python"), "repl_reset_python missing from chat completions: {names:?}");

        // Verify the converted freeform tool has the expected structure.
        let repl_python_tool = chat_json.iter().find(|t| {
            t.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str()) == Some("repl_python")
        }).expect("repl_python tool");

        let func = repl_python_tool.get("function").unwrap();
        let params = func.get("parameters").unwrap();
        assert_eq!(params.get("type").unwrap().as_str(), Some("object"));
        assert!(params.get("properties").unwrap().get("input").is_some(),
            "converted freeform tool should have 'input' parameter");
    }
