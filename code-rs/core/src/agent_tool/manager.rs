use super::*;

// Agent status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

// Agent information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub batch_id: Option<String>,
    pub model: String,
    #[serde(default)]
    pub name: Option<String>,
    pub prompt: String,
    pub context: Option<String>,
    pub output_goal: Option<String>,
    pub files: Vec<String>,
    pub read_only: bool,
    pub status: AgentStatus,
    pub result: Option<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub progress: Vec<String>,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    #[serde(default)]
    pub worktree_base: Option<String>,
    #[serde(default)]
    pub source_kind: Option<AgentSourceKind>,
    #[serde(skip)]
    pub log_tag: Option<String>,
    #[serde(skip)]
    #[allow(dead_code)]
    pub config: Option<AgentConfig>,
    pub reasoning_effort: code_protocol::config_types::ReasoningEffort,
    #[serde(skip)]
    pub last_activity: DateTime<Utc>,
}

// Global agent manager
lazy_static::lazy_static! {
    pub static ref AGENT_MANAGER: Arc<RwLock<AgentManager>> = Arc::new(RwLock::new(AgentManager::new()));
}

pub struct AgentManager {
    agents: HashMap<String, Agent>,
    handles: HashMap<String, JoinHandle<()>>,
    event_sender: Option<mpsc::UnboundedSender<AgentStatusUpdatePayload>>,
    debug_log_root: Option<PathBuf>,
    watchdog_handle: Option<JoinHandle<()>>,
    inactivity_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct AgentStatusUpdatePayload {
    pub agents: Vec<AgentInfo>,
    pub context: Option<String>,
    pub task: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AgentCreateRequest {
    pub model: String,
    pub name: Option<String>,
    pub prompt: String,
    pub context: Option<String>,
    pub output_goal: Option<String>,
    pub files: Vec<String>,
    pub read_only: bool,
    pub batch_id: Option<String>,
    pub config: Option<AgentConfig>,
    pub worktree_branch: Option<String>,
    pub worktree_base: Option<String>,
    pub source_kind: Option<AgentSourceKind>,
    pub reasoning_effort: code_protocol::config_types::ReasoningEffort,
}

impl Default for AgentManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentManager {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            handles: HashMap::new(),
            event_sender: None,
            debug_log_root: None,
            watchdog_handle: None,
            inactivity_timeout: Duration::minutes(30),
        }
    }

    pub fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<AgentStatusUpdatePayload>) {
        self.event_sender = Some(sender);
        self.start_watchdog();
    }

    fn start_watchdog(&mut self) {
        if self.watchdog_handle.is_some() {
            return;
        }

        let timeout = self.inactivity_timeout;
        let manager = Arc::downgrade(&AGENT_MANAGER);
        self.watchdog_handle = Some(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(TokioDuration::from_secs(60));
            loop {
                ticker.tick().await;

                let Some(manager_arc) = manager.upgrade() else { break; };

                let mut mgr = manager_arc.write().await;
                let now = Utc::now();
                let timeout_ids: Vec<String> = mgr
                    .agents
                    .iter()
                    .filter(|(_, agent)| matches!(agent.status, AgentStatus::Pending | AgentStatus::Running))
                    .filter(|(_, agent)| now - agent.last_activity > timeout)
                    .map(|(id, _)| id.clone())
                    .collect();

                if timeout_ids.is_empty() {
                    continue;
                }

                for agent_id in timeout_ids.iter() {
                    if let Some(handle) = mgr.handles.remove(agent_id) {
                        handle.abort();
                    }
                    if let Some(agent) = mgr.agents.get_mut(agent_id) {
                        agent.status = AgentStatus::Failed;
                        agent.error = Some(format!(
                            "Agent timed out after {} minutes of inactivity.",
                            timeout.num_minutes()
                        ));
                        agent.completed_at = Some(now);
                        Self::record_activity(agent);
                    }
                }

                // Notify listeners once per sweep.
                mgr.send_agent_status_update().await;
            }
        }));
    }

    pub fn set_debug_log_root(&mut self, root: Option<PathBuf>) {
        self.debug_log_root = root;
    }

    pub(crate) async fn touch_agent(agent_id: &str) {
        if let Some(manager) = Arc::downgrade(&AGENT_MANAGER).upgrade() {
            let mut mgr = manager.write().await;
            if let Some(agent) = mgr.agents.get_mut(agent_id) {
                Self::record_activity(agent);
            }
        }
    }

    fn record_activity(agent: &mut Agent) {
        agent.last_activity = Utc::now();
    }

    fn append_agent_log(&self, log_tag: &str, line: &str) {
        let Some(root) = &self.debug_log_root else { return; };
        let dir = root.join(log_tag);
        if let Err(err) = fs::create_dir_all(&dir) {
            warn!("failed to create agent log dir {:?}: {}", dir, err);
            return;
        }

        let file = dir.join("progress.log");
        match OpenOptions::new().create(true).append(true).open(&file) {
            Ok(mut fh) => {
                if let Err(err) = writeln!(fh, "{line}") {
                    warn!("failed to write agent log {:?}: {}", file, err);
                }
            }
            Err(err) => warn!("failed to open agent log {:?}: {}", file, err),
        }
    }

    async fn send_agent_status_update(&self) {
        if let Some(ref sender) = self.event_sender {
            let now = Utc::now();
            let agents: Vec<AgentInfo> = self
                .agents
                .values()
                .map(|agent| {
                    // Just show the model name - status provides the useful info
                    let name = agent
                        .name.clone()
                        .unwrap_or_else(|| agent.model.clone());
                    let start = agent.started_at.unwrap_or(agent.created_at);
                    let end = agent.completed_at.unwrap_or(now);
                    let elapsed_ms = match end.signed_duration_since(start).num_milliseconds() {
                        value if value >= 0 => Some(value as u64),
                        _ => None,
                    };

                    AgentInfo {
                        id: agent.id.clone(),
                        name,
                        status: format!("{:?}", agent.status).to_lowercase(),
                        batch_id: agent.batch_id.clone(),
                        model: Some(agent.model.clone()),
                        last_progress: agent.progress.last().cloned(),
                        result: agent.result.clone(),
                        error: agent.error.clone(),
                        elapsed_ms,
                        token_count: None,
                        last_activity_at: match agent.status {
                            AgentStatus::Pending | AgentStatus::Running => {
                                Some(agent.last_activity.to_rfc3339())
                            }
                            _ => None,
                        },
                        seconds_since_last_activity: match agent.status {
                            AgentStatus::Pending | AgentStatus::Running => Some(
                                Utc::now()
                                    .signed_duration_since(agent.last_activity)
                                    .num_seconds()
                                    .max(0) as u64,
                            ),
                            _ => None,
                        },
                        source_kind: agent.source_kind.clone(),
                    }
                })
                .collect();

            // Get context and task from the first agent (they're all the same)
            let (context, task) = self
                .agents
                .values()
                .next()
                .map(|agent| {
                    let context = agent
                        .context
                        .as_ref()
                        .and_then(|value| if value.trim().is_empty() {
                            None
                        } else {
                            Some(value.clone())
                        });
                    let task = if agent.prompt.trim().is_empty() {
                        None
                    } else {
                        Some(agent.prompt.clone())
                    };
                    (context, task)
                })
                .unwrap_or((None, None));
            let payload = AgentStatusUpdatePayload { agents, context, task };
            let _ = sender.send(payload);
        }
    }

    pub async fn create_agent(
        &mut self,
        request: AgentCreateRequest,
    ) -> String {
        self.create_agent_internal(request).await
    }

    pub async fn create_agent_with_config(
        &mut self,
        mut request: AgentCreateRequest,
        config: AgentConfig,
    ) -> String {
        request.config = Some(config);
        self.create_agent_internal(request).await
    }

    #[allow(dead_code)]
    pub async fn create_agent_with_options(
        &mut self,
        request: AgentCreateRequest,
    ) -> String {
        self.create_agent_internal(request).await
    }

    async fn create_agent_internal(
        &mut self,
        request: AgentCreateRequest,
    ) -> String {
        let AgentCreateRequest {
            model,
            name,
            prompt,
            context,
            output_goal,
            files,
            read_only,
            batch_id,
            config,
            worktree_branch,
            worktree_base,
            source_kind,
            reasoning_effort,
        } = request;
        let agent_id = Uuid::new_v4().to_string();

        let log_tag = match source_kind {
            Some(AgentSourceKind::AutoReview) => {
                Some(format!("agents/auto-review/{agent_id}"))
            }
            _ => None,
        };

        let agent = Agent {
            id: agent_id.clone(),
            batch_id,
            model,
            name: normalize_agent_name(name),
            prompt,
            context,
            output_goal,
            files,
            read_only,
            status: AgentStatus::Pending,
            result: None,
            error: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            progress: Vec::new(),
            worktree_path: None,
            branch_name: worktree_branch,
            worktree_base,
            source_kind,
            log_tag,
            config: config.clone(),
            reasoning_effort,
            last_activity: Utc::now(),
        };

        self.agents.insert(agent_id.clone(), agent.clone());

        // Send initial status update
        self.send_agent_status_update().await;

        // Spawn async agent
        let agent_id_clone = agent_id.clone();
        let handle = tokio::spawn(async move {
            execute_agent(agent_id_clone, config).await;
        });

        self.handles.insert(agent_id.clone(), handle);

        agent_id
    }

    pub fn get_agent(&self, agent_id: &str) -> Option<Agent> {
        self.agents.get(agent_id).cloned()
    }

    pub fn get_all_agents(&self) -> impl Iterator<Item = &Agent> {
        self.agents.values()
    }

    pub fn list_agents(
        &self,
        status_filter: Option<AgentStatus>,
        batch_id: Option<String>,
        recent_only: bool,
    ) -> Vec<Agent> {
        let cutoff = if recent_only {
            Some(Utc::now() - Duration::hours(2))
        } else {
            None
        };

        self.agents
            .values()
            .filter(|agent| {
                if let Some(ref filter) = status_filter
                    && agent.status != *filter {
                        return false;
                    }
                if let Some(ref batch) = batch_id
                    && agent.batch_id.as_ref() != Some(batch) {
                        return false;
                    }
                if let Some(cutoff) = cutoff
                    && agent.created_at < cutoff {
                        return false;
                    }
                true
            })
            .cloned()
            .collect()
    }

    pub fn has_active_agents(&self) -> bool {
        self.agents
            .values()
            .any(|agent| matches!(agent.status, AgentStatus::Pending | AgentStatus::Running))
    }

    pub async fn cancel_agent(&mut self, agent_id: &str) -> bool {
        if let Some(handle) = self.handles.remove(agent_id) {
            handle.abort();
            if let Some(agent) = self.agents.get_mut(agent_id) {
                agent.status = AgentStatus::Cancelled;
                agent.completed_at = Some(Utc::now());
            }
            true
        } else {
            false
        }
    }

    pub async fn cancel_batch(&mut self, batch_id: &str) -> usize {
        let agent_ids: Vec<String> = self
            .agents
            .values()
            .filter(|agent| agent.batch_id.as_ref() == Some(&batch_id.to_string()))
            .map(|agent| agent.id.clone())
            .collect();

        let mut count = 0;
        for agent_id in agent_ids {
            if self.cancel_agent(&agent_id).await {
                count += 1;
            }
        }
        count
    }

    pub async fn update_agent_status(&mut self, agent_id: &str, status: AgentStatus) {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.status = status;
            if agent.status == AgentStatus::Running && agent.started_at.is_none() {
                agent.started_at = Some(Utc::now());
            }
            if matches!(
                agent.status,
                AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Cancelled
            ) {
                agent.completed_at = Some(Utc::now());
            }
            Self::record_activity(agent);
            // Send status update event
            self.send_agent_status_update().await;
        }
    }

    pub async fn update_agent_result(&mut self, agent_id: &str, result: Result<String, String>) {
        let debug_enabled = self.debug_log_root.is_some();

        if let Some((log_tag, log_lines)) = self.agents.get_mut(agent_id).map(|agent| {
            let log_tag = if debug_enabled { agent.log_tag.clone() } else { None };

            let mut log_lines: Vec<String> = Vec::new();
            if debug_enabled {
                let stamp = Utc::now().format("%H:%M:%S");
                match &result {
                    Ok(output) => {
                        log_lines.push(format!("{stamp}: [result] completed"));
                        if !output.trim().is_empty() {
                            log_lines.push(output.trim_end().to_string());
                        }
                    }
                    Err(error) => {
                        log_lines.push(format!("{stamp}: [result] failed"));
                        log_lines.push(error.clone());
                    }
                }
            }

            match result {
                Ok(output) => {
                    agent.result = Some(output);
                    agent.status = AgentStatus::Completed;
                }
                Err(error) => {
                    agent.error = Some(error);
                    agent.status = AgentStatus::Failed;
                }
            }
            agent.completed_at = Some(Utc::now());
            Self::record_activity(agent);

            (log_tag, log_lines)
        }) {
            if let Some(tag) = log_tag {
                for line in log_lines {
                    self.append_agent_log(&tag, &line);
                }
            }
            // Send status update event
            self.send_agent_status_update().await;
        }
    }

    pub async fn add_progress(&mut self, agent_id: &str, message: String) {
        let debug_enabled = self.debug_log_root.is_some();

        if let Some((log_tag, entry)) = self.agents.get_mut(agent_id).map(|agent| {
            let entry = format!("{}: {}", Utc::now().format("%H:%M:%S"), message);
            let log_tag = if debug_enabled { agent.log_tag.clone() } else { None };
            agent.progress.push(entry.clone());
            Self::record_activity(agent);
            (log_tag, entry)
        }) {
            if let Some(tag) = log_tag {
                self.append_agent_log(&tag, &entry);
            }
            // Send updated agent status with the latest progress
            self.send_agent_status_update().await;
        }
    }

    pub async fn update_worktree_info(
        &mut self,
        agent_id: &str,
        worktree_path: String,
        branch_name: String,
    ) {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.worktree_path = Some(worktree_path);
            agent.branch_name = Some(branch_name);
        }
    }
}
