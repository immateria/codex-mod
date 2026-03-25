use crate::auto_drive_style::AutoDriveVariant;
use crate::chatwidget::BackgroundOrderTicket;
use crate::user_approval_widget::ApprovalRequest;
use code_common::shell_presets::ShellPreset;
use code_core::config_types::{ShellConfig, TextVerbosity, ThemeName};

use super::chat_composer::ComposerRenderMode;
use super::panes::approval_modal::ApprovalModalView;
use super::panes::auto_coordinator::{AutoCoordinatorView, AutoCoordinatorViewModel};
use super::panes::cloud_tasks::CloudTasksView;
use super::panes::custom_prompt::CustomPromptView;
use super::panes::request_user_input::RequestUserInputView;
use super::panes::resume_selection::{ResumeRow, ResumeSelectionView};
use super::panes::undo_timeline::UndoTimelineView;
use super::settings_pages;
use super::{ActiveViewKind, BottomPane, BottomPaneParams, BottomPaneView, ChatComposer};

impl<'a> BottomPane<'a> {
    pub fn new(params: BottomPaneParams) -> Self {
        let composer = ChatComposer::new(
            params.has_input_focus,
            params.app_event_tx.clone(),
            params.using_chatgpt_auth,
        );

        Self {
            composer,
            active_view: None,
            active_view_kind: ActiveViewKind::None,
            app_event_tx: params.app_event_tx,
            has_input_focus: params.has_input_focus,
            is_task_running: false,
            ctrl_c_quit_hint: false,
            compact_compose: false,
            pending_auto_coordinator: None,
            top_spacer_enabled: true,
            force_top_spacer: false,
            using_chatgpt_auth: params.using_chatgpt_auth,
            auto_drive_variant: params.auto_drive_variant,
            auto_drive_active: false,
            custom_prompts: Vec::new(),
            skills: Vec::new(),
        }
    }

    fn auto_view_mut(&mut self) -> Option<&mut AutoCoordinatorView> {
        if self.active_view_kind != ActiveViewKind::AutoCoordinator {
            return None;
        }
        self.active_view
            .as_mut()
            .and_then(|view| view.as_any_mut())
            .and_then(|any| any.downcast_mut::<AutoCoordinatorView>())
    }

    #[cfg(test)]
    pub(crate) fn auto_view_model(&self) -> Option<AutoCoordinatorViewModel> {
        if self.active_view_kind != ActiveViewKind::AutoCoordinator {
            return None;
        }

        self.active_view
            .as_ref()
            .and_then(|view| view.as_any())
            .and_then(|any| any.downcast_ref::<AutoCoordinatorView>())
            .map(|view| view.model().clone())
    }

    fn apply_auto_drive_style(&mut self) {
        if !self.auto_drive_active {
            self.composer.set_auto_drive_style(None);
            return;
        }

        let style = self.auto_drive_variant.style();
        self.composer.set_auto_drive_active(true);
        self.composer
            .set_auto_drive_style(Some(style.composer.clone()));
        if let Some(view) = self.auto_view_mut() {
            view.set_style(style);
        }

        self.request_redraw();
    }

    fn enable_auto_drive_style(&mut self) {
        if !self.auto_drive_active {
            self.auto_drive_active = true;
            self.composer.set_auto_drive_active(true);
        }
        self.apply_auto_drive_style();
    }

    fn set_view(
        &mut self,
        view: Box<dyn BottomPaneView<'a> + 'a>,
        kind: ActiveViewKind,
        height_change: bool,
    ) {
        self.active_view = Some(view);
        self.active_view_kind = kind;
        if height_change {
            self.request_redraw_with_height_change();
        } else {
            self.request_redraw();
        }
    }

    pub(super) fn set_other_view(&mut self, view: impl BottomPaneView<'a> + 'a, height_change: bool) {
        self.set_view(Box::new(view), ActiveViewKind::Other, height_change);
    }

    pub(super) fn active_view_as<T: 'static>(&mut self) -> Option<&mut T> {
        self.active_view
            .as_mut()?
            .as_any_mut()?
            .downcast_mut::<T>()
    }

    fn flush_pending_auto_coordinator(&mut self) -> bool {
        if self.active_view.is_some() {
            return false;
        }
        let Some(model) = self.pending_auto_coordinator.take() else {
            return false;
        };
        self.show_auto_coordinator_view(model);
        true
    }

    pub(super) fn clear_active_view_state(&mut self) {
        self.active_view = None;
        self.active_view_kind = ActiveViewKind::None;
        self.set_standard_terminal_hint(None);
        let _ = self.flush_pending_auto_coordinator();
    }

    pub(super) fn callback_claimed_active_view(&self, original_kind: ActiveViewKind) -> bool {
        self.active_view.is_some() || self.active_view_kind != original_kind
    }

    fn disable_auto_drive_style(&mut self) {
        if !self.auto_drive_active {
            return;
        }
        self.auto_drive_active = false;
        self.composer.set_auto_drive_active(false);
        self.composer.set_auto_drive_style(None);
        let style = self.auto_drive_variant.style();
        if let Some(view) = self.auto_view_mut() {
            view.set_style(style);
        }
        self.request_redraw();
    }

    pub(crate) fn set_auto_drive_variant(&mut self, variant: AutoDriveVariant) {
        if self.auto_drive_variant == variant {
            return;
        }
        self.auto_drive_variant = variant;
        if self.auto_drive_active {
            self.apply_auto_drive_style();
        }
    }

    pub fn show_notifications_settings(
        &mut self,
        view: settings_pages::notifications::NotificationsSettingsView,
    ) {
        self.set_other_view(view, true);
    }

    pub fn show_network_settings(&mut self, view: settings_pages::network::NetworkSettingsView) {
        self.set_other_view(view, true);
    }

    pub fn show_exec_limits_settings(
        &mut self,
        view: settings_pages::exec_limits::ExecLimitsSettingsView,
    ) {
        self.set_other_view(view, true);
    }

    pub fn show_js_repl_settings(&mut self, view: settings_pages::js_repl::JsReplSettingsView) {
        self.set_other_view(view, true);
    }

    pub fn show_interface_settings(
        &mut self,
        view: settings_pages::interface::InterfaceSettingsView,
    ) {
        self.set_other_view(view, true);
    }

    pub fn show_shell_profiles_settings(
        &mut self,
        view: settings_pages::shell_profiles::ShellProfilesSettingsView,
    ) {
        self.set_other_view(view, true);
    }

    pub(crate) fn apply_shell_profiles_generated_summary(
        &mut self,
        style: code_core::config_types::ShellScriptStyle,
        summary: String,
    ) -> bool {
        let Some(shell_profiles) =
            self.active_view_as::<settings_pages::shell_profiles::ShellProfilesSettingsView>()
        else {
            return false;
        };

        shell_profiles.apply_generated_summary(style, summary);
        self.request_redraw();
        true
    }

    pub(crate) fn apply_shell_profiles_summary_generation_error(
        &mut self,
        style: code_core::config_types::ShellScriptStyle,
        error: String,
    ) -> bool {
        let Some(shell_profiles) =
            self.active_view_as::<settings_pages::shell_profiles::ShellProfilesSettingsView>()
        else {
            return false;
        };

        shell_profiles.set_summary_generation_error(style, error);
        self.request_redraw();
        true
    }

    pub fn show_settings_overview(&mut self, view: settings_pages::overview::SettingsOverviewView) {
        self.set_other_view(view, true);
    }

    pub fn show_update_settings(&mut self, view: settings_pages::updates::UpdateSettingsView) {
        self.set_other_view(view, true);
    }

    pub fn show_status_line_setup(&mut self, view: settings_pages::status_line::StatusLineSetupView) {
        self.set_other_view(view, true);
    }

    pub fn show_planning_settings(&mut self, view: settings_pages::planning::PlanningSettingsView) {
        self.set_other_view(view, true);
    }

    pub fn show_review_settings(&mut self, view: settings_pages::review::ReviewSettingsView) {
        self.set_other_view(view, true);
    }

    pub fn show_validation_settings(
        &mut self,
        view: settings_pages::validation::ValidationSettingsView,
    ) {
        self.set_other_view(view, true);
    }

    pub fn show_prompts_settings(&mut self, view: settings_pages::prompts::PromptsSettingsView) {
        self.set_other_view(view, true);
    }

    pub fn show_skills_settings(&mut self, view: settings_pages::skills::SkillsSettingsView) {
        self.set_other_view(view, true);
    }

    pub fn show_plugins_settings(&mut self, view: settings_pages::plugins::PluginsSettingsView) {
        self.set_other_view(view, true);
    }

    pub fn show_memories_settings(&mut self, view: settings_pages::memories::MemoriesSettingsView) {
        self.set_other_view(view, true);
    }

    pub fn show_auto_drive_settings_panel(&mut self, view: settings_pages::auto_drive::AutoDriveSettingsView) {
        self.set_other_view(view, true);
    }

    pub fn show_login_accounts(&mut self, view: settings_pages::accounts::LoginAccountsView) {
        self.set_other_view(view, false);
    }

    pub fn show_login_add_account(&mut self, view: settings_pages::accounts::LoginAddAccountView) {
        self.set_other_view(view, false);
    }

    pub fn set_using_chatgpt_auth(&mut self, using: bool) {
        if self.using_chatgpt_auth != using {
            self.using_chatgpt_auth = using;
            self.composer.set_using_chatgpt_auth(using);
            self.request_redraw();
        }
    }

    pub(crate) fn has_active_view(&self) -> bool {
        self.active_view.is_some()
    }

    /// True if a modal/overlay view is currently displayed (not the composer popup).
    pub(crate) fn has_active_modal_view(&self) -> bool {
        // Consider a modal inactive once it has completed to avoid blocking
        // Esc routing and other overlay checks after a decision is made.
        match self.active_view.as_ref() {
            Some(_) if matches!(self.active_view_kind, ActiveViewKind::AutoCoordinator) => false,
            Some(view) => !view.is_complete(),
            None => false,
        }
    }

    pub(crate) fn auto_drive_view_active(&self) -> bool {
        matches!(self.active_view_kind, ActiveViewKind::AutoCoordinator)
    }

    pub(crate) fn auto_drive_style_active(&self) -> bool {
        self.auto_drive_active
    }

    /// Check if a specific view kind is currently active.
    pub fn is_view_kind_active(&self, kind: ActiveViewKind) -> bool {
        self.active_view_kind == kind
    }

    /// Called when the agent requests user approval.
    pub fn push_approval_request(&mut self, request: ApprovalRequest, ticket: BackgroundOrderTicket) {
        let (request, ticket) = if let Some(view) = self.active_view.as_mut() {
            match view.try_consume_approval_request(request, ticket) {
                Some((request, ticket)) => (request, ticket),
                None => {
                    self.request_redraw();
                    return;
                }
            }
        } else {
            (request, ticket)
        };

        // Otherwise create a new approval modal overlay.
        let modal = ApprovalModalView::new(request, ticket, self.app_event_tx.clone());
        self.set_other_view(modal, false)
    }

    /// Show the model selection UI
    pub fn show_model_selection(&mut self, params: settings_pages::model::ModelSelectionViewParams) {
        let view = settings_pages::model::ModelSelectionView::new(params, self.app_event_tx.clone());
        self.set_view(Box::new(view), ActiveViewKind::ModelSelection, true)
    }

    /// Show the shell selection UI
    pub fn show_shell_selection(&mut self, current_shell: Option<ShellConfig>, shell_presets: Vec<ShellPreset>) {
        let view = settings_pages::shell::ShellSelectionView::new(
            current_shell,
            shell_presets,
            self.app_event_tx.clone(),
        );
        self.set_view(Box::new(view), ActiveViewKind::ShellSelection, true)
    }

    /// Show the theme selection UI
    pub fn show_theme_selection(
        &mut self,
        current_theme: ThemeName,
        tail_ticket: BackgroundOrderTicket,
        before_ticket: BackgroundOrderTicket,
    ) {
        let view = settings_pages::theme::ThemeSelectionView::new(
            current_theme,
            self.app_event_tx.clone(),
            tail_ticket,
            before_ticket,
        );
        self.set_other_view(view, true)
    }

    /// Show the verbosity selection UI
    pub fn show_verbosity_selection(&mut self, current_verbosity: TextVerbosity) {
        let view = settings_pages::verbosity::VerbositySelectionView::new(
            current_verbosity,
            self.app_event_tx.clone(),
        );
        self.set_other_view(view, true)
    }

    /// Show a multi-line prompt input view (used for custom review instructions)
    pub fn show_custom_prompt(&mut self, view: CustomPromptView) {
        self.set_other_view(view, true);
    }

    pub(crate) fn show_request_user_input(&mut self, view: RequestUserInputView) {
        self.set_view(Box::new(view), ActiveViewKind::RequestUserInput, true);
    }

    pub(crate) fn close_request_user_input_view(&mut self) {
        if self.active_view_kind != ActiveViewKind::RequestUserInput {
            return;
        }

        self.clear_active_view_state();
        self.request_redraw_with_height_change();
    }

    pub(crate) fn clear_active_view(&mut self) {
        self.clear_active_view_state();
    }

    /// Show a generic list selection popup with items and actions.
    pub fn show_list_selection(&mut self, items: crate::components::list_selection_view::ListSelectionView) {
        self.set_other_view(items, true);
    }

    pub fn show_cloud_tasks(&mut self, view: CloudTasksView) {
        self.set_other_view(view, true);
    }

    /// Show the resume selection UI with structured rows
    pub fn show_resume_selection(
        &mut self,
        title: String,
        subtitle: Option<String>,
        rows: Vec<ResumeRow>,
        action: crate::app_event::SessionPickerAction,
    ) {
        let view = ResumeSelectionView::new(
            title,
            subtitle.unwrap_or_default(),
            rows,
            action,
            self.app_event_tx.clone(),
        );
        self.set_other_view(view, true)
    }

    pub fn show_undo_timeline_view(&mut self, view: UndoTimelineView) {
        self.set_other_view(view, true);
    }

    /// Show MCP servers status/toggle UI
    pub fn show_mcp_settings(&mut self, rows: settings_pages::mcp::McpServerRows) {
        let view = settings_pages::mcp::McpSettingsView::new(rows, self.app_event_tx.clone());
        self.set_other_view(view, true);
    }

    pub(crate) fn show_auto_coordinator_view(&mut self, model: AutoCoordinatorViewModel) {
        if let Some(existing) = self.active_view.as_mut()
            && self.active_view_kind == ActiveViewKind::AutoCoordinator
                && let Some(existing_any) = existing.as_any_mut()
                    && let Some(auto_view) = existing_any.downcast_mut::<AutoCoordinatorView>() {
                        self.pending_auto_coordinator = None;
                        auto_view.update_model(model);
                        auto_view.set_style(self.auto_drive_variant.style());
                        let status_text = self
                            .composer
                            .status_message()
                            .map_or_else(String::new, str::to_string);
                        let _ = auto_view.update_status_text(status_text);
                        let mode = if auto_view.composer_visible() {
                            ComposerRenderMode::Full
                        } else {
                            ComposerRenderMode::FooterOnly
                        };
                        self.composer.set_render_mode(mode);
                        self.composer.set_embedded_mode(false);
                        self.enable_auto_drive_style();
                        self.request_redraw();
                        return;
                    }

        if self.active_view.is_some() && self.active_view_kind != ActiveViewKind::AutoCoordinator {
            self.pending_auto_coordinator = Some(model);
            self.composer.set_render_mode(ComposerRenderMode::Full);
            self.enable_auto_drive_style();
            return;
        }

        let mut view = AutoCoordinatorView::new(
            model,
            self.app_event_tx.clone(),
            self.auto_drive_variant.style(),
        );
        let status_text = self
            .composer
            .status_message()
            .map_or_else(String::new, str::to_string);
        let _ = view.update_status_text(status_text);
        let mode = if view.composer_visible() {
            ComposerRenderMode::Full
        } else {
            ComposerRenderMode::FooterOnly
        };
        self.pending_auto_coordinator = None;
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::AutoCoordinator;
        self.composer.set_embedded_mode(false);
        self.composer.set_render_mode(mode);
        self.enable_auto_drive_style();
        self.request_redraw_with_height_change();
    }

    pub(crate) fn clear_auto_coordinator_view(&mut self, disable_style: bool) {
        if self.active_view_kind == ActiveViewKind::AutoCoordinator {
            self.clear_active_view_state();
            self.composer.set_embedded_mode(false);
            self.composer.set_render_mode(ComposerRenderMode::Full);
            if disable_style {
                self.disable_auto_drive_style();
            } else if self.auto_drive_active {
                self.apply_auto_drive_style();
            }
            self.request_redraw();
            return;
        }

        if disable_style {
            self.disable_auto_drive_style();
        }
    }

    pub(crate) fn release_auto_drive_style(&mut self) {
        self.disable_auto_drive_style();
    }
}
