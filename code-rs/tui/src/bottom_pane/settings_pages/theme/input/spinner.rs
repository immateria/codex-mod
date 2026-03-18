use super::*;

impl ThemeSelectionView {
    pub(super) fn spinner_handle_enter(&mut self) {
        let names = crate::spinner::spinner_names();
        if self.selected_spinner_index > names.len() {
            self.selected_spinner_index = names.len().saturating_sub(1);
        }
        if self.selected_spinner_index >= names.len() {
            self.mode = Mode::CreateSpinner(Box::new(CreateState {
                step: std::cell::Cell::new(CreateStep::Prompt),
                prompt: String::new(),
                is_loading: std::cell::Cell::new(false),
                action_idx: 0,
                rx: None,
                thinking_lines: std::cell::RefCell::new(Vec::new()),
                thinking_current: std::cell::RefCell::new(String::new()),
                proposed_interval: std::cell::Cell::new(None),
                proposed_frames: std::cell::RefCell::new(None),
                proposed_name: std::cell::RefCell::new(None),
                last_raw_output: std::cell::RefCell::new(None),
            }));
        } else {
            self.confirm_spinner()
        }
    }

    pub(super) fn spinner_handle_mouse_hover(
        &mut self,
        rel_y: usize,
        body_area: Rect,
    ) -> bool {
        if rel_y < 2 {
            return false;
        }
        let names = crate::spinner::spinner_names();
        let count = names.len().saturating_add(1);
        if count == 0 {
            return false;
        }
        let visible = (body_area.height as usize).saturating_sub(2).clamp(1, 9);
        let (start, _, _) = crate::util::list_window::anchored_window(
            self.selected_spinner_index,
            count,
            visible,
        );
        let row = rel_y.saturating_sub(2);
        let next = start + row;
        if next >= count || self.selected_spinner_index == next {
            return false;
        }
        self.selected_spinner_index = next;
        if let Some(name) = names.get(next) {
            self.current_spinner = name.clone();
            self.app_event_tx
                .send(AppEvent::PreviewSpinner(self.current_spinner.clone()));
        }
        true
    }
}

