use super::*;

impl ThemeSelectionView {
    pub(super) fn create_spinner_nav_prev(s: &mut CreateState) {
        let new_step = match s.step.get() {
            CreateStep::Prompt => CreateStep::Action,
            CreateStep::Action => {
                if s.action_idx > 0 {
                    s.action_idx -= 1;
                }
                CreateStep::Action
            }
            CreateStep::Review => {
                if s.action_idx > 0 {
                    s.action_idx -= 1;
                }
                CreateStep::Review
            }
        };
        s.step.set(new_step);
    }

    pub(super) fn create_spinner_nav_next(s: &mut CreateState) {
        let new_step = match s.step.get() {
            CreateStep::Prompt => CreateStep::Action,
            CreateStep::Action => {
                if s.action_idx < 1 {
                    s.action_idx += 1;
                }
                CreateStep::Action
            }
            CreateStep::Review => {
                if s.action_idx < 1 {
                    s.action_idx += 1;
                }
                CreateStep::Review
            }
        };
        s.step.set(new_step);
    }

    pub(super) fn create_spinner_handle_enter(&mut self, mut s: Box<CreateState>) {
        let mut go_overview = false;
        match s.step.get() {
            CreateStep::Prompt => {
                if !s.is_loading.get() {
                    let user_prompt = s.prompt.clone();
                    s.is_loading.set(true);
                    s.thinking_lines.borrow_mut().clear();
                    s.thinking_current.borrow_mut().clear();
                    s.proposed_interval.set(None);
                    s.proposed_frames.replace(None);
                    let (txp, rxp) = std::sync::mpsc::channel::<ProgressMsg>();
                    s.rx = Some(rxp);
                    self.kickoff_spinner_creation(user_prompt, txp);
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                }
            }
            CreateStep::Action => {
                if s.action_idx == 0 && !s.is_loading.get() {
                    let user_prompt = s.prompt.clone();
                    s.is_loading.set(true);
                    s.thinking_lines.borrow_mut().clear();
                    s.thinking_current.borrow_mut().clear();
                    s.proposed_interval.set(None);
                    s.proposed_frames.replace(None);
                    let (txp, rxp) = std::sync::mpsc::channel::<ProgressMsg>();
                    s.rx = Some(rxp);
                    self.kickoff_spinner_creation(user_prompt, txp);
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                } else {
                    go_overview = true;
                }
            }
            CreateStep::Review => {
                if s.action_idx == 0 {
                    if let (Some(interval), Some(frames)) = (
                        s.proposed_interval.get(),
                        s.proposed_frames.borrow().clone(),
                    ) {
                        let display_name = s
                            .proposed_name
                            .borrow()
                            .as_ref()
                            .cloned()
                            .unwrap_or_else(|| "Custom".to_string());
                        if let Ok(home) = code_core::config::find_code_home() {
                            let _ = code_core::config::set_custom_spinner(
                                &home,
                                "custom",
                                &display_name,
                                interval,
                                &frames,
                            );
                        }
                        crate::spinner::add_custom_spinner(
                            "custom".to_string(),
                            display_name,
                            interval,
                            frames,
                        );
                        crate::spinner::switch_spinner("custom");
                        self.revert_spinner_on_back = "custom".to_string();
                        self.current_spinner = "custom".to_string();
                        self.app_event_tx
                            .send(AppEvent::UpdateSpinner("custom".to_string()));
                        self.send_tail("Custom spinner saved".to_string());
                        go_overview = true;
                    }
                } else {
                    s.thinking_lines.borrow_mut().clear();
                    s.thinking_current.borrow_mut().clear();
                    s.proposed_interval.set(None);
                    s.proposed_frames.replace(None);
                    s.step.set(CreateStep::Prompt);
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                }
            }
        }
        if go_overview {
            self.mode = Mode::Overview;
        } else {
            self.mode = Mode::CreateSpinner(s);
        }
    }

    pub(super) fn create_spinner_handle_char(
        s: &mut CreateState,
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

    pub(super) fn create_spinner_handle_backspace(s: &mut CreateState) {
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

