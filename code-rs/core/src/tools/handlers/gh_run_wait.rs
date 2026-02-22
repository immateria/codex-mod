use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::codex::WaitInterruptReason;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::execute_custom_tool;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;

pub(crate) struct GhRunWaitToolHandler;

#[async_trait]
impl ToolHandler for GhRunWaitToolHandler {
    fn scheduling_hints(&self) -> crate::tools::registry::ToolSchedulingHints {
        crate::tools::registry::ToolSchedulingHints::pure_parallel()
    }

    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Function { arguments } = inv.payload else {
            return ResponseInputItem::FunctionCallOutput {
                call_id: inv.ctx.call_id,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        "gh_run_wait expects function-call arguments".to_string(),
                    ),
                    success: Some(false),
                },
            };
        };

        handle_gh_run_wait(sess, &inv.ctx, arguments).await
    }
}

pub(crate) async fn handle_gh_run_wait(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    use serde::Deserialize;
    use serde_json::Value;
    use std::path::Path;
    use std::time::Duration;

    use crate::protocol::CustomToolCallUpdateEvent;
    use crate::protocol::EventMsg;

    #[derive(Deserialize, Clone)]
    struct Params {
        #[serde(default)]
        run_id: Option<Value>,
        #[serde(default)]
        repo: Option<String>,
        #[serde(default)]
        workflow: Option<String>,
        #[serde(default)]
        branch: Option<String>,
        #[serde(default)]
        interval_seconds: Option<u64>,
    }

    async fn run_gh(args: &[&str], repo: Option<&str>) -> Result<String, String> {
        let mut display_args = Vec::new();
        if let Some(repo) = repo {
            display_args.push("-R");
            display_args.push(repo);
        }
        display_args.extend_from_slice(args);

        let mut command = tokio::process::Command::new("gh");
        if let Some(repo) = repo {
            command.arg("-R").arg(repo);
        }
        let output = command
            .args(args)
            .output()
            .await
            .map_err(|err| format!("failed to run gh {}: {err}", display_args.join(" ")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let message = if !stderr.is_empty() { stderr } else { stdout };
            return Err(format!(
                "gh {} failed{}",
                display_args.join(" "),
                if message.is_empty() {
                    String::new()
                } else {
                    format!(": {message}")
                }
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    async fn run_git(cwd: &Path, args: &[&str]) -> Option<String> {
        let output = tokio::process::Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .await
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let value = String::from_utf8(output.stdout).ok()?;
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    async fn detect_branch(cwd: &Path) -> String {
        if let Some(branch) = run_git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).await
            && branch != "HEAD"
        {
            return branch;
        }

        if let Some(symref) =
            run_git(cwd, &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"]).await
            && let Some((_, name)) = symref.rsplit_once('/')
            && !name.is_empty()
        {
            return name.to_string();
        }

        if let Some(show) = run_git(cwd, &["remote", "show", "origin"]).await {
            for line in show.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("HEAD branch:") {
                    let name = rest.trim();
                    if !name.is_empty() {
                        return name.to_string();
                    }
                }
            }
        }

        "main".to_string()
    }

    let params_for_event = serde_json::from_str::<Value>(&arguments).ok();
    let parsed: Params = match serde_json::from_str(&arguments) {
        Ok(p) => p,
        Err(e) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!(
                        "Invalid gh_run_wait arguments: {e}"
                    )),
                    success: Some(false),
                },
            };
        }
    };

    let cwd = sess.get_cwd().to_path_buf();

    #[derive(Clone, Default, PartialEq, Eq)]
    struct JobFailure {
        name: String,
        conclusion: String,
        step: Option<String>,
    }

    #[derive(Clone, Default, PartialEq, Eq)]
    struct JobSummary {
        total: usize,
        completed: usize,
        in_progress: usize,
        queued: usize,
        success: usize,
        failure: usize,
        cancelled: usize,
        skipped: usize,
        neutral: usize,
        steps_total: usize,
        steps_completed: usize,
        steps_in_progress: usize,
        steps_queued: usize,
        running_names: Vec<String>,
        queued_names: Vec<String>,
        failed_jobs: Vec<JobFailure>,
    }

    #[derive(Clone, PartialEq, Eq)]
    struct UpdateSnapshot {
        jobs: JobSummary,
        url: Option<String>,
    }

    impl JobSummary {
        fn to_json(&self) -> Value {
            serde_json::json!({
                "total": self.total,
                "completed": self.completed,
                "in_progress": self.in_progress,
                "queued": self.queued,
                "success": self.success,
                "failure": self.failure,
                "cancelled": self.cancelled,
                "skipped": self.skipped,
                "neutral": self.neutral,
                "steps_total": self.steps_total,
                "steps_completed": self.steps_completed,
                "steps_in_progress": self.steps_in_progress,
                "steps_queued": self.steps_queued,
                "running_names": self.running_names,
                "queued_names": self.queued_names,
                "failed_jobs": self.failed_jobs.iter().map(|job| {
                    let mut obj = serde_json::Map::new();
                    obj.insert("name".to_string(), Value::String(job.name.clone()));
                    obj.insert("conclusion".to_string(), Value::String(job.conclusion.clone()));
                    if let Some(step) = job.step.as_ref() {
                        obj.insert("step".to_string(), Value::String(step.clone()));
                    }
                    Value::Object(obj)
                }).collect::<Vec<_>>(),
            })
        }
    }

    #[derive(Clone, Default)]
    struct RunSummary {
        status: String,
        conclusion: Option<String>,
        html_url: Option<String>,
    }

    impl RunSummary {
        fn from_json(v: &Value) -> Self {
            let status = v
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let conclusion = v
                .get("conclusion")
                .and_then(Value::as_str)
                .map(std::string::ToString::to_string)
                .filter(|s| !s.is_empty());
            let html_url = v
                .get("url")
                .and_then(Value::as_str)
                .map(std::string::ToString::to_string)
                .filter(|s| !s.is_empty());
            Self {
                status,
                conclusion,
                html_url,
            }
        }

        fn is_done(&self) -> bool {
            if self.status.eq_ignore_ascii_case("completed") {
                return true;
            }
            if let Some(conclusion) = self.conclusion.as_ref() {
                return !conclusion.is_empty();
            }
            false
        }
    }

    #[derive(Clone, Default)]
    struct WaitState {
        run_id: Option<String>,
        repo: Option<String>,
        workflow: Option<String>,
        branch: Option<String>,
        interval_seconds: u64,
    }

    impl WaitState {
        async fn resolve(mut self, cwd: &Path, parsed: Params) -> Result<Self, String> {
            self.interval_seconds = parsed.interval_seconds.unwrap_or(8).clamp(2, 60);

            self.repo = parsed
                .repo
                .or_else(|| std::env::var("GITHUB_REPOSITORY").ok())
                .or_else(|| std::env::var("GH_REPO").ok())
                .or_else(|| std::env::var("GITHUB_REPO").ok());

            let mut run_id = parsed.run_id.and_then(|v| {
                v.as_i64()
                    .map(|id| id.to_string())
                    .or_else(|| v.as_str().map(str::to_owned))
            });

            if run_id.as_ref().is_some_and(|id| id.trim().is_empty()) {
                run_id = None;
            }

            if run_id.is_none() {
                // If no explicit run id, try to detect most recent run matching workflow+branch.
                self.workflow = parsed.workflow;
                self.branch = match parsed.branch {
                    Some(branch) if !branch.trim().is_empty() => Some(branch),
                    _ => Some(detect_branch(cwd).await),
                };
            }

            self.run_id = run_id;

            Ok(self)
        }
    }

    let ctx_clone = ctx.clone();
    let ctx_for_updates = ctx_clone.clone();
    let call_id = ctx.call_id.clone();
    let tool_name = "gh_run_wait".to_string();
    let (initial_wait_epoch, _) = sess.wait_interrupt_snapshot();

    execute_custom_tool(
        sess,
        &ctx_clone,
        tool_name,
        params_for_event,
        move || async move {
            let mut wait_state = WaitState::default();
            wait_state = match wait_state.resolve(&cwd, parsed.clone()).await {
                Ok(s) => s,
                Err(e) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(e),
                            success: Some(false),
                        },
                    };
                }
            };

            let repo = wait_state.repo.as_deref();

            // Resolve the run id if missing by querying the current branch/workflow.
            if wait_state.run_id.is_none() {
                let branch = wait_state
                    .branch
                    .clone()
                    .unwrap_or_else(|| "main".to_string());
                let workflow_filter = wait_state.workflow.clone();

                let mut args = vec!["run", "list", "--json", "databaseId,displayTitle,headBranch,status,conclusion,createdAt,updatedAt,url", "--limit", "20"];
                if let Some(workflow) = workflow_filter.as_ref()
                    && !workflow.trim().is_empty() {
                        args.push("--workflow");
                        args.push(workflow);
                    }
                args.push("--branch");
                args.push(&branch);

                let list = run_gh(&args, repo).await;
                let list = match list {
                    Ok(value) => value,
                    Err(e) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(e),
                                success: Some(false),
                            },
                        };
                    }
                };

                let parsed_list: Value = match serde_json::from_str(&list) {
                    Ok(v) => v,
                    Err(e) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Failed to parse gh run list output: {e}"
                                )),
                                success: Some(false),
                            },
                        };
                    }
                };

                let runs = parsed_list.as_array().cloned().unwrap_or_default();
                let mut selected: Option<Value> = None;
                for run in runs {
                    let head_branch = run
                        .get("headBranch")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if head_branch == branch {
                        selected = Some(run);
                        break;
                    }
                }

                let Some(selected_run) = selected else {
                    let msg = if let Some(workflow) = workflow_filter {
                        format!(
                            "No runs found for workflow {workflow} on branch {branch}"
                        )
                    } else {
                        format!("No runs found on branch {branch}")
                    };
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(msg),
                            success: Some(false),
                        },
                    };
                };

                let run_id = selected_run
                    .get("databaseId")
                    .and_then(Value::as_i64)
                    .map(|id| id.to_string());
                wait_state.run_id = run_id;
            }

            let run_id = match wait_state.run_id.clone() {
                Some(id) if !id.trim().is_empty() => id,
                _ => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "Unable to determine a GitHub run id to wait for".to_string(),
                            ),
                            success: Some(false),
                        },
                    };
                }
            };

            let prepared_url = repo.map(|repo| format!("https://github.com/{repo}/actions/runs/{run_id}"));

            let interval = Duration::from_secs(wait_state.interval_seconds);
            let mut last_update: Option<UpdateSnapshot> = None;

            loop {
                let view = run_gh(
                    &[
                        "run",
                        "view",
                        &run_id,
                        "--json",
                        "databaseId,status,conclusion,createdAt,updatedAt,url,htmlURL",
                    ],
                    repo,
                )
                .await;

                let view = match view {
                    Ok(value) => value,
                    Err(e) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(e),
                                success: Some(false),
                            },
                        };
                    }
                };

                let parsed_view: Value = match serde_json::from_str(&view) {
                    Ok(v) => v,
                    Err(e) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Failed to parse gh run view output: {e}"
                                )),
                                success: Some(false),
                            },
                        };
                    }
                };

                let summary = RunSummary::from_json(&parsed_view);
                let html_url = parsed_view
                    .get("htmlURL")
                    .and_then(Value::as_str)
                    .map(std::string::ToString::to_string)
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| summary.html_url.clone());

                let list_jobs = run_gh(
                    &[
                        "run",
                        "view",
                        &run_id,
                        "--json",
                        "jobs",
                    ],
                    repo,
                )
                .await;

                let list_jobs = match list_jobs {
                    Ok(value) => value,
                    Err(e) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(e),
                                success: Some(false),
                            },
                        };
                    }
                };

                let parsed_jobs: Value = match serde_json::from_str(&list_jobs) {
                    Ok(v) => v,
                    Err(e) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Failed to parse gh run jobs output: {e}"
                                )),
                                success: Some(false),
                            },
                        };
                    }
                };

                let jobs_array = parsed_jobs
                    .get("jobs")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();

                let mut job_summary = JobSummary {
                    total: jobs_array.len(),
                    ..Default::default()
                };

                for job in jobs_array {
                    let name = job
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let status = job
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let conclusion = job
                        .get("conclusion")
                        .and_then(Value::as_str)
                        .unwrap_or("");

                    match status {
                        "completed" => {
                            job_summary.completed += 1;
                            match conclusion {
                                "success" => job_summary.success += 1,
                                "failure" => {
                                    job_summary.failure += 1;
                                    job_summary.failed_jobs.push(JobFailure {
                                        name: name.clone(),
                                        conclusion: conclusion.to_string(),
                                        step: None,
                                    });
                                }
                                "cancelled" => job_summary.cancelled += 1,
                                "skipped" => job_summary.skipped += 1,
                                "neutral" => job_summary.neutral += 1,
                                _ => {}
                            }
                        }
                        "in_progress" => {
                            job_summary.in_progress += 1;
                            if !name.trim().is_empty() {
                                job_summary.running_names.push(name);
                            }
                        }
                        "queued" => {
                            job_summary.queued += 1;
                            if !name.trim().is_empty() {
                                job_summary.queued_names.push(name);
                            }
                        }
                        _ => {}
                    }

                    if let Some(steps) = job.get("steps").and_then(Value::as_array) {
                        job_summary.steps_total += steps.len();
                        for step in steps {
                            let step_status = step
                                .get("status")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown");
                            match step_status {
                                "completed" => job_summary.steps_completed += 1,
                                "in_progress" => job_summary.steps_in_progress += 1,
                                "queued" => job_summary.steps_queued += 1,
                                _ => {}
                            }
                        }
                    }
                }

                if summary.is_done() {
                    let conclusion = summary
                        .conclusion
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());
                    let success = if conclusion.is_empty() {
                        None
                    } else {
                        Some(conclusion == "success")
                    };
                    let output = serde_json::json!({
                        "run_id": run_id,
                        "status": summary.status,
                        "conclusion": summary.conclusion,
                        "url": html_url.clone().or_else(|| prepared_url.clone()),
                        "jobs": job_summary.to_json(),
                    });
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(output.to_string()),
                            success,
                        },
                    };
                }

                let update_url = html_url.clone().or_else(|| prepared_url.clone());
                if job_summary.total > 0 || update_url.is_some() {
                    let snapshot = UpdateSnapshot {
                        jobs: job_summary.clone(),
                        url: update_url.clone(),
                    };
                    if last_update.as_ref() != Some(&snapshot) {
                        last_update = Some(snapshot.clone());
                        let mut update_params = serde_json::Map::new();
                        update_params.insert("jobs".to_string(), snapshot.jobs.to_json());
                        if let Some(url) = snapshot.url.clone() {
                            update_params.insert("url".to_string(), Value::String(url));
                        }
                        let update_msg =
                            EventMsg::CustomToolCallUpdate(CustomToolCallUpdateEvent {
                                call_id: call_id.clone(),
                                tool_name: "gh_run_wait".to_string(),
                                parameters: Some(Value::Object(update_params)),
                            });
                        sess.send_background_ordered_from_ctx(&ctx_for_updates, update_msg)
                            .await;
                    }
                }

                if let Some(budget_text) = sess.maybe_nudge_time_budget() {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!(
                                "{budget_text}\n\nRun {run_id} still in progress. Call gh_run_wait again to continue."
                            )),
                            success: Some(false),
                        },
                    };
                }

                let (current_epoch, reason) = sess.wait_interrupt_snapshot();
                if current_epoch != initial_wait_epoch {
                    let message = match reason {
                        Some(WaitInterruptReason::UserMessage) => {
                            "wait ended due to new user message".to_string()
                        }
                        Some(WaitInterruptReason::SessionAborted) => {
                            "wait ended due to session abort".to_string()
                        }
                        None => "wait ended".to_string(),
                    };
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!(
                                "{message}\n\nRun {run_id} still in progress. Call gh_run_wait again to continue."
                            )),
                            success: Some(false),
                        },
                    };
                }

                tokio::time::sleep(interval).await;
            }
        },
    )
    .await
}
