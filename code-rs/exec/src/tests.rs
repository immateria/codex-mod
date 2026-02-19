use super::*;
use crate::auto_runtime::shutdown_state_after_request;
use crate::auto_runtime::capture_auto_resolve_snapshot;
use crate::prompt_input::PromptDecodeError;
use crate::prompt_input::decode_prompt_bytes;
use crate::review_command::build_review_request;
use crate::review_scope::head_is_ancestor_of_base;
use crate::review_scope::should_skip_followup;
use crate::review_scope::strip_scope_from_prompt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use code_core::config::{ConfigOverrides, ConfigToml};
use code_protocol::models::{ContentItem, ResponseItem};
use code_protocol::ThreadId;
use code_protocol::protocol::{
    EventMsg as ProtoEventMsg, RecordedEvent, RolloutItem, RolloutLine, SessionMeta, SessionMetaLine,
    SessionSource, UserMessageEvent,
};
use filetime::{set_file_mtime, FileTime};
use tempfile::TempDir;
use uuid::Uuid;

#[test]
fn build_review_request_uncommitted() {
    let request = build_review_request(crate::cli::ReviewArgs {
        uncommitted: true,
        base: None,
        commit: None,
        commit_title: None,
        prompt: None,
    })
    .expect("build review request");
    assert!(matches!(
        request.target,
        code_protocol::protocol::ReviewTarget::UncommittedChanges
    ));
    assert!(request.prompt.contains("workspace changes"));
}

#[test]
fn build_review_request_commit_title() {
    let request = build_review_request(crate::cli::ReviewArgs {
        uncommitted: false,
        base: None,
        commit: Some("abc123".to_string()),
        commit_title: Some("Fix race condition".to_string()),
        prompt: None,
    })
    .expect("build review request");
    assert!(matches!(
        request.target,
        code_protocol::protocol::ReviewTarget::Commit { .. }
    ));
    assert!(request.prompt.contains("abc123"));
    assert!(request.prompt.contains("Fix race condition"));
}

#[test]
fn shutdown_state_schedules_grace_on_first_request() {
    let now = Instant::now();
    let (attempt_send, pending, deadline) =
        shutdown_state_after_request(false, false, None, now, true);
    assert!(!attempt_send);
    assert!(pending);
    assert!(deadline.expect("deadline").gt(&now));
}

#[test]
fn shutdown_state_waits_until_deadline() {
    let now = Instant::now();
    let future_deadline = now + tokio::time::Duration::from_millis(100);
    let (attempt_send, pending, deadline) =
        shutdown_state_after_request(false, true, Some(future_deadline), now, true);
    assert!(!attempt_send);
    assert!(pending);
    assert_eq!(deadline, Some(future_deadline));
}

#[test]
fn shutdown_state_attempts_send_after_grace_elapses() {
    let now = Instant::now();
    let expired_deadline = now - tokio::time::Duration::from_millis(1);
    let (attempt_send, pending, deadline) =
        shutdown_state_after_request(false, true, Some(expired_deadline), now, true);
    assert!(attempt_send);
    assert!(pending);
    assert!(deadline.is_none());
}

#[test]
fn shutdown_state_sends_immediately_without_grace() {
    let now = Instant::now();
    let (attempt_send, pending, deadline) = shutdown_state_after_request(false, false, None, now, false);
    assert!(attempt_send);
    assert!(pending);
    assert!(deadline.is_none());
}

#[test]
fn decode_prompt_bytes_strips_utf8_bom() {
    let input = [0xEF, 0xBB, 0xBF, b'h', b'i', b'\n'];
    let output = decode_prompt_bytes(&input);
    assert_eq!(output, Ok("hi\n".to_string()));
}

#[test]
fn decode_prompt_bytes_decodes_utf16le_bom() {
    let input = [0xFF, 0xFE, b'h', 0x00, b'i', 0x00, b'\n', 0x00];
    let output = decode_prompt_bytes(&input);
    assert_eq!(output, Ok("hi\n".to_string()));
}

#[test]
fn decode_prompt_bytes_decodes_utf16be_bom() {
    let input = [0xFE, 0xFF, 0x00, b'h', 0x00, b'i', 0x00, b'\n'];
    let output = decode_prompt_bytes(&input);
    assert_eq!(output, Ok("hi\n".to_string()));
}

#[test]
fn decode_prompt_bytes_rejects_utf32le_bom() {
    let input = [
        0xFF, 0xFE, 0x00, 0x00, b'h', 0x00, 0x00, 0x00, b'i', 0x00, 0x00, 0x00, b'\n', 0x00,
        0x00, 0x00,
    ];
    let output = decode_prompt_bytes(&input);
    assert_eq!(
        output,
        Err(PromptDecodeError::UnsupportedBom {
            encoding: "UTF-32LE",
        })
    );
}

#[test]
fn decode_prompt_bytes_rejects_utf32be_bom() {
    let input = [
        0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, b'h', 0x00, 0x00, 0x00, b'i', 0x00, 0x00,
        0x00, b'\n',
    ];
    let output = decode_prompt_bytes(&input);
    assert_eq!(
        output,
        Err(PromptDecodeError::UnsupportedBom {
            encoding: "UTF-32BE",
        })
    );
}

#[test]
fn decode_prompt_bytes_rejects_invalid_utf8() {
    let input = [0xC3, 0x28];
    let output = decode_prompt_bytes(&input);
    assert_eq!(output, Err(PromptDecodeError::InvalidUtf8 { valid_up_to: 0 }));
}

#[test]
fn write_review_json_includes_snapshot() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("out.json");

    let output = code_core::protocol::ReviewOutputEvent {
        findings: vec![code_core::protocol::ReviewFinding {
            title: "bug".into(),
            body: "details".into(),
            confidence_score: 0.5,
            priority: 1,
            code_location: code_core::protocol::ReviewCodeLocation {
                absolute_file_path: PathBuf::from("src/lib.rs"),
                line_range: code_core::protocol::ReviewLineRange { start: 1, end: 2 },
            },
        }],
        overall_correctness: "incorrect".into(),
        overall_explanation: "needs fixes".into(),
        overall_confidence_score: 0.7,
    };

    let snapshot = code_core::protocol::ReviewSnapshotInfo {
        snapshot_commit: Some("abc123".into()),
        branch: Some("auto-review-branch".into()),
        worktree_path: Some(PathBuf::from("/tmp/wt")),
        repo_root: Some(PathBuf::from("/tmp/repo")),
    };

    write_review_json(path.clone(), &[output], Some(&snapshot)).unwrap();

    let content = std::fs::read_to_string(path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(v["branch"], "auto-review-branch");
    assert_eq!(v["snapshot_commit"], "abc123");
    assert_eq!(v["worktree_path"], "/tmp/wt");
    assert_eq!(v["findings"].as_array().unwrap().len(), 1);
    let runs = v["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["index"], 1);
}

#[test]
fn write_review_json_keeps_all_runs() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("multi.json");

    let first = code_core::protocol::ReviewOutputEvent {
        findings: vec![code_core::protocol::ReviewFinding {
            title: "bug".into(),
            body: "details".into(),
            confidence_score: 0.6,
            priority: 1,
            code_location: code_core::protocol::ReviewCodeLocation {
                absolute_file_path: PathBuf::from("src/lib.rs"),
                line_range: code_core::protocol::ReviewLineRange { start: 1, end: 2 },
            },
        }],
        overall_correctness: "incorrect".into(),
        overall_explanation: "needs fixes".into(),
        overall_confidence_score: 0.7,
    };

    let second = code_core::protocol::ReviewOutputEvent {
        findings: Vec::new(),
        overall_correctness: "correct".into(),
        overall_explanation: "clean".into(),
        overall_confidence_score: 0.9,
    };

    write_review_json(path.clone(), &[first, second], None).unwrap();

    let content = std::fs::read_to_string(path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(v["overall_explanation"], "clean"); // latest run is flattened
    let runs = v["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0]["index"], 1);
    assert_eq!(runs[0]["findings"].as_array().unwrap().len(), 1);
    assert_eq!(runs[1]["index"], 2);
    assert_eq!(runs[1]["findings"].as_array().unwrap().len(), 0);
}

#[test]
fn strip_scope_removes_previous_commit_scope() {
    let prompt = format!(
        "Please review.\nReview scope: commit abc123 (parent deadbeef)\nMore text\n\n{}",
        code_auto_drive_core::AUTO_RESOLVE_REVIEW_FOLLOWUP
    );
    let cleaned = strip_scope_from_prompt(&prompt);
    assert!(!cleaned.contains("abc123"));
    assert!(!cleaned.contains("Review scope"));
    assert!(!cleaned.contains(code_auto_drive_core::AUTO_RESOLVE_REVIEW_FOLLOWUP));
    assert!(cleaned.contains("Please review."));
}

#[test]
fn should_skip_followup_detects_duplicate_snapshot() {
    let temp = TempDir::new().unwrap();
    std::process::Command::new("git")
        .current_dir(temp.path())
        .args(["init"])
        .output()
        .unwrap();
    std::fs::write(temp.path().join("foo.txt"), "hello").unwrap();
    std::process::Command::new("git")
        .current_dir(temp.path())
        .args(["add", "."])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "-m", "init"])
        .output()
        .unwrap();

    let base = capture_auto_resolve_snapshot(temp.path(), None, "base").expect("base snapshot");
    let snap = capture_auto_resolve_snapshot(temp.path(), Some(base.id()), "dup").expect("child");

    assert!(should_skip_followup(Some(snap.id()), &snap));
    assert!(!should_skip_followup(Some("different"), &snap));
    assert!(!should_skip_followup(None, &snap));
}

#[test]
fn base_ancestor_check_matches_git_history() {
    let temp = TempDir::new().unwrap();
    let run_git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(temp.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };

    run_git(["init"].as_slice());
    run_git(["config", "user.email", "codex@example.com"].as_slice());
    run_git(["config", "user.name", "Codex Tester"].as_slice());
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    run_git(["add", "."].as_slice());
    run_git(["commit", "-m", "c1"].as_slice());

    // second commit (represents a snapshot captured off the current HEAD)
    std::fs::write(temp.path().join("a.txt"), "b").unwrap();
    run_git(["commit", "-am", "c2"].as_slice());
    let base = String::from_utf8_lossy(
        &run_git(["rev-parse", "HEAD"].as_slice()).stdout,
    )
    .trim()
    .to_string();

    assert!(head_is_ancestor_of_base(temp.path(), &base));

    // move HEAD back to check false case
    run_git(["checkout", "HEAD~1"].as_slice());
    assert!(!head_is_ancestor_of_base(temp.path(), "deadbeef"));
}

fn test_config(code_home: &Path) -> Config {
    let mut overrides = ConfigOverrides::default();
    let workspace = code_home.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    overrides.cwd = Some(workspace);
    Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        overrides,
        code_home.to_path_buf(),
    )
    .unwrap()
}

#[test]
fn auto_drive_exec_config_uses_auto_drive_reasoning_effort() {
    let temp = TempDir::new().unwrap();
    let mut config = test_config(temp.path());
    config.model_reasoning_effort = code_core::config_types::ReasoningEffort::Low;
    config.auto_drive.model = "gpt-5.2".to_string();
    config.auto_drive.model_reasoning_effort = code_core::config_types::ReasoningEffort::XHigh;

    let auto_config = build_auto_drive_exec_config(&config);
    assert_eq!(auto_config.model, "gpt-5.2");
    assert_eq!(
        auto_config.model_reasoning_effort,
        code_core::config_types::ReasoningEffort::XHigh
    );
}

fn write_rollout(
    code_home: &Path,
    cwd: &Path,
    session_id: Uuid,
    created_at: &str,
    last_event_at: &str,
    source: SessionSource,
    message: &str,
) -> PathBuf {
    let sessions_dir = code_home.join("sessions").join("2025").join("11").join("16");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let filename = format!(
        "rollout-{}-{}.jsonl",
        created_at.replace(':', "-"),
        session_id
    );
    let path = sessions_dir.join(filename);

    let session_meta = SessionMeta {
        id: ThreadId::from_string(&session_id.to_string()).unwrap(),
        timestamp: created_at.to_string(),
        cwd: cwd.to_path_buf(),
        originator: "test".to_string(),
        cli_version: "0.0.0-test".to_string(),
        source,
        model_provider: None,
        base_instructions: None,
        dynamic_tools: None,
        forked_from_id: None,
    };

    let session_line = RolloutLine {
        timestamp: created_at.to_string(),
        item: RolloutItem::SessionMeta(SessionMetaLine {
            meta: session_meta,
            git: None,
        }),
    };
    let event_line = RolloutLine {
        timestamp: last_event_at.to_string(),
        item: RolloutItem::Event(RecordedEvent {
            id: "event-0".to_string(),
            event_seq: 0,
            order: None,
            msg: ProtoEventMsg::UserMessage(UserMessageEvent {
                message: message.to_string(),
                images: None,
                local_images: vec![],
                text_elements: vec![],
            }),
        }),
    };
    let user_line = RolloutLine {
        timestamp: last_event_at.to_string(),
        item: RolloutItem::ResponseItem(ResponseItem::Message {
            id: Some(format!("user-{session_id}")),
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: message.to_string(),
            }],
            end_turn: None,
            phase: None,
        }),
    };

    let assistant_line = RolloutLine {
        timestamp: last_event_at.to_string(),
        item: RolloutItem::ResponseItem(ResponseItem::Message {
            id: Some(format!("msg-{session_id}")),
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: format!("Ack: {message}"),
            }],
            end_turn: None,
            phase: None,
        }),
    };

    let mut writer = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    serde_json::to_writer(&mut writer, &session_line).unwrap();
    writer.write_all(b"\n").unwrap();
    serde_json::to_writer(&mut writer, &event_line).unwrap();
    writer.write_all(b"\n").unwrap();
    serde_json::to_writer(&mut writer, &user_line).unwrap();
    writer.write_all(b"\n").unwrap();
    serde_json::to_writer(&mut writer, &assistant_line).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();

    path
}

#[tokio::test]
async fn exec_resolve_last_prefers_latest_timestamp() {
    let temp = TempDir::new().unwrap();
    let config = test_config(temp.path());
    let older = Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap();
    let newer = Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap();

    write_rollout(
        temp.path(),
        config.cwd.as_path(),
        older,
        "2025-11-10T09:00:00Z",
        "2025-11-10T09:05:00Z",
        SessionSource::Cli,
        "older",
    );
    write_rollout(
        temp.path(),
        config.cwd.as_path(),
        newer,
        "2025-11-16T09:00:00Z",
        "2025-11-16T09:10:00Z",
        SessionSource::Exec,
        "newer",
    );

    let args = crate::cli::ResumeArgs {
        session_id: None,
        last: true,
        all: false,
        images: vec![],
        prompt: None,
    };
    let path = resolve_resume_path(&config, &args)
        .await
        .unwrap()
        .expect("path");
    let path_str = path.to_string_lossy();
    assert!(
        path_str.contains("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
        "resolved path should reference newer session, got {path_str}"
    );
}

#[tokio::test]
async fn exec_resolve_by_id_uses_catalog_bootstrap() {
    let temp = TempDir::new().unwrap();
    let config = test_config(temp.path());
    let session_id = Uuid::parse_str("cccccccc-cccc-4ccc-8ccc-cccccccccccc").unwrap();
    write_rollout(
        temp.path(),
        config.cwd.as_path(),
        session_id,
        "2025-11-12T09:00:00Z",
        "2025-11-12T09:05:00Z",
        SessionSource::Cli,
        "resume",
    );

    let args = crate::cli::ResumeArgs {
        session_id: Some("cccccccc".to_string()),
        last: false,
        all: false,
        images: vec![],
        prompt: None,
    };

    let path = resolve_resume_path(&config, &args)
        .await
        .unwrap()
        .expect("path");
    let path_str = path.to_string_lossy();
    assert!(
        path_str.contains("cccccccc-cccc-4ccc-8ccc-cccccccccccc"),
        "resolved path should match requested session, got {path_str}"
    );
}

#[tokio::test]
async fn exec_resolve_last_ignores_mtime_drift() {
    let temp = TempDir::new().unwrap();
    let config = test_config(temp.path());
    let older = Uuid::parse_str("dddddddd-dddd-4ddd-8ddd-dddddddddddd").unwrap();
    let newer = Uuid::parse_str("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee").unwrap();

    let older_path = write_rollout(
        temp.path(),
        config.cwd.as_path(),
        older,
        "2025-11-01T09:00:00Z",
        "2025-11-01T09:05:00Z",
        SessionSource::Cli,
        "old",
    );
    let newer_path = write_rollout(
        temp.path(),
        config.cwd.as_path(),
        newer,
        "2025-11-20T09:00:00Z",
        "2025-11-20T09:05:00Z",
        SessionSource::Exec,
        "new",
    );

    let base = SystemTime::now();
    set_file_mtime(&older_path, FileTime::from_system_time(base + Duration::from_secs(500))).unwrap();
    set_file_mtime(&newer_path, FileTime::from_system_time(base + Duration::from_secs(10))).unwrap();

    let args = crate::cli::ResumeArgs {
        session_id: None,
        last: true,
        all: false,
        images: vec![],
        prompt: None,
    };
    let path = resolve_resume_path(&config, &args)
        .await
        .unwrap()
        .expect("path");
    let path_str = path.to_string_lossy();
    assert!(
        path_str.contains("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"),
        "resolved path should ignore mtime drift, got {path_str}"
    );
}

#[tokio::test]
async fn exec_resolve_last_honors_all_flag_for_cwd_filtering() {
    let temp = TempDir::new().unwrap();
    let config = test_config(temp.path());
    let current_cwd_session = Uuid::parse_str("abababab-abab-4aba-8aba-abababababab").unwrap();
    let other_cwd_session = Uuid::parse_str("cdcdcdcd-cdcd-4cdc-8cdc-cdcdcdcdcdcd").unwrap();
    let other_cwd = temp.path().join("other-workspace");
    std::fs::create_dir_all(&other_cwd).unwrap();

    write_rollout(
        temp.path(),
        config.cwd.as_path(),
        current_cwd_session,
        "2025-11-10T09:00:00Z",
        "2025-11-10T09:05:00Z",
        SessionSource::Exec,
        "current",
    );
    write_rollout(
        temp.path(),
        other_cwd.as_path(),
        other_cwd_session,
        "2025-11-20T09:00:00Z",
        "2025-11-20T09:05:00Z",
        SessionSource::Exec,
        "other",
    );

    let cwd_filtered = resolve_resume_path(
        &config,
        &crate::cli::ResumeArgs {
            session_id: None,
            last: true,
            all: false,
            images: vec![],
            prompt: None,
        },
    )
    .await
    .unwrap()
    .expect("path");
    assert!(
        cwd_filtered
            .to_string_lossy()
            .contains("abababab-abab-4aba-8aba-abababababab"),
        "expected cwd-filtered resume to pick current workspace session, got {}",
        cwd_filtered.display()
    );

    let all_sessions = resolve_resume_path(
        &config,
        &crate::cli::ResumeArgs {
            session_id: None,
            last: true,
            all: true,
            images: vec![],
            prompt: None,
        },
    )
    .await
    .unwrap()
    .expect("path");
    assert!(
        all_sessions
            .to_string_lossy()
            .contains("cdcdcdcd-cdcd-4cdc-8cdc-cdcdcdcdcdcd"),
        "expected --all resume to ignore cwd filter and pick newest session, got {}",
        all_sessions.display()
    );
}
