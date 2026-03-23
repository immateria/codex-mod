const MAX_OUTPUT_CHARS: usize = 8_000;
const MAX_STEPS: usize = 6;

enum GuidedTerminalMode {
    AgentInstall {
        agent_name: String,
        default_command: String,
        selected_index: usize,
    },
    Prompt { user_prompt: String },
    DirectCommand { command: String },
    Upgrade {
        initial_command: String,
        latest_version: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct InstallDecision {
    finish_status: String,
    message: String,
    #[serde(default)]
    command: Option<String>,
}

pub(super) struct GuidedTerminalControl {
    pub(super) controller: TerminalRunController,
    pub(super) controller_rx: Receiver<TerminalRunEvent>,
}

pub(super) struct AgentInstallSessionArgs {
    pub(super) app_event_tx: AppEventSender,
    pub(super) terminal_id: u64,
    pub(super) agent_name: String,
    pub(super) default_command: String,
    pub(super) cwd: Option<String>,
    pub(super) control: GuidedTerminalControl,
    pub(super) selected_index: usize,
    pub(super) debug_enabled: bool,
}

pub(super) struct UpgradeTerminalSessionArgs {
    pub(super) app_event_tx: AppEventSender,
    pub(super) terminal_id: u64,
    pub(super) initial_command: String,
    pub(super) latest_version: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) control: GuidedTerminalControl,
    pub(super) config: Config,
    pub(super) debug_enabled: bool,
}

struct GuidedTerminalSessionArgs {
    app_event_tx: AppEventSender,
    terminal_id: u64,
    mode: GuidedTerminalMode,
    cwd: Option<String>,
    control: GuidedTerminalControl,
    config: Option<Config>,
    debug_enabled: bool,
}

struct GuidedLoopArgs<'a> {
    app_event_tx: &'a AppEventSender,
    terminal_id: u64,
    mode: &'a GuidedTerminalMode,
    cwd: Option<&'a str>,
    controller: TerminalRunController,
    controller_rx: &'a mut Receiver<TerminalRunEvent>,
    provided_config: Option<Config>,
    debug_enabled: bool,
}
