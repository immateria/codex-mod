// ---- System notice ordering helpers ----
#[derive(Copy, Clone)]
enum SystemPlacement {
    /// Place near the top of the current request (before most provider output)
    Early,
    /// Place at the end of the current request window (after provider output)
    Tail,
    /// Place before the first user prompt of the very first request
    /// (used for pre-turn UI confirmations like theme/spinner changes)
    PrePrompt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoDriveRole {
    User,
    Assistant,
}

pub(crate) struct ChatWidgetInit {
    pub(crate) config: Config,
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) initial_prompt: Option<String>,
    pub(crate) initial_images: Vec<PathBuf>,
    pub(crate) terminal_info: crate::tui::TerminalInfo,
    pub(crate) show_order_overlay: bool,
    pub(crate) latest_upgrade_version: Option<String>,
}

pub(crate) struct ForkedChatWidgetInit {
    pub(crate) config: Config,
    pub(crate) conversation: Arc<code_core::CodexConversation>,
    pub(crate) session_configured: SessionConfiguredEvent,
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) terminal_info: crate::tui::TerminalInfo,
    pub(crate) show_order_overlay: bool,
    pub(crate) latest_upgrade_version: Option<String>,
    pub(crate) auth_manager: Arc<AuthManager>,
    pub(crate) show_welcome: bool,
}

pub(crate) struct BackgroundReviewFinishedEvent {
    pub(crate) worktree_path: std::path::PathBuf,
    pub(crate) branch: String,
    pub(crate) has_findings: bool,
    pub(crate) findings: usize,
    pub(crate) summary: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) agent_id: Option<String>,
    pub(crate) snapshot: Option<String>,
}

pub(crate) struct AutoLaunchRequest {
    pub(crate) goal: String,
    pub(crate) derive_goal_from_history: bool,
    pub(crate) review_enabled: bool,
    pub(crate) subagents_enabled: bool,
    pub(crate) cross_check_enabled: bool,
    pub(crate) qa_automation_enabled: bool,
    pub(crate) continue_mode: AutoContinueMode,
}

pub(crate) struct AutoDecisionEvent {
    pub(crate) seq: u64,
    pub(crate) status: AutoCoordinatorStatus,
    pub(crate) status_title: Option<String>,
    pub(crate) status_sent_to_user: Option<String>,
    pub(crate) goal: Option<String>,
    pub(crate) cli: Option<AutoTurnCliAction>,
    pub(crate) agents_timing: Option<AutoTurnAgentsTiming>,
    pub(crate) agents: Vec<AutoTurnAgentsAction>,
    pub(crate) transcript: Vec<code_protocol::models::ResponseItem>,
}

pub(crate) struct AgentUpdateRequest {
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) args_ro: Option<Vec<String>>,
    pub(crate) args_wr: Option<Vec<String>>,
    pub(crate) instructions: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) command: String,
}
