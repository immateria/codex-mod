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

    const ROWS_WITH_FILE_MANAGER: [RowKind; 17] = [
            RowKind::Scope,
            RowKind::GenerateMemories,
            RowKind::UseMemories,
            RowKind::SkipMcpOrWebSearch,
            RowKind::MaxRawMemories,
            RowKind::MaxRolloutAgeDays,
            RowKind::MaxRolloutsPerStartup,
            RowKind::MinRolloutIdleHours,
            RowKind::ViewSummary,
            RowKind::ViewRawMemories,
            RowKind::BrowseRollouts,
            RowKind::ViewStatus,
            RowKind::RefreshArtifacts,
            RowKind::ClearArtifacts,
            RowKind::OpenDirectory,
            RowKind::Apply,
            RowKind::Close,
    ];

    const ROWS_NO_FILE_MANAGER: [RowKind; 16] = [
        RowKind::Scope,
        RowKind::GenerateMemories,
        RowKind::UseMemories,
        RowKind::SkipMcpOrWebSearch,
        RowKind::MaxRawMemories,
        RowKind::MaxRolloutAgeDays,
        RowKind::MaxRolloutsPerStartup,
        RowKind::MinRolloutIdleHours,
        RowKind::ViewSummary,
        RowKind::ViewRawMemories,
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
        self.current_scope_payload()
            != self
                .current_scope_saved_settings()
                .cloned()
                .unwrap_or_default()
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
                    "Pending".to_owned()
                } else {
                    "Saved".to_owned()
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
            RowKind::ViewSummary => "View injected summary",
            RowKind::ViewRawMemories => "View raw memories",
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
            RowKind::Scope => "Switch between global, active-profile, and current-project memories settings.",
            RowKind::GenerateMemories => "Controls whether this scope contributes sessions to future memory artifact rebuilds.",
            RowKind::UseMemories => "Controls whether memory_summary.md is injected into future developer instructions.",
            RowKind::SkipMcpOrWebSearch => "If enabled, sessions that use MCP or native web search are marked polluted for future extraction.",
            RowKind::MaxRawMemories => "Cap retained sessions written into raw_memories.md and rollout_summaries/*.",
            RowKind::MaxRolloutAgeDays => "Ignore sessions older than this many days during artifact rebuilds.",
            RowKind::MaxRolloutsPerStartup => "Cap how many catalog sessions are scanned during each rebuild.",
            RowKind::MinRolloutIdleHours => "Ignore sessions that are newer than this idle window.",
            RowKind::ViewSummary => "View the memory_summary.md that gets injected into developer instructions.",
            RowKind::ViewRawMemories => "View raw_memories.md containing all extracted session memories.",
            RowKind::BrowseRollouts => "Browse individual rollout summary files in rollout_summaries/.",
            RowKind::ViewStatus => "View detailed memories subsystem status: config sources, artifact freshness, DB stats.",
            RowKind::RefreshArtifacts => "Rebuild memory_summary.md, raw_memories.md, and rollout_summaries/* immediately.",
            RowKind::ClearArtifacts => "Delete generated artifacts only; catalog memory_mode values stay intact.",
            RowKind::OpenDirectory => "Open the memories directory in Finder/Explorer/your file manager.",
            RowKind::Apply => "Persist the current scope to config.toml.",
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
        fn source(s: code_core::MemoriesSettingSource) -> &'static str {
            match s {
                code_core::MemoriesSettingSource::Default => "default",
                code_core::MemoriesSettingSource::Global => "global",
                code_core::MemoriesSettingSource::Profile => "profile",
                code_core::MemoriesSettingSource::Project => "project",
            }
        }
        fn artifact(name: &str, a: &code_core::MemoryArtifactStatus) -> String {
            let modified = a.modified_at.as_deref().unwrap_or("never");
            let present = if a.exists { "present" } else { "missing" };
            format!("  {name}: {present} (modified: {modified})")
        }

        let mut lines = Vec::new();
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
        lines.push(format!("  pending:        {}", status.db.pending_stage1_count));
        lines.push(format!("  running:        {}", status.db.running_stage1_count));
        lines.push(format!("  dead-lettered:  {}", status.db.dead_lettered_stage1_count));
        lines.push(format!("  artifact dirty: {}", on_off(status.db.artifact_dirty)));
        lines.push(format!("  artifact job:   {}", on_off(status.db.artifact_job_running)));
        if let Some(ref last_build) = status.db.last_artifact_build_at {
            lines.push(format!("  last build:     {last_build}"));
        }

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
}
