use super::*;

impl ThemeSelectionView {
    pub(super) fn create_theme_nav_prev(s: &mut CreateThemeState) {
        match s.step.get() {
            CreateStep::Prompt => s.step.set(CreateStep::Action),
            CreateStep::Action => {
                if s.action_idx > 0 {
                    s.action_idx -= 1;
                }
            }
            CreateStep::Review => {
                s.review_focus_is_toggle.set(true);
            }
        }
    }

    pub(super) fn create_theme_nav_next(s: &mut CreateThemeState) {
        match s.step.get() {
            CreateStep::Prompt => s.step.set(CreateStep::Action),
            CreateStep::Action => {
                if s.action_idx < 1 {
                    s.action_idx += 1;
                }
            }
            CreateStep::Review => {
                s.review_focus_is_toggle.set(false);
            }
        }
    }

    pub(super) fn create_theme_handle_enter(&mut self, mut s: Box<CreateThemeState>) {
        let mut go_overview = false;
        match s.step.get() {
            CreateStep::Prompt => {
                if !s.is_loading.get() {
                    let user_prompt = s.prompt.clone();
                    s.is_loading.set(true);
                    s.thinking_lines.borrow_mut().clear();
                    s.thinking_current.borrow_mut().clear();
                    s.proposed_name.replace(None);
                    s.proposed_colors.replace(None);
                    let (txp, rxp) = std::sync::mpsc::channel::<ProgressMsg>();
                    s.rx = Some(rxp);
                    self.kickoff_theme_creation(user_prompt, txp);
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                }
            }
            CreateStep::Action => {
                if s.action_idx == 0 && !s.is_loading.get() {
                    let user_prompt = s.prompt.clone();
                    s.is_loading.set(true);
                    s.thinking_lines.borrow_mut().clear();
                    s.thinking_current.borrow_mut().clear();
                    s.proposed_name.replace(None);
                    s.proposed_colors.replace(None);
                    let (txp, rxp) = std::sync::mpsc::channel::<ProgressMsg>();
                    s.rx = Some(rxp);
                    self.kickoff_theme_creation(user_prompt, txp);
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                } else {
                    go_overview = true;
                }
            }
            CreateStep::Review => {
                if s.review_focus_is_toggle.get() {
                    let now_on = !s.preview_on.get();
                    s.preview_on.set(now_on);
                    if now_on {
                        if let (Some(name), Some(colors)) = (
                            s.proposed_name.borrow().clone(),
                            s.proposed_colors.borrow().clone(),
                        ) {
                            crate::theme::set_custom_theme_colors(colors.clone());
                            crate::theme::set_custom_theme_label(name.clone());
                            crate::theme::init_theme(
                                &code_core::config_types::ThemeConfig {
                                    name: ThemeName::Custom,
                                    colors,
                                    label: Some(name),
                                    is_dark: s.proposed_is_dark.get(),
                                },
                            );
                        }
                    } else {
                        let fallback =
                            if self.revert_theme_on_back == ThemeName::Custom {
                                ThemeName::LightPhoton
                            } else {
                                self.revert_theme_on_back
                            };
                        self.app_event_tx.send(AppEvent::PreviewTheme(fallback));
                    }
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                } else if s.action_idx == 0 {
                    if let (Some(name), Some(colors)) = (
                        s.proposed_name.borrow().clone(),
                        s.proposed_colors.borrow().clone(),
                    ) {
                        if let Ok(home) = code_core::config::find_code_home() {
                            let _ = code_core::config::set_custom_theme(
                                &home,
                                &name,
                                &colors,
                                s.preview_on.get(),
                                s.proposed_is_dark.get(),
                            );
                        }
                        crate::theme::set_custom_theme_label(name.clone());
                        crate::theme::set_custom_theme_colors(colors.clone());
                        crate::theme::set_custom_theme_is_dark(s.proposed_is_dark.get());
                        if s.preview_on.get() {
                        crate::theme::init_theme(
                            &code_core::config_types::ThemeConfig {
                                name: ThemeName::Custom,
                                colors,
                                label: Some(name.clone()),
                                is_dark: s.proposed_is_dark.get(),
                            },
                        );
                            self.revert_theme_on_back = ThemeName::Custom;
                            self.current_theme = ThemeName::Custom;
                            self.app_event_tx
                                .send(AppEvent::UpdateTheme(ThemeName::Custom));
                        } else {
                            self.app_event_tx
                                .send(AppEvent::PreviewTheme(self.revert_theme_on_back));
                        }
                        if s.preview_on.get() {
                            self.send_before_next_output(format!("Set theme to {name}"));
                        } else {
                            self.send_before_next_output(format!(
                                "Saved custom theme {name} (not active)"
                            ));
                        }
                        go_overview = true;
                    }
                } else {
                    s.thinking_lines.borrow_mut().clear();
                    s.thinking_current.borrow_mut().clear();
                    s.proposed_name.replace(None);
                    s.proposed_colors.replace(None);
                    s.step.set(CreateStep::Prompt);
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                    self.app_event_tx
                        .send(AppEvent::PreviewTheme(self.revert_theme_on_back));
                }
            }
        }
        if go_overview {
            self.mode = Mode::Overview;
        } else {
            self.mode = Mode::CreateTheme(s);
        }
    }

    pub(super) fn create_theme_handle_char(
        s: &mut CreateThemeState,
        c: char,
        modifiers: KeyModifiers,
    ) {
        if s.is_loading.get() {
            return;
        }
        if matches!(modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT) {
            match s.step.get() {
                CreateStep::Prompt => s.prompt.push(c),
                CreateStep::Action | CreateStep::Review => {}
            }
        }
    }

    pub(super) fn create_theme_handle_backspace(s: &mut CreateThemeState) {
        if s.is_loading.get() {
            return;
        }
        match s.step.get() {
            CreateStep::Prompt => {
                if let Some((idx, _)) = s.prompt.grapheme_indices(true).next_back()
                {
                    s.prompt.truncate(idx);
                } else {
                    s.prompt.clear();
                }
            }
            CreateStep::Action | CreateStep::Review => {}
        }
    }
}

