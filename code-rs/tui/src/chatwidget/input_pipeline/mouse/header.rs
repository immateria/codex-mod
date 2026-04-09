impl ChatWidget<'_> {
    pub(in super::super) fn handle_click(&mut self, pos: (u16, u16)) {
        let (x, y) = pos;

        // Check clickable regions from last render and find matching action.
        // Iterate in reverse so later-registered (more specific) regions take
        // priority over earlier (broader) ones when they overlap.
        let action_opt: Option<ClickableAction> = {
            let regions = self.clickable_regions.borrow();

            regions.iter().rev().find_map(|region| {
                region.contains(x, y).then(|| region.action.clone())
            })
        };

        // Execute the action after dropping the borrow
        if let Some(action) = action_opt {
            self.handle_clickable_action(action);
        }
    }

    pub(in crate::chatwidget) fn handle_clickable_action(&mut self, action: ClickableAction) {
        match action {
            ClickableAction::OpenSettings => {
                self.show_settings_overlay(None);
            }
            ClickableAction::ShowModelSelector => {
                // Open model selector with empty args (opens selector UI)
                self.handle_model_command(String::new());
            }
            ClickableAction::ToggleServiceTier => {
                if !code_core::model_family::supports_service_tier(&self.config.model) {
                    return;
                }
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
                if !crate::platform_caps::supports_native_picker() {
                    self.bottom_pane.flash_footer_notice(
                        "Directory picker is not supported on this platform; type the path."
                            .to_string(),
                    );
                    return;
                }
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
            #[cfg(feature = "managed-network-proxy")]
            ClickableAction::ShowNetworkSettings => {
                self.ensure_settings_overlay_section(crate::bottom_pane::SettingsSection::Network);
            }
            ClickableAction::ShowMcpSettings => {
                self.show_settings_overlay(Some(crate::bottom_pane::SettingsSection::Mcp));
            }
            ClickableAction::JumpToCallId(call_id) => {
                self.jump_to_call_id(&call_id);
            }
            ClickableAction::ToggleFoldAtIndex(idx) => {
                self.toggle_fold_at_index(idx);
            }
            ClickableAction::DismissHistoryCellAtIndex(idx) => {
                if idx >= self.history_cells.len() {
                    return;
                }
                if !matches!(
                    self.history_cells[idx].kind(),
                    crate::history_cell::HistoryCellType::BackgroundEvent
                ) {
                    return;
                }
                self.history_remove_at(idx);
            }
            ClickableAction::CopyMarkdownAtIndex(idx) => {
                if let Some(cell) = self.history_cells.get(idx) {
                    if let Some(md) = cell.copyable_markdown() {
                        crate::clipboard_copy::copy_to_clipboard_osc52(&md);
                        self.bottom_pane.flash_footer_notice("Copied to clipboard");
                    }
                }
            }
            ClickableAction::ScrollToTopOfCell(idx) => {
                layout_scroll::jump_to_history_index(self, idx);
            }
        }
    }

    pub(in crate::chatwidget) fn jump_to_call_id(&mut self, call_id: &str) {
        let Some(idx) = self
            .history_cells
            .iter()
            .rposition(|cell| cell.call_id() == Some(call_id))
        else {
            self.bottom_pane.update_status_text("parent tool call not found");
            self.request_redraw();
            return;
        };

        layout_scroll::jump_to_history_index(self, idx);
    }

    fn update_header_hover_state(&mut self, pos: (u16, u16)) -> bool {
        self.last_mouse_pos.set(Some(pos));
        let (x, y) = pos;
        let hovered = {
            let regions = self.clickable_regions.borrow();
            regions.iter().find_map(|region| {
                region.contains(x, y).then(|| region.action.clone())
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
