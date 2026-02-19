// In default human mode, stdout should contain only the final assistant message.
// In --json mode, stdout must be valid JSONL. All other output goes to stderr.
#![deny(clippy::print_stdout)]

mod cli;
mod auto_runtime;
mod auto_drive_session;
mod auto_review_status;
mod event_processor;
mod event_processor_with_human_output;
mod event_processor_with_json_output;
mod prompt_input;
mod review_command;
mod review_output;
mod review_scope;
mod run_setup;
mod session_runtime;
mod session_resume;
mod slash;

pub use cli::Cli;
pub use cli::Command;
pub use cli::ReviewArgs;
use code_core::AuthManager;
use code_core::BUILT_IN_OSS_MODEL_PROVIDER_ID;
use code_core::ConversationManager;
use code_core::NewConversation;
use code_core::config::set_default_originator;
use code_core::config::Config;
use code_core::config::ConfigOverrides;
use code_core::config_types::AutoDriveContinueMode;
use code_core::model_family::{derive_default_model_family, find_family_for_model};
use code_core::git_info::get_git_repo_root;
use code_core::protocol::AskForApproval;
use code_protocol::protocol::SessionSource;
use code_ollama::DEFAULT_OSS_MODEL;
use code_protocol::config_types::SandboxMode;
use event_processor_with_human_output::EventProcessorWithHumanOutput;
use event_processor_with_json_output::EventProcessorWithJsonOutput;
use std::path::PathBuf;
use supports_color::Stream;
use tokio::time::{Duration, Instant};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::auto_drive_session::build_auto_drive_exec_config;
use crate::auto_drive_session::run_auto_drive_session;
use crate::auto_runtime::merge_developer_message;
use crate::cli::Command as ExecCommand;
use crate::event_processor::EventProcessor;
use crate::prompt_input::load_output_schema;
use crate::review_output::write_review_json;
use crate::run_setup::PreparedRunInputs;
use crate::run_setup::prepare_run_inputs;
use crate::session_runtime::SessionRuntimeParams;
use crate::session_runtime::run_session_runtime;
use crate::session_resume::resolve_resume_path;
use crate::slash::{process_exec_slash_command, SlashContext, SlashDispatch};
use code_auto_drive_core::AutoResolveState;
use code_core::protocol::SandboxPolicy;
use code_core::timeboxed_exec_guidance::AUTO_EXEC_TIMEBOXED_CLI_GUIDANCE;

pub async fn run_main(cli: Cli, code_linux_sandbox_exe: Option<PathBuf>) -> anyhow::Result<()> {
    if let Err(err) = set_default_originator("code_exec") {
        tracing::warn!(?err, "Failed to set codex exec originator override {err:?}");
    }

    let Cli {
        command,
        images,
        model: model_cli_arg,
        oss,
        config_profile,
        full_auto,
        dangerously_bypass_approvals_and_sandbox,
        cwd,
        skip_git_repo_check,
        color,
        last_message_file,
        json: json_mode,
        sandbox_mode: sandbox_mode_cli_arg,
        prompt,
        output_schema: output_schema_path,
        include_plan_tool,
        config_overrides,
        auto_drive,
        auto_review,
        max_seconds,
        turn_cap,
        review_output_json,
        ..
    } = cli;

    let run_deadline = max_seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds));
    let run_deadline_std = run_deadline.map(tokio::time::Instant::into_std);

    let PreparedRunInputs {
        mut review_request,
        mut prompt_to_send,
        mut summary_prompt,
        auto_drive_goal,
        images,
        timeboxed_auto_exec,
    } = prepare_run_inputs(&command, prompt, images, auto_drive, max_seconds);

    let _output_schema = load_output_schema(output_schema_path);

    let (stdout_with_ansi, stderr_with_ansi) = match color {
        cli::Color::Always => (true, true),
        cli::Color::Never => (false, false),
        cli::Color::Auto => (
            supports_color::on_cached(Stream::Stdout).is_some(),
            supports_color::on_cached(Stream::Stderr).is_some(),
        ),
    };

    // Build fmt layer (existing logging) to compose with OTEL layer.
    let default_level = "error";

    // Build env_filter separately and attach via with_filter.
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(default_level))
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_ansi(stderr_with_ansi)
        .with_writer(std::io::stderr)
        .try_init();

    let sandbox_mode = if full_auto {
        Some(SandboxMode::WorkspaceWrite)
    } else if dangerously_bypass_approvals_and_sandbox {
        Some(SandboxMode::DangerFullAccess)
    } else {
        sandbox_mode_cli_arg.map(Into::<SandboxMode>::into)
    };

    // When using `--oss`, let the bootstrapper pick the model (defaulting to
    // gpt-oss:20b) and ensure it is present locally. Also, force the built‑in
    // `oss` model provider.
    let model = if let Some(model) = model_cli_arg {
        Some(model)
    } else if oss {
        Some(DEFAULT_OSS_MODEL.to_owned())
    } else {
        None // No model specified, will use the default.
    };

    let model_provider = if oss {
        Some(BUILT_IN_OSS_MODEL_PROVIDER_ID.to_string())
    } else {
        None // No specific model provider override.
    };

    // Load configuration and determine approval policy
    let overrides = ConfigOverrides {
        model,
        review_model: None,
        config_profile,
        // This CLI is intended to be headless and has no affordances for asking
        // the user for approval.
        approval_policy: Some(AskForApproval::Never),
        sandbox_mode,
        cwd: cwd.map(|p| p.canonicalize().unwrap_or(p)),
        model_provider,
        code_linux_sandbox_exe,
        base_instructions: None,
        include_plan_tool: Some(include_plan_tool),
        include_apply_patch_tool: None,
        include_view_image_tool: None,
        disable_response_storage: None,
        debug: None,
        show_raw_agent_reasoning: oss.then_some(true),
        tools_web_search_request: None,
        mcp_servers: None,
        experimental_client_tools: None,
        dynamic_tools: None,
        compact_prompt_override: None,
        compact_prompt_override_file: None,
    };
    // Parse `-c` overrides.
    let cli_kv_overrides = match config_overrides.parse_overrides() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing -c overrides: {e}");
            std::process::exit(1);
        }
    };

    let mut config = Config::load_with_cli_overrides(cli_kv_overrides, overrides)?;
    config.max_run_seconds = max_seconds;
    config.max_run_deadline = run_deadline_std;
    config.demo_developer_message = cli.demo_developer_message.clone();
    config.timeboxed_exec_mode = timeboxed_auto_exec;
    if timeboxed_auto_exec {
        config.demo_developer_message = merge_developer_message(
            config.demo_developer_message.take(),
            AUTO_EXEC_TIMEBOXED_CLI_GUIDANCE,
        );
    }
    if auto_drive_goal.is_some() {
        // Exec is non-interactive; don't burn time on countdown delays between Auto Drive turns.
        config.auto_drive.continue_mode = AutoDriveContinueMode::Immediate;
        if let Some(turn_cap) = turn_cap {
            config.auto_drive.coordinator_turn_cap = turn_cap;
        }
    }
    if review_request.is_none() {
        let slash_context = SlashContext {
            agents: &config.agents,
            subagent_commands: &config.subagent_commands,
        };

        match process_exec_slash_command(prompt_to_send.trim(), slash_context) {
            Ok(SlashDispatch::NotSlash) => {}
            Ok(SlashDispatch::ExpandedPrompt { prompt, summary }) => {
                prompt_to_send = prompt;
                if auto_drive_goal.is_none() {
                    summary_prompt = summary;
                }
            }
            Ok(SlashDispatch::Review { request, summary }) => {
                review_request = Some(request);
                if auto_drive_goal.is_none() {
                    summary_prompt = summary;
                }
            }
            Err(msg) => {
                eprintln!("{msg}");
                std::process::exit(1);
            }
        }
    }

    let is_auto_review = auto_review;

    if is_auto_review {
        if config.auto_review_use_chat_model {
            config.review_model = config.model.clone();
            config.review_model_reasoning_effort = config.model_reasoning_effort;
        } else {
            config.review_model = config.auto_review_model.clone();
            config.review_model_reasoning_effort = config.auto_review_model_reasoning_effort;
        }
        config.review_use_chat_model = config.auto_review_use_chat_model;

        if config.auto_review_resolve_use_chat_model {
            config.review_resolve_model = config.model.clone();
            config.review_resolve_model_reasoning_effort = config.model_reasoning_effort;
        } else {
            config.review_resolve_model = config.auto_review_resolve_model.clone();
            config.review_resolve_model_reasoning_effort =
                config.auto_review_resolve_model_reasoning_effort;
        }
        config.review_resolve_use_chat_model = config.auto_review_resolve_use_chat_model;
    }

    let review_auto_resolve_requested = review_request.is_some()
        && if is_auto_review {
            config.tui.auto_review_enabled
        } else {
            config.tui.review_auto_resolve
        };
    if review_auto_resolve_requested && matches!(config.sandbox_policy, SandboxPolicy::ReadOnly) {
        config.sandbox_policy = SandboxPolicy::new_workspace_write_policy();
        eprintln!(
            "Auto-resolve enabled for /review; upgrading sandbox to workspace-write so fixes can be applied."
        );
    }

    let max_auto_resolve_attempts: u32 = if is_auto_review {
        config.auto_drive.auto_review_followup_attempts.get()
    } else {
        config.auto_drive.auto_resolve_review_attempts.get()
    };
    let auto_resolve_state: Option<AutoResolveState> = review_request.as_ref().and_then(|req| {
        if review_auto_resolve_requested {
            Some(AutoResolveState::new_with_limit(
                req.target.clone(),
                req.prompt.clone(),
                req.user_facing_hint.clone().unwrap_or_default(),
                None,
                max_auto_resolve_attempts,
            ))
        } else {
            None
        }
    });
    let resolve_model_for_auto_resolve = if is_auto_review {
        if config.auto_review_resolve_use_chat_model {
            config.model.clone()
        } else {
            config.auto_review_resolve_model.clone()
        }
    } else if config.review_resolve_use_chat_model {
        config.model.clone()
    } else {
        config.review_resolve_model.clone()
    };
    let resolve_effort_for_auto_resolve = if is_auto_review {
        if config.auto_review_resolve_use_chat_model {
            config.model_reasoning_effort
        } else {
            config.auto_review_resolve_model_reasoning_effort
        }
    } else if config.review_resolve_use_chat_model {
        config.model_reasoning_effort
    } else {
        config.review_resolve_model_reasoning_effort
    };
    if review_auto_resolve_requested
        && (!resolve_model_for_auto_resolve.eq_ignore_ascii_case(&config.model)
            || resolve_effort_for_auto_resolve != config.model_reasoning_effort)
    {
        let resolve_family = find_family_for_model(&resolve_model_for_auto_resolve)
            .unwrap_or_else(|| derive_default_model_family(&resolve_model_for_auto_resolve));
        config.model = resolve_model_for_auto_resolve.clone();
        config.model_family = resolve_family.clone();
        config.model_reasoning_effort = resolve_effort_for_auto_resolve;
        if let Some(cw) = resolve_family.context_window {
            config.model_context_window = Some(cw);
        }
        if let Some(max) = resolve_family.max_output_tokens {
            config.model_max_output_tokens = Some(max);
        }
        config.model_auto_compact_token_limit = resolve_family.auto_compact_token_limit();
    }
    let stop_on_task_complete = auto_drive_goal.is_none() && auto_resolve_state.is_none();
    let mut event_processor: Box<dyn EventProcessor> = if json_mode {
        Box::new(EventProcessorWithJsonOutput::new(last_message_file.clone()))
    } else {
        Box::new(EventProcessorWithHumanOutput::create_with_ansi(
            stdout_with_ansi,
            &config,
            last_message_file.clone(),
            stop_on_task_complete,
        ))
    };

    if oss {
        code_ollama::ensure_oss_ready(&config)
            .await
            .map_err(|e| anyhow::anyhow!("OSS setup failed: {e}"))?;
    }

    // Print the effective configuration and prompt so users can see what Codex
    // is using.
    let default_cwd = config.cwd.to_path_buf();
    let _default_approval_policy = config.approval_policy;
    let _default_sandbox_policy = config.sandbox_policy.clone();
    let _default_model = config.model.clone();
    let _default_effort = config.model_reasoning_effort;
    let _default_summary = config.model_reasoning_summary;

    if !skip_git_repo_check && get_git_repo_root(&default_cwd).is_none() {
        eprintln!("Not inside a trusted directory and --skip-git-repo-check was not specified.");
        std::process::exit(1);
    }

    let auth_manager = AuthManager::shared_with_mode_and_originator(
        config.code_home.clone(),
        code_app_server_protocol::AuthMode::ApiKey,
        config.responses_originator_header.clone(),
        config.cli_auth_credentials_store_mode,
    );
    let conversation_manager = ConversationManager::new(auth_manager.clone(), SessionSource::Exec);

    // Handle resume subcommand by resolving a rollout path and using explicit resume API.
    let NewConversation {
        conversation_id: _,
        conversation,
        session_configured,
    } = if let Some(ExecCommand::Resume(args)) = command {
        let resume_path = resolve_resume_path(&config, &args).await?;

        if let Some(path) = resume_path {
            conversation_manager
                .resume_conversation_from_rollout(config.clone(), path, auth_manager.clone())
                .await?
        } else {
            conversation_manager
                .new_conversation(config.clone())
                .await?
        }
    } else {
        conversation_manager
            .new_conversation(config.clone())
            .await?
    };
    if auto_drive_goal.is_some() {
        let summary_config = build_auto_drive_exec_config(&config);
        event_processor.print_config_summary(&summary_config, &summary_prompt);
    } else {
        event_processor.print_config_summary(&config, &summary_prompt);
    }
    info!("Codex initialized with event: {session_configured:?}");

    if let Some(goal) = auto_drive_goal {
        return run_auto_drive_session(
            goal,
            images,
            config,
            conversation,
            event_processor,
            last_message_file,
            run_deadline,
        )
        .await;
    }

    let runtime_outcome = run_session_runtime(SessionRuntimeParams {
        conversation,
        config: &config,
        event_processor: event_processor.as_mut(),
        review_request,
        prompt_to_send,
        images,
        run_deadline,
        max_seconds,
        auto_resolve_state,
        max_auto_resolve_attempts,
        is_auto_review,
    })
    .await?;
    if let Some(path) = review_output_json
        && !runtime_outcome.review_outputs.is_empty()
    {
        let _ = write_review_json(
            path,
            &runtime_outcome.review_outputs,
            runtime_outcome.final_review_snapshot.as_ref(),
        );
    }
    if runtime_outcome.review_runs > 0 {
        eprintln!(
            "Review runs: {} (auto_resolve={} max_attempts={})",
            runtime_outcome.review_runs,
            config.tui.review_auto_resolve,
            max_auto_resolve_attempts
        );
    }
    event_processor.print_final_output();
    if runtime_outcome.error_seen {
        std::process::exit(1);
    }

    Ok(())
}


#[cfg(test)]
mod tests;
