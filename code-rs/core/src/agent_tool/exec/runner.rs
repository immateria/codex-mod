use super::*;

async fn get_git_root() -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .await
        .map_err(|e| format!("Git not installed or not in a git repository: {e}"))?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(PathBuf::from(path))
    } else {
        Err("Not in a git repository".to_string())
    }
}

use crate::git_worktree::sanitize_ref_component;

fn generate_branch_id(model: &str, agent: &str) -> String {
    // Extract first few meaningful words from agent for the branch name
    let stop = ["the", "and", "for", "with", "from", "into", "goal"]; // skip boilerplate
    let words: Vec<&str> = agent
        .split_whitespace()
        .filter(|w| w.len() > 2 && !stop.contains(&w.to_ascii_lowercase().as_str()))
        .take(3)
        .collect();

    let raw_suffix = if words.is_empty() {
        Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("agent")
            .to_string()
    } else {
        words.join("-")
    };

    // Sanitize both model and suffix for safety
    let model_s = sanitize_ref_component(model);
    let mut suffix_s = sanitize_ref_component(&raw_suffix);

    // Constrain length to keep branch names readable
    if suffix_s.len() > 40 {
        suffix_s.truncate(40);
        suffix_s = suffix_s.trim_matches('-').to_string();
        if suffix_s.is_empty() {
            suffix_s = "agent".to_string();
        }
    }

    format!("code-{model_s}-{suffix_s}")
}

use crate::git_worktree::setup_worktree;

pub(crate) async fn execute_agent(agent_id: String, config: Option<AgentConfig>) {
    let mut manager = AGENT_MANAGER.write().await;

    // Get agent details
    let agent = match manager.get_agent(&agent_id) {
        Some(t) => t,
        None => return,
    };

    // Update status to running
    manager
        .update_agent_status(&agent_id, AgentStatus::Running)
        .await;
    manager
        .add_progress(
            &agent_id,
            format!("Starting agent with model: {}", agent.model),
        )
        .await;

    let model = agent.model.clone();
    let model_spec = agent_model_spec(&model);
    let prompt = agent.prompt.clone();
    let read_only = agent.read_only;
    let context = agent.context.clone();
    let output_goal = agent.output_goal.clone();
    let files = agent.files.clone();
    let reasoning_effort = agent.reasoning_effort;
    let source_kind = agent.source_kind.clone();
    let log_tag = agent.log_tag.clone();

    drop(manager); // Release the lock before executing

    // Build the full prompt with context
    let mut full_prompt = prompt.clone();
    // Prepend any per-agent instructions from config when available
    if let Some(cfg) = config.as_ref()
        && let Some(instr) = cfg.instructions.as_ref()
            && !instr.trim().is_empty() {
                full_prompt = format!("{}\n\n{}", instr.trim(), full_prompt);
            }
    if let Some(context) = &context {
        let trimmed = full_prompt.trim_start();
        if trimmed.starts_with('/') {
            // Preserve leading slash commands so downstream executors can parse them.
            full_prompt = format!("{full_prompt}\n\nContext: {context}");
        } else {
            full_prompt = format!("Context: {context}\n\nAgent: {full_prompt}");
        }
    }
    if let Some(output_goal) = &output_goal {
        full_prompt = format!("{full_prompt}\n\nDesired output: {output_goal}");
    }
    if !files.is_empty() {
        full_prompt = format!("{}\n\nFiles to consider: {}", full_prompt, files.join(", "));
    }

    // Setup working directory and execute
    let gating_error_message = |spec: &crate::agent_defaults::AgentModelSpec| {
        if let Some(flag) = spec.gating_env {
            format!(
                "agent model '{}' is disabled; set {}=1 to enable it",
                spec.slug, flag
            )
        } else {
            format!("agent model '{}' is disabled", spec.slug)
        }
    };

    // Track optional review output path for /review agents (AutoReview)
    let mut review_output_json_path_capture: Option<PathBuf> = None;

    let result = if !read_only {
        // Check git and setup worktree for non-read-only mode
        match get_git_root().await {
            Ok(git_root) => {
                let branch_id = agent
                    .branch_name
                    .clone()
                    .unwrap_or_else(|| generate_branch_id(&model, &prompt));

                let mut manager = AGENT_MANAGER.write().await;
                manager
                    .add_progress(&agent_id, format!("Creating git worktree: {branch_id}"))
                    .await;
                drop(manager);

                match setup_worktree(&git_root, &branch_id, agent.worktree_base.as_deref()).await {
                    Ok((worktree_path, used_branch)) => {
                        let mut manager = AGENT_MANAGER.write().await;
                        manager
                            .add_progress(
                                &agent_id,
                                format!("Executing in worktree: {}", worktree_path.display()),
                            )
                            .await;
                        manager
                            .update_worktree_info(
                                &agent_id,
                                worktree_path.display().to_string(),
                                used_branch.clone(),
                            )
                            .await;
                        drop(manager);

                        // Prepare optional review-output JSON path for /review agents
                        let review_output_json_path: Option<PathBuf> = agent
                            .source_kind
                            .as_ref()
                            .and_then(|kind| matches!(kind, AgentSourceKind::AutoReview).then(|| {
                                let filename = format!("{agent_id}.review-output.json");
                                std::env::temp_dir().join(filename)
                            }));
                        review_output_json_path_capture = review_output_json_path.clone();

                        // Execute with full permissions in the worktree
                        let use_built_in_cloud = config.is_none()
                            && model_spec
                                .map(|spec| spec.cli.eq_ignore_ascii_case("cloud"))
                                .unwrap_or_else(|| model.eq_ignore_ascii_case("cloud"));

                        if use_built_in_cloud {
                            if let Some(spec) = model_spec {
                                if !spec.is_enabled() {
                                    Err(gating_error_message(spec))
                                } else {
                                    cloud::execute_cloud_built_in_streaming(
                                        &agent_id,
                                        &full_prompt,
                                        Some(worktree_path),
                                        config.clone(),
                                        spec.slug,
                                    )
                                    .await
                                }
                            } else {
                                cloud::execute_cloud_built_in_streaming(
                                    &agent_id,
                                    &full_prompt,
                                    Some(worktree_path),
                                    config.clone(),
                                    model.as_str(),
                                )
                                .await
                            }
                        } else {
                            execute_model_with_permissions(ExecuteModelRequest {
                                agent_id: &agent_id,
                                model: &model,
                                prompt: &full_prompt,
                                read_only: false,
                                working_dir: Some(worktree_path),
                                config: config.clone(),
                                reasoning_effort,
                                review_output_json_path: review_output_json_path.as_ref(),
                                source_kind: source_kind.clone(),
                                log_tag: log_tag.as_deref(),
                            })
                            .await
                        }
                    }
                    Err(e) => Err(format!("Failed to setup worktree: {e}")),
                }
            }
            Err(e) => Err(format!("Git is required for non-read-only agents: {e}")),
        }
    } else {
        // Execute in read-only mode
        full_prompt = format!(
            "{full_prompt}\n\n[Running in read-only mode - no modifications allowed]"
        );
        let use_built_in_cloud = config.is_none()
            && model_spec
                .map(|spec| spec.cli.eq_ignore_ascii_case("cloud"))
                .unwrap_or_else(|| model.eq_ignore_ascii_case("cloud"));

        if use_built_in_cloud {
            if let Some(spec) = model_spec {
                if !spec.is_enabled() {
                    Err(gating_error_message(spec))
                } else {
                    cloud::execute_cloud_built_in_streaming(&agent_id, &full_prompt, None, config, spec.slug).await
                }
            } else {
                cloud::execute_cloud_built_in_streaming(&agent_id, &full_prompt, None, config, model.as_str()).await
            }
        } else {
            execute_model_with_permissions(ExecuteModelRequest {
                agent_id: &agent_id,
                model: &model,
                prompt: &full_prompt,
                read_only: true,
                working_dir: None,
                config,
                reasoning_effort,
                review_output_json_path: None,
                source_kind,
                log_tag: log_tag.as_deref(),
            })
            .await
        }
    };

    // Update result; if a review-output JSON was produced, prefer its contents.
    let final_result = prefer_json_result(review_output_json_path_capture.as_ref(), result);
    let mut manager = AGENT_MANAGER.write().await;
    manager.update_agent_result(&agent_id, final_result).await;
}

pub(crate) fn prefer_json_result(path: Option<&PathBuf>, fallback: Result<String, String>) -> Result<String, String> {
    if let Some(p) = path
        && let Ok(json) = std::fs::read_to_string(p) {
            return Ok(json);
        }
    fallback
}

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

pub(crate) async fn execute_model_with_permissions(request: ExecuteModelRequest<'_>) -> Result<String, String> {
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
    // Helper: cross‑platform check whether an executable is available in PATH
    // and is directly spawnable by std::process::Command (no shell wrappers).
fn command_exists(cmd: &str) -> bool {
        // Absolute/relative path with separators: check directly (files only).
        if cmd.contains(std::path::MAIN_SEPARATOR) || cmd.contains('/') || cmd.contains('\\') {
            let path = std::path::Path::new(cmd);
            if path.extension().is_some() {
                return std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false);
            }

            #[cfg(target_os = "windows")]
            {
                const DEFAULT_EXTS: &[&str] = &[".exe", ".com", ".cmd", ".bat"];
                for ext in default_pathext_or_default() {
                    let candidate = path.with_extension("");
                    let candidate = candidate.with_extension(ext.trim_start_matches('.'));
                    if std::fs::metadata(&candidate)
                        .map(|m| m.is_file())
                        .unwrap_or(false)
                    {
                        return true;
                    }
                }
            }

            return std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false);
        }

        #[cfg(target_os = "windows")]
        {
            let exts = default_pathext_or_default();
            let path_var = std::env::var_os("PATH");
            let path_iter = path_var
                .as_ref()
                .map(std::env::split_paths)
                .into_iter()
                .flatten();

            let candidates: Vec<String> = if std::path::Path::new(cmd).extension().is_some() {
                vec![cmd.to_string()]
            } else {
                exts
                    .iter()
                    .map(|ext| format!("{cmd}{ext}"))
                    .collect()
            };

            for dir in path_iter {
                for candidate in &candidates {
                    let p = dir.join(candidate);
                    if p.is_file() {
                        return true;
                    }
                }
            }

            false
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let Some(path_os) = std::env::var_os("PATH") else { return false; };
            for dir in std::env::split_paths(&path_os) {
                if dir.as_os_str().is_empty() { continue; }
                let candidate = dir.join(cmd);
                if let Ok(meta) = std::fs::metadata(&candidate)
                    && meta.is_file() {
                        let mode = meta.permissions().mode();
                        if mode & 0o111 != 0 { return true; }
                    }
            }
            false
        }
    }

    let spec_opt = agent_model_spec(model)
        .or_else(|| config.as_ref().and_then(|cfg| agent_model_spec(&cfg.name)))
        .or_else(|| config.as_ref().and_then(|cfg| agent_model_spec(&cfg.command)));

    if let Some(spec) = spec_opt
        && !spec.is_enabled() {
            if let Some(flag) = spec.gating_env {
                return Err(format!(
                    "agent model '{}' is disabled; set {}=1 to enable it",
                    spec.slug, flag
                ));
            }
            return Err(format!("agent model '{}' is disabled", spec.slug));
        }

    // Use config command if provided, otherwise fall back to the spec CLI (or the
    // lowercase model string).
    let command = if let Some(ref cfg) = config {
        let cmd = cfg.command.trim();
        if !cmd.is_empty() {
            cfg.command.clone()
        } else if let Some(spec) = spec_opt {
            spec.cli.to_string()
        } else {
            cfg.name.clone()
        }
    } else if let Some(spec) = spec_opt {
        spec.cli.to_string()
    } else {
        model.to_lowercase()
    };

    let (command_base, command_extra_args) = split_command_and_args(&command);
    let command_for_spawn = if command_base.is_empty() {
        command.clone()
    } else {
        command_base.clone()
    };

    // Special case: for the built‑in Codex agent, prefer invoking the currently
    // running executable with the `exec` subcommand rather than relying on a
    // `codex` binary to be present on PATH. This improves portability,
    // especially on Windows where global shims may be missing.
    let model_lower = model.to_lowercase();
    let command_lower = command_for_spawn.to_ascii_lowercase();
    fn is_known_family(s: &str) -> bool {
        matches!(s, "claude" | "gemini" | "qwen" | "codex" | "code" | "cloud" | "coder")
    }

    let slug_for_defaults = spec_opt.map(|spec| spec.slug).unwrap_or(model);
    let spec_family = spec_opt.map(|spec| spec.family);
    let family = if let Some(spec_family) = spec_family {
        spec_family
    } else if is_known_family(model_lower.as_str()) {
        model_lower.as_str()
    } else if is_known_family(command_lower.as_str()) {
        command_lower.as_str()
    } else {
        model_lower.as_str()
    };

    let command_missing = !command_exists(&command_for_spawn);
    let use_current_exe = should_use_current_exe_for_agent(family, command_missing, config.as_ref());

    let mut final_args: Vec<String> = command_extra_args;

    if let Some(ref cfg) = config {
        if read_only {
            if let Some(ro) = cfg.args_read_only.as_ref() {
                final_args.extend(ro.iter().cloned());
            } else {
                final_args.extend(cfg.args.iter().cloned());
            }
        } else if let Some(w) = cfg.args_write.as_ref() {
            final_args.extend(w.iter().cloned());
        } else {
            final_args.extend(cfg.args.iter().cloned());
        }
    }

    command::strip_model_flags(&mut final_args);

    let spec_model_args: Vec<String> = if let Some(spec) = spec_opt {
        spec.model_args.iter().map(|arg| (*arg).to_string()).collect()
    } else {
        Vec::new()
    };

    let built_in_cloud = family == "cloud" && config.is_none();

    // Clamp reasoning effort to what the target model supports.
    let clamped_effort = match reasoning_effort {
        code_protocol::config_types::ReasoningEffort::XHigh => {
            let lower = slug_for_defaults.to_ascii_lowercase();
            if lower.contains("max") {
                reasoning_effort
            } else {
                code_protocol::config_types::ReasoningEffort::High
            }
        }
        other => other,
    };

    // Configuration overrides for Codex CLI families. External CLIs (claude,
    // gemini, qwen) do not understand our config flags, so only attach these
    // when launching Codex binaries.
    let effort_override = format!(
        "model_reasoning_effort={}",
        clamped_effort.to_string().to_ascii_lowercase()
    );
    let auto_effort_override = format!(
        "auto_drive.model_reasoning_effort={}",
        clamped_effort.to_string().to_ascii_lowercase()
    );
    match family {
        "claude" | "gemini" | "qwen" => {
            let mut defaults = default_params_for(slug_for_defaults, read_only);
            command::strip_model_flags(&mut defaults);
            final_args.extend(defaults);
            final_args.extend(spec_model_args.iter().cloned());
            final_args.push("-p".into());
            final_args.push(prompt.to_string());
        }
        "codex" | "code" => {
            let have_mode_args = config
                .as_ref()
                .map(|c| if read_only { c.args_read_only.is_some() } else { c.args_write.is_some() })
                .unwrap_or(false);
            if !have_mode_args {
                let mut defaults = default_params_for(slug_for_defaults, read_only);
                command::strip_model_flags(&mut defaults);
                final_args.extend(defaults);
            }
            final_args.extend(spec_model_args.iter().cloned());
            final_args.push("-c".into());
            final_args.push(effort_override.clone());
            final_args.push("-c".into());
            final_args.push(auto_effort_override.clone());
            final_args.push(prompt.to_string());
        }
        "cloud" => {
            if built_in_cloud {
                final_args.extend(["cloud", "submit", "--wait"].map(String::from));
            }
            let have_mode_args = config
                .as_ref()
                .map(|c| if read_only { c.args_read_only.is_some() } else { c.args_write.is_some() })
                .unwrap_or(false);
            if !have_mode_args {
                let mut defaults = default_params_for(slug_for_defaults, read_only);
                command::strip_model_flags(&mut defaults);
                final_args.extend(defaults);
            }
            final_args.extend(spec_model_args.iter().cloned());
            final_args.push("-c".into());
            final_args.push(effort_override.clone());
            final_args.push("-c".into());
            final_args.push(auto_effort_override);
            final_args.push(prompt.to_string());
        }
        _ => {
            final_args.extend(spec_model_args.iter().cloned());
            final_args.push(prompt.to_string());
        }
    }

    let log_tag_owned = log_tag.map(str::to_string);
    let debug_subagent = command::debug_subagents_enabled()
        && matches!(source_kind, Some(AgentSourceKind::AutoReview));
    let child_log_tag: Option<String> = if debug_subagent {
        Some(log_tag_owned.clone().unwrap_or_else(|| format!("agents/{agent_id}")))
    } else {
        log_tag_owned
    };

    if debug_subagent && use_current_exe && !command::has_debug_flag(&final_args) {
        final_args.insert(0, "--debug".to_string());
    }

    if let Some(path) = review_output_json_path {
        final_args.push("--review-output-json".to_string());
        final_args.push(path.display().to_string());
    }

    if use_current_exe
        && (final_args.iter().any(|arg| arg == "exec") || review_output_json_path.is_some())
    {
        let mut reordered: Vec<String> = Vec::with_capacity(final_args.len() + 1);
        reordered.push("exec".to_string());
        for arg in final_args.into_iter() {
            if arg != "exec" {
                reordered.push(arg);
            }
        }
        final_args = reordered;
    }

    // Proactively check for presence of external command before spawn when not
    // using the current executable fallback. This avoids confusing OS errors
    // like "program not found" and lets us surface a cleaner message.
    let requires_command_check =
        family != "codex" && family != "code" && !(family == "cloud" && config.is_none());
    if requires_command_check && !command_exists(&command_for_spawn)
    {
        return Err(runtime_paths::format_agent_not_found_error(&command, &command_for_spawn));
    }

    // Agents: run without OS sandboxing; rely on per-branch worktrees for isolation.
    use crate::protocol::SandboxPolicy;
    use crate::spawn::StdioPolicy;
    // Build env from current process then overlay any config-provided vars.
    let mut env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let orig_home: Option<String> = env.get("HOME").cloned();
    if let Some(ref cfg) = config
        && let Some(ref e) = cfg.env { for (k, v) in e { env.insert(k.clone(), v.clone()); } }

    if debug_subagent {
        env.entry("CODE_SUBAGENT_DEBUG".to_string())
            .or_insert_with(|| "1".to_string());
        if let Some(tag) = child_log_tag.as_ref() {
            env.insert("CODE_DEBUG_LOG_TAG".to_string(), tag.clone());
        }
    }

    // Tag OpenAI requests originating from agent runs so server-side telemetry
    // can distinguish subagent traffic.
    if use_current_exe || family == "codex" || family == "code" {
        let subagent = match source_kind {
            Some(AgentSourceKind::AutoReview) => "review",
            _ => "agent",
        };
        env.entry("CODE_OPENAI_SUBAGENT".to_string())
            .or_insert_with(|| subagent.to_string());
    }

    // Convenience: map common key names so external CLIs "just work".
    if let Some(google_key) = env.get("GOOGLE_API_KEY").cloned() {
        env.entry("GEMINI_API_KEY".to_string()).or_insert(google_key);
    }
    if let Some(claude_key) = env.get("CLAUDE_API_KEY").cloned() {
        env.entry("ANTHROPIC_API_KEY".to_string()).or_insert(claude_key);
    }
    if let Some(anthropic_key) = env.get("ANTHROPIC_API_KEY").cloned() {
        env.entry("CLAUDE_API_KEY".to_string()).or_insert(anthropic_key);
    }
    if let Some(anthropic_base) = env.get("ANTHROPIC_BASE_URL").cloned() {
        env.entry("CLAUDE_BASE_URL".to_string()).or_insert(anthropic_base);
    }
    // Qwen/DashScope convenience: mirror API keys and base URLs both ways so
    // either variable name works across tools.
    if let Some(qwen_key) = env.get("QWEN_API_KEY").cloned() {
        env.entry("DASHSCOPE_API_KEY".to_string()).or_insert(qwen_key);
    }
    if let Some(dashscope_key) = env.get("DASHSCOPE_API_KEY").cloned() {
        env.entry("QWEN_API_KEY".to_string()).or_insert(dashscope_key);
    }
    if let Some(qwen_base) = env.get("QWEN_BASE_URL").cloned() {
        env.entry("DASHSCOPE_BASE_URL".to_string()).or_insert(qwen_base);
    }
    if let Some(ds_base) = env.get("DASHSCOPE_BASE_URL").cloned() {
        env.entry("QWEN_BASE_URL".to_string()).or_insert(ds_base);
    }
    if family == "qwen" {
        env.insert("OPENAI_API_KEY".to_string(), String::new());
    }
    // Reduce startup overhead for Claude CLI: disable auto-updater/telemetry.
    env.entry("DISABLE_AUTOUPDATER".to_string()).or_insert("1".to_string());
    env.entry("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string()).or_insert("1".to_string());
    env.entry("DISABLE_ERROR_REPORTING".to_string()).or_insert("1".to_string());
    // Prefer explicit Claude config dir to avoid touching $HOME/.claude.json.
    // Do not force CLAUDE_CONFIG_DIR here; leave CLI free to use its default
    // (including Keychain) unless we explicitly redirect HOME below.

    // If GEMINI_API_KEY not provided, try pointing to host config for read‑only
    // discovery (Gemini CLI supports GEMINI_CONFIG_DIR). We keep HOME as-is so
    // CLIs that require ~/.gemini and ~/.claude continue to work with your
    // existing config.
    maybe_set_gemini_config_dir(&mut env, orig_home.clone());

    let output = if !read_only {
        // Resolve the command and args we prepared above into Vec<String> for spawn helpers.
        let program = resolve_program_path(use_current_exe, &command_for_spawn)?;
        let args = final_args.clone();

        let child_result: std::io::Result<tokio::process::Child> = crate::spawn::spawn_child_async(
            program.clone(),
            args.clone(),
            Some(program.to_string_lossy().as_ref()),
            working_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))),
            &SandboxPolicy::DangerFullAccess,
            StdioPolicy::RedirectForShellTool,
            env.clone(),
        )
        .await;

        match child_result {
            Ok(child) => process_output::stream_child_output(agent_id, child).await?,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(runtime_paths::format_agent_not_found_error(&command, &command_for_spawn));
                }
                return Err(format!("Failed to spawn sandboxed agent: {e}"));
            }
        }
    } else {
        // Read-only path: must honor resolve_program_path (and CODE_BINARY_PATH) just
        // like the write path; skipping this can regress to PATH resolution and
        // launch the npm shim on Windows (issue #497).
        let program = resolve_program_path(use_current_exe, &command_for_spawn)?;
        let mut cmd = Command::new(program);

        if let Some(dir) = working_dir.clone() {
            cmd.current_dir(dir);
        }

        cmd.args(final_args.clone());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        for (k, v) in &env {
            cmd.env(k, v);
        }

        // Ensure the child is terminated if this process dies unexpectedly.
        cmd.kill_on_drop(true);

        match spawn_tokio_command_with_retry(&mut cmd).await {
            Ok(child) => process_output::stream_child_output(agent_id, child).await?,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(runtime_paths::format_agent_not_found_error(&command, &command_for_spawn));
                }

                return Err(format!("Failed to execute {model}: {e}"));
            }
        }
    };

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
