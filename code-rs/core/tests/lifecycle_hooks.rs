#![allow(clippy::unwrap_used)]

mod common;

use common::{
    load_default_config_for_test,
    load_sse_fixture_with_id,
    mount_sse_once,
    wait_for_event,
};

use code_core::built_in_model_providers;
use code_core::config_types::{ProjectHookConfig, ProjectHookEvent};
use code_core::project_features::ProjectHooks;
use code_core::protocol::{AskForApproval, EventMsg, InputItem, Op, SandboxPolicy};
use code_core::{CodexAuth, ConversationManager, ModelProviderInfo};
use serde_json::json;
use std::fs::File;
use tempfile::TempDir;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sse_response(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(body)
}

fn find_message_index(
    input: &[serde_json::Value],
    role: &str,
    needle: &str,
) -> Option<usize> {
    input.iter().enumerate().find_map(|(idx, item)| {
        if item.get("type").and_then(|v| v.as_str()) != Some("message") {
            return None;
        }
        if item.get("role").and_then(|v| v.as_str()) != Some(role) {
            return None;
        }
        let content = item.get("content").and_then(|v| v.as_array())?;
        let matches = content.iter().any(|content_item| {
            content_item.get("type").and_then(|v| v.as_str()) == Some("input_text")
                && content_item
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|text| text.contains(needle))
                    .unwrap_or(false)
        });
        matches.then_some(idx)
    })
}

#[cfg(not(windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_hooks_session_start_injects_context_before_user_prompt() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();

    std::fs::write(
        code_home.path().join("hooks.json"),
        r#"
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          { "type": "command", "command": "echo session-start-context" }
        ]
      }
    ]
  }
}
"#,
    )
    .unwrap();

    let server = MockServer::start().await;
    let sse = load_sse_fixture_with_id("tests/fixtures/completed_template.json", "resp-1");
    let resp_mock = mount_sse_once(&server, sse).await;

    let mut config = load_default_config_for_test(&code_home);
    config.cwd = project_dir.path().to_path_buf();
    config.approval_policy = AskForApproval::Never;
    config.sandbox_policy = SandboxPolicy::DangerFullAccess;
    config.model_provider = ModelProviderInfo {
        base_url: Some(format!("{}/v1", server.uri())),
        ..built_in_model_providers()["openai"].clone()
    };
    config.model = "gpt-5.1-codex".to_string();

    let conversation_manager = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "hello".into(),
            }],
            final_output_json_schema: None,
        })
        .await
        .unwrap();

    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;

    let body = resp_mock.single_body_json();
    let input = body["input"].as_array().expect("responses request should include input");

    let injected_idx =
        find_message_index(input, "developer", "session-start-context").expect("missing injected context");
    let user_idx = find_message_index(input, "user", "hello").expect("missing user input");
    assert!(
        injected_idx < user_idx,
        "expected session-start injected context before user prompt"
    );
}

#[cfg(not(windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_hooks_user_prompt_submit_blocking_prevents_model_call_and_prompt_recording() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();

    std::fs::write(
        code_home.path().join("hooks.json"),
        r#"
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "hooks": [
          { "type": "command", "command": "if grep -q blockme; then echo blocked-by-hook 1>&2; exit 2; fi; exit 0" }
        ]
      }
    ]
  }
}
"#,
    )
    .unwrap();

    let server = MockServer::start().await;
    let sse = load_sse_fixture_with_id("tests/fixtures/completed_template.json", "resp-1");
    let resp_mock = mount_sse_once(&server, sse).await;

    let mut config = load_default_config_for_test(&code_home);
    config.cwd = project_dir.path().to_path_buf();
    config.approval_policy = AskForApproval::Never;
    config.sandbox_policy = SandboxPolicy::DangerFullAccess;
    config.model_provider = ModelProviderInfo {
        base_url: Some(format!("{}/v1", server.uri())),
        ..built_in_model_providers()["openai"].clone()
    };
    config.model = "gpt-5.1-codex".to_string();

    let conversation_manager = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "blockme".into(),
            }],
            final_output_json_schema: None,
        })
        .await
        .unwrap();
    let warning = wait_for_event(&codex, |ev| matches!(ev, EventMsg::Warning(_))).await;
    match warning {
        EventMsg::Warning(ev) => assert!(
            ev.message.contains("input blocked by hooks.json lifecycle hook"),
            "expected hook block warning, got: {}",
            ev.message
        ),
        other => panic!("expected warning event, got {other:?}"),
    }
    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 0, "blocked prompt should not invoke the model");

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "allowed".into(),
            }],
            final_output_json_schema: None,
        })
        .await
        .unwrap();
    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "expected one model request after allowed prompt");

    let body = resp_mock.single_body_json();
    let input = body["input"].as_array().expect("responses request should include input");
    assert!(
        find_message_index(input, "user", "allowed").is_some(),
        "expected allowed prompt in model request input"
    );
    assert!(
        find_message_index(input, "user", "blockme").is_none(),
        "blocked prompt should not be recorded into subsequent requests"
    );
}

#[cfg(not(windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_hooks_pre_tool_use_blocking_prevents_project_tool_before_hooks() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("hooks.log");
    File::create(&log_path).unwrap();

    std::fs::write(
        code_home.path().join("hooks.json"),
        r#"
{
  "hooks": {
    "PreToolUse": [
      {
        "hooks": [
          { "type": "command", "command": "echo blocked-by-pre-tool-use 1>&2; exit 2" }
        ]
      }
    ]
  }
}
"#,
    )
    .unwrap();

    let mut config = load_default_config_for_test(&code_home);
    config.cwd = project_dir.path().to_path_buf();
    config.approval_policy = AskForApproval::Never;
    config.sandbox_policy = SandboxPolicy::DangerFullAccess;

    let hook_cmd = |label: &str| {
        vec![
            "bash".to_string(),
            "-lc".to_string(),
            format!("echo {label}:${{CODE_HOOK_EVENT}} >> {}", log_path.display()),
        ]
    };
    let hook_configs = vec![ProjectHookConfig {
        event: ProjectHookEvent::ToolBefore,
        name: Some("before".to_string()),
        command: hook_cmd("before"),
        cwd: None,
        env: None,
        timeout_ms: None,
        run_in_background: Some(false),
    }];
    config.project_hooks = ProjectHooks::from_configs(&hook_configs, &config.cwd);

    let server = MockServer::start().await;

    let exec_body = format!("echo exec-body >> {}", log_path.display());
    let function_call_args = json!({
        "command": ["bash", "-lc", exec_body],
        "workdir": config.cwd,
        "timeout_ms": null,
        "sandbox_permissions": null,
        "justification": null,
    });
    let function_call_item = json!({
        "type": "response.output_item.done",
        "item": {
            "type": "function_call",
            "id": "call-1",
            "call_id": "call-1",
            "name": "shell",
            "arguments": function_call_args.to_string(),
        }
    });
    let completed_one = json!({
        "type": "response.completed",
        "response": {
            "id": "resp-1",
            "usage": {
                "input_tokens": 0,
                "input_tokens_details": null,
                "output_tokens": 0,
                "output_tokens_details": null,
                "total_tokens": 0
            }
        }
    });
    let body_one = format!(
        "event: response.output_item.done\ndata: {function_call_item}\n\n\
event: response.completed\ndata: {completed_one}\n\n"
    );

    let completed_two = json!({
        "type": "response.completed",
        "response": {
            "id": "resp-2",
            "usage": {
                "input_tokens": 0,
                "input_tokens_details": null,
                "output_tokens": 0,
                "output_tokens_details": null,
                "total_tokens": 0
            }
        }
    });
    let body_two = format!(
        "event: response.completed\ndata: {completed_two}\n\n"
    );

    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(body_one))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(body_two))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    config.model_provider = ModelProviderInfo {
        base_url: Some(format!("{}/v1", server.uri())),
        ..built_in_model_providers()["openai"].clone()
    };
    config.model = "gpt-5.1-codex".to_string();

    let conversation_manager =
        ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "trigger tool".into(),
            }],
            final_output_json_schema: None,
        })
        .await
        .unwrap();
    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;

    let log_contents = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        log_contents.trim().is_empty(),
        "expected tool.before hooks and exec command to be skipped when pre-tool-use blocks, got:\n{log_contents}"
    );
}

#[cfg(not(windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_hooks_stop_hook_blocking_injects_continuation_prompt_and_retries_turn() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();

    std::fs::write(
        code_home.path().join("hooks.json"),
        r#"
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "if grep -q '\"stop_hook_active\":true'; then exit 0; else echo stop-hook-continue 1>&2; exit 2; fi"
          }
        ]
      }
    ]
  }
}
"#,
    )
    .unwrap();

    let server = MockServer::start().await;
    let sse_one = load_sse_fixture_with_id("tests/fixtures/completed_template.json", "resp-1");
    let sse_two = load_sse_fixture_with_id("tests/fixtures/completed_template.json", "resp-2");

    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(sse_one))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(sse_two))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let mut config = load_default_config_for_test(&code_home);
    config.cwd = project_dir.path().to_path_buf();
    config.approval_policy = AskForApproval::Never;
    config.sandbox_policy = SandboxPolicy::DangerFullAccess;
    config.model_provider = ModelProviderInfo {
        base_url: Some(format!("{}/v1", server.uri())),
        ..built_in_model_providers()["openai"].clone()
    };
    config.model = "gpt-5.1-codex".to_string();

    let conversation_manager =
        ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "hello".into(),
            }],
            final_output_json_schema: None,
        })
        .await
        .unwrap();

    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;

    let requests = server.received_requests().await.unwrap();
    let responses_requests = requests
        .iter()
        .filter(|req| req.url.path().ends_with("/responses"))
        .collect::<Vec<_>>();
    assert_eq!(
        responses_requests.len(),
        2,
        "expected two model requests (stop hook continuation), got {}",
        responses_requests.len()
    );

    let first_body: serde_json::Value = responses_requests[0].body_json().unwrap();
    let first_input = first_body["input"]
        .as_array()
        .expect("responses request should include input");
    assert!(
        find_message_index(first_input, "user", "hello").is_some(),
        "expected initial user prompt in first request input"
    );

    let second_body: serde_json::Value = responses_requests[1].body_json().unwrap();
    let second_input = second_body["input"]
        .as_array()
        .expect("responses request should include input");
    assert!(
        find_message_index(second_input, "user", "stop-hook-continue").is_some(),
        "expected stop-hook continuation prompt in second request input"
    );
}
