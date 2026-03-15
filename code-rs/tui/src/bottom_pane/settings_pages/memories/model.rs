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
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
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

    pub(super) fn rows() -> [RowKind; 13] {
        [
            RowKind::Scope,
            RowKind::GenerateMemories,
            RowKind::UseMemories,
            RowKind::SkipMcpOrWebSearch,
            RowKind::MaxRawMemories,
            RowKind::MaxRolloutAgeDays,
            RowKind::MaxRolloutsPerStartup,
            RowKind::MinRolloutIdleHours,
            RowKind::RefreshArtifacts,
            RowKind::ClearArtifacts,
            RowKind::OpenDirectory,
            RowKind::Apply,
            RowKind::Close,
        ]
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
            MemoriesScopeChoice::Global => true,
            MemoriesScopeChoice::Profile => self.active_profile.is_some(),
            MemoriesScopeChoice::Project => true,
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
            MemoriesScopeChoice::Global => Self::bool_label(effective).to_string(),
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
            MemoriesScopeChoice::Global => "Global".to_string(),
            MemoriesScopeChoice::Profile => match self.active_profile.as_deref() {
                Some(name) => format!("Active profile ({name})"),
                None => "Active profile (unavailable)".to_string(),
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
            RowKind::RefreshArtifacts => "Bypass throttle".to_string(),
            RowKind::ClearArtifacts => "Generated files only".to_string(),
            RowKind::OpenDirectory => self.code_home.join("memories").display().to_string(),
            RowKind::Apply => {
                if self.current_scope_dirty() {
                    "Pending".to_string()
                } else {
                    "Saved".to_string()
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
            RowKind::RefreshArtifacts => "Rebuild memory_summary.md, raw_memories.md, and rollout_summaries/* immediately.",
            RowKind::ClearArtifacts => "Delete generated artifacts only; catalog memory_mode values stay intact.",
            RowKind::OpenDirectory => "Open the memories directory in Finder/Explorer/your file manager.",
            RowKind::Apply => "Persist the current scope to config.toml.",
            RowKind::Close => "Dismiss the Memories settings view.",
        }
    }

    pub(crate) fn framed(&self) -> MemoriesSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> MemoriesSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> MemoriesSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> MemoriesSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_view_complete(&self) -> bool {
        self.is_complete
    }
}

