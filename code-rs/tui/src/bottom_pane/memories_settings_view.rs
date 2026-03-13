use std::cell::Cell;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use code_core::config_types::{MemoriesConfig, MemoriesToml};

use crate::app_event::{AppEvent, MemoriesArtifactsAction, MemoriesSettingsScope};
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::editor_page::SettingsEditorPage;
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::row_page::SettingsRowPage;
use super::settings_ui::rows::{KeyValueRow, StyledText};
use super::BottomPane;

const DEFAULT_VISIBLE_ROWS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemoriesScopeChoice {
    Global,
    Profile,
    Project,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    Scope,
    GenerateMemories,
    UseMemories,
    SkipMcpOrWebSearch,
    MaxRawMemories,
    MaxRolloutAgeDays,
    MaxRolloutsPerStartup,
    MinRolloutIdleHours,
    RefreshArtifacts,
    ClearArtifacts,
    OpenDirectory,
    Apply,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditTarget {
    MaxRawMemories,
    MaxRolloutAgeDays,
    MaxRolloutsPerStartup,
    MinRolloutIdleHours,
}

#[derive(Debug)]
enum ViewMode {
    Main,
    Edit {
        target: EditTarget,
        field: FormTextField,
        error: Option<String>,
    },
    Transition,
}

pub(crate) struct MemoriesSettingsView {
    code_home: PathBuf,
    current_project: PathBuf,
    active_profile: Option<String>,
    global_settings: MemoriesToml,
    saved_global_settings: MemoriesToml,
    profile_settings: Option<MemoriesToml>,
    saved_profile_settings: Option<MemoriesToml>,
    project_settings: Option<MemoriesToml>,
    saved_project_settings: Option<MemoriesToml>,
    scope: MemoriesScopeChoice,
    mode: ViewMode,
    status: Option<(String, bool)>,
    state: Cell<ScrollState>,
    viewport_rows: Cell<usize>,
    is_complete: bool,
    app_event_tx: AppEventSender,
}

pub(crate) type MemoriesSettingsViewFramed<'v> =
    super::chrome_view::Framed<'v, MemoriesSettingsView>;
pub(crate) type MemoriesSettingsViewContentOnly<'v> =
    super::chrome_view::ContentOnly<'v, MemoriesSettingsView>;
pub(crate) type MemoriesSettingsViewFramedMut<'v> =
    super::chrome_view::FramedMut<'v, MemoriesSettingsView>;
pub(crate) type MemoriesSettingsViewContentOnlyMut<'v> =
    super::chrome_view::ContentOnlyMut<'v, MemoriesSettingsView>;

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

    fn rows() -> [RowKind; 13] {
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

    fn app_scope(&self) -> MemoriesSettingsScope {
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

    fn supports_scope(&self, scope: MemoriesScopeChoice) -> bool {
        match scope {
            MemoriesScopeChoice::Global => true,
            MemoriesScopeChoice::Profile => self.active_profile.is_some(),
            MemoriesScopeChoice::Project => true,
        }
    }

    fn effective_settings(&self) -> MemoriesConfig {
        code_core::config_types::resolve_memories_config(
            Some(&self.global_settings),
            self.profile_settings.as_ref(),
            self.project_settings.as_ref(),
        )
    }

    fn current_status(&self) -> Result<Option<code_core::MemoriesStatus>, String> {
        code_core::get_cached_memories_status(
            &self.code_home,
            Some(&self.global_settings),
            self.profile_settings.as_ref(),
            self.project_settings.as_ref(),
        )
        .map_err(|err| err.to_string())
    }

    fn selected_row(&self) -> RowKind {
        let rows = Self::rows();
        let idx = self
            .state
            .get()
            .selected_idx
            .unwrap_or(0)
            .min(rows.len().saturating_sub(1));
        rows[idx]
    }

    fn current_scope_settings(&self) -> Option<&MemoriesToml> {
        match self.scope {
            MemoriesScopeChoice::Global => Some(&self.global_settings),
            MemoriesScopeChoice::Profile => self.profile_settings.as_ref(),
            MemoriesScopeChoice::Project => self.project_settings.as_ref(),
        }
    }

    fn ensure_current_scope_settings_mut(&mut self) -> &mut MemoriesToml {
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

    fn current_scope_saved_settings(&self) -> Option<&MemoriesToml> {
        match self.scope {
            MemoriesScopeChoice::Global => Some(&self.saved_global_settings),
            MemoriesScopeChoice::Profile => self.saved_profile_settings.as_ref(),
            MemoriesScopeChoice::Project => self.saved_project_settings.as_ref(),
        }
    }

    fn current_scope_payload(&self) -> MemoriesToml {
        self.current_scope_settings().cloned().unwrap_or_default()
    }

    fn mark_scope_saved(&mut self) {
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

    fn current_scope_dirty(&self) -> bool {
        self.current_scope_payload() != self.current_scope_saved_settings().cloned().unwrap_or_default()
    }

    fn prune_optional_scope(&mut self) {
        match self.scope {
            MemoriesScopeChoice::Global => {}
            MemoriesScopeChoice::Profile => {
                if self.profile_settings.as_ref().is_some_and(MemoriesToml::is_empty) {
                    self.profile_settings = None;
                }
            }
            MemoriesScopeChoice::Project => {
                if self.project_settings.as_ref().is_some_and(MemoriesToml::is_empty) {
                    self.project_settings = None;
                }
            }
        }
    }

    fn bool_label(value: bool) -> &'static str {
        if value { "On" } else { "Off" }
    }

    fn source_label(source: code_core::MemoriesSettingSource) -> &'static str {
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
                Some(value) => format!("override {}", Self::bool_label(value).to_ascii_lowercase()),
                None => format!("inherit ({})", Self::bool_label(effective).to_ascii_lowercase()),
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
            MemoriesScopeChoice::Project => format!("Current project ({})", self.current_project.display()),
        }
    }

    fn row_value(&self, row: RowKind) -> String {
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

    fn row_label(row: RowKind) -> &'static str {
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

    fn row_description(&self, row: RowKind) -> &'static str {
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

    fn render_header_lines(&self) -> Vec<Line<'static>> {
        let dim = Style::default().fg(colors::text_dim());
        match self.current_status() {
            Ok(Some(status)) => {
                let mut lines = vec![
                    Line::from(Span::styled(
                        format!(
                            "Effective: generate {} ({}) · use {} ({}) · skip {} ({})",
                            Self::bool_label(status.effective.generate_memories).to_ascii_lowercase(),
                            Self::source_label(status.sources.generate_memories),
                            Self::bool_label(status.effective.use_memories).to_ascii_lowercase(),
                            Self::source_label(status.sources.use_memories),
                            Self::bool_label(status.effective.no_memories_if_mcp_or_web_search)
                                .to_ascii_lowercase(),
                            Self::source_label(status.sources.no_memories_if_mcp_or_web_search),
                        ),
                        dim,
                    )),
                    Line::from(Span::styled(
                        format!(
                            "Limits: retained {} ({}) · age {}d ({}) · scan {} ({}) · idle {}h ({})",
                            status.effective.max_raw_memories_for_consolidation,
                            Self::source_label(status.sources.max_raw_memories_for_consolidation),
                            status.effective.max_rollout_age_days,
                            Self::source_label(status.sources.max_rollout_age_days),
                            status.effective.max_rollouts_per_startup,
                            Self::source_label(status.sources.max_rollouts_per_startup),
                            status.effective.min_rollout_idle_hours,
                            Self::source_label(status.sources.min_rollout_idle_hours),
                        ),
                        dim,
                    )),
                    Line::from(Span::styled(
                        format!(
                            "Artifacts: summary={} · raw={} · rollout_summaries={} (count={})",
                            if status.artifacts.summary.exists { "present" } else { "missing" },
                            if status.artifacts.raw_memories.exists { "present" } else { "missing" },
                            if status.artifacts.rollout_summaries.exists { "present" } else { "missing" },
                            status.artifacts.rollout_summary_count,
                        ),
                        dim,
                    )),
                    Line::from(Span::styled(
                        format!(
                            "SQLite: {} · threads {} · stage1 {} · pending {} · running {} · dead_lettered {} · dirty {}",
                            if status.db.db_exists { "present" } else { "missing" },
                            status.db.thread_count,
                            status.db.stage1_epoch_count,
                            status.db.pending_stage1_count,
                            status.db.running_stage1_count,
                            status.db.dead_lettered_stage1_count,
                            if status.db.artifact_dirty { "yes" } else { "no" },
                        ),
                        dim,
                    )),
                ];
                if self.active_profile.is_none() {
                    lines.push(Line::from(Span::styled(
                        "Active profile scope is unavailable in this session.",
                        dim,
                    )));
                }
                lines.push(Line::from(""));
                lines
            }
            Ok(None) => {
                let mut lines = vec![Line::from(Span::styled(
                    "Memories status loading…",
                    dim,
                ))];
                if self.active_profile.is_none() {
                    lines.push(Line::from(Span::styled(
                        "Active profile scope is unavailable in this session.",
                        dim,
                    )));
                }
                lines.push(Line::from(""));
                lines
            }
            Err(err) => vec![
                Line::from(Span::styled(format!("Memories status unavailable: {err}"), dim)),
                Line::from(""),
            ],
        }
    }

    fn render_footer_lines(&self) -> Vec<Line<'static>> {
        vec![Line::from(vec![
            Span::styled("↑↓", Style::default().fg(colors::function())),
            Span::styled(" move  ", Style::default().fg(colors::text_dim())),
            Span::styled("←/→", Style::default().fg(colors::function())),
            Span::styled(" cycle  ", Style::default().fg(colors::text_dim())),
            Span::styled("Enter", Style::default().fg(colors::success())),
            Span::styled(" edit/activate  ", Style::default().fg(colors::text_dim())),
            Span::styled("Ctrl+S", Style::default().fg(colors::success())),
            Span::styled(" apply  ", Style::default().fg(colors::text_dim())),
            Span::styled("Esc", Style::default().fg(colors::error())),
            Span::styled(" close", Style::default().fg(colors::text_dim())),
        ])]
    }

    fn main_footer_lines(&self) -> Vec<Line<'static>> {
        let footer_text = self
            .status
            .as_ref()
            .map(|(text, _)| text.clone())
            .unwrap_or_else(|| self.row_description(self.selected_row()).to_string());
        let footer_style = if self.status.as_ref().is_some_and(|(_, is_error)| *is_error) {
            Style::default().fg(colors::error())
        } else {
            Style::default().fg(colors::text_dim())
        };

        let mut lines = vec![Line::from(Span::styled(footer_text, footer_style))];
        lines.extend(self.render_footer_lines());
        lines
    }

    fn edit_page(
        scope: MemoriesScopeChoice,
        target: EditTarget,
        error: Option<&str>,
    ) -> SettingsEditorPage<'static> {
        let label = match target {
            EditTarget::MaxRawMemories => "Max retained memories",
            EditTarget::MaxRolloutAgeDays => "Max rollout age (days)",
            EditTarget::MaxRolloutsPerStartup => "Max rollouts per refresh",
            EditTarget::MinRolloutIdleHours => "Min rollout idle (hours)",
        };
        let scope_note = match scope {
            MemoriesScopeChoice::Global => "Global scope saves a concrete value.",
            MemoriesScopeChoice::Profile | MemoriesScopeChoice::Project => {
                "Leave blank to inherit from the next broader scope."
            }
        };
        let post_field_lines = match error {
            Some(message) => vec![Line::from(Span::styled(
                message.to_string(),
                Style::default().fg(colors::warning()),
            ))],
            None => vec![Line::from(Span::styled(
                "Ctrl+S or Enter to save. Esc to cancel.",
                Style::default().fg(colors::text_dim()),
            ))],
        };
        SettingsEditorPage::new(
            " Memories ",
            SettingsPanelStyle::bottom_pane(),
            label,
            vec![
                Line::from(Span::styled(scope_note, Style::default().fg(colors::text_dim()))),
                Line::from(""),
            ],
            post_field_lines,
        )
        .with_field_margin(Margin::new(2, 0))
    }

    fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status = Some((text.into(), is_error));
    }

    fn clear_status(&mut self) {
        self.status = None;
    }

    fn cycle_scope(&mut self, forward: bool) {
        let ordered = [
            MemoriesScopeChoice::Global,
            MemoriesScopeChoice::Profile,
            MemoriesScopeChoice::Project,
        ];
        let current_idx = ordered
            .iter()
            .position(|scope| *scope == self.scope)
            .unwrap_or(0);
        for step in 1..=ordered.len() {
            let idx = if forward {
                (current_idx + step) % ordered.len()
            } else {
                (current_idx + ordered.len() - step) % ordered.len()
            };
            let candidate = ordered[idx];
            if self.supports_scope(candidate) {
                self.scope = candidate;
                self.clear_status();
                return;
            }
        }
        self.set_status("Active profile scope is unavailable.", true);
    }

    fn cycle_scope_from_row(&mut self, forward: bool) {
        if self.active_profile.is_none() {
            self.set_status("Active profile scope is unavailable.", true);
        }
        self.cycle_scope(forward);
    }

    fn toggle_bool_row(&mut self, row: RowKind) {
        let effective = self.effective_settings();
        match self.scope {
            MemoriesScopeChoice::Global => {
                let settings = &mut self.global_settings;
                match row {
                    RowKind::GenerateMemories => {
                        settings.generate_memories = Some(!effective.generate_memories)
                    }
                    RowKind::UseMemories => settings.use_memories = Some(!effective.use_memories),
                    RowKind::SkipMcpOrWebSearch => {
                        settings.no_memories_if_mcp_or_web_search =
                            Some(!effective.no_memories_if_mcp_or_web_search)
                    }
                    _ => {}
                }
            }
            MemoriesScopeChoice::Profile | MemoriesScopeChoice::Project => {
                let inherited = match row {
                    RowKind::GenerateMemories => effective.generate_memories,
                    RowKind::UseMemories => effective.use_memories,
                    RowKind::SkipMcpOrWebSearch => {
                        effective.no_memories_if_mcp_or_web_search
                    }
                    _ => false,
                };
                let settings = self.ensure_current_scope_settings_mut();
                let target = match row {
                    RowKind::GenerateMemories => &mut settings.generate_memories,
                    RowKind::UseMemories => &mut settings.use_memories,
                    RowKind::SkipMcpOrWebSearch => {
                        &mut settings.no_memories_if_mcp_or_web_search
                    }
                    _ => return,
                };
                *target = match *target {
                    None => Some(!inherited),
                    Some(true) => Some(false),
                    Some(false) => None,
                };
                self.prune_optional_scope();
            }
        }
        self.clear_status();
    }

    fn edit_target_for_row(row: RowKind) -> Option<EditTarget> {
        match row {
            RowKind::MaxRawMemories => Some(EditTarget::MaxRawMemories),
            RowKind::MaxRolloutAgeDays => Some(EditTarget::MaxRolloutAgeDays),
            RowKind::MaxRolloutsPerStartup => Some(EditTarget::MaxRolloutsPerStartup),
            RowKind::MinRolloutIdleHours => Some(EditTarget::MinRolloutIdleHours),
            _ => None,
        }
    }

    fn field_text_for_target(&self, target: EditTarget) -> String {
        let scoped = self.current_scope_settings();
        match (self.scope, target) {
            (MemoriesScopeChoice::Global, EditTarget::MaxRawMemories) => self
                .global_settings
                .max_raw_memories_for_consolidation
                .unwrap_or(self.effective_settings().max_raw_memories_for_consolidation)
                .to_string(),
            (MemoriesScopeChoice::Global, EditTarget::MaxRolloutAgeDays) => self
                .global_settings
                .max_rollout_age_days
                .unwrap_or(self.effective_settings().max_rollout_age_days)
                .to_string(),
            (MemoriesScopeChoice::Global, EditTarget::MaxRolloutsPerStartup) => self
                .global_settings
                .max_rollouts_per_startup
                .unwrap_or(self.effective_settings().max_rollouts_per_startup)
                .to_string(),
            (MemoriesScopeChoice::Global, EditTarget::MinRolloutIdleHours) => self
                .global_settings
                .min_rollout_idle_hours
                .unwrap_or(self.effective_settings().min_rollout_idle_hours)
                .to_string(),
            (_, EditTarget::MaxRawMemories) => scoped
                .and_then(|settings| {
                    settings
                        .max_raw_memories_for_consolidation
                        .or(settings.max_raw_memories_for_global)
                })
                .map(|value| value.to_string())
                .unwrap_or_default(),
            (_, EditTarget::MaxRolloutAgeDays) => scoped
                .and_then(|settings| settings.max_rollout_age_days)
                .map(|value| value.to_string())
                .unwrap_or_default(),
            (_, EditTarget::MaxRolloutsPerStartup) => scoped
                .and_then(|settings| settings.max_rollouts_per_startup)
                .map(|value| value.to_string())
                .unwrap_or_default(),
            (_, EditTarget::MinRolloutIdleHours) => scoped
                .and_then(|settings| settings.min_rollout_idle_hours)
                .map(|value| value.to_string())
                .unwrap_or_default(),
        }
    }

    fn open_edit_for(&mut self, target: EditTarget) {
        let mut field = FormTextField::new_single_line();
        if !matches!(self.scope, MemoriesScopeChoice::Global) {
            field.set_placeholder("inherit");
        }
        field.set_text(&self.field_text_for_target(target));
        self.mode = ViewMode::Edit {
            target,
            field,
            error: None,
        };
    }

    fn apply_numeric_edit(&mut self, target: EditTarget, text: &str) -> Result<(), String> {
        match (self.scope, target) {
            (MemoriesScopeChoice::Global, EditTarget::MaxRawMemories) => {
                let value: usize = text.trim().parse().map_err(|_| "Max retained memories must be an integer >= 1".to_string())?;
                if value == 0 {
                    return Err("Max retained memories must be >= 1".to_string());
                }
                self.global_settings.max_raw_memories_for_consolidation = Some(value);
                self.global_settings.max_raw_memories_for_global = None;
            }
            (MemoriesScopeChoice::Global, EditTarget::MaxRolloutAgeDays) => {
                let value: i64 = text.trim().parse().map_err(|_| "Max rollout age must be an integer >= 0".to_string())?;
                if value < 0 {
                    return Err("Max rollout age must be >= 0".to_string());
                }
                self.global_settings.max_rollout_age_days = Some(value);
            }
            (MemoriesScopeChoice::Global, EditTarget::MaxRolloutsPerStartup) => {
                let value: usize = text.trim().parse().map_err(|_| "Max rollouts per refresh must be an integer >= 1".to_string())?;
                if value == 0 {
                    return Err("Max rollouts per refresh must be >= 1".to_string());
                }
                self.global_settings.max_rollouts_per_startup = Some(value);
            }
            (MemoriesScopeChoice::Global, EditTarget::MinRolloutIdleHours) => {
                let value: i64 = text.trim().parse().map_err(|_| "Min rollout idle must be an integer >= 0".to_string())?;
                if value < 0 {
                    return Err("Min rollout idle must be >= 0".to_string());
                }
                self.global_settings.min_rollout_idle_hours = Some(value);
            }
            (_, EditTarget::MaxRawMemories) => {
                let settings = self.ensure_current_scope_settings_mut();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    settings.max_raw_memories_for_consolidation = None;
                    settings.max_raw_memories_for_global = None;
                } else {
                    let value: usize = trimmed.parse().map_err(|_| "Max retained memories must be an integer >= 1".to_string())?;
                    if value == 0 {
                        return Err("Max retained memories must be >= 1".to_string());
                    }
                    settings.max_raw_memories_for_consolidation = Some(value);
                    settings.max_raw_memories_for_global = None;
                }
                self.prune_optional_scope();
            }
            (_, EditTarget::MaxRolloutAgeDays) => {
                let settings = self.ensure_current_scope_settings_mut();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    settings.max_rollout_age_days = None;
                } else {
                    let value: i64 = trimmed.parse().map_err(|_| "Max rollout age must be an integer >= 0".to_string())?;
                    if value < 0 {
                        return Err("Max rollout age must be >= 0".to_string());
                    }
                    settings.max_rollout_age_days = Some(value);
                }
                self.prune_optional_scope();
            }
            (_, EditTarget::MaxRolloutsPerStartup) => {
                let settings = self.ensure_current_scope_settings_mut();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    settings.max_rollouts_per_startup = None;
                } else {
                    let value: usize = trimmed.parse().map_err(|_| "Max rollouts per refresh must be an integer >= 1".to_string())?;
                    if value == 0 {
                        return Err("Max rollouts per refresh must be >= 1".to_string());
                    }
                    settings.max_rollouts_per_startup = Some(value);
                }
                self.prune_optional_scope();
            }
            (_, EditTarget::MinRolloutIdleHours) => {
                let settings = self.ensure_current_scope_settings_mut();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    settings.min_rollout_idle_hours = None;
                } else {
                    let value: i64 = trimmed.parse().map_err(|_| "Min rollout idle must be an integer >= 0".to_string())?;
                    if value < 0 {
                        return Err("Min rollout idle must be >= 0".to_string());
                    }
                    settings.min_rollout_idle_hours = Some(value);
                }
                self.prune_optional_scope();
            }
        }
        Ok(())
    }

    fn dispatch_apply(&mut self) {
        if matches!(self.scope, MemoriesScopeChoice::Profile) && self.active_profile.is_none() {
            self.set_status("Active profile scope is unavailable.", true);
            return;
        }
        let payload = self.current_scope_payload();
        self.app_event_tx.send(AppEvent::SetMemoriesSettings {
            scope: self.app_scope(),
            settings: payload,
        });
        self.mark_scope_saved();
        self.set_status("Applying memories settings…", false);
    }

    fn trigger_action(&mut self, action: MemoriesArtifactsAction) {
        let message = match action {
            MemoriesArtifactsAction::Refresh => "Refreshing memories artifacts…",
            MemoriesArtifactsAction::Clear => "Clearing generated memories artifacts…",
        };
        self.app_event_tx
            .send(AppEvent::RunMemoriesArtifactsAction { action });
        self.set_status(message, false);
    }

    fn open_memories_directory(&mut self) {
        let path = self.code_home.join("memories");
        match crate::native_file_manager::reveal_path(&path) {
            Ok(()) => self.set_status(format!("Opened {}", path.display()), false),
            Err(err) => self.set_status(format!("Failed to open {}: {err}", path.display()), true),
        }
    }

    fn activate_selected(&mut self) {
        match self.selected_row() {
            RowKind::Scope => self.cycle_scope_from_row(true),
            RowKind::GenerateMemories | RowKind::UseMemories | RowKind::SkipMcpOrWebSearch => {
                self.toggle_bool_row(self.selected_row())
            }
            RowKind::MaxRawMemories
            | RowKind::MaxRolloutAgeDays
            | RowKind::MaxRolloutsPerStartup
            | RowKind::MinRolloutIdleHours => {
                if let Some(target) = Self::edit_target_for_row(self.selected_row()) {
                    self.open_edit_for(target);
                }
            }
            RowKind::RefreshArtifacts => self.trigger_action(MemoriesArtifactsAction::Refresh),
            RowKind::ClearArtifacts => self.trigger_action(MemoriesArtifactsAction::Clear),
            RowKind::OpenDirectory => self.open_memories_directory(),
            RowKind::Apply => self.dispatch_apply(),
            RowKind::Close => self.is_complete = true,
        }
    }

    fn process_main_key_event(&mut self, key_event: KeyEvent) -> bool {
        let rows = Self::rows();
        let mut state = self.state.get();
        let visible = self.viewport_rows.get().max(1);
        match key_event.code {
            KeyCode::Esc => {
                self.is_complete = true;
                self.state.set(state);
                return true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.move_up_wrap_visible(rows.len(), visible);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.move_down_wrap_visible(rows.len(), visible);
            }
            KeyCode::Left => match self.selected_row() {
                RowKind::Scope => self.cycle_scope_from_row(false),
                RowKind::GenerateMemories | RowKind::UseMemories | RowKind::SkipMcpOrWebSearch => {
                    self.toggle_bool_row(self.selected_row())
                }
                _ => {}
            },
            KeyCode::Right => match self.selected_row() {
                RowKind::Scope => self.cycle_scope_from_row(true),
                RowKind::GenerateMemories | RowKind::UseMemories | RowKind::SkipMcpOrWebSearch => {
                    self.toggle_bool_row(self.selected_row())
                }
                _ => {}
            },
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.activate_selected();
            }
            KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.dispatch_apply();
            }
            _ => {
                self.state.set(state);
                return false;
            }
        }
        state.ensure_visible(rows.len(), visible);
        self.state.set(state);
        true
    }

    fn process_edit_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        let ViewMode::Edit {
            target,
            mut field,
            mut error,
        } = mode else {
            self.mode = mode;
            return false;
        };

        let handled = match key_event.code {
            KeyCode::Esc => {
                self.mode = ViewMode::Main;
                true
            }
            KeyCode::Enter => {
                let text = field.text().to_string();
                match self.apply_numeric_edit(target, &text) {
                    Ok(()) => {
                        self.mode = ViewMode::Main;
                        self.clear_status();
                    }
                    Err(err) => {
                        error = Some(err);
                    }
                }
                true
            }
            KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                let text = field.text().to_string();
                match self.apply_numeric_edit(target, &text) {
                    Ok(()) => {
                        self.mode = ViewMode::Main;
                        self.clear_status();
                    }
                    Err(err) => {
                        error = Some(err);
                    }
                }
                true
            }
            _ => {
                error = None;
                field.handle_key(key_event)
            }
        };

        if matches!(self.mode, ViewMode::Transition) {
            self.mode = ViewMode::Edit {
                target,
                field,
                error,
            };
        }
        handled
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match self.mode {
            ViewMode::Main => self.process_main_key_event(key_event),
            ViewMode::Edit { .. } => self.process_edit_key_event(key_event),
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    pub(crate) fn framed(&self) -> MemoriesSettingsViewFramed<'_> {
        super::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> MemoriesSettingsViewContentOnly<'_> {
        super::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> MemoriesSettingsViewFramedMut<'_> {
        super::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> MemoriesSettingsViewContentOnlyMut<'_> {
        super::chrome_view::ContentOnlyMut::new(self)
    }

    fn main_page(&self) -> SettingsRowPage<'_> {
        SettingsRowPage::new(
            " Memories ",
            self.render_header_lines(),
            self.main_footer_lines(),
        )
    }

    fn selection_index_at_framed(&self, x: u16, y: u16, area: Rect) -> Option<usize> {
        let layout = self.main_page().framed().layout(area)?;
        SettingsRowPage::selection_index_at(
            layout.body,
            x,
            y,
            self.state.get().scroll_top,
            Self::rows().len(),
        )
    }

    fn selection_index_at_content_only(&self, x: u16, y: u16, area: Rect) -> Option<usize> {
        let layout = self.main_page().content_only().layout(area)?;
        SettingsRowPage::selection_index_at(
            layout.body,
            x,
            y,
            self.state.get().scroll_top,
            Self::rows().len(),
        )
    }

    fn handle_mouse_event_direct_content_only(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        if matches!(self.mode, ViewMode::Main) {
            let rows = Self::rows();
            let mut selected = self.state.get().selected_idx.unwrap_or(0);
            let result = route_selectable_list_mouse_with_config(
                mouse_event,
                &mut selected,
                rows.len(),
                |x, y| self.selection_index_at_content_only(x, y, area),
                SelectableListMouseConfig {
                    hover_select: false,
                    activate_on_left_click: true,
                    scroll_select: true,
                    require_pointer_hit_for_scroll: false,
                    scroll_behavior: ScrollSelectionBehavior::Wrap,
                },
            );
            let mut state = self.state.get();
            state.selected_idx = Some(selected);
            state.ensure_visible(rows.len(), self.viewport_rows.get().max(1));
            self.state.set(state);
            if matches!(result, SelectableListMouseResult::Activated) {
                self.activate_selected();
            }
            return result.handled();
        }

        match &mut self.mode {
            ViewMode::Edit { target, field, error } => match mouse_event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let Some(field_area) = Self::edit_page(self.scope, *target, error.as_deref())
                        .content_only()
                        .layout(area)
                        .map(|layout| layout.field)
                    else {
                        return false;
                    };
                    field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                }
                MouseEventKind::ScrollDown => field.handle_mouse_scroll(true),
                MouseEventKind::ScrollUp => field.handle_mouse_scroll(false),
                _ => false,
            },
            ViewMode::Main | ViewMode::Transition => false,
        }
    }

    fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        if matches!(self.mode, ViewMode::Main) {
            let rows = Self::rows();
            let mut selected = self.state.get().selected_idx.unwrap_or(0);
            let result = route_selectable_list_mouse_with_config(
                mouse_event,
                &mut selected,
                rows.len(),
                |x, y| self.selection_index_at_framed(x, y, area),
                SelectableListMouseConfig {
                    hover_select: false,
                    activate_on_left_click: true,
                    scroll_select: true,
                    require_pointer_hit_for_scroll: false,
                    scroll_behavior: ScrollSelectionBehavior::Wrap,
                },
            );
            let mut state = self.state.get();
            state.selected_idx = Some(selected);
            state.ensure_visible(rows.len(), self.viewport_rows.get().max(1));
            self.state.set(state);
            if matches!(result, SelectableListMouseResult::Activated) {
                self.activate_selected();
            }
            return result.handled();
        }

        match &mut self.mode {
            ViewMode::Edit { target, field, error } => match mouse_event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let Some(field_area) = Self::edit_page(self.scope, *target, error.as_deref())
                        .framed()
                        .layout(area)
                        .map(|layout| layout.field)
                    else {
                        return false;
                    };
                    field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                }
                MouseEventKind::ScrollDown => field.handle_mouse_scroll(true),
                MouseEventKind::ScrollUp => field.handle_mouse_scroll(false),
                _ => false,
            },
            ViewMode::Main | ViewMode::Transition => false,
        }
    }

    fn render_main_with(&self, area: Rect, buf: &mut Buffer, content_only: bool) {
        let rows = Self::rows();
        let total = rows.len();
        let mut state = self.state.get();
        state.clamp_selection(total);

        let selected = state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
        let scroll_top = state.scroll_top.min(total.saturating_sub(1));
        let row_specs: Vec<KeyValueRow<'_>> = rows
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                let is_selected = idx == selected;
                let mut spec = KeyValueRow::new(Self::row_label(*row));
                let value = self.row_value(*row);
                if !value.is_empty() {
                    spec = spec.with_value(StyledText::new(
                        value,
                        if is_selected {
                            Style::default().fg(colors::text_bright()).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(colors::text_dim())
                        },
                    ));
                }
                spec
            })
            .collect();
        let page = self.main_page();
        let Some(layout) = (if content_only {
            page.content_only()
                .render(area, buf, scroll_top, Some(selected), &row_specs)
        } else {
            page.framed()
                .render(area, buf, scroll_top, Some(selected), &row_specs)
        }) else {
            return;
        };
        state.ensure_visible(total, layout.visible_rows());
        self.viewport_rows.set(layout.visible_rows());
        self.state.set(state);
    }

    fn render_edit_with(
        &self,
        area: Rect,
        buf: &mut Buffer,
        target: EditTarget,
        field: &FormTextField,
        error: Option<&str>,
        content_only: bool,
    ) {
        let page = Self::edit_page(self.scope, target, error);
        if content_only {
            let _ = page.content_only().render(area, buf, field);
        } else {
            let _ = page.framed().render(area, buf, field);
        }
    }

    fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main | ViewMode::Transition => self.render_main_with(area, buf, true),
            ViewMode::Edit { target, field, error } => {
                self.render_edit_with(area, buf, *target, field, error.as_deref(), true)
            }
        }
    }

    fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main | ViewMode::Transition => self.render_main_with(area, buf, false),
            ViewMode::Edit { target, field, error } => {
                self.render_edit_with(area, buf, *target, field, error.as_deref(), false)
            }
        }
    }

    pub(crate) fn is_view_complete(&self) -> bool {
        self.is_complete
    }
}

impl super::chrome_view::ChromeRenderable for MemoriesSettingsView {
    fn render_in_framed_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_framed(area, buf);
    }

    fn render_in_content_only_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_content_only(area, buf);
    }
}

impl super::chrome_view::ChromeMouseHandler for MemoriesSettingsView {
    fn handle_mouse_event_direct_in_framed_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_framed(mouse_event, area)
    }

    fn handle_mouse_event_direct_in_content_only_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_content_only(mouse_event, area)
    }
}

impl<'a> BottomPaneView<'a> for MemoriesSettingsView {
    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_direct(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(
            self.framed_mut()
                .handle_mouse_event_direct(mouse_event, area),
        )
    }

    fn is_complete(&self) -> bool {
        self.is_view_complete()
    }

    fn desired_height(&self, _width: u16) -> u16 {
        19
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.framed().render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::mpsc::channel;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tempfile::tempdir;

    use super::MemoriesSettingsView;
    use crate::app_event::{AppEvent, MemoriesSettingsScope};
    use crate::app_event_sender::AppEventSender;

    fn make_view(sender: AppEventSender) -> MemoriesSettingsView {
        MemoriesSettingsView::new(
            PathBuf::from("/tmp/code-home"),
            PathBuf::from("/tmp/project"),
            Some("work".to_string()),
            None,
            None,
            None,
            sender,
        )
    }

    #[test]
    fn apply_emits_global_memories_settings_event() {
        let (tx, rx) = channel();
        let sender = AppEventSender::new(tx);
        let mut view = make_view(sender);

        view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        view.handle_key_event_direct(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        for _ in 0..10 {
            view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        match rx.recv().expect("app event") {
            AppEvent::SetMemoriesSettings { scope, settings } => {
                assert_eq!(scope, MemoriesSettingsScope::Global);
                assert_eq!(settings.generate_memories, Some(false));
            }
            other => panic!("expected SetMemoriesSettings event, got: {other:?}"),
        }
    }

    #[test]
    fn apply_can_target_project_scope() {
        let (tx, rx) = channel();
        let sender = AppEventSender::new(tx);
        let mut view = make_view(sender);

        view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        view.handle_key_event_direct(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        for _ in 0..10 {
            view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        match rx.recv().expect("app event") {
            AppEvent::SetMemoriesSettings { scope, settings } => {
                assert_eq!(
                    scope,
                    MemoriesSettingsScope::Project {
                        path: PathBuf::from("/tmp/project"),
                    }
                );
                assert_eq!(settings.generate_memories, Some(false));
            }
            other => panic!("expected SetMemoriesSettings event, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn header_shows_loading_when_db_exists_but_status_snapshot_is_missing() {
        let (tx, _rx) = channel();
        let sender = AppEventSender::new(tx);
        let temp = tempdir().expect("tempdir");
        tokio::fs::write(temp.path().join("memories_state.sqlite"), "")
            .await
            .expect("create db marker");
        let view = MemoriesSettingsView::new(
            temp.path().to_path_buf(),
            PathBuf::from("/tmp/project"),
            Some("work".to_string()),
            None,
            None,
            None,
            sender,
        );

        let rendered = view
            .render_header_lines()
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("Memories status loading"));
    }

    #[tokio::test]
    async fn header_shows_cached_snapshot_after_status_load() {
        let (tx, _rx) = channel();
        let sender = AppEventSender::new(tx);
        let temp = tempdir().expect("tempdir");
        tokio::fs::write(temp.path().join("memories_state.sqlite"), "")
            .await
            .expect("create db marker");
        code_core::load_memories_status(temp.path(), None, None, None)
            .await
            .expect("seed memories status cache");

        let view = MemoriesSettingsView::new(
            temp.path().to_path_buf(),
            PathBuf::from("/tmp/project"),
            Some("work".to_string()),
            None,
            None,
            None,
            sender,
        );

        let rendered = view
            .render_header_lines()
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!rendered.contains("Memories status loading"));
        assert!(rendered.contains("SQLite: present"));
    }
}
