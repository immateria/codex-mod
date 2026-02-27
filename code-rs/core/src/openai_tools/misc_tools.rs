use std::collections::BTreeMap;

use super::json_schema::JsonSchema;
use super::types::{OpenAiTool, ResponsesApiTool};

// ——————————————————————————————————————————————————————————————
// Background waiting tool (for long-running shell calls)
// ——————————————————————————————————————————————————————————————

pub fn create_wait_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "call_id".to_string(),
        JsonSchema::String {
            description: Some("Background call_id to wait for.".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "timeout_ms".to_string(),
        JsonSchema::Number {
            description: Some(
                "Maximum time in milliseconds to wait (default 600000 = 10 minutes, max 3600000 = 60 minutes)."
                    .to_string(),
            ),
        },
    );
    OpenAiTool::Function(ResponsesApiTool {
        name: "wait".to_string(),
        description: "Wait for the background command identified by call_id to finish (optionally bounded by timeout_ms).".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["call_id".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub fn create_kill_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "call_id".to_string(),
        JsonSchema::String {
            description: Some("Background call_id to terminate.".to_string()),
            allowed_values: None,
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "kill".to_string(),
        description: "Terminate a running background command by call_id.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["call_id".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub fn create_gh_run_wait_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "run_id".to_string(),
        JsonSchema::String {
            description: Some("GitHub Actions run id to wait for.".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "repo".to_string(),
        JsonSchema::String {
            description: Some("Repository in OWNER/REPO form (optional).".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "workflow".to_string(),
        JsonSchema::String {
            description: Some(
                "Workflow name or filename (used to select latest run when run_id is omitted)."
                    .to_string(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "branch".to_string(),
        JsonSchema::String {
            description: Some(
                "Branch to filter when selecting latest run (default: current branch, falling back to main)."
                    .to_string(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "interval_seconds".to_string(),
        JsonSchema::Number {
            description: Some("Polling interval in seconds (default 8).".to_string()),
        },
    );
    OpenAiTool::Function(ResponsesApiTool {
        name: "gh_run_wait".to_string(),
        description: "Wait for a GitHub Actions run to finish, using gh run view polling. If run_id is omitted, selects the latest run for the workflow/branch; if both are omitted, selects the latest run on the current branch."
            .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: None,
            additional_properties: Some(false.into()),
        },
    })
}

pub fn create_bridge_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();

    properties.insert(
        "action".to_string(),
        JsonSchema::String {
            description: Some(
                "Required: subscribe (set level + persist), screenshot (request a screenshot), javascript (run JS on the bridge client)."
                    .to_string(),
            ),
            allowed_values: Some(vec![
                "subscribe".to_string(),
                "screenshot".to_string(),
                "javascript".to_string(),
            ]),
        },
    );

    properties.insert(
        "level".to_string(),
        JsonSchema::String {
            description: Some(
                "For action=subscribe: log level to receive (errors|warn|info|trace)."
                    .to_string(),
            ),
            allowed_values: Some(vec![
                "errors".to_string(),
                "warn".to_string(),
                "info".to_string(),
                "trace".to_string(),
            ]),
        },
    );

    properties.insert(
        "code".to_string(),
        JsonSchema::String {
            description: Some("For action=javascript: JS to execute on the bridge client.".to_string()),
            allowed_values: None,
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "code_bridge".to_string(),
        description:
            "Code Bridge = local Sentry-style event stream + two-way control (errors/console/pageviews/screenshots/control). Actions: subscribe (set level, persists, requests full capabilities), screenshot (ask bridges for a screenshot), javascript (send JS to execute and return result). Examples: {\"action\":\"subscribe\",\"level\":\"trace\"}, {\"action\":\"screenshot\"}, {\"action\":\"javascript\",\"code\":\"window.location.href\"}.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["action".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

