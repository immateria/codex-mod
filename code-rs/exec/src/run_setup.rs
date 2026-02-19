use crate::auto_runtime::append_timeboxed_auto_drive_goal;
use crate::cli::Command as ExecCommand;
use crate::prompt_input::resolve_prompt;
use crate::review_command::build_review_request;
use crate::review_command::review_summary;
use code_core::protocol::ReviewRequest;
use std::path::PathBuf;

pub(crate) struct PreparedRunInputs {
    pub(crate) review_request: Option<ReviewRequest>,
    pub(crate) prompt_to_send: String,
    pub(crate) summary_prompt: String,
    pub(crate) auto_drive_goal: Option<String>,
    pub(crate) images: Vec<PathBuf>,
    pub(crate) timeboxed_auto_exec: bool,
}

pub(crate) fn prepare_run_inputs(
    command: &Option<ExecCommand>,
    prompt: Option<String>,
    images: Vec<PathBuf>,
    auto_drive: bool,
    max_seconds: Option<u64>,
) -> PreparedRunInputs {
    let review_request = match command {
        Some(ExecCommand::Review(args)) => Some(build_review_request(args.clone()).unwrap_or_else(|err| {
            eprintln!("{err}");
            std::process::exit(1);
        })),
        _ => None,
    };

    // Determine the prompt source (parent or subcommand) and read from stdin if needed.
    let prompt_arg = match command {
        // Allow prompt before the subcommand by falling back to the parent-level prompt
        // when the Resume subcommand did not provide its own prompt.
        Some(ExecCommand::Resume(args)) => args.prompt.clone().or(prompt),
        Some(ExecCommand::Review(_)) => None,
        None => prompt,
    };
    let images = match command {
        Some(ExecCommand::Resume(args)) => {
            let mut merged = images;
            merged.extend(args.images.iter().cloned());
            merged
        }
        Some(ExecCommand::Review(_)) => images,
        None => images,
    };

    if review_request.is_some() && auto_drive {
        eprintln!("--auto is not supported with the `review` subcommand.");
        std::process::exit(1);
    }

    let prompt = if review_request.is_some() {
        String::new()
    } else {
        resolve_prompt(prompt_arg)
    };

    let mut auto_drive_goal: Option<String> = None;
    let trimmed_prompt = prompt.trim();
    if trimmed_prompt.starts_with("/auto") {
        auto_drive_goal = Some(trimmed_prompt.trim_start_matches("/auto").trim().to_string());
    }
    if auto_drive {
        if trimmed_prompt.is_empty() {
            eprintln!("Auto Drive requires a goal. Provide one after --auto or prefix the prompt with /auto.");
            std::process::exit(1);
        }
        if auto_drive_goal
            .as_deref()
            .is_none_or(str::is_empty)
        {
            auto_drive_goal = Some(trimmed_prompt.to_string());
        }
    }

    if auto_drive_goal
        .as_ref()
        .is_some_and(|g| g.trim().is_empty())
    {
        eprintln!("Auto Drive requires a goal. Provide one after /auto or --auto.");
        std::process::exit(1);
    }

    let timeboxed_auto_exec = auto_drive_goal.is_some() && max_seconds.is_some();
    if timeboxed_auto_exec
        && let Some(goal) = auto_drive_goal.as_mut() {
            *goal = append_timeboxed_auto_drive_goal(goal);
        }

    let prompt_to_send = prompt.clone();
    let summary_prompt = if let Some(request) = review_request.as_ref() {
        review_summary(request)
    } else if let Some(goal) = auto_drive_goal.as_ref() {
        format!("/auto {goal}")
    } else {
        prompt
    };

    PreparedRunInputs {
        review_request,
        prompt_to_send,
        summary_prompt,
        auto_drive_goal,
        images,
        timeboxed_auto_exec,
    }
}
