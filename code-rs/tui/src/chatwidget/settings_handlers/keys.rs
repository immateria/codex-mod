/// Handle key presses for the full-screen settings overlay. Returns true when the
/// key has been consumed (overlay stays modal while active).
pub(super) fn handle_settings_key(chat: &mut ChatWidget<'_>, key_event: KeyEvent) -> bool {
    if chat.settings.overlay.is_none() {
        return false;
    }

    if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return true;
    }

    if chat
        .settings
        .overlay
        .as_ref()
        .is_some_and(super::settings_overlay::SettingsOverlayView::is_help_visible)
    {
        match key_event.code {
            KeyCode::Esc => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    overlay.hide_help();
                }
                chat.request_redraw();
            }
            KeyCode::Char('?') => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    overlay.hide_help();
                }
                chat.request_redraw();
            }
            _ => {}
        }
        return true;
    }

    if matches!(key_event.code, KeyCode::Char('?')) {
        if let Some(overlay) = chat.settings.overlay.as_mut() {
            overlay.show_help(overlay.is_menu_active());
        }
        chat.request_redraw();
        return true;
    }

    if chat
        .settings
        .overlay
        .as_ref()
        .is_some_and(super::settings_overlay::SettingsOverlayView::is_menu_active)
    {
        let mut handled = true;
        let mut changed = false;

        match key_event.code {
            KeyCode::Enter => {
                let section = chat
                    .settings
                    .overlay
                    .as_ref()
                    .map(super::settings_overlay::SettingsOverlayView::active_section)
                    .unwrap_or(crate::bottom_pane::SettingsSection::Model);
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    overlay.set_mode_section(section);
                }
                if section == crate::bottom_pane::SettingsSection::Limits {
                    chat.show_limits_settings_ui();
                } else {
                    chat.request_redraw();
                }
                return true;
            }
            KeyCode::Esc => {
                chat.close_settings_overlay();
                return true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    changed = overlay.select_previous();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    changed = overlay.select_next();
                }
            }
            KeyCode::Tab => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    changed = overlay.select_next();
                }
            }
            KeyCode::BackTab => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    changed = overlay.select_previous();
                }
            }
            KeyCode::Home => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    changed = overlay.set_section(crate::bottom_pane::SettingsSection::Model);
                }
            }
            KeyCode::End => {
                let last = crate::bottom_pane::SettingsSection::ALL
                    .last()
                    .copied()
                    .unwrap_or(crate::bottom_pane::SettingsSection::Model);
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    changed = overlay.set_section(last);
                }
            }
            _ => {
                handled = false;
            }
        }

        if changed {
            chat.request_redraw();
        }

        return handled;
    }

    // Fast toggle between the two shell-related pages without cycling through
    // the full sidebar. Use a control chord so text fields can still accept
    // normal character input.
    if key_event.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key_event.code, KeyCode::Char('p'))
    {
        let changed = if let Some(overlay) = chat.settings.overlay.as_mut() {
            match overlay.active_section() {
                crate::bottom_pane::SettingsSection::Shell => {
                    overlay.set_mode_section(crate::bottom_pane::SettingsSection::ShellProfiles);
                    true
                }
                crate::bottom_pane::SettingsSection::ShellProfiles => {
                    overlay.set_mode_section(crate::bottom_pane::SettingsSection::Shell);
                    true
                }
                _ => false,
            }
        } else {
            false
        };

        if changed {
            chat.request_redraw();
        }
        return changed;
    }

    let sidebar_focused = chat
        .settings
        .overlay
        .as_ref()
        .is_some_and(super::settings_overlay::SettingsOverlayView::is_sidebar_focused);
    let content_focused = chat
        .settings
        .overlay
        .as_ref()
        .is_some_and(super::settings_overlay::SettingsOverlayView::is_content_focused);

    // Two-pane focus model:
    // - Tab moves focus from the sidebar to the content pane.
    // - Shift+Tab moves focus from the content pane back to the sidebar.
    //
    // This avoids "getting stuck" when a section view captures navigation keys.
    match key_event.code {
        KeyCode::Tab if key_event.modifiers.is_empty() && sidebar_focused => {
            let changed = chat
                .settings
                .overlay
                .as_mut()
                .is_some_and(super::settings_overlay::SettingsOverlayView::set_focus_content);
            if changed {
                chat.request_redraw();
            }
            return true;
        }
        KeyCode::BackTab if content_focused => {
            let changed = chat
                .settings
                .overlay
                .as_mut()
                .is_some_and(super::settings_overlay::SettingsOverlayView::set_focus_sidebar);
            if changed {
                chat.request_redraw();
            }
            return true;
        }
        _ => {}
    }

    if sidebar_focused {
        let mut handled = true;
        let mut changed = false;

        match key_event.code {
            KeyCode::Esc if key_event.modifiers.is_empty() => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    overlay.set_mode_menu(None);
                }
                chat.request_redraw();
                return true;
            }
            KeyCode::Enter if key_event.modifiers.is_empty() => {
                let focus_changed = chat
                    .settings
                    .overlay
                    .as_mut()
                    .is_some_and(super::settings_overlay::SettingsOverlayView::set_focus_content);
                if chat.activate_current_settings_section() {
                    return true;
                }
                if focus_changed {
                    chat.request_redraw();
                }
                return true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    changed = overlay.select_previous();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    changed = overlay.select_next();
                }
            }
            KeyCode::Home => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    changed = overlay.set_section(crate::bottom_pane::SettingsSection::Model);
                }
            }
            KeyCode::End => {
                let last = crate::bottom_pane::SettingsSection::ALL
                    .last()
                    .copied()
                    .unwrap_or(crate::bottom_pane::SettingsSection::Model);
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    changed = overlay.set_section(last);
                }
            }
            _ => {
                handled = false;
            }
        }

        if changed {
            chat.request_redraw();
        }

        return handled;
    }

    // Give the active content first chance to handle keys (including Esc)
    let completed_section = chat
        .settings
        .overlay
        .as_ref()
        .map(super::settings_overlay::SettingsOverlayView::active_section)
        .unwrap_or(crate::bottom_pane::SettingsSection::Model);
    let (handled_by_content, did_complete) = {
        let Some(overlay) = chat.settings.overlay.as_mut() else {
            return true;
        };
        match overlay.active_content_mut() {
            Some(content) => {
                if content.handle_key(key_event) {
                    (true, content.is_complete())
                } else {
                    (false, false)
                }
            }
            None => (false, false),
        }
    };

    if handled_by_content {
        if did_complete {
            // Shell sections are frequently used together; keeping the overlay
            // open makes it easy to switch between shell selection and profiles.
            if matches!(
                completed_section,
                crate::bottom_pane::SettingsSection::Shell
                    | crate::bottom_pane::SettingsSection::ShellProfiles
            ) {
                let shell_content = (completed_section == crate::bottom_pane::SettingsSection::Shell)
                    .then(|| chat.build_shell_settings_content());
                let shell_profiles_content =
                    (completed_section == crate::bottom_pane::SettingsSection::ShellProfiles)
                        .then(|| chat.build_shell_profiles_settings_content());

                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    overlay.set_mode_menu(Some(completed_section));
                    if let Some(content) = shell_content {
                        overlay.set_shell_content(content);
                    }
                    if let Some(content) = shell_profiles_content {
                        overlay.set_shell_profiles_content(content);
                    }
                }

                chat.request_redraw();
                return true;
            }

            chat.close_settings_overlay();
            return true;
        }

        chat.request_redraw();
        return true;
    }

    match key_event.code {
        KeyCode::Esc if key_event.modifiers.is_empty() => {
            if let Some(overlay) = chat.settings.overlay.as_mut() {
                overlay.set_mode_menu(None);
            }
            chat.request_redraw();
            return true;
        }
        _ => {}
    }

    if matches!(key_event.code, KeyCode::Enter) && key_event.modifiers.is_empty() {
        if chat.activate_current_settings_section() {
            return true;
        }
        return true;
    }

    false
}
