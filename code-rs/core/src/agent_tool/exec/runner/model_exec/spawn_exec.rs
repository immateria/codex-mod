use super::*;

pub(super) struct SpawnAgentProcessRequest<'a> {
    pub(super) agent_id: &'a str,
    pub(super) model: &'a str,
    pub(super) read_only: bool,
    pub(super) working_dir: Option<PathBuf>,
    pub(super) use_current_exe: bool,
    pub(super) command: &'a str,
    pub(super) command_for_spawn: &'a str,
    pub(super) final_args: &'a [String],
    pub(super) env: &'a std::collections::HashMap<String, String>,
}

pub(super) async fn run_agent_process(
    request: SpawnAgentProcessRequest<'_>,
) -> Result<(std::process::ExitStatus, String, String), String> {
    let SpawnAgentProcessRequest {
        agent_id,
        model,
        read_only,
        working_dir,
        use_current_exe,
        command,
        command_for_spawn,
        final_args,
        env,
    } = request;

    use crate::protocol::SandboxPolicy;
    use crate::spawn::StdioPolicy;

    if !read_only {
        // Resolve the command and args we prepared above into Vec<String> for spawn helpers.
        let program = resolve_program_path(use_current_exe, command_for_spawn)?;
        let args = final_args.to_vec();

        let child_result: std::io::Result<tokio::process::Child> = crate::spawn::spawn_child_async(
            program.clone(),
            args,
            Some(program.to_string_lossy().as_ref()),
            working_dir.clone().unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
            }),
            &SandboxPolicy::DangerFullAccess,
            StdioPolicy::RedirectForShellTool,
            env.clone(),
        )
        .await;

        match child_result {
            Ok(child) => process_output::stream_child_output(agent_id, child).await,
            Err(err) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    return Err(runtime_paths::format_agent_not_found_error(
                        command,
                        command_for_spawn,
                    ));
                }
                Err(format!("Failed to spawn sandboxed agent: {err}"))
            }
        }
    } else {
        // Read-only path: must honor resolve_program_path (and CODE_BINARY_PATH) just
        // like the write path; skipping this can regress to PATH resolution and
        // launch the npm shim on Windows (issue #497).
        let program = resolve_program_path(use_current_exe, command_for_spawn)?;
        let mut cmd = Command::new(program);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        cmd.args(final_args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        for (key, value) in env {
            cmd.env(key, value);
        }

        // Ensure the child is terminated if this process dies unexpectedly.
        cmd.kill_on_drop(true);

        match spawn_tokio_command_with_retry(&mut cmd).await {
            Ok(child) => process_output::stream_child_output(agent_id, child).await,
            Err(err) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    return Err(runtime_paths::format_agent_not_found_error(
                        command,
                        command_for_spawn,
                    ));
                }

                Err(format!("Failed to execute {model}: {err}"))
            }
        }
    }
}
