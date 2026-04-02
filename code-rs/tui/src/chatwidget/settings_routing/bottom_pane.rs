impl ChatWidget<'_> {
    fn open_model_settings_section(&mut self) -> bool {
        let presets = self.available_model_presets();
        let current_model = self.config.model.clone();
        let current_effort = self.config.model_reasoning_effort;
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_model_selection(ModelSelectionViewParams {
                presets,
                current_model,
                current_effort,
                current_service_tier: this.config.service_tier,
                current_context_mode: this.config.context_mode,
                current_context_window: this.config.model_context_window,
                current_auto_compact_token_limit: this.config.model_auto_compact_token_limit,
                use_chat_model: false,
                target: ModelSelectionTarget::Session,
            });
        })
    }

    fn open_theme_settings_section(&mut self) -> bool {
        self.open_bottom_pane_settings(Self::show_theme_selection)
    }

    fn open_updates_settings_section(&mut self) -> bool {
        let Some(view) = self.prepare_update_settings_view() else {
            return false;
        };
        self.open_bottom_pane_settings(move |this| this.bottom_pane.show_update_settings(view))
    }

    fn open_accounts_settings_section(&mut self) -> bool {
        let view = crate::bottom_pane::settings_pages::accounts::AccountSwitchSettingsView::new(
            self.app_event_tx.clone(),
            self.config.auto_switch_accounts_on_rate_limit,
            self.config.api_key_fallback_on_all_accounts_limited,
            self.config.cli_auth_credentials_store_mode,
        );
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_account_switch_settings(view);
        })
    }

    fn open_memories_settings_section(&mut self) -> bool {
        let view = self.build_memories_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_memories_settings(view);
        })
    }

    fn open_prompts_settings_section(&mut self) -> bool {
        let view = self.build_prompts_settings_view();
        self.open_bottom_pane_settings(move |this| this.bottom_pane.show_prompts_settings(view))
    }

    fn open_skills_settings_section(&mut self) -> bool {
        let view = self.build_skills_settings_view();
        self.open_bottom_pane_settings(move |this| this.bottom_pane.show_skills_settings(view))
    }

    fn open_plugins_settings_section(&mut self) -> bool {
        let view = self.build_plugins_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_plugins_settings(view);
        })
    }

    fn open_secrets_settings_section(&mut self) -> bool {
        let view = self.build_secrets_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_secrets_settings(view);
        })
    }

    fn open_apps_settings_section(&mut self) -> bool {
        let view = self.build_apps_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_apps_settings(view);
        })
    }

    fn open_auto_drive_settings_section(&mut self) -> bool {
        let view = self.build_auto_drive_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_auto_drive_settings_panel(view);
        })
    }

    fn open_review_settings_section(&mut self) -> bool {
        let view = self.build_review_settings_view();
        self.open_bottom_pane_settings(move |this| this.bottom_pane.show_review_settings(view))
    }

    fn open_planning_settings_section(&mut self) -> bool {
        let view = self.build_planning_settings_view();
        self.open_bottom_pane_settings(move |this| this.bottom_pane.show_planning_settings(view))
    }

    fn open_validation_settings_section(&mut self) -> bool {
        let view = self.build_validation_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_validation_settings(view);
        })
    }

    fn open_notifications_settings_section(&mut self) -> bool {
        let view = self.build_notifications_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_notifications_settings(view);
        })
    }

    #[cfg(feature = "managed-network-proxy")]
    fn open_network_settings_section(&mut self) -> bool {
        let view = self.build_network_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_network_settings(view);
        })
    }

    fn open_exec_limits_settings_section(&mut self) -> bool {
        let view = self.build_exec_limits_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_exec_limits_settings(view);
        })
    }

    fn open_js_repl_settings_section(&mut self) -> bool {
        let view = self.build_js_repl_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_js_repl_settings(view);
        })
    }

    fn open_shell_settings_section(&mut self) -> bool {
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane
                .show_shell_selection(this.config.shell.clone(), this.available_shell_presets());
        })
    }

    fn open_shell_escalation_settings_section(&mut self) -> bool {
        let view = self.build_shell_escalation_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_shell_escalation_settings(view);
        })
    }

    fn open_shell_profiles_settings_section(&mut self) -> bool {
        let skills = self
            .bottom_pane
            .skills()
            .iter()
            .map(|skill| (skill.name.clone(), skill.description.clone()))
            .collect::<Vec<_>>();
        let mcp_servers = self
            .config
            .mcp_servers
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let view = ShellProfilesSettingsView::new(
            self.config.code_home.clone(),
            self.config.shell.as_ref(),
            self.config.shell_style_profiles.clone(),
            skills,
            mcp_servers,
            self.app_event_tx.clone(),
        );
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_shell_profiles_settings(view);
        })
    }

    fn open_interface_settings_section(&mut self) -> bool {
        let view = self.build_interface_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_interface_settings(view);
        })
    }

    fn open_experimental_features_settings_section(&mut self) -> bool {
        let view = self.build_experimental_features_settings_view();
        self.open_bottom_pane_settings(move |this| {
            this.bottom_pane.show_experimental_features_settings(view);
        })
    }

    fn open_mcp_settings_section(&mut self) -> bool {
        let Some(rows) = self.build_mcp_server_rows() else {
            return false;
        };
        self.open_bottom_pane_settings(move |this| this.bottom_pane.show_mcp_settings(rows))
    }

    pub(crate) fn open_settings_section_in_bottom_pane(
        &mut self,
        section: SettingsSection,
    ) -> bool {
        let opened = match section {
            SettingsSection::Model                              => self.open_model_settings_section(),
            SettingsSection::Theme                              => self.open_theme_settings_section(),
            SettingsSection::Interface                          => self.open_interface_settings_section(),
            SettingsSection::Experimental                       => self.open_experimental_features_settings_section(),
            SettingsSection::Shell                              => self.open_shell_settings_section(),
            SettingsSection::ShellEscalation                    => self.open_shell_escalation_settings_section(),
            SettingsSection::ShellProfiles                       => self.open_shell_profiles_settings_section(),
            SettingsSection::ExecLimits                         => self.open_exec_limits_settings_section(),
            SettingsSection::Updates                            => self.open_updates_settings_section(),
            SettingsSection::Accounts                           => self.open_accounts_settings_section(),
            SettingsSection::Secrets                            => self.open_secrets_settings_section(),
            SettingsSection::Apps                               => self.open_apps_settings_section(),
            SettingsSection::Memories                           => self.open_memories_settings_section(),
            SettingsSection::Prompts                            => self.open_prompts_settings_section(),
            SettingsSection::Skills                             => self.open_skills_settings_section(),
            SettingsSection::Plugins                            => self.open_plugins_settings_section(),
            SettingsSection::AutoDrive                          => self.open_auto_drive_settings_section(),
            SettingsSection::Review                             => self.open_review_settings_section(),
            SettingsSection::Planning                           => self.open_planning_settings_section(),
            SettingsSection::Validation                         => self.open_validation_settings_section(),
            SettingsSection::Notifications                      => self.open_notifications_settings_section(),
            SettingsSection::Mcp                                => self.open_mcp_settings_section(),
            SettingsSection::JsRepl                             => self.open_js_repl_settings_section(),
            #[cfg(feature = "managed-network-proxy")]
            SettingsSection::Network                            => self.open_network_settings_section(),

            SettingsSection::Agents | SettingsSection::Limits => false,
            #[cfg(feature = "browser-automation")]
            SettingsSection::Chrome => false,
        };

        if opened {
            self.settings.bottom_route = Some(Some(section));
        }
        opened
    }

    fn section_supported_in_bottom_pane(section: SettingsSection) -> bool {
        if matches!(section, SettingsSection::Agents | SettingsSection::Limits) {
            return false;
        }

        #[cfg(feature = "browser-automation")]
        if matches!(section, SettingsSection::Chrome) {
            return false;
        }

        true
    }

    pub(crate) fn activate_current_settings_section(&mut self) -> bool {
        let section = match self
            .settings
            .overlay
            .as_ref()
            .map(super::settings_overlay::SettingsOverlayView::active_section)
        {
            Some(section) => section,
            None => return false,
        };

        let handled = match section {
            SettingsSection::Agents => {
                self.show_agents_overview_ui();
                false
            }
            SettingsSection::Limits => {
                self.show_limits_settings_ui();
                false
            }
            #[cfg(feature = "browser-automation")]
            SettingsSection::Chrome => {
                self.show_chrome_options(None);
                true
            }
            SettingsSection::Model
            | SettingsSection::Theme
            | SettingsSection::Interface
            | SettingsSection::Experimental
            | SettingsSection::Shell
            | SettingsSection::ShellEscalation
            | SettingsSection::ShellProfiles
            | SettingsSection::ExecLimits
            | SettingsSection::Planning
            | SettingsSection::Updates
            | SettingsSection::Review
            | SettingsSection::Validation
            | SettingsSection::AutoDrive
            | SettingsSection::Mcp
            | SettingsSection::JsRepl
            | SettingsSection::Notifications
            | SettingsSection::Prompts
            | SettingsSection::Accounts
            | SettingsSection::Secrets
            | SettingsSection::Apps
            | SettingsSection::Memories
            | SettingsSection::Skills
            | SettingsSection::Plugins => false,
            #[cfg(feature = "managed-network-proxy")]
            SettingsSection::Network => false,
        };

        if handled {
            self.close_settings_overlay();
        }

        handled
    }

    pub(crate) fn settings_section_from_hint(section: &str) -> Option<SettingsSection> {
        SettingsSection::from_hint(section)
    }
}
