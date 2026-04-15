use super::*;

use crate::app_event::MemoriesSettingsScope;
use code_core::config_types::MemoriesConfig;

impl MemoriesSettingsView {
    pub(crate) fn new(
        code_home: PathBuf,
        current_project: PathBuf,
        active_profile: Option<String>,
        global_settings: Option<MemoriesToml>,
        profile_settings: Option<MemoriesToml>,
        project_settings: Option<MemoriesToml>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let resolved_global = code_core::config_types::resolve_memories_config(
            global_settings.as_ref(),
            None,
            None,
        )
        .to_toml();
        let state = ScrollState::with_first_selected();
        Self {
            code_home,
            current_project,
            active_profile,
            global_settings: resolved_global.clone(),
            saved_global_settings: resolved_global,
            profile_settings: profile_settings.clone(),
            saved_profile_settings: profile_settings,
            project_settings: project_settings.clone(),
            saved_project_settings: project_settings,
            scope: MemoriesScopeChoice::Global,
            mode: ViewMode::Main,
            status: None,
            state: Cell::new(state),
            viewport_rows: Cell::new(DEFAULT_VISIBLE_ROWS),
            is_complete: false,
            app_event_tx,
        }
    }

    const ROWS_WITH_FILE_MANAGER: [RowKind; 21] = [
            RowKind::Scope,
            RowKind::GenerateMemories,
            RowKind::UseMemories,
            RowKind::SkipMcpOrWebSearch,
            RowKind::MaxRawMemories,
            RowKind::MaxRolloutAgeDays,
            RowKind::MaxRolloutsPerStartup,
            RowKind::MinRolloutIdleHours,
            RowKind::ManageUserMemories,
            RowKind::BrowseTags,
            RowKind::BrowseEpochs,
            RowKind::ViewSummary,
            RowKind::ViewRawMemories,
            RowKind::ViewModelPrompt,
            RowKind::BrowseRollouts,
            RowKind::ViewStatus,
            RowKind::RefreshArtifacts,
            RowKind::ClearArtifacts,
            RowKind::OpenDirectory,
            RowKind::Apply,
            RowKind::Close,
    ];

    const ROWS_NO_FILE_MANAGER: [RowKind; 20] = [
        RowKind::Scope,
        RowKind::GenerateMemories,
        RowKind::UseMemories,
        RowKind::SkipMcpOrWebSearch,
        RowKind::MaxRawMemories,
        RowKind::MaxRolloutAgeDays,
        RowKind::MaxRolloutsPerStartup,
        RowKind::MinRolloutIdleHours,
        RowKind::ManageUserMemories,
        RowKind::BrowseTags,
        RowKind::BrowseEpochs,
        RowKind::ViewSummary,
        RowKind::ViewRawMemories,
        RowKind::ViewModelPrompt,
        RowKind::BrowseRollouts,
        RowKind::ViewStatus,
        RowKind::RefreshArtifacts,
        RowKind::ClearArtifacts,
        RowKind::Apply,
        RowKind::Close,
    ];

    pub(super) fn rows() -> &'static [RowKind] {
        if crate::platform_caps::supports_reveal_in_file_manager() {
            &Self::ROWS_WITH_FILE_MANAGER
        } else {
            &Self::ROWS_NO_FILE_MANAGER
        }
    }

    pub(super) fn app_scope(&self) -> MemoriesSettingsScope {
        match self.scope {
            MemoriesScopeChoice::Global => MemoriesSettingsScope::Global,
            MemoriesScopeChoice::Profile => MemoriesSettingsScope::Profile {
                name: self.active_profile.clone().unwrap_or_default(),
            },
            MemoriesScopeChoice::Project => MemoriesSettingsScope::Project {
                path: self.current_project.clone(),
            },
        }
    }

    pub(super) fn supports_scope(&self, scope: MemoriesScopeChoice) -> bool {
        match scope {
            MemoriesScopeChoice::Global | MemoriesScopeChoice::Project => true,
            MemoriesScopeChoice::Profile => self.active_profile.is_some(),
        }
    }

    pub(super) fn effective_settings(&self) -> MemoriesConfig {
        code_core::config_types::resolve_memories_config(
            Some(&self.global_settings),
            self.profile_settings.as_ref(),
            self.project_settings.as_ref(),
        )
    }

    pub(super) fn current_status(&self) -> Result<Option<code_core::MemoriesStatus>, String> {
        code_core::get_cached_memories_status(
            &self.code_home,
            Some(&self.global_settings),
            self.profile_settings.as_ref(),
            self.project_settings.as_ref(),
        )
        .map_err(|err| err.to_string())
    }

    pub(super) fn selected_row(&self) -> RowKind {
        let rows = Self::rows();
        let idx = self
            .state
            .get()
            .selected_idx
            .unwrap_or(0)
            .min(rows.len().saturating_sub(1));
        rows[idx]
    }

    pub(super) fn current_scope_settings(&self) -> Option<&MemoriesToml> {
        match self.scope {
            MemoriesScopeChoice::Global => Some(&self.global_settings),
            MemoriesScopeChoice::Profile => self.profile_settings.as_ref(),
            MemoriesScopeChoice::Project => self.project_settings.as_ref(),
        }
    }

    pub(super) fn ensure_current_scope_settings_mut(&mut self) -> &mut MemoriesToml {
        match self.scope {
            MemoriesScopeChoice::Global => &mut self.global_settings,
            MemoriesScopeChoice::Profile => {
                self.profile_settings.get_or_insert_with(MemoriesToml::default)
            }
            MemoriesScopeChoice::Project => {
                self.project_settings.get_or_insert_with(MemoriesToml::default)
            }
        }
    }

    pub(super) fn current_scope_saved_settings(&self) -> Option<&MemoriesToml> {
        match self.scope {
            MemoriesScopeChoice::Global => Some(&self.saved_global_settings),
            MemoriesScopeChoice::Profile => self.saved_profile_settings.as_ref(),
            MemoriesScopeChoice::Project => self.saved_project_settings.as_ref(),
        }
    }

    pub(super) fn current_scope_payload(&self) -> MemoriesToml {
        self.current_scope_settings().cloned().unwrap_or_default()
    }

    pub(super) fn mark_scope_saved(&mut self) {
        match self.scope {
            MemoriesScopeChoice::Global => {
                self.saved_global_settings = self.global_settings.clone();
            }
            MemoriesScopeChoice::Profile => {
                self.saved_profile_settings = self.profile_settings.clone();
            }
            MemoriesScopeChoice::Project => {
                self.saved_project_settings = self.project_settings.clone();
            }
        }
    }

    pub(super) fn current_scope_dirty(&self) -> bool {
        let current = self.current_scope_settings();
        let saved = self.current_scope_saved_settings();
        match (current, saved) {
            (Some(c), Some(s)) => c != s,
            (None, None) => false,
            _ => true,
        }
    }

    pub(super) fn prune_optional_scope(&mut self) {
        match self.scope {
            MemoriesScopeChoice::Global => {}
            MemoriesScopeChoice::Profile => {
                if self
                    .profile_settings
                    .as_ref()
                    .is_some_and(MemoriesToml::is_empty)
                {
                    self.profile_settings = None;
                }
            }
            MemoriesScopeChoice::Project => {
                if self
                    .project_settings
                    .as_ref()
                    .is_some_and(MemoriesToml::is_empty)
                {
                    self.project_settings = None;
                }
            }
        }
    }

    pub(super) fn bool_label(value: bool) -> &'static str {
        if value { "On" } else { "Off" }
    }

    pub(super) fn source_label(source: code_core::MemoriesSettingSource) -> &'static str {
        match source {
            code_core::MemoriesSettingSource::Default => "default",
            code_core::MemoriesSettingSource::Global => "global",
            code_core::MemoriesSettingSource::Profile => "profile",
            code_core::MemoriesSettingSource::Project => "project",
        }
    }

    fn current_scope_value_label(&self, explicit: Option<bool>, effective: bool) -> String {
        match self.scope {
            MemoriesScopeChoice::Global => Self::bool_label(effective).to_owned(),
            MemoriesScopeChoice::Profile | MemoriesScopeChoice::Project => match explicit {
                Some(value) => format!(
                    "override {}",
                    Self::bool_label(value).to_ascii_lowercase()
                ),
                None => format!(
                    "inherit ({})",
                    Self::bool_label(effective).to_ascii_lowercase()
                ),
            },
        }
    }

    fn current_scope_number_label<T: std::fmt::Display>(
        &self,
        explicit: Option<T>,
        effective: impl std::fmt::Display,
    ) -> String {
        match self.scope {
            MemoriesScopeChoice::Global => effective.to_string(),
            MemoriesScopeChoice::Profile | MemoriesScopeChoice::Project => match explicit {
                Some(value) => format!("override {value}"),
                None => format!("inherit ({effective})"),
            },
        }
    }

    fn scope_value_label(&self) -> String {
        match self.scope {
            MemoriesScopeChoice::Global => "Global".to_owned(),
            MemoriesScopeChoice::Profile => match self.active_profile.as_deref() {
                Some(name) => format!("Active profile ({name})"),
                None => "Active profile (unavailable)".to_owned(),
            },
            MemoriesScopeChoice::Project => {
                format!("Current project ({})", self.current_project.display())
            }
        }
    }

    pub(super) fn row_value(&self, row: RowKind) -> String {
        let effective = self.effective_settings();
        let scoped = self.current_scope_settings();
        match row {
            RowKind::Scope => self.scope_value_label(),
            RowKind::GenerateMemories => self.current_scope_value_label(
                scoped.and_then(|settings| settings.generate_memories),
                effective.generate_memories,
            ),
            RowKind::UseMemories => self.current_scope_value_label(
                scoped.and_then(|settings| settings.use_memories),
                effective.use_memories,
            ),
            RowKind::SkipMcpOrWebSearch => self.current_scope_value_label(
                scoped.and_then(|settings| settings.no_memories_if_mcp_or_web_search),
                effective.no_memories_if_mcp_or_web_search,
            ),
            RowKind::MaxRawMemories => self.current_scope_number_label(
                scoped.and_then(|settings| {
                    settings
                        .max_raw_memories_for_consolidation
                        .or(settings.max_raw_memories_for_global)
                }),
                effective.max_raw_memories_for_consolidation,
            ),
            RowKind::MaxRolloutAgeDays => self.current_scope_number_label(
                scoped.and_then(|settings| settings.max_rollout_age_days),
                effective.max_rollout_age_days,
            ),
            RowKind::MaxRolloutsPerStartup => self.current_scope_number_label(
                scoped.and_then(|settings| settings.max_rollouts_per_startup),
                effective.max_rollouts_per_startup,
            ),
            RowKind::MinRolloutIdleHours => self.current_scope_number_label(
                scoped.and_then(|settings| settings.min_rollout_idle_hours),
                effective.min_rollout_idle_hours,
            ),
            RowKind::ManageUserMemories => {
                match self.current_status() {
                    Ok(Some(status)) => {
                        let count = status.db.user_memory_count;
                        if count == 0 {
                            "none — add pinned memories".to_owned()
                        } else {
                            format!("{count} pinned")
                        }
                    }
                    _ => "loading…".to_owned(),
                }
            }
            RowKind::BrowseTags => {
                match self.current_status() {
                    Ok(Some(status)) => {
                        let epochs = status.db.stage1_epoch_count;
                        if epochs == 0 {
                            "no epochs yet".to_owned()
                        } else {
                            format!("{epochs} epochs tagged")
                        }
                    }
                    _ => "loading…".to_owned(),
                }
            }
            RowKind::BrowseEpochs => {
                match self.current_status() {
                    Ok(Some(status)) => {
                        let derived = status.db.derived_epoch_count;
                        let total = status.db.stage1_epoch_count;
                        if total == 0 {
                            "none extracted".to_owned()
                        } else {
                            format!("{derived} derived / {total} total")
                        }
                    }
                    _ => "loading…".to_owned(),
                }
            }
            RowKind::ViewSummary => {
                match self.current_status() {
                    Ok(Some(status)) if status.artifacts.summary.exists => "available".to_owned(),
                    _ => "not generated".to_owned(),
                }
            }
            RowKind::ViewRawMemories => {
                match self.current_status() {
                    Ok(Some(status)) if status.artifacts.raw_memories.exists => "available".to_owned(),
                    _ => "not generated".to_owned(),
                }
            }
            RowKind::ViewModelPrompt => {
                match self.current_status() {
                    Ok(Some(status)) if status.artifacts.summary.exists => "available".to_owned(),
                    _ => "no artifacts".to_owned(),
                }
            }
            RowKind::BrowseRollouts => {
                match self.current_status() {
                    Ok(Some(status)) => {
                        let count = status.artifacts.rollout_summary_count;
                        if count == 0 {
                            "none".to_owned()
                        } else {
                            format!("{count} files")
                        }
                    }
                    _ => "unknown".to_owned(),
                }
            }
            RowKind::ViewStatus => {
                match self.current_status() {
                    Ok(Some(status)) => {
                        let sessions = status.db.thread_count;
                        let pending = status.db.pending_stage1_count;
                        let dirty = if status.db.artifact_dirty { " · dirty" } else { "" };
                        format!("{sessions} sessions · {pending} pending{dirty}")
                    }
                    Ok(None) => "loading…".to_owned(),
                    Err(_) => "error".to_owned(),
                }
            }
            RowKind::RefreshArtifacts => "Bypass throttle".to_owned(),
            RowKind::ClearArtifacts => "Generated files only".to_owned(),
            RowKind::OpenDirectory => self.code_home.join("memories").display().to_string(),
            RowKind::Apply => {
                if self.current_scope_dirty() {
                    "● Unsaved changes".to_owned()
                } else {
                    "✓ Saved".to_owned()
                }
            }
            RowKind::Close => String::new(),
        }
    }

    pub(super) fn row_label(row: RowKind) -> &'static str {
        match row {
            RowKind::Scope => "Editing scope",
            RowKind::GenerateMemories => "Generate artifacts",
            RowKind::UseMemories => "Inject memory prompt",
            RowKind::SkipMcpOrWebSearch => "Skip MCP/web sessions",
            RowKind::MaxRawMemories => "Max retained memories",
            RowKind::MaxRolloutAgeDays => "Max rollout age (days)",
            RowKind::MaxRolloutsPerStartup => "Max rollouts per refresh",
            RowKind::MinRolloutIdleHours => "Min rollout idle (hours)",
            RowKind::ManageUserMemories => "Pinned memories",
            RowKind::BrowseTags => "Browse tags",
            RowKind::BrowseEpochs => "Browse epochs",
            RowKind::ViewSummary => "View memory summary",
            RowKind::ViewRawMemories => "View raw memories",
            RowKind::ViewModelPrompt => "View LLM prompt",
            RowKind::BrowseRollouts => "Browse rollout summaries",
            RowKind::ViewStatus => "View diagnostic status",
            RowKind::RefreshArtifacts => "Refresh artifacts now",
            RowKind::ClearArtifacts => "Clear generated artifacts",
            RowKind::OpenDirectory => "Open memories directory",
            RowKind::Apply => "Apply",
            RowKind::Close => "Close",
        }
    }

    pub(super) fn row_description(&self, row: RowKind) -> &'static str {
        match row {
            RowKind::Scope => "Switch scope: global (all sessions), profile (named config), or project (workspace-specific).",
            RowKind::GenerateMemories => "When on, each session's conversation is extracted into memory epochs after it ends. These become the building blocks of what the LLM remembers about your work.",
            RowKind::UseMemories => "When on, the memory summary is injected into the LLM's system prompt at the start of each turn. The LLM uses this to recall workspace context, past decisions, and conventions.",
            RowKind::SkipMcpOrWebSearch => "Sessions are marked polluted when they invoke MCP tools or web search, and polluted sessions are excluded from memory extraction. This avoids learning from externally-sourced tool output.",
            RowKind::MaxRawMemories => "Maximum number of session epochs retained in memory artifacts. Higher values give the LLM more context but use more prompt budget (12KB max).",
            RowKind::MaxRolloutAgeDays => "Sessions older than this are ignored during extraction. Keeps memories focused on recent work.",
            RowKind::MaxRolloutsPerStartup => "Maximum sessions scanned per refresh cycle. Limits startup time for projects with many sessions.",
            RowKind::MinRolloutIdleHours => "Sessions must be idle for at least this long before extraction. Prevents extracting from sessions still in progress.",
            RowKind::ManageUserMemories => "Create, edit, and delete pinned memories. These are prioritized for inclusion in the LLM prompt when memory usage is enabled, and are packed before auto-extracted epochs subject to the prompt budget.",
            RowKind::BrowseTags => "View all semantic tags across auto-extracted epochs and pinned memories. Shows how many epochs and user memories use each tag. Enter a tag to see matching epochs.",
            RowKind::BrowseEpochs => "Browse individual auto-extracted memory epochs with metadata: workspace, branch, tags, provenance, and content preview. These are the building blocks selected for LLM prompt injection.",
            RowKind::ViewSummary => "View the memory summary: a ranked list of your session interactions. Each entry shows workspace, branch, timestamp, and what you asked. Empty/trivial entries are filtered out.",
            RowKind::ViewRawMemories => "View all extracted memory data including internal metadata (thread IDs, provenance, timestamps). Used for debugging the memory pipeline.",
            RowKind::ViewModelPrompt => "Preview the exact developer instructions injected into the LLM prompt, including the decision boundary, memory layout, and selected memory entries.",
            RowKind::BrowseRollouts => "Browse individual session summaries. Each rollout captures one session's metadata and the user's last request. Entries are ranked by relevance.",
            RowKind::ViewStatus => "Detailed diagnostics: effective config with sources, artifact freshness, SQLite DB stats (sessions, epochs, pending jobs, build history).",
            RowKind::RefreshArtifacts => "Force an immediate rebuild of all memory artifacts, bypassing the normal throttle. Use after changing settings or clearing artifacts.",
            RowKind::ClearArtifacts => "Delete generated memory files (summary, raw, rollouts). The session catalog and database are preserved — a refresh will regenerate everything.",
            RowKind::OpenDirectory => "Open the memories directory in Finder/Explorer/your file manager.",
            RowKind::Apply => "Save the current settings for this scope to config.toml.",
            RowKind::Close => "Dismiss the Memories settings view.",
        }
    }

    pub(crate) fn is_view_complete(&self) -> bool {
        self.is_complete
    }

    pub(super) fn open_text_viewer(
        &mut self,
        title: &'static str,
        content: String,
        parent: TextViewerParent,
    ) {
        let lines: Vec<String> = content.lines().map(String::from).collect();
        self.mode = ViewMode::TextViewer(Box::new(TextViewerState {
            title,
            lines,
            scroll_top: Cell::new(0),
            viewport_rows: Cell::new(DEFAULT_VISIBLE_ROWS),
            parent,
            search: None,
        }));
    }

    pub(super) fn open_summary_viewer(&mut self) {
        match code_core::read_memory_summary(&self.code_home) {
            Ok(Some(content)) => {
                self.open_text_viewer(" Memory Summary ", content, TextViewerParent::Main);
            }
            Ok(None) => {
                self.status = Some(("memory_summary.md not found. Run Refresh first.".to_owned(), true));
            }
            Err(err) => {
                self.status = Some((format!("Error reading summary: {err}"), true));
            }
        }
    }

    pub(super) fn open_raw_viewer(&mut self) {
        match code_core::read_raw_memories(&self.code_home) {
            Ok(Some(content)) => {
                self.open_text_viewer(" Raw Memories ", content, TextViewerParent::Main);
            }
            Ok(None) => {
                self.status = Some(("raw_memories.md not found. Run Refresh first.".to_owned(), true));
            }
            Err(err) => {
                self.status = Some((format!("Error reading raw memories: {err}"), true));
            }
        }
    }

    pub(super) fn open_model_prompt_viewer(&mut self) {
        match code_core::preview_model_prompt_sync(&self.code_home) {
            Ok(Some(content)) => {
                self.open_text_viewer(" LLM Prompt Preview ", content, TextViewerParent::Main);
            }
            Ok(None) => {
                self.status = Some(("No memory artifacts generated yet. Run Refresh first.".to_owned(), true));
            }
            Err(err) => {
                self.status = Some((format!("Error generating prompt preview: {err}"), true));
            }
        }
    }

    pub(super) fn open_rollout_list(&mut self) {
        match code_core::list_rollout_summaries(&self.code_home) {
            Ok(entries) => {
                if entries.is_empty() {
                    self.status = Some(("No rollout summaries found. Run Refresh first.".to_owned(), true));
                    return;
                }
                self.mode = ViewMode::RolloutList(Box::new(RolloutListState {
                    entries,
                    list_state: Cell::new(ScrollState::with_first_selected()),
                    viewport_rows: Cell::new(DEFAULT_VISIBLE_ROWS),
                    pending_delete: None,
                }));
            }
            Err(err) => {
                self.status = Some((format!("Error listing rollouts: {err}"), true));
            }
        }
    }

    pub(super) fn open_status_viewer(&mut self) {
        match self.current_status() {
            Ok(Some(status)) => {
                let content = Self::format_status_report(&status);
                self.open_text_viewer(" Diagnostic Status ", content, TextViewerParent::Main);
            }
            Ok(None) => {
                self.status = Some(("Status not loaded yet. Try again in a moment.".to_owned(), true));
            }
            Err(err) => {
                self.status = Some((format!("Error loading status: {err}"), true));
            }
        }
    }

    fn format_status_report(status: &code_core::MemoriesStatus) -> String {
        fn on_off(v: bool) -> &'static str {
            if v { "on" } else { "off" }
        }
        let source = Self::source_label;
        fn artifact(name: &str, a: &code_core::MemoryArtifactStatus) -> String {
            let modified = a.modified_at.as_deref().unwrap_or("never");
            let present = if a.exists { "present" } else { "missing" };
            format!("  {name}: {present} (modified: {modified})")
        }

        let mut lines = Vec::with_capacity(40);
        lines.push("── How Memories Work ──".to_owned());
        lines.push("  Sessions → Epochs: Each session's conversation is split into".to_owned());
        lines.push("  memory epochs. An epoch captures your workspace, branch, and".to_owned());
        lines.push("  what you asked. Empty sessions produce empty epochs (filtered".to_owned());
        lines.push("  from prompts). Derived epochs contain real user interactions.".to_owned());
        lines.push(String::new());
        lines.push("  Auto-tags: Epochs are automatically tagged with categories".to_owned());
        lines.push("  (e.g. \"rust\", \"testing\", \"refactor\") based on content heuristics.".to_owned());
        lines.push("  Tagged epochs rank higher when they share tags with your pinned".to_owned());
        lines.push("  memories.".to_owned());
        lines.push(String::new());
        lines.push("  Pinned memories: User-created entries that are prioritized".to_owned());
        lines.push("  for inclusion in the LLM prompt when memory usage is enabled.".to_owned());
        lines.push("  They are packed before auto-extracted epochs, subject to".to_owned());
        lines.push("  the same prompt-budget limits.".to_owned());
        lines.push(String::new());
        lines.push("  Epochs → Prompt: The best epochs are ranked by workspace match,".to_owned());
        lines.push("  tag affinity, branch proximity, platform/shell compatibility,".to_owned());
        lines.push("  and recency, then packed into a 12KB prompt budget. This becomes".to_owned());
        lines.push("  the MEMORY_SUMMARY block in the LLM's developer instructions.".to_owned());
        lines.push(String::new());
        lines.push("  LLM behavior: The model skims the summary, searches MEMORY.md".to_owned());
        lines.push("  for deeper context when relevant, and updates stale entries".to_owned());
        lines.push("  when it detects conflicts with current workspace state.".to_owned());
        lines.push(String::new());
        lines.push("── Effective Configuration ──".to_owned());
        lines.push(format!(
            "  generate_memories: {} (source: {})",
            on_off(status.effective.generate_memories),
            source(status.sources.generate_memories),
        ));
        lines.push(format!(
            "  use_memories:      {} (source: {})",
            on_off(status.effective.use_memories),
            source(status.sources.use_memories),
        ));
        lines.push(format!(
            "  skip_mcp_web:      {} (source: {})",
            on_off(status.effective.no_memories_if_mcp_or_web_search),
            source(status.sources.no_memories_if_mcp_or_web_search),
        ));
        lines.push(String::new());
        lines.push("── Numeric Limits ──".to_owned());
        lines.push(format!(
            "  max_raw_memories:        {} (source: {})",
            status.effective.max_raw_memories_for_consolidation,
            source(status.sources.max_raw_memories_for_consolidation),
        ));
        lines.push(format!(
            "  max_rollout_age_days:    {} (source: {})",
            status.effective.max_rollout_age_days,
            source(status.sources.max_rollout_age_days),
        ));
        lines.push(format!(
            "  max_rollouts_per_startup:{} (source: {})",
            status.effective.max_rollouts_per_startup,
            source(status.sources.max_rollouts_per_startup),
        ));
        lines.push(format!(
            "  min_rollout_idle_hours:  {} (source: {})",
            status.effective.min_rollout_idle_hours,
            source(status.sources.min_rollout_idle_hours),
        ));
        lines.push(String::new());
        lines.push("── Artifacts ──".to_owned());
        lines.push(format!(
            "  memory_root: {}",
            status.artifacts.memory_root.display(),
        ));
        lines.push(artifact("memory_summary.md", &status.artifacts.summary));
        lines.push(artifact("raw_memories.md", &status.artifacts.raw_memories));
        lines.push(format!(
            "  rollout_summaries/: {} ({} files)",
            if status.artifacts.rollout_summaries.exists { "present" } else { "missing" },
            status.artifacts.rollout_summary_count,
        ));
        lines.push(String::new());
        lines.push("── Database ──".to_owned());
        lines.push(format!(
            "  sqlite: {}",
            if status.db.db_exists { "present" } else { "missing" },
        ));
        lines.push(format!("  sessions:       {}", status.db.thread_count));
        lines.push(format!("  stage1 epochs:  {}", status.db.stage1_epoch_count));
        lines.push(format!("  ├─ derived:     {}  (extracted from session data — used in prompts)", status.db.derived_epoch_count));
        {
            let fallback = status.db.stage1_epoch_count
                .saturating_sub(status.db.derived_epoch_count)
                .saturating_sub(status.db.empty_epoch_count);
            if fallback > 0 {
                lines.push(format!("  ├─ fallback:    {fallback}  (minimal extraction — used as backup)"));
            }
        }
        lines.push(format!("  └─ empty:       {}  (no useful content — filtered from prompts)", status.db.empty_epoch_count));
        lines.push(format!("  pending:        {}", status.db.pending_stage1_count));
        lines.push(format!("  running:        {}", status.db.running_stage1_count));
        lines.push(format!("  dead-lettered:  {}", status.db.dead_lettered_stage1_count));
        lines.push(format!("  artifact dirty: {}", on_off(status.db.artifact_dirty)));
        lines.push(format!("  artifact job:   {}", on_off(status.db.artifact_job_running)));
        if let Some(ref last_build) = status.db.last_artifact_build_at {
            lines.push(format!("  last build:     {last_build}"));
        }
        lines.push(format!("  user memories:  {}", status.db.user_memory_count));

        lines.join("\n")
    }

    pub(super) fn open_rollout_detail(
        &mut self,
        list_state: Box<RolloutListState>,
        slug: &str,
    ) {
        match code_core::read_rollout_summary(&self.code_home, slug) {
            Ok(Some(content)) => {
                self.open_text_viewer(
                    " Rollout Detail ",
                    content,
                    TextViewerParent::RolloutList(list_state),
                );
            }
            Ok(None) => {
                self.mode = ViewMode::RolloutList(list_state);
                self.status = Some((format!("Rollout {slug}.md not found."), true));
            }
            Err(err) => {
                self.mode = ViewMode::RolloutList(list_state);
                self.status = Some((format!("Error reading rollout: {err}"), true));
            }
        }
    }

    /// Delete a rollout from the list and filesystem.
    pub(super) fn delete_rollout(&mut self, slug: &str) {
        match code_core::delete_rollout_summary(&self.code_home, slug) {
            Ok(true) => {
                self.status = Some((format!("Deleted {slug}.md"), false));
                if let ViewMode::RolloutList(ref mut list) = self.mode {
                    list.entries.retain(|e| e.slug != slug);
                    list.pending_delete = None;
                    let mut state = list.list_state.get();
                    state.clamp_selection(list.entries.len());
                    list.list_state.set(state);
                    if list.entries.is_empty() {
                        self.mode = ViewMode::Main;
                    }
                }
            }
            Ok(false) => {
                self.status = Some((format!("{slug}.md already removed."), true));
                if let ViewMode::RolloutList(ref mut list) = self.mode {
                    list.pending_delete = None;
                }
            }
            Err(err) => {
                self.status = Some((format!("Delete failed: {err}"), true));
                if let ViewMode::RolloutList(ref mut list) = self.mode {
                    list.pending_delete = None;
                }
            }
        }
    }

    /// Build search state from a query against the viewer's lines.
    pub(super) fn execute_search(lines: &[String], query: &str) -> Option<TextSearchState> {
        if query.is_empty() {
            return None;
        }
        let lower = query.to_ascii_lowercase();
        let matches: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.to_ascii_lowercase().contains(&lower))
            .map(|(idx, _)| idx)
            .collect();
        Some(TextSearchState {
            query: query.to_owned(),
            matches,
            current: 0,
        })
    }

    pub(crate) fn has_back_navigation(&self) -> bool {
        !matches!(self.mode, ViewMode::Main)
    }

    // ── User memory management ──────────────────────────────────────────

    /// Open the user memory list view, loading entries from the DB.
    pub(super) fn open_user_memory_list(&mut self) {
        match code_core::list_user_memories_sync(&self.code_home) {
            Ok(entries) => {
                self.mode = ViewMode::UserMemoryList(Box::new(UserMemoryListState {
                    entries,
                    list_state: Cell::new(ScrollState::with_first_selected()),
                    viewport_rows: Cell::new(DEFAULT_VISIBLE_ROWS),
                    pending_delete: None,
                }));
            }
            Err(err) => {
                self.status = Some((format!("Error loading user memories: {err}"), true));
            }
        }
    }

    /// Open the editor to create a new user memory.
    pub(super) fn open_user_memory_create(&mut self, parent_list: Box<UserMemoryListState>) {
        self.mode = ViewMode::UserMemoryEditor(Box::new(UserMemoryEditorState {
            editing_id: None,
            content_field: FormTextField::new_multi_line(),
            tags_field: FormTextField::new_single_line(),
            focus: UserMemoryEditorFocus::Content,
            error: None,
            parent_list,
        }));
    }

    /// Open the editor to edit an existing user memory.
    pub(super) fn open_user_memory_edit(
        &mut self,
        parent_list: Box<UserMemoryListState>,
        memory: &UserMemory,
    ) {
        let tags_text = memory.tags.join(", ");
        let mut content_field = FormTextField::new_multi_line();
        content_field.set_text(&memory.content);
        let mut tags_field = FormTextField::new_single_line();
        tags_field.set_text(&tags_text);
        self.mode = ViewMode::UserMemoryEditor(Box::new(UserMemoryEditorState {
            editing_id: Some(memory.id.clone()),
            content_field,
            tags_field,
            focus: UserMemoryEditorFocus::Content,
            error: None,
            parent_list,
        }));
    }

    /// Save the user memory editor state (create or update).
    pub(super) fn save_user_memory_editor(&mut self) {
        let ViewMode::UserMemoryEditor(ref editor) = self.mode else {
            return;
        };

        let content = editor.content_field.text().trim().to_owned();
        if content.is_empty() {
            if let ViewMode::UserMemoryEditor(ref mut editor) = self.mode {
                editor.error = Some("Content cannot be empty.".to_owned());
            }
            return;
        }

        let tags: Vec<String> = editor
            .tags_field
            .text()
            .split(',')
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let memory = UserMemory {
            id: editor
                .editing_id
                .clone()
                .unwrap_or_else(|| format!("user-{}", uuid_v4_hex())),
            content,
            tags,
            scope: None,
            created_at: now,
            updated_at: now,
            pinned: true,
        };

        let is_update = editor.editing_id.is_some();
        let code_home = self.code_home.clone();

        let result = if is_update {
            code_core::update_user_memory_sync(&code_home, &memory).map(|_| ())
        } else {
            code_core::insert_user_memory_sync(&code_home, &memory)
        };

        match result {
            Ok(()) => {
                let verb = if is_update { "Updated" } else { "Created" };
                self.status = Some((format!("{verb} pinned memory."), false));
                // Return to list and refresh it.
                self.open_user_memory_list();
            }
            Err(err) => {
                if let ViewMode::UserMemoryEditor(ref mut editor) = self.mode {
                    editor.error = Some(format!("Save failed: {err}"));
                }
            }
        }
    }

    /// Delete a user memory by ID.
    pub(super) fn delete_user_memory_by_id(&mut self, id: &str) {
        match code_core::delete_user_memory_sync(&self.code_home, id) {
            Ok(true) => {
                self.status = Some(("Deleted pinned memory.".to_owned(), false));
                if let ViewMode::UserMemoryList(ref mut list) = self.mode {
                    list.entries.retain(|m| m.id != id);
                    list.pending_delete = None;
                    let mut state = list.list_state.get();
                    state.clamp_selection(list.entries.len());
                    list.list_state.set(state);
                    if list.entries.is_empty() {
                        self.mode = ViewMode::Main;
                    }
                }
            }
            Ok(false) => {
                self.status = Some(("Memory already deleted.".to_owned(), true));
                if let ViewMode::UserMemoryList(ref mut list) = self.mode {
                    list.pending_delete = None;
                }
            }
            Err(err) => {
                self.status = Some((format!("Delete failed: {err}"), true));
                if let ViewMode::UserMemoryList(ref mut list) = self.mode {
                    list.pending_delete = None;
                }
            }
        }
    }

    // ── Tag & epoch browsing ────────────────────────────────────────────

    /// Open the tag browser, showing all tags with counts.
    pub(super) fn open_tag_browser(&mut self) {
        match code_core::get_all_tag_counts_sync(&self.code_home) {
            Ok(tags) => {
                self.mode = ViewMode::TagBrowser(Box::new(TagBrowserState {
                    tags,
                    list_state: Cell::new(ScrollState::with_first_selected()),
                    viewport_rows: Cell::new(DEFAULT_VISIBLE_ROWS),
                }));
            }
            Err(err) => {
                self.status = Some((format!("Error loading tags: {err}"), true));
            }
        }
    }

    /// Open the epoch browser, optionally filtered by tag.
    pub(super) fn open_epoch_browser(&mut self, filter_tag: Option<String>) {
        let result = match &filter_tag {
            Some(tag) => code_core::list_epochs_by_tag_sync(&self.code_home, tag),
            None => code_core::list_epoch_summaries_sync(&self.code_home),
        };
        match result {
            Ok(epochs) => {
                self.mode = ViewMode::EpochBrowser(Box::new(EpochBrowserState {
                    epochs,
                    list_state: Cell::new(ScrollState::with_first_selected()),
                    viewport_rows: Cell::new(DEFAULT_VISIBLE_ROWS),
                    filter_tag,
                }));
            }
            Err(err) => {
                self.status = Some((format!("Error loading epochs: {err}"), true));
            }
        }
    }

    /// Open a text viewer showing the full epoch detail.
    pub(super) fn open_epoch_detail(
        &mut self,
        summary: &code_core::EpochSummary,
        parent_browser: Box<EpochBrowserState>,
    ) {
        let mut lines = Vec::new();

        // Human-friendly header — rollout_slug is the readable session name.
        let age = {
            let dt = chrono::DateTime::from_timestamp(summary.source_updated_at, 0);
            dt.map_or("unknown date".to_owned(), |d| d.format("%Y-%m-%d %H:%M UTC").to_string())
        };
        let provenance_label = summary.provenance.display_label();
        lines.push(summary.display_name());
        lines.push(format!("Epoch #{} · {} · {}", summary.id.epoch_index, provenance_label, age));
        lines.push(String::new());

        // Context
        lines.push("── Context ──────────────────────────".to_owned());
        if let Some(ref ws) = summary.workspace_root {
            lines.push(format!("  Workspace:  {ws}"));
        }
        if let Some(ref branch) = summary.git_branch {
            lines.push(format!("  Branch:     {branch}"));
        }
        if !summary.cwd_display.is_empty() {
            lines.push(format!("  Directory:  {}", summary.cwd_display));
        }
        if summary.workspace_root.is_none() && summary.git_branch.is_none() && summary.cwd_display.is_empty() {
            lines.push("  (no context recorded)".to_owned());
        }
        lines.push(String::new());

        // Tags & usage
        lines.push("── Tags & Usage ─────────────────────".to_owned());
        if summary.tags.is_empty() {
            lines.push("  Tags:       (none — refresh to auto-tag)".to_owned());
        } else {
            lines.push(format!("  Tags:       {}", summary.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join("  ")));
        }
        lines.push(format!("  Prompt use: {}×", summary.usage_count));
        lines.push(String::new());

        // Preview
        lines.push("── Content Preview ──────────────────".to_owned());
        if summary.preview.is_empty() {
            lines.push("  (empty — this epoch has no extracted content)".to_owned());
        } else {
            for line in summary.preview.lines() {
                lines.push(format!("  {line}"));
            }
        }

        // Internal ID (collapsed at bottom for debugging)
        lines.push(String::new());
        lines.push(format!("Session: {} ({})", summary.short_id(), summary.rollout_slug));

        self.mode = ViewMode::TextViewer(Box::new(TextViewerState {
            title: " Epoch Detail ",
            lines,
            scroll_top: Cell::new(0),
            viewport_rows: Cell::new(DEFAULT_VISIBLE_ROWS),
            parent: TextViewerParent::EpochBrowser(parent_browser),
            search: None,
        }));
    }
}

/// Generate a short hex ID (first 16 chars of a random uuid-like string).
fn uuid_v4_hex() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    format!("{nanos:x}{pid:x}")
}
