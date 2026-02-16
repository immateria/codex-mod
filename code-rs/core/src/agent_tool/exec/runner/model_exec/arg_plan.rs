use super::*;
use super::command_detection::{command_exists, is_known_family};

pub(super) struct PreparedModelExecution {
    pub(super) command: String,
    pub(super) command_for_spawn: String,
    pub(super) family: String,
    pub(super) use_current_exe: bool,
    pub(super) final_args: Vec<String>,
    pub(super) debug_subagent: bool,
    pub(super) child_log_tag: Option<String>,
}

pub(super) struct PrepareModelExecutionRequest<'a> {
    pub(super) agent_id: &'a str,
    pub(super) model: &'a str,
    pub(super) prompt: &'a str,
    pub(super) read_only: bool,
    pub(super) config: Option<&'a AgentConfig>,
    pub(super) spec_opt: Option<&'a crate::agent_defaults::AgentModelSpec>,
    pub(super) reasoning_effort: code_protocol::config_types::ReasoningEffort,
    pub(super) review_output_json_path: Option<&'a PathBuf>,
    pub(super) source_kind: Option<&'a AgentSourceKind>,
    pub(super) log_tag: Option<&'a str>,
}

pub(super) fn prepare_model_execution(
    request: PrepareModelExecutionRequest<'_>,
) -> PreparedModelExecution {
    let PrepareModelExecutionRequest {
        agent_id,
        model,
        prompt,
        read_only,
        config,
        spec_opt,
        reasoning_effort,
        review_output_json_path,
        source_kind,
        log_tag,
    } = request;
    // Use config command if provided, otherwise fall back to the spec CLI (or the
    // lowercase model string).
    let command = if let Some(cfg) = config {
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
        command_base
    };

    // Special case: for the built-in Codex agent, prefer invoking the currently
    // running executable with the `exec` subcommand rather than relying on a
    // `codex` binary to be present on PATH. This improves portability,
    // especially on Windows where global shims may be missing.
    let model_lower = model.to_lowercase();
    let command_lower = command_for_spawn.to_ascii_lowercase();

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
    let use_current_exe = should_use_current_exe_for_agent(family, command_missing, config);

    let mut final_args: Vec<String> = command_extra_args;

    if let Some(cfg) = config {
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
                .map(|cfg| {
                    if read_only {
                        cfg.args_read_only.is_some()
                    } else {
                        cfg.args_write.is_some()
                    }
                })
                .unwrap_or(false);
            if !have_mode_args {
                let mut defaults = default_params_for(slug_for_defaults, read_only);
                command::strip_model_flags(&mut defaults);
                final_args.extend(defaults);
            }
            final_args.extend(spec_model_args.iter().cloned());
            final_args.push("-c".into());
            final_args.push(effort_override);
            final_args.push("-c".into());
            final_args.push(auto_effort_override);
            final_args.push(prompt.to_string());
        }
        "cloud" => {
            if built_in_cloud {
                final_args.extend(["cloud", "submit", "--wait"].map(String::from));
            }
            let have_mode_args = config
                .map(|cfg| {
                    if read_only {
                        cfg.args_read_only.is_some()
                    } else {
                        cfg.args_write.is_some()
                    }
                })
                .unwrap_or(false);
            if !have_mode_args {
                let mut defaults = default_params_for(slug_for_defaults, read_only);
                command::strip_model_flags(&mut defaults);
                final_args.extend(defaults);
            }
            final_args.extend(spec_model_args.iter().cloned());
            final_args.push("-c".into());
            final_args.push(effort_override);
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
        Some(
            log_tag
                .map(str::to_string)
                .unwrap_or_else(|| format!("agents/{agent_id}")),
        )
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
        for arg in final_args {
            if arg != "exec" {
                reordered.push(arg);
            }
        }
        final_args = reordered;
    }

    PreparedModelExecution {
        command,
        command_for_spawn,
        family: family.to_string(),
        use_current_exe,
        final_args,
        debug_subagent,
        child_log_tag,
    }
}
