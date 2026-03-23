impl ChatWidget<'_> {
    pub(in super::super) fn handle_click(&mut self, pos: (u16, u16)) {
        let (x, y) = pos;

        // Check clickable regions from last render and find matching action.
        let action_opt: Option<ClickableAction> = {
            let regions = self.clickable_regions.borrow();

            regions.iter().find_map(|region| {
                // Check if click is inside this region
                if x >= region.rect.x
                    && x < region.rect.x + region.rect.width
                    && y >= region.rect.y
                    && y < region.rect.y + region.rect.height
                {
                    Some(region.action.clone())
                } else {
                    None
                }
            })
        };

        // Execute the action after dropping the borrow
        if let Some(action) = action_opt {
            self.handle_clickable_action(action);
        }
    }

    pub(in crate::chatwidget) fn handle_clickable_action(&mut self, action: ClickableAction) {
        match action {
            ClickableAction::ShowModelSelector => {
                // Open model selector with empty args (opens selector UI)
                self.handle_model_command(String::new());
            }
            ClickableAction::ToggleServiceTier => {
                let service_tier = if matches!(
                    self.config.service_tier,
                    Some(code_core::config_types::ServiceTier::Fast)
                ) {
                    None
                } else {
                    Some(code_core::config_types::ServiceTier::Fast)
                };
                self.app_event_tx
                    .send(AppEvent::UpdateServiceTierSelection { service_tier });
            }
            ClickableAction::ShowShellSelector => {
                self.show_shell_selector();
            }
            ClickableAction::ShowReasoningSelector => {
                // Cycle through reasoning efforts
                use code_core::config_types::ReasoningEffort;
                let current = self.config.model_reasoning_effort;
                let next = match current {
                    ReasoningEffort::None => ReasoningEffort::Minimal,
                    ReasoningEffort::Minimal => ReasoningEffort::Low,
                    ReasoningEffort::Low => ReasoningEffort::Medium,
                    ReasoningEffort::Medium => ReasoningEffort::High,
                    ReasoningEffort::High => ReasoningEffort::XHigh,
                    ReasoningEffort::XHigh => ReasoningEffort::None,
                };
                self.set_reasoning_effort(next);
            }
            ClickableAction::ShowDirectoryPicker => {
                match crate::native_picker::pick_path(
                    crate::native_picker::NativePickerKind::Folder,
                    "Select working directory",
                ) {
                    Ok(Some(path)) => {
                        if path == self.config.cwd {
                            self.bottom_pane
                                .flash_footer_notice(format!("Already using {}", path.display()));
                        } else if !path.is_dir() {
                            self.bottom_pane.flash_footer_notice(format!(
                                "Selected path is not a directory: {}",
                                path.display()
                            ));
                        } else {
                            self.app_event_tx.send(AppEvent::SwitchCwd(path, None));
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        self.bottom_pane.flash_footer_notice(format!(
                            "Directory picker failed: {err}"
                        ));
                    }
                }
            }
            ClickableAction::ShowNetworkSettings => {
                self.ensure_settings_overlay_section(crate::bottom_pane::SettingsSection::Network);
            }
            ClickableAction::AcceptStartupModelMigration => {
                if let Some(notice) = self.startup_model_migration_notice.clone() {
                    self.app_event_tx
                        .send(AppEvent::AcceptStartupModelMigration(notice));
                }
            }
            ClickableAction::DismissStartupModelMigration => {
                if let Some(notice) = self.startup_model_migration_notice.clone() {
                    self.app_event_tx
                        .send(AppEvent::DismissStartupModelMigration(notice));
                }
            }
            ClickableAction::JumpToCallId(call_id) => {
                self.jump_to_call_id(&call_id);
            }
            ClickableAction::ToggleFoldAtIndex(idx) => {
                self.toggle_fold_at_index(idx);
            }
        }
    }

    pub(in crate::chatwidget) fn jump_to_call_id(&mut self, call_id: &str) {
        let Some(idx) = self
            .history_cells
            .iter()
            .rposition(|cell| cell.call_id() == Some(call_id))
        else {
            self.bottom_pane.update_status_text("parent tool call not found".to_string());
            self.request_redraw();
            return;
        };

        layout_scroll::jump_to_history_index(self, idx);
    }

    fn update_header_hover_state(&mut self, pos: (u16, u16)) -> bool {
        let (x, y) = pos;
        let hovered = {
            let regions = self.clickable_regions.borrow();
            regions.iter().find_map(|region| {
                if x >= region.rect.x
                    && x < region.rect.x + region.rect.width
                    && y >= region.rect.y
                    && y < region.rect.y + region.rect.height
                {
                    Some(region.action.clone())
                } else {
                    None
                }
            })
        };
        let mut current = self.hovered_clickable_action.borrow_mut();
        if *current == hovered {
            false
        } else {
            *current = hovered;
            true
        }
    }
}
