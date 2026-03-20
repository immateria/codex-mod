impl ChatWidget<'_> {
    pub(crate) fn on_memories_status_loaded(
        &mut self,
        target: crate::app_event::MemoriesStatusLoadTarget,
        result: Result<code_core::MemoriesStatus, String>,
    ) {
        match target {
            crate::app_event::MemoriesStatusLoadTarget::SlashCommand => match result {
                Ok(status) => {
                    self.history_push_plain_paragraphs(
                        crate::history::state::PlainMessageKind::Notice,
                        Self::format_memories_status_report(&status),
                    );
                }
                Err(err) => {
                    self.history_push_plain_state(history_cell::new_error_event(format!(
                        "Failed to read memories status: {err}",
                    )));
                }
            },
            crate::app_event::MemoriesStatusLoadTarget::SettingsView => {
                if let Err(err) = result {
                    self.flash_footer_notice(format!("Failed to load memories status: {err}"));
                }
            }
            crate::app_event::MemoriesStatusLoadTarget::RefreshCacheOnly => {}
        }
    }

    pub(crate) fn format_memories_status_report(status: &code_core::MemoriesStatus) -> Vec<String> {
        fn on_off(value: bool) -> &'static str {
            if value { "on" } else { "off" }
        }

        fn source_label(source: code_core::MemoriesSettingSource) -> &'static str {
            match source {
                code_core::MemoriesSettingSource::Default => "default",
                code_core::MemoriesSettingSource::Global => "global",
                code_core::MemoriesSettingSource::Profile => "profile",
                code_core::MemoriesSettingSource::Project => "project",
            }
        }

        fn artifact_label(
            name: &str,
            artifact: &code_core::MemoryArtifactStatus,
        ) -> String {
            let modified = artifact
                .modified_at
                .as_deref()
                .unwrap_or("never");
            format!(
                "{name}: {} ({modified})",
                if artifact.exists { "present" } else { "missing" }
            )
        }

        vec![
            format!(
                "Memories root: {}",
                status.artifacts.memory_root.display()
            ),
            format!(
                "Effective: generate={} ({}) · use={} ({}) · skip_mcp_web={} ({})",
                on_off(status.effective.generate_memories),
                source_label(status.sources.generate_memories),
                on_off(status.effective.use_memories),
                source_label(status.sources.use_memories),
                on_off(status.effective.no_memories_if_mcp_or_web_search),
                source_label(status.sources.no_memories_if_mcp_or_web_search),
            ),
            format!(
                "Limits: retained={} ({}) · age={}d ({}) · scan={} ({}) · idle={}h ({})",
                status.effective.max_raw_memories_for_consolidation,
                source_label(status.sources.max_raw_memories_for_consolidation),
                status.effective.max_rollout_age_days,
                source_label(status.sources.max_rollout_age_days),
                status.effective.max_rollouts_per_startup,
                source_label(status.sources.max_rollouts_per_startup),
                status.effective.min_rollout_idle_hours,
                source_label(status.sources.min_rollout_idle_hours),
            ),
            artifact_label("memory_summary.md", &status.artifacts.summary),
            artifact_label("raw_memories.md", &status.artifacts.raw_memories),
            format!(
                "rollout_summaries/: {} ({}) · count={}",
                if status.artifacts.rollout_summaries.exists {
                    "present"
                } else {
                    "missing"
                },
                status
                    .artifacts
                    .rollout_summaries
                    .modified_at
                    .as_deref()
                    .unwrap_or("never"),
                status.artifacts.rollout_summary_count,
            ),
            format!(
                "SQLite: {} · threads={} · stage1={} · pending={} · running={} · dead_lettered={} · artifact_dirty={} · artifact_job={}{}",
                if status.db.db_exists { "present" } else { "missing" },
                status.db.thread_count,
                status.db.stage1_epoch_count,
                status.db.pending_stage1_count,
                status.db.running_stage1_count,
                status.db.dead_lettered_stage1_count,
                on_off(status.db.artifact_dirty),
                on_off(status.db.artifact_job_running),
                status
                    .db
                    .last_artifact_build_at
                    .as_deref()
                    .map(|value| format!(" · last_build={value}"))
                    .unwrap_or_default(),
            ),
        ]
    }

    pub(crate) fn handle_login_command(&mut self) {
        self.show_login_accounts_view();
    }

    pub(crate) fn auth_manager(&self) -> Arc<AuthManager> {
        self.auth_manager.clone()
    }

    pub(crate) fn reload_auth(&self) -> bool {
        self.auth_manager.reload()
    }

    pub(crate) fn show_login_accounts_view(&mut self) {
        let ticket = self.make_background_tail_ticket();
        let (view, state_rc) = LoginAccountsView::new(
            self.config.code_home.clone(),
            self.app_event_tx.clone(),
            ticket,
            self.config.cli_auth_credentials_store_mode,
        );
        self.login_view_state = Some(LoginAccountsState::weak_handle(&state_rc));
        self.login_add_view_state = None;

        let showing_accounts_in_overlay = self.settings.overlay.as_ref().is_some_and(|overlay| {
            !overlay.is_menu_active() && overlay.active_section() == SettingsSection::Accounts
        });
        if showing_accounts_in_overlay
            && let Some(overlay) = self.settings.overlay.as_mut()
            && let Some(content) = overlay.accounts_content_mut() {
                content.show_manage_accounts(state_rc);
                self.request_redraw();
                return;
            }

        self.bottom_pane.show_login_accounts(view);
        self.request_redraw();
    }

    pub(crate) fn show_login_add_account_view(&mut self) {
        let ticket = self.make_background_tail_ticket();
        let (view, state_rc) = LoginAddAccountView::new(
            self.config.code_home.clone(),
            self.app_event_tx.clone(),
            ticket,
            self.config.cli_auth_credentials_store_mode,
        );
        self.login_add_view_state = Some(LoginAddAccountState::weak_handle(&state_rc));
        self.login_view_state = None;

        let showing_accounts_in_overlay = self.settings.overlay.as_ref().is_some_and(|overlay| {
            !overlay.is_menu_active() && overlay.active_section() == SettingsSection::Accounts
        });
        if showing_accounts_in_overlay
            && let Some(overlay) = self.settings.overlay.as_mut()
            && let Some(content) = overlay.accounts_content_mut() {
                content.show_add_account(state_rc);
                self.request_redraw();
                return;
            }

        self.bottom_pane.show_login_add_account(view);
        self.request_redraw();
    }

    fn with_login_add_view<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(&mut LoginAddAccountState),
    {
        if let Some(weak) = &self.login_add_view_state
            && let Some(state_rc) = weak.upgrade() {
                f(&mut state_rc.borrow_mut());
                self.request_redraw();
                return true;
            }
        false
    }

    pub(crate) fn notify_login_chatgpt_started(&mut self, auth_url: String) {
        if self.with_login_add_view(|state| state.acknowledge_chatgpt_started(auth_url.clone())) {
        }
    }

    pub(crate) fn notify_login_chatgpt_failed(&mut self, error: String) {
        if self.with_login_add_view(|state| state.acknowledge_chatgpt_failed(error.clone())) {
        }
    }

    pub(crate) fn notify_login_chatgpt_complete(&mut self, result: Result<(), String>) {
        if self.with_login_add_view(|state| state.on_chatgpt_complete(result.clone())) {
        }
    }

    pub(crate) fn notify_login_device_code_pending(&mut self) {
        let _ =
            self.with_login_add_view(
                crate::bottom_pane::settings_pages::accounts::LoginAddAccountState::begin_device_code_flow,
            );
    }

    pub(crate) fn notify_login_device_code_ready(&mut self, authorize_url: String, user_code: String) {
        let _ = self.with_login_add_view(|state| state.set_device_code_ready(authorize_url.clone(), user_code.clone()));
    }

    pub(crate) fn notify_login_device_code_failed(&mut self, error: String) {
        let _ = self.with_login_add_view(|state| state.on_device_code_failed(error.clone()));
    }

    pub(crate) fn notify_login_device_code_complete(&mut self, result: Result<(), String>) {
        if self.with_login_add_view(|state| state.on_chatgpt_complete(result.clone())) {
        }
    }

    pub(crate) fn notify_login_flow_cancelled(&mut self) {
        let _ =
            self.with_login_add_view(
                crate::bottom_pane::settings_pages::accounts::LoginAddAccountState::cancel_active_flow,
            );
    }

    pub(crate) fn login_add_view_active(&self) -> bool {
        self.login_add_view_state
            .as_ref()
            .and_then(std::rc::Weak::upgrade)
            .is_some()
    }

    pub(crate) fn set_using_chatgpt_auth(&mut self, using: bool) {
        self.config.using_chatgpt_auth = using;
        self.bottom_pane.set_using_chatgpt_auth(using);
    }

}
