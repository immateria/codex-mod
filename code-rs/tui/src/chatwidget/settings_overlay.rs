use ratatui::layout::Rect;
use std::cell::{Cell, RefCell};

use crate::bottom_pane::SettingsSection;

/// Two disjoint hit ranges per overview row (label + optional summary).
type OverviewHitRanges = Vec<[Option<(u16, u16)>; 2]>;

mod agents;
#[cfg(feature = "browser-automation")]
mod chrome;
mod contents;
mod limits;
mod overlay_input;
mod overlay_render;
mod types;

pub(crate) use self::agents::{AgentOverviewRow, AgentsSettingsContent};
#[cfg(feature = "browser-automation")]
pub(crate) use self::chrome::ChromeSettingsContent;
pub(crate) use self::contents::{
    AccountsSettingsContent,
    AppsSettingsContent,
    AutoDriveSettingsContent,
    ExecLimitsSettingsContent,
    InterfaceSettingsContent,
    ReplSettingsContent,
    MemoriesSettingsContent,
    McpSettingsContent,
    ModelSettingsContent,
    NotificationsSettingsContent,
    PlanningSettingsContent,
    PluginsSettingsContent,
    PromptsSettingsContent,
    PersonalitySettingsContent,
    ReviewSettingsContent,
    SecretsSettingsContent,
    ShellSettingsContent,
    ShellEscalationSettingsContent,
    ShellProfilesSettingsContent,
    SkillsSettingsContent,
    ThemeSettingsContent,
    UpdatesSettingsContent,
    ValidationSettingsContent,
};
#[cfg(feature = "managed-network-proxy")]
pub(crate) use self::contents::NetworkSettingsContent;
pub(crate) use self::limits::LimitsSettingsContent;
pub(crate) use self::types::{SettingsContent, SettingsOverviewRow};

use self::types::{
    MenuState,
    SectionState,
    SettingsHelpOverlay,
    SettingsOverlayFocus,
    SettingsOverlayMode,
};

pub(crate) struct SettingsOverlayView {
    overview_rows: Vec<SettingsOverviewRow>,
    mode: SettingsOverlayMode,
    focus: SettingsOverlayFocus,
    last_section: SettingsSection,
    help: Option<SettingsHelpOverlay>,
    model_content: Option<ModelSettingsContent>,
    planning_content: Option<PlanningSettingsContent>,
    personality_content: Option<PersonalitySettingsContent>,
    theme_content: Option<ThemeSettingsContent>,
    interface_content: Option<InterfaceSettingsContent>,
    shell_content: Option<ShellSettingsContent>,
    shell_escalation_content: Option<ShellEscalationSettingsContent>,
    shell_profiles_content: Option<ShellProfilesSettingsContent>,
    exec_limits_content: Option<ExecLimitsSettingsContent>,
    updates_content: Option<UpdatesSettingsContent>,
    notifications_content: Option<NotificationsSettingsContent>,
    accounts_content: Option<AccountsSettingsContent>,
    secrets_content: Option<SecretsSettingsContent>,
    apps_content: Option<AppsSettingsContent>,
    memories_content: Option<MemoriesSettingsContent>,
    prompts_content: Option<PromptsSettingsContent>,
    skills_content: Option<SkillsSettingsContent>,
    plugins_content: Option<PluginsSettingsContent>,
    mcp_content: Option<McpSettingsContent>,
    repl_content: Option<ReplSettingsContent>,
    #[cfg(feature = "managed-network-proxy")]
    network_content: Option<NetworkSettingsContent>,
    agents_content: Option<AgentsSettingsContent>,
    review_content: Option<ReviewSettingsContent>,
    validation_content: Option<ValidationSettingsContent>,
    auto_drive_content: Option<AutoDriveSettingsContent>,
    limits_content: Option<LimitsSettingsContent>,
    #[cfg(feature = "browser-automation")]
    chrome_content: Option<ChromeSettingsContent>,
    /// Last overlay content area for mouse calculations
    last_content_area: RefCell<Rect>,
    /// Exact sidebar rectangle from the last render pass for hover hit testing.
    last_sidebar_area: RefCell<Rect>,
    /// Last rendered overview list area for menu-mode mouse hit testing.
    last_overview_list_area: RefCell<Rect>,
    /// Section mapping for each rendered overview list line (None for separators).
    last_overview_line_sections: RefCell<Vec<Option<SettingsSection>>>,
    /// Horizontal hit ranges for each rendered overview list line (separators use `[None; 2]`).
    /// Stored as (`start_x`, `end_x`) in terminal cell coordinates. Some lines have
    /// two disjoint hit ranges (e.g., label + summary).
    last_overview_line_hit_ranges: RefCell<OverviewHitRanges>,
    /// Vertical scroll offset used for the overview list.
    last_overview_scroll: RefCell<usize>,
    /// Last panel inner area where content is rendered (for mouse forwarding)
    last_panel_inner_area: RefCell<Rect>,
    /// Currently hovered section in sidebar (for visual feedback)
    hovered_section: RefCell<Option<SettingsSection>>,
    /// Cached sidebar hit ranges for each rendered row (label only).
    /// Each entry corresponds to the visible row at `y = last_sidebar_area.y + i`.
    last_sidebar_line_hit_ranges: RefCell<Vec<Option<(u16, u16)>>>,
    /// Cached logical section index for each rendered sidebar row.
    /// The index is the row index used by `sidebar_section_at()`.
    last_sidebar_line_indices: RefCell<Vec<Option<usize>>>,
    /// Whether the sidebar is collapsed (hidden) in section view.
    sidebar_collapsed: Cell<bool>,
    /// Rectangle of the sidebar toggle button for mouse hit testing.
    last_sidebar_toggle_area: RefCell<Rect>,
    /// Rectangle of the close button [x] in the title bar for mouse hit testing.
    last_close_button_area: RefCell<Rect>,
    /// Whether the close button is currently hovered.
    close_button_hovered: Cell<bool>,
    /// Set when the user clicks the close button; the parent reads and clears.
    pub(crate) close_requested: Cell<bool>,
    /// Hit areas for clickable shortcut hints in the footer bar.
    last_hint_hit_areas: RefCell<Vec<crate::bottom_pane::settings_ui::hints::HintHitArea>>,
}

impl SettingsOverlayView {
    pub(crate) fn new(section: SettingsSection) -> Self {
        let section_state = SectionState::new(section);
        Self {
            overview_rows: Vec::new(),
            mode: SettingsOverlayMode::Section(section_state),
            focus: SettingsOverlayFocus::Content,
            last_section: section,
            help: None,
            model_content: None,
            planning_content: None,
            personality_content: None,
            theme_content: None,
            interface_content: None,
            shell_content: None,
            shell_escalation_content: None,
            shell_profiles_content: None,
            exec_limits_content: None,
            updates_content: None,
            notifications_content: None,
            accounts_content: None,
            secrets_content: None,
            apps_content: None,
            memories_content: None,
            prompts_content: None,
            skills_content: None,
            plugins_content: None,
            mcp_content: None,
            repl_content: None,
            #[cfg(feature = "managed-network-proxy")]
            network_content: None,
            agents_content: None,
            review_content: None,
            validation_content: None,
            auto_drive_content: None,
            limits_content: None,
            #[cfg(feature = "browser-automation")]
            chrome_content: None,
            last_content_area: RefCell::new(Rect::default()),
            last_sidebar_area: RefCell::new(Rect::default()),
            last_overview_list_area: RefCell::new(Rect::default()),
            last_overview_line_sections: RefCell::new(Vec::new()),
            last_overview_line_hit_ranges: RefCell::new(Vec::new()),
            last_overview_scroll: RefCell::new(0),
            last_panel_inner_area: RefCell::new(Rect::default()),
            hovered_section: RefCell::new(None),
            last_sidebar_line_hit_ranges: RefCell::new(Vec::new()),
            last_sidebar_line_indices: RefCell::new(Vec::new()),
            sidebar_collapsed: Cell::new(false),
            last_sidebar_toggle_area: RefCell::new(Rect::default()),
            last_close_button_area: RefCell::new(Rect::default()),
            close_button_hovered: Cell::new(false),
            close_requested: Cell::new(false),
            last_hint_hit_areas: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn active_section(&self) -> SettingsSection {
        match self.mode {
            SettingsOverlayMode::Menu(state) => state.selected(),
            SettingsOverlayMode::Section(state) => state.active(),
        }
    }

    pub(crate) fn is_menu_active(&self) -> bool {
        matches!(self.mode, SettingsOverlayMode::Menu(_))
    }

    pub(crate) fn is_sidebar_focused(&self) -> bool {
        matches!(self.focus, SettingsOverlayFocus::Sidebar)
    }

    pub(crate) fn is_content_focused(&self) -> bool {
        matches!(self.focus, SettingsOverlayFocus::Content)
    }

    pub(crate) fn is_sidebar_collapsed(&self) -> bool {
        self.sidebar_collapsed.get()
    }

    pub(crate) fn toggle_sidebar_collapsed(&mut self) -> bool {
        let collapsed = !self.sidebar_collapsed.get();
        self.sidebar_collapsed.set(collapsed);
        if collapsed && self.is_sidebar_focused() {
            self.focus = SettingsOverlayFocus::Content;
        }
        true
    }

    pub(crate) fn set_focus_sidebar(&mut self) -> bool {
        self.set_focus(SettingsOverlayFocus::Sidebar)
    }

    pub(crate) fn set_focus_content(&mut self) -> bool {
        self.set_focus(SettingsOverlayFocus::Content)
    }

    fn set_focus(&mut self, focus: SettingsOverlayFocus) -> bool {
        if self.focus == focus {
            return false;
        }
        self.focus = focus;
        true
    }

    pub(crate) fn set_mode_menu(&mut self, selected: Option<SettingsSection>) {
        if !self.is_menu_active() {
            if let Some(content) = self.active_content_mut() {
                content.on_deactivate();
            }
        }
        let section = selected.unwrap_or(self.last_section);
        self.mode = SettingsOverlayMode::Menu(MenuState::new(section));
        self.focus = SettingsOverlayFocus::Sidebar;
        if self.help.is_some() {
            self.show_help(true);
        }
    }

    pub(crate) fn set_mode_section(&mut self, section: SettingsSection) {
        let was_menu = self.is_menu_active();
        if let SettingsOverlayMode::Section(state) = self.mode
            && state.active() != section
            && let Some(content) = self.active_content_mut()
        {
            content.on_deactivate();
        }

        self.mode = SettingsOverlayMode::Section(SectionState::new(section));
        self.last_section = section;
        if was_menu {
            self.focus = SettingsOverlayFocus::Content;
        }
        if self.help.is_some() {
            self.show_help(false);
        }
    }

    pub(crate) fn is_help_visible(&self) -> bool {
        self.help.is_some()
    }

    pub(crate) fn show_help(&mut self, menu_active: bool) {
        self.help = Some(if menu_active {
            SettingsHelpOverlay::overview()
        } else {
            let content_has_back = self
                .active_content()
                .is_some_and(SettingsContent::has_back_navigation);
            SettingsHelpOverlay::section(self.active_section(), self.focus, content_has_back)
        });
    }

    pub(crate) fn hide_help(&mut self) {
        self.help = None;
    }

    pub(crate) fn set_overview_rows(&mut self, rows: Vec<SettingsOverviewRow>) {
        let fallback = rows.first().map_or(self.last_section, |row| row.section);
        let active_visible = rows.iter().any(|row| row.section == self.active_section());
        if !active_visible {
            self.set_mode_menu(Some(fallback));
        }
        self.overview_rows = rows;
    }

    pub(crate) fn set_model_content(&mut self, content: ModelSettingsContent) {
        self.model_content = Some(content);
    }

    pub(crate) fn set_planning_content(&mut self, content: PlanningSettingsContent) {
        self.planning_content = Some(content);
    }

    pub(crate) fn set_personality_content(&mut self, content: PersonalitySettingsContent) {
        self.personality_content = Some(content);
    }

    pub(crate) fn set_theme_content(&mut self, content: ThemeSettingsContent) {
        self.theme_content = Some(content);
    }

    pub(crate) fn set_interface_content(&mut self, content: InterfaceSettingsContent) {
        self.interface_content = Some(content);
    }

    pub(crate) fn set_shell_content(&mut self, content: ShellSettingsContent) {
        self.shell_content = Some(content);
    }

    pub(crate) fn set_shell_escalation_content(&mut self, content: ShellEscalationSettingsContent) {
        self.shell_escalation_content = Some(content);
    }

    pub(crate) fn set_shell_profiles_content(&mut self, content: ShellProfilesSettingsContent) {
        self.shell_profiles_content = Some(content);
    }

    pub(crate) fn set_exec_limits_content(&mut self, content: ExecLimitsSettingsContent) {
        self.exec_limits_content = Some(content);
    }

    pub(crate) fn shell_profiles_content_mut(&mut self) -> Option<&mut ShellProfilesSettingsContent> {
        self.shell_profiles_content.as_mut()
    }

    pub(crate) fn set_updates_content(&mut self, content: UpdatesSettingsContent) {
        self.updates_content = Some(content);
    }

    pub(crate) fn set_notifications_content(&mut self, content: NotificationsSettingsContent) {
        self.notifications_content = Some(content);
    }

    pub(crate) fn set_accounts_content(&mut self, content: AccountsSettingsContent) {
        self.accounts_content = Some(content);
    }

    pub(crate) fn accounts_content_mut(&mut self) -> Option<&mut AccountsSettingsContent> {
        self.accounts_content.as_mut()
    }

    pub(crate) fn set_secrets_content(&mut self, content: SecretsSettingsContent) {
        self.secrets_content = Some(content);
    }

    pub(crate) fn set_apps_content(&mut self, content: AppsSettingsContent) {
        self.apps_content = Some(content);
    }

    pub(crate) fn set_memories_content(&mut self, content: MemoriesSettingsContent) {
        self.memories_content = Some(content);
    }

    pub(crate) fn set_prompts_content(&mut self, content: PromptsSettingsContent) {
        self.prompts_content = Some(content);
    }

    pub(crate) fn set_skills_content(&mut self, content: SkillsSettingsContent) {
        self.skills_content = Some(content);
    }

    pub(crate) fn set_plugins_content(&mut self, content: PluginsSettingsContent) {
        self.plugins_content = Some(content);
    }

    pub(crate) fn set_mcp_content(&mut self, content: McpSettingsContent) {
        self.mcp_content = Some(content);
    }

    pub(crate) fn mcp_content(&self) -> Option<&McpSettingsContent> {
        self.mcp_content.as_ref()
    }

    pub(crate) fn set_repl_content(&mut self, content: ReplSettingsContent) {
        self.repl_content = Some(content);
    }

    #[cfg(feature = "managed-network-proxy")]
    pub(crate) fn set_network_content(&mut self, content: NetworkSettingsContent) {
        self.network_content = Some(content);
    }

    pub(crate) fn set_agents_content(&mut self, content: AgentsSettingsContent) {
        self.agents_content = Some(content);
    }

    pub(crate) fn set_review_content(&mut self, content: ReviewSettingsContent) {
        self.review_content = Some(content);
    }

    pub(crate) fn set_validation_content(&mut self, content: ValidationSettingsContent) {
        self.validation_content = Some(content);
    }

    pub(crate) fn set_auto_drive_content(&mut self, content: AutoDriveSettingsContent) {
        self.auto_drive_content = Some(content);
    }

    pub(crate) fn set_limits_content(&mut self, content: LimitsSettingsContent) {
        self.limits_content = Some(content);
    }

    #[cfg(feature = "browser-automation")]
    pub(crate) fn set_chrome_content(&mut self, content: ChromeSettingsContent) {
        self.chrome_content = Some(content);
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub(crate) fn agents_content(&self) -> Option<&AgentsSettingsContent> {
        self.agents_content.as_ref()
    }

    pub(crate) fn agents_content_mut(&mut self) -> Option<&mut AgentsSettingsContent> {
        self.agents_content.as_mut()
    }

    pub(crate) fn review_content_mut(&mut self) -> Option<&mut ReviewSettingsContent> {
        self.review_content.as_mut()
    }

    pub(crate) fn planning_content_mut(&mut self) -> Option<&mut PlanningSettingsContent> {
        self.planning_content.as_mut()
    }

    pub(crate) fn auto_drive_content_mut(&mut self) -> Option<&mut AutoDriveSettingsContent> {
        self.auto_drive_content.as_mut()
    }

    pub(crate) fn limits_content_mut(&mut self) -> Option<&mut LimitsSettingsContent> {
        self.limits_content.as_mut()
    }

    pub(crate) fn limits_content(&self) -> Option<&LimitsSettingsContent> {
        self.limits_content.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn overview_sections(&self) -> Vec<SettingsSection> {
        self.overview_rows.iter().map(|row| row.section).collect()
    }

    #[cfg(test)]
    pub(crate) fn has_plugins_content(&self) -> bool {
        self.plugins_content.is_some()
    }

    pub(crate) fn set_section(&mut self, section: SettingsSection) -> bool {
        if self.active_section() == section {
            return false;
        }
        self.last_section = section;
        match &mut self.mode {
            SettingsOverlayMode::Menu(state) => state.set_selected(section),
            SettingsOverlayMode::Section(state) => state.set_active(section),
        }
        if self.help.is_some() {
            self.show_help(self.is_menu_active());
        }
        true
    }

    pub(crate) fn select_next(&mut self) -> bool {
        if !self.overview_rows.is_empty() {
            let sections: Vec<SettingsSection> =
                self.overview_rows.iter().map(|row| row.section).collect();
            if let Some(idx) = sections
                .iter()
                .position(|section| *section == self.active_section())
            {
                let next = sections[(idx + 1) % sections.len()];
                return self.set_section(next);
            }
        }
        let mut idx = self.index_of(self.active_section());
        idx = (idx + 1) % SettingsSection::ALL.len();
        self.set_section(SettingsSection::ALL[idx])
    }

    pub(crate) fn select_previous(&mut self) -> bool {
        if !self.overview_rows.is_empty() {
            let sections: Vec<SettingsSection> =
                self.overview_rows.iter().map(|row| row.section).collect();
            if let Some(idx) = sections
                .iter()
                .position(|section| *section == self.active_section())
            {
                let new_idx = idx.checked_sub(1).unwrap_or(sections.len() - 1);
                return self.set_section(sections[new_idx]);
            }
        }
        let mut idx = self.index_of(self.active_section());
        idx = idx.checked_sub(1).unwrap_or(SettingsSection::ALL.len() - 1);
        self.set_section(SettingsSection::ALL[idx])
    }

    fn index_of(&self, section: SettingsSection) -> usize {
        SettingsSection::ALL
            .iter()
            .position(|s| *s == section)
            .unwrap_or(0)
    }

    pub(crate) fn active_content(&self) -> Option<&dyn SettingsContent> {
        if self.is_menu_active() {
            return None;
        }

        match self.active_section() {
            SettingsSection::Model => self
                .model_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Interface => self
                .interface_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Shell => self
                .shell_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::ShellEscalation => self
                .shell_escalation_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::ShellProfiles => self
                .shell_profiles_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::ExecLimits => self
                .exec_limits_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Planning => self
                .planning_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Personality => self
                .personality_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Theme => self
                .theme_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Updates => self
                .updates_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Agents => self
                .agents_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Accounts => self
                .accounts_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Secrets => self
                .secrets_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Apps => self
                .apps_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Memories => self
                .memories_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Prompts => self
                .prompts_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Skills => self
                .skills_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Plugins => self
                .plugins_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::AutoDrive => self
                .auto_drive_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Review => self
                .review_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Validation => self
                .validation_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Limits => self
                .limits_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            #[cfg(feature = "browser-automation")]
            SettingsSection::Chrome => self
                .chrome_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Notifications => self
                .notifications_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Mcp => self
                .mcp_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            SettingsSection::Repl => self
                .repl_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
            #[cfg(feature = "managed-network-proxy")]
            SettingsSection::Network => self
                .network_content
                .as_ref()
                .map(|content| content as &dyn SettingsContent),
        }
    }

    pub(crate) fn active_content_mut(&mut self) -> Option<&mut dyn SettingsContent> {
        if self.is_menu_active() {
            return None;
        }

        match self.active_section() {
            SettingsSection::Model => self
                .model_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Interface => self
                .interface_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Shell => self
                .shell_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::ShellEscalation => self
                .shell_escalation_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::ShellProfiles => self
                .shell_profiles_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::ExecLimits => self
                .exec_limits_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Planning => self
                .planning_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Personality => self
                .personality_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Theme => self
                .theme_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Updates => self
                .updates_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Agents => self
                .agents_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Accounts => self
                .accounts_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Secrets => self
                .secrets_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Apps => self
                .apps_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Memories => self
                .memories_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Prompts => self
                .prompts_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Skills => self
                .skills_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Plugins => self
                .plugins_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::AutoDrive => self
                .auto_drive_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Review => self
                .review_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Validation => self
                .validation_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Limits => self
                .limits_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            #[cfg(feature = "browser-automation")]
            SettingsSection::Chrome => self
                .chrome_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Notifications => self
                .notifications_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Mcp => self
                .mcp_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            SettingsSection::Repl => self
                .repl_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
            #[cfg(feature = "managed-network-proxy")]
            SettingsSection::Network => self
                .network_content
                .as_mut()
                .map(|content| content as &mut dyn SettingsContent),
        }
    }

    pub(crate) fn notify_close(&mut self) {
        if let Some(content) = self.active_content_mut() {
            content.on_deactivate();
            content.on_close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_overview_rows_repairs_section_mode_when_active_disappears() {
        let mut overlay = SettingsOverlayView::new(SettingsSection::Apps);
        // Overlay starts in section mode.
        assert!(!overlay.is_menu_active());
        assert_eq!(overlay.active_section(), SettingsSection::Apps);

        overlay.set_overview_rows(vec![SettingsOverviewRow::new(SettingsSection::Model, None)]);

        assert!(overlay.is_menu_active());
        assert_eq!(overlay.active_section(), SettingsSection::Model);
    }
}
