use code_auto_drive_core::AUTO_RESOLVE_REVIEW_FOLLOWUP;
use code_auto_drive_core::AutoResolveState;
use code_core::protocol::ReviewRequest;
use code_git_tooling::GhostCommit;
use std::path::Path;

use crate::review_output::format_review_findings;

pub(crate) fn snapshot_parent_diff_paths(cwd: &Path, parent: &str, head: &str) -> Option<Vec<String>> {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["diff", "--name-only", parent, head])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let paths: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(std::string::ToString::to_string)
        .collect();

    Some(paths)
}

pub(crate) fn apply_commit_scope_to_review_request(
    mut request: ReviewRequest,
    commit: &str,
    parent: &str,
    paths: Option<&[String]>,
) -> ReviewRequest {
    let short_commit: String = commit.chars().take(7).collect();
    let short_parent: String = parent.chars().take(7).collect();

    let mut prompt = request.prompt.trim_end().to_string();
    prompt.push_str("\n\nReview scope: changes captured in commit ");
    prompt.push_str(commit);
    prompt.push_str(" (parent ");
    prompt.push_str(parent);
    prompt.push(')');
    prompt.push('.');

    if let Some(paths) = paths
        && !paths.is_empty() {
            prompt.push_str("\nFiles changed in this snapshot:\n");
            for path in paths {
                prompt.push_str("- ");
                prompt.push_str(path);
                prompt.push('\n');
            }
        }

    request.prompt = prompt;
    request.user_facing_hint = Some(format!("commit {short_commit} (parent {short_parent})"));
    request.target = code_protocol::protocol::ReviewTarget::Custom {
        instructions: request.prompt.clone(),
    };
    request
}

pub(crate) fn capture_snapshot_against_base(
    cwd: &Path,
    base: &GhostCommit,
    message: &'static str,
    capture_snapshot: impl Fn(&Path, Option<&str>, &'static str) -> Option<GhostCommit>,
    bump_snapshot_epoch: impl Fn(&Path),
) -> Option<(GhostCommit, Vec<String>)> {
    let snapshot = capture_snapshot(cwd, Some(base.id()), message)?;
    let diff_paths = snapshot_parent_diff_paths(cwd, base.id(), snapshot.id())?;
    if diff_paths.is_empty() {
        return None;
    }
    bump_snapshot_epoch(cwd);
    Some((snapshot, diff_paths))
}

pub(crate) fn strip_scope_from_prompt(prompt: &str) -> String {
    let mut base = prompt.trim_end().to_string();
    if let Some(idx) = base.find(AUTO_RESOLVE_REVIEW_FOLLOWUP) {
        base = base[..idx].trim_end().to_string();
    }
    let filtered: Vec<&str> = base
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with("Review scope:") || trimmed.starts_with("commit "))
        })
        .collect();
    filtered.join("\n")
}

/// Remove lines that pin the review to specific commit hashes so follow-up
/// reviews can safely re-scope to the newest snapshot.
fn strip_commit_mentions(prompt: &str, commits: &[&str]) -> String {
    prompt
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed
                .to_ascii_lowercase()
                .contains("analyze only changes made in commit")
            {
                return false;
            }
            for c in commits {
                if !c.is_empty() && trimmed.contains(c) {
                    return false;
                }
            }
            true
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn should_skip_followup(last_reviewed_commit: Option<&str>, next_snapshot: &GhostCommit) -> bool {
    match last_reviewed_commit {
        Some(prev) => prev == next_snapshot.id(),
        None => false,
    }
}

/// Returns true if the current HEAD is an ancestor of `base_commit`.
///
/// Ghost snapshots are created as children of the then-current HEAD. That means
/// HEAD should be an ancestor of the snapshot immediately after creation. If
/// HEAD moves later (new commits, rebases, etc.) it may no longer be an
/// ancestor, which indicates the snapshot is stale relative to the live branch
/// we plan to patch against.
pub(crate) fn head_is_ancestor_of_base(cwd: &Path, base_commit: &str) -> bool {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["merge-base", "--is-ancestor", "HEAD", base_commit])
        .output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

pub(crate) async fn build_followup_review_request(
    state: &AutoResolveState,
    _cwd: &Path,
    snapshot: Option<&GhostCommit>,
    diff_paths: Option<&[String]>,
    parent_commit: Option<&str>,
) -> ReviewRequest {
    let mut prompt = strip_scope_from_prompt(&state.prompt);

    let mut user_facing_hint = (!state.hint.trim().is_empty()).then(|| state.hint.clone());

    if let (Some(snapshot), Some(parent)) = (snapshot, parent_commit) {
        let updated = apply_commit_scope_to_review_request(
            ReviewRequest {
                target: code_protocol::protocol::ReviewTarget::Custom {
                    instructions: prompt.clone(),
                },
                prompt: prompt.clone(),
                user_facing_hint: user_facing_hint.clone(),
            },
            snapshot.id(),
            parent,
            diff_paths,
        );
        prompt = updated.prompt;
        user_facing_hint = updated.user_facing_hint;
    }

    // Strip lingering references to earlier commits so follow-up /review scopes to
    // the freshly captured snapshot instead of the original hash baked into the
    // user prompt.
    let mut commit_ids: Vec<&str> = Vec::new();
    if let Some(last) = state.last_reviewed_commit.as_deref() {
        commit_ids.push(last);
    }
    if let Some(parent) = parent_commit {
        commit_ids.push(parent);
    }
    prompt = strip_commit_mentions(&prompt, &commit_ids);

    if let Some(last_review) = state.last_review.as_ref() {
        let recap = format_review_findings(last_review);
        if !recap.is_empty() {
            prompt.push_str("\n\nPreviously reported findings to re-validate:\n");
            prompt.push_str(&recap);
        }
    }

    if !prompt.contains(AUTO_RESOLVE_REVIEW_FOLLOWUP) {
        prompt.push_str("\n\n");
        prompt.push_str(AUTO_RESOLVE_REVIEW_FOLLOWUP);
    }

    let target = code_protocol::protocol::ReviewTarget::Custom {
        instructions: prompt.clone(),
    };
    ReviewRequest {
        target,
        user_facing_hint,
        prompt,
    }
}
