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
