use super::super::*;

pub(crate) const DOUBLE_ESC_HINT: &str = "undo timeline";
pub(crate) const AUTO_ESC_EXIT_HINT: &str = "Press Esc to exit Auto Drive";
pub(crate) const AUTO_ESC_EXIT_HINT_DOUBLE: &str = "Press Esc again to exit Auto Drive";
pub(crate) const AUTO_COMPLETION_CELEBRATION_DURATION: Duration = Duration::from_secs(5);
pub(crate) const HISTORY_ANIMATION_FRAME_INTERVAL: Duration = Duration::from_millis(120);
pub(crate) const AUTO_BOOTSTRAP_GOAL_PLACEHOLDER: &str = "Deriving goal from recent conversation";
pub(crate) const AUTO_DRIVE_SESSION_SUMMARY_NOTICE: &str = "Summarizing session";
pub(crate) const AUTO_DRIVE_SESSION_SUMMARY_PROMPT: &str =
    include_str!("../../../prompt_for_auto_drive_session_summary.md");
pub(crate) const CONTEXT_DELTA_HISTORY: usize = 10;

pub(crate) struct MergeRepoState {
    pub(crate) git_root: PathBuf,
    pub(crate) worktree_path: PathBuf,
    pub(crate) worktree_branch: String,
    pub(crate) worktree_sha: String,
    pub(crate) worktree_status: String,
    pub(crate) worktree_dirty: bool,
    pub(crate) worktree_status_ok: bool,
    pub(crate) worktree_diff_summary: Option<String>,
    pub(crate) repo_status: String,
    pub(crate) repo_dirty: bool,
    pub(crate) repo_status_ok: bool,
    pub(crate) default_branch: Option<String>,
    pub(crate) default_branch_exists: bool,
    pub(crate) repo_head_branch: Option<String>,
    pub(crate) repo_has_in_progress_op: bool,
    pub(crate) fast_forward_possible: bool,
}

impl MergeRepoState {
    pub(crate) async fn gather(worktree_path: PathBuf, git_root: PathBuf) -> Result<Self, String> {
        use tokio::process::Command;

        let worktree_branch = match Command::new("git")
            .current_dir(&worktree_path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .await
        {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            }
            _ => {
                return Err("failed to detect worktree branch name".to_string());
            }
        };

        let worktree_sha = match Command::new("git")
            .current_dir(&worktree_path)
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .await
        {
            Ok(out) if out.status.success() => {
                let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if sha.is_empty() {
                    "unknown".to_string()
                } else {
                    sha
                }
            }
            _ => "unknown".to_string(),
        };

        let worktree_status_raw = ChatWidget::git_short_status(&worktree_path).await;
        let (worktree_status, worktree_dirty, worktree_status_ok) =
            Self::normalize_status(worktree_status_raw);
        let worktree_diff_summary = if worktree_dirty {
            ChatWidget::git_diff_stat(&worktree_path)
                .await
                .ok()
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
        } else {
            None
        };

        let branch_metadata = code_core::git_worktree::load_branch_metadata(&worktree_path);
        let mut default_branch = branch_metadata
            .as_ref()
            .and_then(|meta| meta.base_branch.clone());
        if default_branch.is_none() {
            default_branch = code_core::git_worktree::detect_default_branch(&git_root).await;
        }

        let repo_status_raw = ChatWidget::git_short_status(&git_root).await;
        let (repo_status, repo_dirty, repo_status_ok) = Self::normalize_status(repo_status_raw);

        let repo_head_branch = match Command::new("git")
            .current_dir(&git_root)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .await
        {
            Ok(out) if out.status.success() => {
                Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
            }
            _ => None,
        };

        let (default_branch_exists, fast_forward_possible) =
            if let Some(ref default_branch) = default_branch {
                let exists = Command::new("git")
                    .current_dir(&git_root)
                    .args([
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("refs/heads/{default_branch}"),
                    ])
                    .output()
                    .await
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                let fast_forward = if exists {
                    Command::new("git")
                        .current_dir(&git_root)
                        .args([
                            "merge-base",
                            "--is-ancestor",
                            &format!("refs/heads/{default_branch}"),
                            &format!("refs/heads/{worktree_branch}"),
                        ])
                        .output()
                        .await
                        .map(|o| o.status.success())
                        .unwrap_or(false)
                } else {
                    false
                };
                (exists, fast_forward)
            } else {
                (false, false)
            };

        let git_dir = match Command::new("git")
            .current_dir(&git_root)
            .args(["rev-parse", "--git-dir"])
            .output()
            .await
        {
            Ok(out) if out.status.success() => {
                let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let candidate = PathBuf::from(&raw);
                if candidate.is_absolute() {
                    candidate
                } else {
                    git_root.join(raw)
                }
            }
            _ => git_root.join(".git"),
        };
        let repo_has_in_progress_op = [
            "MERGE_HEAD",
            "rebase-apply",
            "rebase-merge",
            "CHERRY_PICK_HEAD",
            "BISECT_LOG",
        ]
        .iter()
        .any(|name| git_dir.join(name).exists());

        remember_worktree_root_hint(&worktree_path, &git_root);
        Ok(MergeRepoState {
            git_root,
            worktree_path,
            worktree_branch,
            worktree_sha,
            worktree_status,
            worktree_dirty,
            worktree_status_ok,
            worktree_diff_summary,
            repo_status,
            repo_dirty,
            repo_status_ok,
            default_branch,
            default_branch_exists,
            repo_head_branch,
            repo_has_in_progress_op,
            fast_forward_possible,
        })
    }

    pub(crate) fn normalize_status(result: Result<String, String>) -> (String, bool, bool) {
        match result {
            Ok(s) => {
                let trimmed = s.trim().to_string();
                if trimmed.is_empty() {
                    ("clean".to_string(), false, true)
                } else {
                    (trimmed, true, true)
                }
            }
            Err(err) => (format!("status unavailable: {err}"), true, false),
        }
    }

    pub(crate) fn snapshot_summary(&self) -> String {
        let worktree_state = if !self.worktree_status_ok {
            "unknown"
        } else if self.worktree_dirty {
            "dirty"
        } else {
            "clean"
        };
        let repo_state = if !self.repo_status_ok {
            "unknown"
        } else if self.repo_dirty {
            "dirty"
        } else {
            "clean"
        };
        format!(
            "`/merge` — repo snapshot: worktree '{}' ({}) → default '{}' ({}), fast-forward: {}",
            self.worktree_branch,
            worktree_state,
            self.default_branch_label(),
            repo_state,
            if self.fast_forward_possible { "yes" } else { "no" }
        )
    }

    pub(crate) fn auto_fast_forward_blockers(&self) -> Vec<String> {
        let mut reasons = Vec::new();
        if !self.worktree_status_ok {
            reasons.push("unable to read worktree status".to_string());
        }
        if self.worktree_dirty {
            reasons.push("worktree has uncommitted changes".to_string());
        }
        if !self.repo_status_ok {
            reasons.push("unable to read repo status".to_string());
        }
        if self.repo_dirty {
            reasons.push(format!(
                "{} checkout has uncommitted changes",
                self.default_branch_label()
            ));
        }
        if self.repo_has_in_progress_op {
            reasons.push(
                "default checkout has an in-progress merge/rebase/cherry-pick".to_string(),
            );
        }
        if self.default_branch.is_none() {
            reasons.push("default branch is unknown".to_string());
        }
        if self.default_branch.is_some() && !self.default_branch_exists {
            reasons.push(format!(
                "default branch '{}' missing locally",
                self.default_branch_label()
            ));
        }
        match (&self.repo_head_branch, &self.default_branch) {
            (Some(head), Some(default)) if head == default => {}
            (Some(head), Some(default)) => reasons.push(format!(
                "repo root is on '{head}' instead of '{default}'"
            )),
            (Some(_), None) => reasons.push(
                "repo root branch detected but default branch is still unknown".to_string(),
            ),
            (None, _) => reasons.push("unable to detect branch currently checked out in repo root".to_string()),
        }
        if !self.fast_forward_possible {
            reasons.push("fast-forward merge is not possible".to_string());
        }
        reasons
    }

    pub(crate) fn default_branch_label(&self) -> String {
        self.default_branch
            .as_deref()
            .unwrap_or("default branch (determine before merging)")
            .to_string()
    }

    pub(crate) fn agent_preface(&self, reason_text: &str) -> String {
        let default_branch_line = self
            .default_branch
            .as_deref()
            .unwrap_or("unknown default branch (determine before merging)");
        let worktree_status = Self::format_status_for_context(&self.worktree_status);
        let repo_status = Self::format_status_for_context(&self.repo_status);
        let fast_forward_label = if self.fast_forward_possible { "yes" } else { "no" };
        let mut preface = format!(
            "[developer] Automation skipped because: {reason_text}. Finish the merge manually with the steps below.\n\nContext:\n- Worktree path: {worktree_path} — branch {worktree_branch} @ {worktree_sha}, status {worktree_status}\n- Repo root path (current cwd): {git_root} — target {default_branch_line} checkout, status {repo_status}\n- Fast-forward possible: {fast_forward_label}\n",
            reason_text = reason_text,
            worktree_path = self.worktree_path.display(),
            worktree_branch = self.worktree_branch.as_str(),
            worktree_sha = self.worktree_sha.as_str(),
            worktree_status = worktree_status,
            git_root = self.git_root.display(),
            default_branch_line = default_branch_line,
            repo_status = repo_status,
            fast_forward_label = fast_forward_label,
        );
        preface.push_str(
            "\nNOTE: Each command runs in its own shell. `/merge` switches the working directory to the repo root; use `git -C <path> ...` or `cd <path> && ...` whenever you need to operate in a different directory.\n",
        );
        preface.push_str(&format!(
            "\n1. Worktree prep (worktree {worktree_path} on {worktree_branch}):\n   - Review `git status`.\n   - Stage and commit every change that belongs in the merge. Use descriptive messages; no network commands and no resets.\n",
            worktree_path = self.worktree_path.display(),
            worktree_branch = self.worktree_branch.as_str(),
        ));
        preface.push_str(&format!(
            "   - Run worktree commands as `git -C {worktree_path}` (or `cd {worktree_path} && ...`) so they execute inside the worktree.\n",
            worktree_path = self.worktree_path.display(),
        ));
        if let Some(ref default_branch) = self.default_branch {
            preface.push_str(&format!(
                "2. Default-branch checkout prep (repo root {git_root}):\n   - If HEAD is not {default_branch}, run `git checkout {default_branch}`.\n   - If this checkout is dirty, stash with a clear message before continuing.\n",
                git_root = self.git_root.display(),
                default_branch = default_branch,
            ));
        } else {
            preface.push_str(&format!(
                "2. Default-branch checkout prep (repo root {git_root}):\n   - Determine the correct default branch for this repo (metadata missing) and check it out.\n   - If this checkout is dirty, stash with a clear message before continuing.\n",
                git_root = self.git_root.display(),
            ));
        }
        let default_branch_for_copy = self
            .default_branch
            .as_deref()
            .unwrap_or("the default branch you selected");
        preface.push_str(&format!(
            "3. Merge locally (repo root {git_root} on {default_branch_for_copy}):\n   - Run `git merge --no-ff {worktree_branch}`.\n   - Resolve conflicts line by line; keep intent from both branches.\n   - No network commands, no `git reset --hard`, no `git checkout -- .`, no `git clean`, and no `-X ours/theirs`.\n   - WARNING: Do not delete files, rewrite them in full, or checkout/prefer commits from one branch over another. Instead use apply_patch to surgically resolve conflicts, even if they are large in scale. Work on each conflict, line by line, so both branches' changes survive.\n   - If you stashed in step 2, apply/pop it now and commit if needed.\n",
            git_root = self.git_root.display(),
            default_branch_for_copy = default_branch_for_copy,
            worktree_branch = self.worktree_branch.as_str(),
        ));
        preface.push_str(&format!(
            "4. Verify in {git_root}:\n   - `git status` is clean.\n   - `git merge-base --is-ancestor {worktree_branch} HEAD` succeeds.\n   - No MERGE_HEAD/rebase/cherry-pick artifacts remain.\n",
            git_root = self.git_root.display(),
            worktree_branch = self.worktree_branch.as_str(),
        ));
        preface.push_str(&format!(
            "5. Cleanup:\n   - `git worktree remove {worktree_path}` (only after verification).\n   - `git branch -D {worktree_branch}` in {git_root} if the branch still exists.\n",
            worktree_path = self.worktree_path.display(),
            worktree_branch = self.worktree_branch.as_str(),
            git_root = self.git_root.display(),
        ));
        preface.push_str(
            "6. Report back with a concise command log and any conflicts you resolved.\n\nAbsolute rules: no network operations, no resets, no dropping local history, no blanket \"ours/theirs\" strategies.\n",
        );
        if let Some(diff) = &self.worktree_diff_summary {
            preface.push_str("\nWorktree diff summary:\n");
            preface.push_str(diff);
        }
        preface
    }

    pub(crate) fn format_status_for_context(status: &str) -> String {
        if status == "clean" {
            return "clean".to_string();
        }
        status
            .lines()
            .enumerate()
            .map(|(idx, line)| if idx == 0 { line.to_string() } else { format!("  {line}") })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub(crate) async fn run_fast_forward_merge(state: &MergeRepoState) -> Result<(), String> {
    use tokio::process::Command;

    let merge = Command::new("git")
        .current_dir(&state.git_root)
        .args(["merge", "--ff-only", &state.worktree_branch])
        .output()
        .await
        .map_err(|err| format!("failed to run git merge --ff-only: {err}"))?;
    if !merge.status.success() {
        return Err(format!(
            "fast-forward merge failed: {}",
            describe_command_failure(&merge, "git merge --ff-only failed")
        ));
    }

    bump_snapshot_epoch_for(&state.git_root);

    let worktree_remove = Command::new("git")
        .current_dir(&state.git_root)
        .args(["worktree", "remove"])
        .arg(&state.worktree_path)
        .arg("--force")
        .output()
        .await
        .map_err(|err| format!("failed to remove worktree: {err}"))?;
    if !worktree_remove.status.success() {
        return Err(format!(
            "failed to remove worktree: {}",
            describe_command_failure(&worktree_remove, "git worktree remove failed")
        ));
    }

    let branch_delete = Command::new("git")
        .current_dir(&state.git_root)
        .args(["branch", "-D", &state.worktree_branch])
        .output()
        .await
        .map_err(|err| format!("failed to delete branch: {err}"))?;
    if !branch_delete.status.success() {
        return Err(format!(
            "failed to delete branch '{}': {}",
            state.worktree_branch,
            describe_command_failure(&branch_delete, "git branch -D failed")
        ));
    }

    Ok(())
}

pub(crate) fn describe_command_failure(out: &Output, fallback: &str) -> String {
    let stderr_s = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let stdout_s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !stderr_s.is_empty() {
        stderr_s
    } else if !stdout_s.is_empty() {
        stdout_s
    } else {
        fallback.to_string()
    }
}
