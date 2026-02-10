use super::*;

pub(super) async fn execute_cloud_built_in_streaming(
    agent_id: &str,
    prompt: &str,
    working_dir: Option<std::path::PathBuf>,
    _config: Option<AgentConfig>,
    model_slug: &str,
) -> Result<String, String> {
    // Program and argv
    let program = current_code_binary_path()?;
    let mut args: Vec<String> = vec!["cloud".into(), "submit".into(), "--wait".into()];
    if let Some(spec) = agent_model_spec(model_slug) {
        args.extend(spec.model_args.iter().map(|arg| (*arg).to_string()));
    }
    args.push(prompt.into());

    // Baseline env mirrors behavior in execute_model_with_permissions
    let env: std::collections::HashMap<String, String> = std::env::vars().collect();

    use crate::protocol::SandboxPolicy;
    use crate::spawn::StdioPolicy;
    let mut child = crate::spawn::spawn_child_async(
        program.clone(),
        args.clone(),
        Some(program.to_string_lossy().as_ref()),
        working_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))),
        &SandboxPolicy::DangerFullAccess,
        StdioPolicy::RedirectForShellTool,
        env,
    )
    .await
    .map_err(|e| format!("Failed to spawn cloud submit: {e}"))?;

    // Stream stderr to HUD
    let stderr_task = if let Some(stderr) = child.stderr.take() {
        let agent = agent_id.to_string();
        Some(tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let msg = line.trim();
                if msg.is_empty() { continue; }
                let mut mgr = AGENT_MANAGER.write().await;
                mgr.add_progress(&agent, msg.to_string()).await;
            }
        }))
    } else { None };

    // Collect stdout fully (final result)
    let mut stdout_buf = String::new();
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            stdout_buf.push_str(&line);
            stdout_buf.push('\n');
        }
    }

    let status = child.wait().await.map_err(|e| format!("Failed to wait: {e}"))?;
    if let Some(t) = stderr_task { let _ = t.await; }
    if !status.success() {
        return Err(format!("cloud submit exited with status {status}"));
    }

    if let Some(dir) = working_dir.as_ref() {
        let diff_text_opt = if stdout_buf.starts_with("diff --git ") {
            Some(stdout_buf.trim())
        } else {
            stdout_buf
                .find("\ndiff --git ")
                .map(|idx| stdout_buf[idx + 1..].trim())
        };

        if let Some(diff_text) = diff_text_opt
            && !diff_text.is_empty() {
                let mut apply = Command::new("git");
                apply.arg("apply").arg("--whitespace=nowarn");
                apply.current_dir(dir);
                apply.stdin(Stdio::piped());

                let mut child = spawn_tokio_command_with_retry(&mut apply)
                    .await
                    .map_err(|e| format!("Failed to spawn git apply: {e}"))?;

                if let Some(mut stdin) = child.stdin.take() {
                    stdin
                        .write_all(diff_text.as_bytes())
                        .await
                        .map_err(|e| format!("Failed to write diff to git apply: {e}"))?;
                }

                let status = child
                    .wait()
                    .await
                    .map_err(|e| format!("Failed to wait for git apply: {e}"))?;

                if !status.success() {
                    return Err(format!(
                        "git apply exited with status {status} while applying cloud diff"
                    ));
                }
            }
    }

    // Truncate large outputs
    const MAX_BYTES: usize = 500_000; // ~500 KB
    if stdout_buf.len() > MAX_BYTES {
        let omitted = stdout_buf.len() - MAX_BYTES;
        let mut truncated = String::with_capacity(MAX_BYTES + 128);
        truncated.push_str(&stdout_buf[..MAX_BYTES]);
        truncated.push_str(&format!("\n… [truncated: {omitted} bytes omitted]"));
        Ok(truncated)
    } else {
        Ok(stdout_buf)
    }
}
