use super::*;

pub(super) fn handle_key_event_direct(view: &mut ShellProfilesSettingsView, key: KeyEvent) -> bool {
    if view.is_complete {
        return true;
    }

    let mut mode_guard = ModeGuard::replace(&mut view.mode, ViewMode::Main, |mode| {
        matches!(mode, ViewMode::Main)
    });
    match mode_guard.mode_mut() {
        ViewMode::Main => match key {
            KeyEvent { code: KeyCode::Esc, .. } => {
                view.is_complete = true;
                true
            }
            KeyEvent {
                code: KeyCode::Char('p'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                view.open_shell_selection();
                true
            }
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                view.status = None;
                let rows = ShellProfilesSettingsView::rows();
                let visible = view.viewport_rows.get().max(1);
                view.scroll.move_up_wrap_visible(rows.len(), visible);
                true
            }
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                view.status = None;
                let rows = ShellProfilesSettingsView::rows();
                let visible = view.viewport_rows.get().max(1);
                view.scroll.move_down_wrap_visible(rows.len(), visible);
                true
            }
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } if view.selected_row() == RowKind::Style => {
                view.cycle_style_next();
                true
            }
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } if view.selected_row() == RowKind::Style => {
                view.cycle_style_next();
                true
            }
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                view.status = None;
                view.activate_selected_row();
                true
            }
            _ => false,
        },
        ViewMode::EditList { target, before } => match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                match *target {
                    ListTarget::Summary => view.summary_field.set_text(before.as_str()),
                    ListTarget::References => view.references_field.set_text(before.as_str()),
                    ListTarget::SkillRoots => view.skill_roots_field.set_text(before.as_str()),
                }
                mode_guard.disarm();
                true
            }
            (KeyCode::Char('p'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                view.open_shell_selection();
                mode_guard.disarm();
                true
            }
            (KeyCode::Char('s'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                view.stage_pending_profile_from_fields();
                view.dirty = true;
                view.status = Some("Changes staged. Select Apply to persist.".to_string());
                mode_guard.disarm();
                true
            }
            (KeyCode::Char('g'), mods)
                if mods.contains(KeyModifiers::CONTROL)
                    && matches!(*target, ListTarget::Summary) =>
            {
                view.request_summary_generation();
                true
            }
            (KeyCode::Char('o'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                if matches!(*target, ListTarget::References | ListTarget::SkillRoots) {
                    view.editor_append_picker_path(*target);
                    true
                } else {
                    false
                }
            }
            (KeyCode::Char('v'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                if matches!(*target, ListTarget::References | ListTarget::SkillRoots) {
                    view.editor_show_last_path(*target);
                    true
                } else {
                    false
                }
            }
            _ => view.editor_field_mut(*target).handle_key(key),
        },
        ViewMode::PickList(state) => match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                mode_guard.disarm();
                true
            }
            (KeyCode::Char('s'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                view.save_picker(state);
                mode_guard.disarm();
                true
            }
            (KeyCode::Up, KeyModifiers::NONE) => {
                let visible = view.pick_viewport_rows.get().max(1);
                state.scroll.move_up_wrap_visible(state.items.len(), visible);
                true
            }
            (KeyCode::Down, KeyModifiers::NONE) => {
                let visible = view.pick_viewport_rows.get().max(1);
                state.scroll.move_down_wrap_visible(state.items.len(), visible);
                true
            }
            (KeyCode::Char(' '), KeyModifiers::NONE) | (KeyCode::Enter, KeyModifiers::NONE) => {
                ShellProfilesSettingsView::toggle_picker_selection(state)
            }
            _ => false,
        },
    }
}

pub(super) fn handle_paste_direct(view: &mut ShellProfilesSettingsView, text: String) -> bool {
    if view.is_complete {
        return false;
    }

    let target = match &view.mode {
        ViewMode::Main => return false,
        ViewMode::EditList { target, .. } => *target,
        ViewMode::PickList(_) => return false,
    };
    view.editor_field_mut(target).handle_paste(text);
    true
}

