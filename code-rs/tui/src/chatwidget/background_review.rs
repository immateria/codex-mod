use super::*;

pub(super) async fn run_background_review_inner(
    config: Config,
    app_event_tx: AppEventSender,
    base_snapshot: Option<GhostCommit>,
    turn_context: Option<String>,
    prefer_fallback: bool,
) {
    // Best-effort: clean up any stale lock left by a cancelled review process.
    let _ = code_core::review_coord::clear_stale_lock_if_dead(Some(&config.cwd));

    // Prevent duplicate auto-reviews within this process: if any AutoReview agent
    // is already pending/running, bail early with a benign notice.
    {
        let mgr = code_core::AGENT_MANAGER.read().await;
        let busy = mgr
            .list_agents(None, Some("auto-review".to_string()), false)
            .into_iter()
            .any(|agent| {
                let status = format!("{:?}", agent.status).to_ascii_lowercase();
                status == "running" || status == "pending"
            });
        if busy {
            app_event_tx.send(AppEvent::BackgroundReviewFinished {
                worktree_path: std::path::PathBuf::new(),
                branch: String::new(),
                has_findings: false,
                findings: 0,
                summary: Some("Auto review skipped: another auto review is already running.".to_string()),
                error: None,
                agent_id: None,
                snapshot: None,
            });
            return;
        }
    }

    let app_event_tx_clone = app_event_tx.clone();
    let outcome = async move {
        let git_root = code_core::git_worktree::get_git_root_from(&config.cwd)
            .await
            .map_err(|e| format!("failed to detect git root: {e}"))?;

        let snapshot = task::spawn_blocking({
            let repo_path = config.cwd.clone();
            let base_snapshot = base_snapshot.clone();
            move || {
                let mut options = CreateGhostCommitOptions::new(repo_path.as_path())
                    .message("auto review snapshot");
                if let Some(base) = base_snapshot.as_ref() {
                    options = options.parent(base.id());
                }
                let hook_repo = repo_path.clone();
                let hook = move || bump_snapshot_epoch_for(&hook_repo);
                create_ghost_commit(&options.post_commit_hook(&hook))
            }
        })
        .await
        .map_err(|e| format!("failed to spawn snapshot task: {e}"))
        .and_then(|res| res.map_err(|e| format!("failed to capture snapshot: {e}")))?;

        let snapshot_id = snapshot.id().to_string();
        bump_snapshot_epoch_for(&config.cwd);

        // Attempt to hold the shared review lock; if busy or a previous review
        // with findings is still surfaced, fall back to a per-request
        // auto-review worktree to avoid clobbering pending fixes.
        let (worktree_path, branch, worktree_guard) = if prefer_fallback {
            let (path, name, guard) =
                allocate_fallback_auto_review_worktree(&git_root, &snapshot_id).await?;
            (path, name, guard)
        } else {
            match try_acquire_lock("review", &config.cwd) {
                Ok(Some(g)) => {
                    let path = code_core::git_worktree::prepare_reusable_worktree(
                        &git_root,
                        AUTO_REVIEW_SHARED_WORKTREE,
                        snapshot_id.as_str(),
                        true,
                    )
                    .await
                    .map_err(|e| format!("failed to prepare worktree: {e}"))?;
                    (path, AUTO_REVIEW_SHARED_WORKTREE.to_string(), g)
                }
                Ok(None) => {
                    let (path, name, guard) =
                        allocate_fallback_auto_review_worktree(&git_root, &snapshot_id).await?;
                    (path, name, guard)
                }
                Err(err) => {
                    return Err(format!("could not acquire review lock: {err}"));
                }
            }
        };

        // Ensure Codex models are invoked via the `code-` CLI shim so they exist on PATH.
        fn ensure_code_prefix(model: &str) -> String {
            let lower = model.to_ascii_lowercase();
            if lower.starts_with("code-") {
                model.to_string()
            } else {
                format!("code-{model}")
            }
        }

        let review_model = ensure_code_prefix(&config.auto_review_model);

        // Allow the spawned agent to reuse the parent's review lock without blocking.
        let mut env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        env.insert("CODE_REVIEW_LOCK_LEASE".to_string(), "1".to_string());
        let agent_config = code_core::config_types::AgentConfig {
            name: review_model.clone(),
            command: String::new(),
            args: Vec::new(),
            read_only: false,
            enabled: true,
            description: None,
            env: Some(env),
            args_read_only: None,
            args_write: None,
            instructions: None,
        };

        // Use the /review entrypoint so upstream wiring (model defaults, review formatting) stays intact.
        let mut review_prompt = format!(
            "/review Analyze only changes made in commit {snapshot_id}. Identify critical bugs, regressions, security/performance/concurrency risks or incorrect assumptions. Provide actionable feedback and references to the changed code; ignore minor style or formatting nits."
        );

        if let Some(context) = turn_context {
            review_prompt.push_str("\n\n");
            review_prompt.push_str(&context);
        }

        let mut manager = code_core::AGENT_MANAGER.write().await;
        let agent_id = manager
            .create_agent_with_options(code_core::AgentCreateRequest {
                model: review_model,
                name: Some("Auto Review".to_string()),
                prompt: review_prompt,
                context: None,
                output_goal: None,
                files: Vec::new(),
                read_only: false,
                batch_id: Some(branch.clone()),
                config: Some(agent_config.clone()),
                worktree_branch: Some(branch.clone()),
                worktree_base: Some(snapshot_id.clone()),
                source_kind: Some(code_core::protocol::AgentSourceKind::AutoReview),
                reasoning_effort: config.auto_review_model_reasoning_effort.into(),
            })
            .await;
        insert_background_lock(&agent_id, worktree_guard);
        drop(manager);

        app_event_tx_clone.send(AppEvent::BackgroundReviewStarted {
            worktree_path: worktree_path.clone(),
            branch: branch.clone(),
            agent_id: Some(agent_id.clone()),
            snapshot: Some(snapshot_id.clone()),
        });
        Ok::<(PathBuf, String, String, String), String>((worktree_path, branch, agent_id, snapshot_id))
    }
    .await;

    if let Err(err) = outcome {
        app_event_tx.send(AppEvent::BackgroundReviewFinished {
            worktree_path: std::path::PathBuf::new(),
            branch: String::new(),
            has_findings: false,
            findings: 0,
            summary: None,
            error: Some(err),
            agent_id: None,
            snapshot: None,
        });
    }
}

pub(super) fn insert_background_lock_inner(
    agent_id: &str,
    guard: code_core::review_coord::ReviewGuard,
) {
    if let Ok(mut map) = BACKGROUND_REVIEW_LOCKS.lock() {
        map.insert(agent_id.to_string(), guard);
    }
}

pub(super) fn release_background_lock_inner(agent_id: &Option<String>) {
    if let Some(id) = agent_id
        && let Ok(mut map) = BACKGROUND_REVIEW_LOCKS.lock() {
            map.remove(id);
        }
}
