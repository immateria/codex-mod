use super::StatusLineItem;

impl StatusLineItem {
    pub(crate) fn label(self) -> &'static str {
        match self {
            StatusLineItem::ModelName => "Model name",
            StatusLineItem::ModelWithReasoning => "Model + reasoning",
            StatusLineItem::ServiceTier => "Speed mode",
            StatusLineItem::Shell => "Shell",
            StatusLineItem::ShellStyle => "Shell style",
            StatusLineItem::CurrentDir => "Current directory",
            StatusLineItem::ProjectRoot => "Project root",
            StatusLineItem::GitBranch => "Git branch",
            #[cfg(feature = "managed-network-proxy")]
            StatusLineItem::NetworkMediation => "Network mediation",
            StatusLineItem::Approval => "Approval policy",
            StatusLineItem::Sandbox => "Sandbox policy",
            StatusLineItem::ContextRemaining => "Context remaining",
            StatusLineItem::ContextUsed => "Context used",
            StatusLineItem::FiveHourLimit => "5-hour limit",
            StatusLineItem::WeeklyLimit => "Weekly limit",
            StatusLineItem::CodexVersion => "Version",
            StatusLineItem::ContextWindowSize => "Context window size",
            StatusLineItem::UsedTokens => "Used tokens",
            StatusLineItem::TotalInputTokens => "Total input tokens",
            StatusLineItem::TotalOutputTokens => "Total output tokens",
            StatusLineItem::SessionId => "Session id",
            StatusLineItem::JsRepl => "JS REPL kernel",
            StatusLineItem::ActiveProfile => "Active shell profile",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            StatusLineItem::ModelName => "Current model name.",
            StatusLineItem::ModelWithReasoning => "Current model with reasoning level.",
            StatusLineItem::ServiceTier => "Current GPT-5.4 response speed mode (fast or standard).",
            StatusLineItem::Shell => "Selected shell executable for tool execution.",
            StatusLineItem::ShellStyle => "Active shell script style for routing and profiles.",
            StatusLineItem::CurrentDir => "Current working directory.",
            StatusLineItem::ProjectRoot => "Detected project root directory.",
            StatusLineItem::GitBranch => "Current git branch when available.",
            #[cfg(feature = "managed-network-proxy")]
            StatusLineItem::NetworkMediation => "Managed network mediation state (enabled/mode).",
            StatusLineItem::Approval => "Current approval policy for command execution.",
            StatusLineItem::Sandbox => "Current sandbox policy for tool execution.",
            StatusLineItem::ContextRemaining => "Remaining model context percentage.",
            StatusLineItem::ContextUsed => "Used model context percentage.",
            StatusLineItem::FiveHourLimit => "Primary rate-limit window usage.",
            StatusLineItem::WeeklyLimit => "Secondary rate-limit window usage.",
            StatusLineItem::CodexVersion => "App version.",
            StatusLineItem::ContextWindowSize => "Model context window size.",
            StatusLineItem::UsedTokens => "Total tokens used in this session.",
            StatusLineItem::TotalInputTokens => "Total input tokens.",
            StatusLineItem::TotalOutputTokens => "Total output tokens.",
            StatusLineItem::SessionId => "Current session identifier.",
            StatusLineItem::JsRepl => {
                "JS REPL kernel status and runtime version (hidden when js_repl disabled)."
            }
            StatusLineItem::ActiveProfile => {
                "Active shell profile name (hidden when no profile is set)."
            }
        }
    }

    pub(super) fn sample(self) -> &'static str {
        match self {
            StatusLineItem::ModelName => "GPT-5.3-Codex",
            StatusLineItem::ModelWithReasoning => "GPT-5.3-Codex High",
            StatusLineItem::ServiceTier => "standard",
            StatusLineItem::Shell => "sh /bin/zsh",
            StatusLineItem::ShellStyle => "style zsh",
            StatusLineItem::CurrentDir => "~/code-termux",
            StatusLineItem::ProjectRoot => "code-termux",
            StatusLineItem::GitBranch => "main",
            #[cfg(feature = "managed-network-proxy")]
            StatusLineItem::NetworkMediation => "net limited",
            StatusLineItem::Approval => "approval on-request",
            StatusLineItem::Sandbox => "sbx workspace-write",
            StatusLineItem::ContextRemaining => "64% left",
            StatusLineItem::ContextUsed => "36% used",
            StatusLineItem::FiveHourLimit => "5h 27%",
            StatusLineItem::WeeklyLimit => "weekly 4%",
            StatusLineItem::CodexVersion => "v0.0.0",
            StatusLineItem::ContextWindowSize => "256K window",
            StatusLineItem::UsedTokens => "12.4K used",
            StatusLineItem::TotalInputTokens => "9.3K in",
            StatusLineItem::TotalOutputTokens => "3.1K out",
            StatusLineItem::SessionId => "a18f2f0d-01d4-4dbf-b2b6-2f53",
            StatusLineItem::JsRepl => "js node v20",
            StatusLineItem::ActiveProfile => "profile work",
        }
    }
}
