use clap::Args;
use clap::FromArgMatches;
use clap::Parser;
use clap::ValueEnum;
use code_common::CliConfigOverrides;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version)]
pub struct Cli {
    /// Action to perform. If omitted, runs a new non-interactive session.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Run Auto Drive instead of a single non-interactive turn.
    #[arg(long = "auto", default_value_t = false)]
    pub auto_drive: bool,

    /// Use Auto Review models and limits for /review runs.
    #[arg(long = "auto-review", default_value_t = false)]
    pub auto_review: bool,

    /// Optional image(s) to attach to the initial prompt.
    #[arg(
        long = "image",
        short = 'i',
        value_name = "FILE",
        value_delimiter = ',',
        num_args = 1..
    )]
    pub images: Vec<PathBuf>,

    /// Model the agent should use.
    #[arg(long, short = 'm', global = true)]
    pub model: Option<String>,

    #[arg(long = "oss", default_value_t = false)]
    pub oss: bool,

    /// Select the sandbox policy to use when executing model-generated shell
    /// commands.
    #[arg(long = "sandbox", short = 's', value_enum)]
    pub sandbox_mode: Option<code_common::SandboxModeCliArg>,

    /// Configuration profile from config.toml to specify default options.
    #[arg(long = "profile", short = 'p')]
    pub config_profile: Option<String>,

    /// Convenience alias for low-friction sandboxed automatic execution (-a on-failure, --sandbox workspace-write).
    #[arg(long = "full-auto", default_value_t = false, global = true)]
    pub full_auto: bool,

    /// Skip all confirmation prompts and execute commands without sandboxing.
    /// EXTREMELY DANGEROUS. Intended solely for running in environments that are externally sandboxed.
    #[arg(
        long = "dangerously-bypass-approvals-and-sandbox",
        alias = "yolo",
        default_value_t = false,
        global = true,
        conflicts_with = "full_auto"
    )]
    pub dangerously_bypass_approvals_and_sandbox: bool,

    /// Tell the agent to use the specified directory as its working root.
    #[clap(long = "cd", short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// Enable debug logging of all LLM requests and responses to files.
    #[clap(long = "debug", short = 'd', default_value_t = false)]
    pub debug: bool,

    /// Allow running Codex outside a Git repository.
    #[arg(long = "skip-git-repo-check", default_value_t = false, global = true)]
    pub skip_git_repo_check: bool,

    /// Path to a JSON Schema file describing the model's final response shape.
    #[arg(long = "output-schema", value_name = "FILE")]
    pub output_schema: Option<PathBuf>,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Optional developer-role message to prepend to every turn for demos.
    /// Set by the multitool root CLI via `--demo`.
    #[clap(skip)]
    pub demo_developer_message: Option<String>,

    /// Specifies color settings for use in the output.
    #[arg(long = "color", value_enum, default_value_t = Color::Auto)]
    pub color: Color,

    /// Print events to stdout as JSONL.
    #[arg(
        long = "json",
        alias = "experimental-json",
        default_value_t = false,
        global = true
    )]
    pub json: bool,

    /// Maximum wall-clock time budget (seconds) before aborting the run.
    ///
    /// When this budget is at least 50% consumed, the coordinator receives
    /// increasingly frequent reminders to converge and finish.
    #[arg(
        long = "max-seconds",
        value_name = "SECONDS",
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub max_seconds: Option<u64>,

    /// Maximum number of Auto Drive coordinator turns before stopping (0 = unlimited).
    /// Only meaningful when --auto is enabled.
    #[arg(long = "turn-cap", value_name = "TURNS")]
    pub turn_cap: Option<u32>,

    /// Whether to include the plan tool in the conversation.
    #[arg(long = "include-plan-tool", default_value_t = false)]
    pub include_plan_tool: bool,

    /// Specifies file where the last message from the agent should be written.
    #[arg(long = "output-last-message", short = 'o', value_name = "FILE")]
    pub last_message_file: Option<PathBuf>,

    /// When running /review, write the structured review output JSON to this file.
    /// Includes every review pass (e.g., auto-resolve loops) so findings are not
    /// lost when later passes report "no issues".
    #[arg(long = "review-output-json", value_name = "FILE")]
    pub review_output_json: Option<PathBuf>,

    /// Initial instructions for the agent. If not provided as an argument (or
    /// if `-` is used), instructions are read from stdin.
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Resume a previous session by id or pick the most recent with --last.
    Resume(ResumeArgs),

    /// Run a code review against the current repository.
    Review(ReviewArgs),
}

#[derive(Args, Debug)]
struct ResumeArgsRaw {
    // Note: This is the direct clap shape. We reinterpret the positional when --last is set
    // so `code-exec resume --last <prompt>` treats the positional as a prompt, not a session id.
    /// Conversation/session id (UUID). When provided, resumes this session.
    /// If omitted, use --last to pick the most recent recorded session.
    #[arg(value_name = "SESSION_ID", value_hint = clap::ValueHint::Other)]
    session_id: Option<String>,

    /// Resume the most recent recorded session (newest) without specifying an id.
    #[arg(long = "last", default_value_t = false)]
    last: bool,

    /// Show all sessions (disable cwd filtering) when combined with --last.
    #[arg(long = "all", default_value_t = false)]
    all: bool,

    /// Optional image(s) to attach to the prompt sent after resuming.
    #[arg(
        long = "image",
        short = 'i',
        value_name = "FILE",
        value_delimiter = ',',
        num_args = 1..
    )]
    images: Vec<PathBuf>,

    /// Prompt to send after resuming the session. If `-` is used, read from stdin.
    #[arg(value_name = "PROMPT", value_hint = clap::ValueHint::Other)]
    prompt: Option<String>,
}

#[derive(Debug)]
pub struct ResumeArgs {
    /// Conversation/session id (UUID). When provided, resumes this session.
    /// If omitted, use --last to pick the most recent recorded session.
    pub session_id: Option<String>,

    /// Resume the most recent recorded session (newest) without specifying an id.
    pub last: bool,

    /// Show all sessions (disable cwd filtering) when combined with --last.
    pub all: bool,

    /// Optional image(s) to attach to the prompt sent after resuming.
    pub images: Vec<PathBuf>,

    /// Prompt to send after resuming the session. If `-` is used, read from stdin.
    pub prompt: Option<String>,
}

impl From<ResumeArgsRaw> for ResumeArgs {
    fn from(raw: ResumeArgsRaw) -> Self {
        // When --last is used without an explicit prompt, treat the positional as the prompt.
        let (session_id, prompt) = if raw.last && raw.prompt.is_none() {
            (None, raw.session_id)
        } else {
            (raw.session_id, raw.prompt)
        };
        Self {
            session_id,
            last: raw.last,
            all: raw.all,
            images: raw.images,
            prompt,
        }
    }
}

impl Args for ResumeArgs {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        ResumeArgsRaw::augment_args(cmd)
    }

    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        ResumeArgsRaw::augment_args_for_update(cmd)
    }
}

impl FromArgMatches for ResumeArgs {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        ResumeArgsRaw::from_arg_matches(matches).map(Self::from)
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = ResumeArgsRaw::from_arg_matches(matches).map(Self::from)?;
        Ok(())
    }
}

#[derive(Parser, Debug, Clone)]
pub struct ReviewArgs {
    /// Review staged, unstaged, and untracked changes.
    #[arg(
        long = "uncommitted",
        default_value_t = false,
        conflicts_with_all = ["base", "commit", "prompt"]
    )]
    pub uncommitted: bool,

    /// Review changes against the given base branch.
    #[arg(
        long = "base",
        value_name = "BRANCH",
        conflicts_with_all = ["uncommitted", "commit", "prompt"]
    )]
    pub base: Option<String>,

    /// Review the changes introduced by a commit.
    #[arg(
        long = "commit",
        value_name = "SHA",
        conflicts_with_all = ["uncommitted", "base", "prompt"]
    )]
    pub commit: Option<String>,

    /// Optional commit title to display in the review summary.
    #[arg(long = "title", value_name = "TITLE", requires = "commit")]
    pub commit_title: Option<String>,

    /// Custom review instructions. If `-` is used, read from stdin.
    #[arg(value_name = "PROMPT", value_hint = clap::ValueHint::Other)]
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Color {
    Always,
    Never,
    #[default]
    Auto,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_last_reinterprets_positional_as_prompt() {
        let cli = Cli::parse_from(["code-exec", "resume", "--last", "echo hi"]);
        let Some(Command::Resume(args)) = cli.command else {
            panic!("expected resume command");
        };
        assert!(args.last);
        assert_eq!(args.session_id, None);
        assert_eq!(args.prompt.as_deref(), Some("echo hi"));
    }

    #[test]
    fn resume_parses_all_and_images() {
        let cli = Cli::parse_from([
            "code-exec",
            "resume",
            "--last",
            "--all",
            "--image",
            "/tmp/a.png,/tmp/b.png",
            "echo hi",
        ]);
        let Some(Command::Resume(args)) = cli.command else {
            panic!("expected resume command");
        };
        assert!(args.last);
        assert!(args.all);
        assert_eq!(args.prompt.as_deref(), Some("echo hi"));
        assert_eq!(args.images.len(), 2);
    }

    #[test]
    fn review_parses_base_branch_scope() {
        let cli = Cli::parse_from(["code-exec", "review", "--base", "main"]);
        let Some(Command::Review(args)) = cli.command else {
            panic!("expected review command");
        };
        assert_eq!(args.base.as_deref(), Some("main"));
        assert!(!args.uncommitted);
        assert_eq!(args.commit, None);
    }
}
