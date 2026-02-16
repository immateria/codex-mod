use super::*;

mod arg_plan;
mod command_detection;
mod env_setup;
mod spawn_exec;

use arg_plan::PrepareModelExecutionRequest;
use arg_plan::prepare_model_execution;
use command_detection::command_exists;
use env_setup::build_agent_env;
use spawn_exec::SpawnAgentProcessRequest;
use spawn_exec::run_agent_process;

pub(crate) struct ExecuteModelRequest<'a> {
    pub(crate) agent_id: &'a str,
    pub(crate) model: &'a str,
    pub(crate) prompt: &'a str,
    pub(crate) read_only: bool,
    pub(crate) working_dir: Option<PathBuf>,
    pub(crate) config: Option<AgentConfig>,
    pub(crate) reasoning_effort: code_protocol::config_types::ReasoningEffort,
    pub(crate) review_output_json_path: Option<&'a PathBuf>,
    pub(crate) source_kind: Option<AgentSourceKind>,
    pub(crate) log_tag: Option<&'a str>,
}

pub(crate) async fn execute_model_with_permissions(
    request: ExecuteModelRequest<'_>,
) -> Result<String, String> {
    let ExecuteModelRequest {
        agent_id,
        model,
        prompt,
        read_only,
        working_dir,
        config,
        reasoning_effort,
        review_output_json_path,
        source_kind,
        log_tag,
    } = request;

    let spec_opt = agent_model_spec(model)
        .or_else(|| config.as_ref().and_then(|cfg| agent_model_spec(&cfg.name)))
        .or_else(|| config.as_ref().and_then(|cfg| agent_model_spec(&cfg.command)));

    if let Some(spec) = spec_opt
        && !spec.is_enabled()
    {
        if let Some(flag) = spec.gating_env {
            return Err(format!(
                "agent model '{}' is disabled; set {}=1 to enable it",
                spec.slug, flag
            ));
        }
        return Err(format!("agent model '{}' is disabled", spec.slug));
    }

    let prepared = prepare_model_execution(PrepareModelExecutionRequest {
        agent_id,
        model,
        prompt,
        read_only,
        config: config.as_ref(),
        spec_opt,
        reasoning_effort,
        review_output_json_path,
        source_kind: source_kind.as_ref(),
        log_tag,
    });

    // Proactively check for presence of external command before spawn when not
    // using the current executable fallback. This avoids confusing OS errors
    // like "program not found" and lets us surface a cleaner message.
    let requires_command_check =
        prepared.family != "codex" && prepared.family != "code" && !(prepared.family == "cloud" && config.is_none());
    if requires_command_check && !command_exists(&prepared.command_for_spawn) {
        return Err(runtime_paths::format_agent_not_found_error(
            &prepared.command,
            &prepared.command_for_spawn,
        ));
    }

    let env = build_agent_env(
        config.as_ref(),
        prepared.debug_subagent,
        prepared.child_log_tag.as_deref(),
        prepared.use_current_exe,
        &prepared.family,
        source_kind.as_ref(),
    );

    let output = run_agent_process(SpawnAgentProcessRequest {
        agent_id,
        model,
        read_only,
        working_dir,
        use_current_exe: prepared.use_current_exe,
        command: &prepared.command,
        command_for_spawn: &prepared.command_for_spawn,
        final_args: &prepared.final_args,
        env: &env,
    })
    .await?;

    let (status, stdout_buf, stderr_buf) = output;

    if status.success() {
        Ok(stdout_buf)
    } else {
        let stderr = stderr_buf.trim();
        let stdout = stdout_buf.trim();
        let combined = if stderr.is_empty() {
            stdout.to_string()
        } else if stdout.is_empty() {
            stderr.to_string()
        } else {
            format!("{stderr}\n{stdout}")
        };
        Err(format!("Command failed: {combined}"))
    }
}
