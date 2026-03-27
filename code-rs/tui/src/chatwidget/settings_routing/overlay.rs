impl ChatWidget<'_> {
    fn mcp_tool_definition_for(
        &self,
        server_name: &str,
        tool_name: &str,
    ) -> Option<mcp_types::Tool> {
        if let Some(tool) = self.mcp_tool_catalog_by_id.get(tool_name) {
            return Some(tool.clone());
        }

        let fully_qualified = format!("{server_name}.{tool_name}");
        if let Some(tool) = self.mcp_tool_catalog_by_id.get(&fully_qualified) {
            return Some(tool.clone());
        }

        self.mcp_tool_catalog_by_id.iter().find_map(|(id, tool)| {
            let id_tool_name = id.rsplit('.').next().unwrap_or(id.as_str());
            let id_server = id
                .split_once('.')
                .map(|(server, _)| server)
                .or_else(|| id.split_once("__").map(|(server, _)| server));
            let id_has_tool_suffix = id.ends_with(tool_name)
                || id.ends_with(&format!("__{tool_name}"))
                || id.ends_with(&format!(".{tool_name}"));

            if tool.name == tool_name
                || (id_tool_name == tool_name && id_server.is_some_and(|server| server == server_name))
                || (id_has_tool_suffix && id_server.is_some_and(|server| server == server_name))
            {
                Some(tool.clone())
            } else {
                None
            }
        })
    }

    pub(crate) fn show_theme_selection(&mut self) {
        let tail_ticket = self.make_background_tail_ticket();
        let before_ticket = self.make_background_before_next_output_ticket();
        self.bottom_pane.show_theme_selection(
            crate::theme::current_theme_name(),
            tail_ticket,
            before_ticket,
        );
    }

    fn close_settings_overlay_if_open(&mut self) {
        if self.settings.overlay.is_some() {
            self.close_settings_overlay();
        }
    }

    fn open_bottom_pane_settings<F>(&mut self, show: F) -> bool
    where
        F: FnOnce(&mut Self),
    {
        self.close_settings_overlay_if_open();
        show(self);
        true
    }

    fn populate_settings_overlay_content(&mut self, overlay: &mut SettingsOverlayView) {
        overlay.set_model_content(self.build_model_settings_content());
        overlay.set_planning_content(self.build_planning_settings_content());
        overlay.set_theme_content(self.build_theme_settings_content());
        overlay.set_interface_content(self.build_interface_settings_content());
        overlay.set_shell_content(self.build_shell_settings_content());
        overlay.set_shell_profiles_content(self.build_shell_profiles_settings_content());
        overlay.set_exec_limits_content(self.build_exec_limits_settings_content());
        if let Some(update_content) = self.build_updates_settings_content() {
            overlay.set_updates_content(update_content);
        }
        overlay.set_accounts_content(self.build_accounts_settings_content());
        overlay.set_apps_content(self.build_apps_settings_content());
        overlay.set_memories_content(self.build_memories_settings_content());
        overlay.set_notifications_content(self.build_notifications_settings_content());
        overlay.set_prompts_content(self.build_prompts_settings_content());
        overlay.set_skills_content(self.build_skills_settings_content());
        overlay.set_plugins_content(self.build_plugins_settings_content());
        if let Some(mcp_content) = self.build_mcp_settings_content() {
            overlay.set_mcp_content(mcp_content);
        }
        overlay.set_js_repl_content(self.build_js_repl_settings_content());
        overlay.set_network_content(self.build_network_settings_content());
        overlay.set_agents_content(self.build_agents_settings_content());
        overlay.set_auto_drive_content(self.build_auto_drive_settings_content());
        overlay.set_review_content(self.build_review_settings_content());
        overlay.set_validation_content(self.build_validation_settings_content());
        overlay.set_limits_content(self.build_limits_settings_content());
        overlay.set_chrome_content(self.build_chrome_settings_content(None));
        overlay.set_overview_rows(self.build_settings_overview_rows());
    }

    pub(crate) fn apply_reloaded_config(&mut self, config: code_core::config::Config) {
        self.config = config;
        self.refresh_settings_overview_rows();
        let skills_content = self.build_skills_settings_content();
        let plugins_content = self.build_plugins_settings_content();
        let mcp_content = self.build_mcp_settings_content();

        if let Some(overlay) = self.settings.overlay.as_mut() {
            overlay.set_skills_content(skills_content);
            overlay.set_plugins_content(plugins_content);
            if let Some(mcp_content) = mcp_content {
                overlay.set_mcp_content(mcp_content);
            }
        }
        self.request_redraw();
    }

    pub(crate) fn apply_reloaded_config_keep_settings_state(&mut self, config: code_core::config::Config) {
        self.config = config;
        self.plugins_set_sources_snapshot(self.config.plugins.clone());
        self.apps_set_sources_snapshot(self.config.active_profile.clone(), self.config.apps_sources.clone());
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    fn apply_settings_overlay_mode(
        overlay: &mut SettingsOverlayView,
        section: Option<SettingsSection>,
    ) {
        match section {
            Some(section) => overlay.set_mode_section(section),
            None => overlay.set_mode_menu(None),
        }
    }

    fn should_open_settings_overlay(&self) -> bool {
        use code_core::config_types::SettingsMenuOpenMode;
        match self.config.tui.settings_menu.open_mode {
            SettingsMenuOpenMode::Overlay => true,
            SettingsMenuOpenMode::Bottom => false,
            SettingsMenuOpenMode::Auto => {
                let width = self.layout.last_frame_width.get();
                width >= self.config.tui.settings_menu.overlay_min_width
            }
        }
    }

    fn show_settings_overlay_view(&mut self, section: Option<SettingsSection>) {
        let initial_section = section
            .or_else(|| {
                self.settings
                    .overlay
                    .as_ref()
                    .map(super::settings_overlay::SettingsOverlayView::active_section)
            })
            .unwrap_or(SettingsSection::Model);

        let mut overlay = SettingsOverlayView::new(initial_section);
        self.populate_settings_overlay_content(&mut overlay);
        Self::apply_settings_overlay_mode(&mut overlay, section);

        self.settings.overlay = Some(overlay);
        self.settings.bottom_route = None;
        self.request_redraw();
    }

    fn show_settings_bottom_pane(&mut self, section: Option<SettingsSection>) {
        let initial_section = section
            .or_else(|| {
                self.settings
                    .overlay
                    .as_ref()
                    .map(super::settings_overlay::SettingsOverlayView::active_section)
            })
            .unwrap_or(SettingsSection::Model);

        if let Some(section) = section {
            if self.open_settings_section_in_bottom_pane(section) {
                self.settings.bottom_route = Some(Some(section));
                return;
            }
            // Some sections only exist in the overlay; fall back.
            self.show_settings_overlay_view(Some(section));
            return;
        }

        let rows = self
            .build_settings_overview_rows()
            .into_iter()
            .map(|row| (row.section, row.summary))
            .collect();
        let view = SettingsOverviewView::new(rows, initial_section, self.app_event_tx.clone());
        self.open_bottom_pane_settings(move |this| this.bottom_pane.show_settings_overview(view));
        self.settings.bottom_route = Some(None);
    }

    pub(crate) fn show_settings_overlay(&mut self, section: Option<SettingsSection>) {
        if self.should_open_settings_overlay() {
            self.show_settings_overlay_view(section);
        } else {
            self.show_settings_bottom_pane(section);
        }
    }

    pub(crate) fn sync_settings_route_for_width(&mut self, width: u16) {
        use code_core::config_types::SettingsMenuOpenMode;

        if self.config.tui.settings_menu.open_mode != SettingsMenuOpenMode::Auto {
            return;
        }

        // If a prior bottom-pane settings view was closed, drop stale routing state.
        if self.settings.bottom_route.is_some() && !self.bottom_pane.has_active_view() {
            self.settings.bottom_route = None;
        }

        let prefer_overlay = width >= self.config.tui.settings_menu.overlay_min_width;

        if let Some(overlay) = self.settings.overlay.as_ref() {
            if !prefer_overlay {
                let target_section = if overlay.is_menu_active() {
                    None
                } else {
                    Some(overlay.active_section())
                };
                if let Some(section) = target_section
                    && !Self::section_supported_in_bottom_pane(section)
                {
                    return;
                }
                self.show_settings_bottom_pane(target_section);
            }
            return;
        }

        if let Some(route) = self.settings.bottom_route
            && prefer_overlay
        {
            self.bottom_pane.clear_active_view();
            self.show_settings_overlay_view(route);
        }
    }

    pub(crate) fn ensure_settings_overlay_section(&mut self, section: SettingsSection) {
        match self.settings.overlay.as_mut() {
            Some(overlay) => {
                let was_menu = overlay.is_menu_active();
                let changed_section = overlay.active_section() != section;
                overlay.set_mode_section(section);
                let focus_changed = overlay.set_focus_content();
                if was_menu || changed_section || focus_changed {
                    self.request_redraw();
                }
            }
            None => {
                self.show_settings_overlay(Some(section));
            }
        }
    }

}
