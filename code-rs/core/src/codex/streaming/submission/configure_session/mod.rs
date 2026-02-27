use super::*;

use crate::config_types::McpServerConfig;
use crate::config_types::ReasoningEffort as ReasoningEffortConfig;
use crate::config_types::ReasoningSummary as ReasoningSummaryConfig;
use crate::config_types::ShellConfig;
use crate::config_types::ShellScriptStyle;
use crate::config_types::ShellStyleProfileConfig;
use crate::config_types::TextVerbosity as TextVerbosityConfig;
use crate::model_provider_info::ModelProviderInfo;
use crate::protocol::AskForApproval;
use crate::protocol::CollaborationModeKind;
use crate::protocol::SandboxPolicy;
use crate::shell::Shell;
use code_protocol::dynamic_tools::DynamicToolSpec;

mod build_session;
mod emit;
mod prepare;

pub(super) struct ConfigureSessionState {
    pub(super) session_id: Uuid,
    pub(super) config: Arc<Config>,
    pub(super) sess: Option<Arc<Session>>,
    pub(super) agent_manager_initialized: bool,
}

pub(super) enum ConfigureSessionControl {
    Continue,
    Exit,
}

pub(super) async fn handle_configure_session(
    state: ConfigureSessionState,
    auth_manager: Option<Arc<AuthManager>>,
    tx_event: &Sender<Event>,
    file_watcher: &crate::file_watcher::FileWatcher,
    sub_id: String,
    op: Op,
) -> (ConfigureSessionState, ConfigureSessionControl) {
    let ConfigureSessionState {
        session_id,
        config,
        sess,
        agent_manager_initialized,
    } = state;

    let Op::ConfigureSession {
        provider,
        model,
        model_explicit,
        model_reasoning_effort,
        preferred_model_reasoning_effort,
        model_reasoning_summary,
        model_text_verbosity,
        user_instructions: provided_user_instructions,
        base_instructions: provided_base_instructions,
        approval_policy,
        sandbox_policy,
        disable_response_storage,
        notify,
        cwd,
        resume_path,
        demo_developer_message,
        dynamic_tools,
        shell: shell_override,
        shell_style_profiles,
        network,
        collaboration_mode,
    } = op else {
        unreachable!("handle_configure_session called with non-ConfigureSession op");
    };

    let req = ConfigureSessionRequest {
        submission_id: sub_id,
        provider,
        model,
        model_explicit,
        model_reasoning_effort,
        preferred_model_reasoning_effort,
        model_reasoning_summary,
        model_text_verbosity,
        provided_user_instructions,
        provided_base_instructions,
        approval_policy,
        sandbox_policy,
        disable_response_storage,
        notify,
        cwd,
        resume_path,
        demo_developer_message,
        dynamic_tools,
        shell_override,
        shell_style_profiles,
        network,
        collaboration_mode,
    };

    let mut runner = Runner {
        session_id,
        config,
        sess,
        agent_manager_initialized,
        auth_manager,
        tx_event,
        file_watcher,
    };

    let control = runner.run(req).await;
    let state = runner.into_state();
    (state, control)
}

struct ConfigureSessionRequest {
    submission_id: String,
    provider: ModelProviderInfo,
    model: String,
    model_explicit: bool,
    model_reasoning_effort: ReasoningEffortConfig,
    preferred_model_reasoning_effort: Option<ReasoningEffortConfig>,
    model_reasoning_summary: ReasoningSummaryConfig,
    model_text_verbosity: TextVerbosityConfig,
    provided_user_instructions: Option<String>,
    provided_base_instructions: Option<String>,
    approval_policy: AskForApproval,
    sandbox_policy: SandboxPolicy,
    disable_response_storage: bool,
    notify: Option<Vec<String>>,
    cwd: PathBuf,
    resume_path: Option<PathBuf>,
    demo_developer_message: Option<String>,
    dynamic_tools: Vec<DynamicToolSpec>,
    shell_override: Option<ShellConfig>,
    shell_style_profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
    network: Option<crate::config::NetworkProxySettingsToml>,
    collaboration_mode: CollaborationModeKind,
}

struct Prepared {
    submission_id: String,
    provider: ModelProviderInfo,
    model: String,
    model_reasoning_effort: ReasoningEffortConfig,
    model_reasoning_summary: ReasoningSummaryConfig,
    model_text_verbosity: TextVerbosityConfig,
    approval_policy: AskForApproval,
    sandbox_policy: SandboxPolicy,
    disable_response_storage: bool,
    notify: Option<Vec<String>>,
    cwd: PathBuf,
    collaboration_mode: CollaborationModeKind,
    demo_developer_message: Option<String>,
    dynamic_tools: Vec<DynamicToolSpec>,
    shell_override_present: bool,
    base_instructions: Option<String>,
    effective_user_instructions: Option<String>,
    resolved_shell: Shell,
    command_safety_profile: crate::safety::ResolvedCommandSafetyProfile,
    active_shell_style: Option<ShellScriptStyle>,
    active_shell_style_label: Option<String>,
    shell_style_profile_messages: Vec<String>,
    shell_style_mcp_include: HashSet<String>,
    shell_style_mcp_exclude: HashSet<String>,
    effective_mcp_servers: HashMap<String, McpServerConfig>,
    session_skills: Vec<crate::skills::model::SkillMetadata>,
    restored_items: Option<Vec<RolloutItem>>,
    restored_history_snapshot: Option<crate::history::HistorySnapshot>,
    resume_notice: Option<String>,
    rollout_recorder: Option<RolloutRecorder>,
}

struct Built {
    submission_id: String,
    model: String,
    mcp_connection_errors: Vec<String>,
    restored_items: Option<Vec<RolloutItem>>,
    restored_history_snapshot: Option<crate::history::HistorySnapshot>,
    replay_history_items: Option<Vec<ResponseItem>>,
    resume_notice: Option<String>,
}

struct Runner<'a> {
    session_id: Uuid,
    config: Arc<Config>,
    sess: Option<Arc<Session>>,
    agent_manager_initialized: bool,
    auth_manager: Option<Arc<AuthManager>>,
    tx_event: &'a Sender<Event>,
    file_watcher: &'a crate::file_watcher::FileWatcher,
}

impl Runner<'_> {
    fn into_state(self) -> ConfigureSessionState {
        ConfigureSessionState {
            session_id: self.session_id,
            config: self.config,
            sess: self.sess,
            agent_manager_initialized: self.agent_manager_initialized,
        }
    }

    async fn run(&mut self, req: ConfigureSessionRequest) -> ConfigureSessionControl {
        let prepared = match self.prepare(req).await {
            Ok(prepared) => prepared,
            Err(control) => return control,
        };
        let built = self.build_session(prepared).await;
        self.emit(built).await;
        ConfigureSessionControl::Continue
    }

    async fn send_error_event(&self, sub_id: &str, message: String) {
        error!("{message}");
        let event = Event {
            id: sub_id.to_string(),
            event_seq: 0,
            msg: EventMsg::Error(ErrorEvent { message }),
            order: None,
        };
        if let Err(e) = self.tx_event.send(event).await {
            error!("failed to send error message: {e:?}");
        }
    }

    async fn send_no_session_event(&self, sub_id: &str) {
        let event = Event {
            id: sub_id.to_string(),
            event_seq: 0,
            msg: EventMsg::Error(ErrorEvent {
                message: "No session initialized, expected 'ConfigureSession' as first Op"
                    .to_string(),
            }),
            order: None,
        };
        let _ = self.tx_event.send(event).await;
    }
}
