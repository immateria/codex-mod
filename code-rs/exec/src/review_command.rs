use crate::cli::ReviewArgs;
use crate::prompt_input::resolve_prompt;
use code_core::protocol::ReviewRequest;
use code_protocol::protocol::ReviewTarget;

pub(crate) fn build_review_request(args: ReviewArgs) -> anyhow::Result<ReviewRequest> {
    let (target, prompt, hint) = if args.uncommitted {
        let prompt = "Review the current workspace changes and highlight bugs, regressions, risky patterns, and missing tests before merge.".to_string();
        (
            ReviewTarget::UncommittedChanges,
            prompt,
            Some("current workspace changes".to_string()),
        )
    } else if let Some(branch) = args.base {
        let prompt =
            format!("Review the current branch changes against base branch `{branch}`.");
        (
            ReviewTarget::BaseBranch {
                branch: branch.clone(),
            },
            prompt,
            Some(format!("changes against base branch {branch}")),
        )
    } else if let Some(sha) = args.commit {
        let prompt = match args.commit_title.as_deref() {
            Some(title) if !title.trim().is_empty() => {
                format!(
                    "Review changes introduced by commit {sha} ({title})."
                )
            }
            _ => format!("Review changes introduced by commit {sha}."),
        };
        (
            ReviewTarget::Commit {
                sha,
                title: args.commit_title,
            },
            prompt,
            Some("selected commit".to_string()),
        )
    } else if let Some(prompt_arg) = args.prompt {
        let prompt = resolve_prompt(Some(prompt_arg)).trim().to_string();
        if prompt.is_empty() {
            anyhow::bail!("Review prompt cannot be empty");
        }
        (
            ReviewTarget::Custom {
                instructions: prompt.clone(),
            },
            prompt.clone(),
            Some(prompt),
        )
    } else {
        anyhow::bail!(
            "Specify --uncommitted, --base, --commit, or provide custom review instructions"
        );
    };

    Ok(ReviewRequest {
        target,
        user_facing_hint: hint,
        prompt,
    })
}

pub(crate) fn review_summary(review_request: &ReviewRequest) -> String {
    match &review_request.target {
        ReviewTarget::UncommittedChanges => "/review --uncommitted".to_string(),
        ReviewTarget::BaseBranch { branch } => {
            format!("/review --base {branch}")
        }
        ReviewTarget::Commit { sha, .. } => {
            format!("/review --commit {sha}")
        }
        ReviewTarget::Custom { instructions } => {
            let trimmed = instructions.replace('\n', " ").trim().to_string();
            if trimmed.is_empty() {
                "/review".to_string()
            } else {
                format!("/review {trimmed}")
            }
        }
    }
}
