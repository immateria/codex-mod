use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::openai_tools::JsonSchema;
use crate::openai_tools::OpenAiTool;
use crate::openai_tools::ResponsesApiTool;

pub fn create_agent_tool(allowed_models: &[String]) -> OpenAiTool {
    let mut properties = BTreeMap::new();

    properties.insert(
        "action".to_string(),
        JsonSchema::String {
            description: Some(
                "Required: choose one of ['create','status','wait','result','cancel','list']"
                    .to_string(),
            ),
            allowed_values: Some(
                ["create", "status", "wait", "result", "cancel", "list"]
                    .into_iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
            ),
        },
    );

    let mut create_properties = BTreeMap::new();
    create_properties.insert(
        "name".to_string(),
        JsonSchema::String {
            description: Some(
                "Display name shown in the UI (e.g., \"Plan TUI Refactor\")".to_string(),
            ),
            allowed_values: None,
        },
    );
    create_properties.insert(
        "task".to_string(),
        JsonSchema::String {
            description: Some("Task prompt to execute".to_string()),
            allowed_values: None,
        },
    );
    create_properties.insert(
        "context".to_string(),
        JsonSchema::String {
            description: Some("Optional background context".to_string()),
            allowed_values: None,
        },
    );
    create_properties.insert(
        "models".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: if allowed_models.is_empty() {
                    None
                } else {
                    Some(allowed_models.to_vec())
                },
            }),
            description: Some(
                "Optional array of model names (e.g., ['code-gpt-5.2','claude-sonnet-4.5','code-gpt-5.2-codex','gemini-3-flash'])".to_string(),
            ),
        },
    );
    create_properties.insert(
        "files".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some("Optional array of file paths to include in context".to_string()),
        },
    );
    create_properties.insert(
        "output".to_string(),
        JsonSchema::String {
            description: Some("Optional desired output description".to_string()),
            allowed_values: None,
        },
    );
    create_properties.insert(
        "write".to_string(),
        JsonSchema::Boolean {
            description: Some(
                "Enable isolated write worktrees for each agent (default: true). Set false to keep the agent read-only.".to_string(),
            ),
        },
    );
    create_properties.insert(
        "read_only".to_string(),
        JsonSchema::Boolean {
            description: Some(
                "Deprecated: inverse of `write`. Prefer setting `write` instead.".to_string(),
            ),
        },
    );
    properties.insert(
        "create".to_string(),
        JsonSchema::Object {
            properties: create_properties,
            required: Some(vec!["task".to_string()]),
            additional_properties: Some(false.into()),
        },
    );

    let mut status_properties = BTreeMap::new();
    status_properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some("Agent identifier to inspect".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "status".to_string(),
        JsonSchema::Object {
            properties: status_properties,
            required: Some(vec!["agent_id".to_string()]),
            additional_properties: Some(false.into()),
        },
    );

    let mut result_properties = BTreeMap::new();
    result_properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some("Agent identifier whose result should be fetched".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "result".to_string(),
        JsonSchema::Object {
            properties: result_properties,
            required: Some(vec!["agent_id".to_string()]),
            additional_properties: Some(false.into()),
        },
    );

    let mut cancel_properties = BTreeMap::new();
    cancel_properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some("Cancel a specific agent".to_string()),
            allowed_values: None,
        },
    );
    cancel_properties.insert(
        "batch_id".to_string(),
        JsonSchema::String {
            description: Some("Cancel all agents in the batch".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "cancel".to_string(),
        JsonSchema::Object {
            properties: cancel_properties,
            required: Some(Vec::new()),
            additional_properties: Some(false.into()),
        },
    );

    let mut wait_properties = BTreeMap::new();
    wait_properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some("Wait for a specific agent".to_string()),
            allowed_values: None,
        },
    );
    wait_properties.insert(
        "batch_id".to_string(),
        JsonSchema::String {
            description: Some("Wait for any agent in the batch".to_string()),
            allowed_values: None,
        },
    );
    wait_properties.insert(
        "timeout_seconds".to_string(),
        JsonSchema::Number {
            description: Some("Optional timeout before giving up (default 300, max 600)".to_string()),
        },
    );
    wait_properties.insert(
        "return_all".to_string(),
        JsonSchema::Boolean {
            description: Some(
                "When waiting on a batch, return all completed agents instead of the first"
                    .to_string(),
            ),
        },
    );
    properties.insert(
        "wait".to_string(),
        JsonSchema::Object {
            properties: wait_properties,
            required: Some(Vec::new()),
            additional_properties: Some(false.into()),
        },
    );

    let mut list_properties = BTreeMap::new();
    list_properties.insert(
        "status_filter".to_string(),
        JsonSchema::String {
            description: Some(
                "Optional status filter (pending, running, completed, failed, cancelled)".to_string(),
            ),
            allowed_values: None,
        },
    );
    list_properties.insert(
        "batch_id".to_string(),
        JsonSchema::String {
            description: Some("Limit results to a batch".to_string()),
            allowed_values: None,
        },
    );
    list_properties.insert(
        "recent_only".to_string(),
        JsonSchema::Boolean {
            description: Some("When true, only include agents from the last two hours".to_string()),
        },
    );
    properties.insert(
        "list".to_string(),
        JsonSchema::Object {
            properties: list_properties,
            required: Some(Vec::new()),
            additional_properties: Some(false.into()),
        },
    );

    let required = Some(vec!["action".to_string()]);

    OpenAiTool::Function(ResponsesApiTool {
        name: "agent".to_string(),
        description:
            "Unified agent manager for launching, monitoring, and collecting results from asynchronous agents.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required,
            additional_properties: Some(false.into()),
        },
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunAgentParams {
    pub task: String,
    #[serde(default, deserialize_with = "deserialize_models_field")]
    pub models: Vec<String>,
    pub context: Option<String>,
    pub output: Option<String>,
    pub files: Option<Vec<String>>,
    #[serde(default)]
    pub write: Option<bool>,
    #[serde(default)]
    pub read_only: Option<bool>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCreateOptions {
    pub task: Option<String>,
    #[serde(default, deserialize_with = "deserialize_models_field")]
    pub models: Vec<String>,
    pub context: Option<String>,
    pub output: Option<String>,
    pub files: Option<Vec<String>>,
    #[serde(default)]
    pub write: Option<bool>,
    #[serde(default)]
    pub read_only: Option<bool>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentifierOptions {
    pub agent_id: Option<String>,
    pub batch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCancelOptions {
    pub agent_id: Option<String>,
    pub batch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentWaitOptions {
    pub agent_id: Option<String>,
    pub batch_id: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub return_all: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListOptions {
    pub status_filter: Option<String>,
    pub batch_id: Option<String>,
    pub recent_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolRequest {
    pub action: String,
    pub create: Option<AgentCreateOptions>,
    pub status: Option<AgentIdentifierOptions>,
    pub result: Option<AgentIdentifierOptions>,
    pub cancel: Option<AgentCancelOptions>,
    pub wait: Option<AgentWaitOptions>,
    pub list: Option<AgentListOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckAgentStatusParams {
    pub agent_id: String,
    pub batch_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAgentResultParams {
    pub agent_id: String,
    pub batch_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelAgentParams {
    pub agent_id: Option<String>,
    pub batch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitForAgentParams {
    pub agent_id: Option<String>,
    pub batch_id: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub return_all: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAgentsParams {
    pub status_filter: Option<String>,
    pub batch_id: Option<String>,
    pub recent_only: Option<bool>,
}

fn deserialize_models_field<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ModelsInput {
        Seq(Vec<String>),
        One(String),
    }

    let parsed = Option::<ModelsInput>::deserialize(deserializer)?;
    Ok(match parsed {
        Some(ModelsInput::Seq(seq)) => seq,
        Some(ModelsInput::One(single)) => vec![single],
        None => Vec::new(),
    })
}
