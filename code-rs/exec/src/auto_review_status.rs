use code_core::git_info::get_git_repo_root;
use code_core::protocol::AgentSourceKind;
use code_core::protocol::AgentStatusUpdateEvent;
use code_core::protocol::ReviewOutputEvent;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

#[derive(Default, Debug, Clone)]
struct AutoReviewSummary {
    has_findings: bool,
    findings: usize,
    summary: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AutoReviewCompletion {
    branch: Option<String>,
    worktree_path: Option<PathBuf>,
    summary: AutoReviewSummary,
    error: Option<String>,
}

#[derive(Default)]
pub(crate) struct AutoReviewTracker {
    running: HashSet<String>,
    processed: HashSet<String>,
    git_root: PathBuf,
}

impl AutoReviewTracker {
    pub(crate) fn new(cwd: &Path) -> Self {
        let git_root = get_git_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());

        Self {
            running: HashSet::new(),
            processed: HashSet::new(),
            git_root,
        }
    }

    pub(crate) fn update(&mut self, event: &AgentStatusUpdateEvent) -> Vec<AutoReviewCompletion> {
        let mut completions: Vec<AutoReviewCompletion> = Vec::new();

        for agent in event.agents.iter() {
            if !matches!(agent.source_kind, Some(AgentSourceKind::AutoReview)) {
                continue;
            }

            let status = agent.status.to_ascii_lowercase();
            if status == "pending" || status == "running" {
                self.running.insert(agent.id.clone());
                continue;
            }

            let is_terminal = matches!(
                status.as_str(),
                "completed" | "failed" | "cancelled"
            );
            if !is_terminal || self.processed.contains(&agent.id) {
                continue;
            }

            self.running.remove(&agent.id);
            self.processed.insert(agent.id.clone());

            let summary = agent
                .result
                .as_deref()
                .map(parse_auto_review_summary)
                .unwrap_or_default();

            completions.push(AutoReviewCompletion {
                branch: agent.batch_id.clone(),
                worktree_path: agent
                    .batch_id
                    .as_deref()
                    .and_then(|branch| resolve_auto_review_worktree_path(&self.git_root, branch)),
                summary,
                error: agent.error.clone(),
            });
        }

        completions
    }

    pub(crate) fn is_running(&self) -> bool {
        !self.running.is_empty()
    }
}

pub(crate) fn emit_auto_review_completion(completion: &AutoReviewCompletion) {
    let branch = completion.branch.as_deref().unwrap_or("auto-review");

    if let Some(err) = completion.error.as_deref() {
        eprintln!("[auto-review] {branch}: failed: {err}");
        return;
    }

    let summary_text = completion
        .summary
        .summary
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("No issues reported.");

    if completion.summary.has_findings {
        let count = completion.summary.findings.max(1);
        if let Some(path) = completion.worktree_path.as_ref() {
            eprintln!(
                "[auto-review] {branch}: {count} issue(s) found. Merge {} to apply fixes. Summary: {summary_text}",
                path.display()
            );
        } else {
            eprintln!(
                "[auto-review] {branch}: {count} issue(s) found. Summary: {summary_text}"
            );
        }
    } else if summary_text == "No issues reported." {
        eprintln!("[auto-review] {branch}: no issues found.");
    } else {
        eprintln!("[auto-review] {branch}: no issues found. {summary_text}");
    }
}

fn parse_auto_review_summary(raw: &str) -> AutoReviewSummary {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return AutoReviewSummary::default();
    }

    #[derive(serde::Deserialize)]
    struct MultiRun {
        #[serde(flatten)]
        latest: ReviewOutputEvent,
        #[serde(default)]
        runs: Vec<ReviewOutputEvent>,
    }

    if let Ok(wrapper) = serde_json::from_str::<MultiRun>(trimmed) {
        let mut runs = wrapper.runs;
        if runs.is_empty() {
            runs.push(wrapper.latest);
        }
        return summary_from_runs(&runs);
    }

    if let Ok(output) = serde_json::from_str::<ReviewOutputEvent>(trimmed) {
        return summary_from_output(&output);
    }

    if let Some(start) = trimmed.find("```")
        && let Some((body, _)) = trimmed[start + 3..].split_once("```") {
            let candidate = body.trim_start_matches("json").trim();
            if let Ok(output) = serde_json::from_str::<ReviewOutputEvent>(candidate) {
                return summary_from_output(&output);
            }
        }

    let lowered = trimmed.to_ascii_lowercase();
    let clean_phrases = [
        "no issues",
        "no findings",
        "clean",
        "looks good",
        "nothing to fix",
    ];
    let skip_phrases = [
        "already running",
        "another review",
        "skipping this",
        "skip this",
    ];
    let issue_markers = [
        "issue",
        "issues",
        "finding",
        "findings",
        "bug",
        "bugs",
        "problem",
        "problems",
        "error",
        "errors",
    ];

    if skip_phrases.iter().any(|p| lowered.contains(p)) {
        return AutoReviewSummary {
            has_findings: false,
            findings: 0,
            summary: Some(trimmed.to_string()),
        };
    }

    if clean_phrases.iter().any(|p| lowered.contains(p)) {
        return AutoReviewSummary {
            has_findings: false,
            findings: 0,
            summary: Some(trimmed.to_string()),
        };
    }

    let has_findings = issue_markers.iter().any(|p| lowered.contains(p));

    AutoReviewSummary {
        has_findings,
        findings: 0,
        summary: Some(trimmed.to_string()),
    }
}

fn summary_from_runs(outputs: &[ReviewOutputEvent]) -> AutoReviewSummary {
    let Some(latest) = outputs.last() else {
        return AutoReviewSummary::default();
    };
    let mut summary = summary_from_output(latest);

    if let Some(idx) = outputs.iter().rposition(|o| !o.findings.is_empty()) {
        let with_findings = summary_from_output(&outputs[idx]);
        if with_findings.has_findings {
            summary.has_findings = true;
            summary.findings = with_findings.findings;
            summary.summary = with_findings.summary.or(summary.summary);

            if latest.findings.is_empty() {
                let tail = "Final pass reported no issues after auto-resolve.";
                summary.summary = match summary.summary {
                    Some(ref existing) if existing.contains(tail) => Some(existing.clone()),
                    Some(existing) => Some(format!("{existing} \n{tail}")),
                    None => Some(tail.to_string()),
                };
            }
        }
    }

    summary
}

fn summary_from_output(output: &ReviewOutputEvent) -> AutoReviewSummary {
    let findings = output.findings.len();
    let has_findings = findings > 0;

    let mut parts: Vec<String> = Vec::new();
    if !output.overall_explanation.trim().is_empty() {
        parts.push(output.overall_explanation.trim().to_string());
    }
    if has_findings {
        let titles: Vec<String> = output
            .findings
            .iter()
            .filter_map(|f| {
                let title = f.title.trim();
                (!title.is_empty()).then_some(title.to_string())
            })
            .collect();
        if !titles.is_empty() {
            parts.push(format!("Findings: {}", titles.join("; ")));
        }
    }

    let summary = (!parts.is_empty()).then(|| parts.join(" \n"));

    AutoReviewSummary {
        has_findings,
        findings,
        summary,
    }
}

fn auto_review_branches_dir(git_root: &Path) -> Option<PathBuf> {
    let repo_name = git_root.file_name()?.to_str()?;
    let mut code_home = code_core::config::find_code_home().ok()?;
    code_home = code_home.join("working").join(repo_name).join("branches");
    std::fs::create_dir_all(&code_home).ok()?;
    Some(code_home)
}

fn resolve_auto_review_worktree_path(git_root: &Path, branch: &str) -> Option<PathBuf> {
    if branch.is_empty() {
        return None;
    }

    let branches_dir = auto_review_branches_dir(git_root)?;
    let candidate = branches_dir.join(branch);
    candidate.exists().then_some(candidate)
}
