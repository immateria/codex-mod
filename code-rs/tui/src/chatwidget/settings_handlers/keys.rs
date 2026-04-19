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
            KeyCode::Esc | KeyCode::Char('?') => {
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
                    .map_or(crate::bottom_pane::SettingsSection::Model, super::settings_overlay::SettingsOverlayView::active_section);
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
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    changed = overlay.select_previous();
                }
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
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

    // Ctrl+B toggles the sidebar in section view (like VS Code / many editors).
    if key_event.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key_event.code, KeyCode::Char('b'))
        && let Some(overlay) = chat.settings.overlay.as_mut()
        && !overlay.is_menu_active()
    {
        overlay.toggle_sidebar_collapsed();
        chat.request_redraw();
        return true;
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
        // BackTab: let the active content handle it first (for multi-field forms).
        // If not handled, treat it as a focus jump back to the sidebar.
        KeyCode::BackTab if content_focused => {
            let handled_by_content = chat
                .settings
                .overlay
                .as_mut()
                .and_then(|overlay| overlay.active_content_mut())
                .is_some_and(|content| content.handle_key(key_event));
            if handled_by_content {
                chat.request_redraw();
                return true;
            }

            if let Some(overlay) = chat.settings.overlay.as_mut() {
                if overlay.is_sidebar_collapsed() {
                    overlay.toggle_sidebar_collapsed();
                }
                overlay.set_focus_sidebar();
            }
            chat.request_redraw();
            return true;
        }
        // Esc in content pane: if the content has internal back-navigation
        // (e.g. a detail sub-view), let it handle Esc first. Otherwise
        // go back to the overview (matching the footer hint + help copy).
        KeyCode::Esc if content_focused => {
            let has_back = chat
                .settings
                .overlay
                .as_ref()
                .and_then(|o| o.active_content())
                .is_some_and(super::settings_overlay::SettingsContent::has_back_navigation);
            if !has_back {
                if let Some(overlay) = chat.settings.overlay.as_mut() {
                    overlay.set_mode_menu(None);
                }
                chat.request_redraw();
                return true;
            }
            // has_back_navigation is true — fall through so the content
            // handler at line ~260 can process the Esc internally.
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
        .map_or(crate::bottom_pane::SettingsSection::Model, super::settings_overlay::SettingsOverlayView::active_section);
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

#[cfg(test)]
mod tests {
    use crate::bottom_pane::SettingsSection;
    use crate::chatwidget::smoke_helpers::ChatWidgetHarness;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn esc_returns_to_overview_then_closes() {
        crate::icons::with_test_icon_mode(code_core::config_types::IconMode::Unicode, || {
            let mut harness = ChatWidgetHarness::new();
            harness.with_chat(|chat| {
                chat.ensure_settings_overlay_section(SettingsSection::Interface);
                let overlay = chat.settings.overlay.as_ref().expect("overlay");
                assert!(!overlay.is_menu_active());
                assert!(overlay.is_content_focused());
            });

            harness.send_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            harness.with_chat(|chat| {
                let overlay = chat.settings.overlay.as_ref().expect("overlay after esc");
                assert!(overlay.is_menu_active());
            });

            harness.send_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            harness.with_chat(|chat| {
                assert!(chat.settings.overlay.is_none());
            });
        });
    }

    #[test]
    fn interface_icon_mode_preview_reverts_when_returning_to_overview() {
        crate::icons::with_test_icon_mode(code_core::config_types::IconMode::Unicode, || {
            let mut harness = ChatWidgetHarness::new();
            harness.with_chat(|chat| {
                chat.ensure_settings_overlay_section(SettingsSection::Interface);
            });

            harness.with_chat(|chat| {
                let overlay = chat.settings.overlay.as_mut().expect("overlay");
                let content = overlay.active_content_mut().expect("content");
                // Cycle icon mode via the page itself:
                // move selection to the icon row, then Right to cycle.
                assert!(content.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
                assert!(content.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
                assert!(content.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)));
                assert_eq!(crate::icons::icon_mode(), code_core::config_types::IconMode::NerdFonts);
            });

            // Esc returns to overview and should revert the preview.
            harness.send_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            assert_eq!(crate::icons::icon_mode(), code_core::config_types::IconMode::Unicode);
        });
    }

    #[test]
    fn prompts_new_prompt_accepts_typing() {
        use crate::test_helpers::render_chat_widget_to_vt100;

        let mut harness = ChatWidgetHarness::new();
        harness.with_chat(|chat| {
            chat.ensure_settings_overlay_section(SettingsSection::Prompts);
            let overlay = chat.settings.overlay.as_ref().expect("overlay");
            assert!(!overlay.is_menu_active());
            assert!(overlay.active_content().is_some(), "expected prompts content to exist");
        });

        // Enter the editor ("Add new..." when empty).
        harness.send_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        harness.with_chat(|chat| {
            let overlay = chat.settings.overlay.as_ref().expect("overlay");
            let content = overlay.active_content().expect("content");
            assert!(
                content.has_back_navigation(),
                "expected prompts page to enter edit mode"
            );
        });

        let slug = "my-test-prompt";
        for ch in slug.chars() {
            harness.send_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }

        let screen = render_chat_widget_to_vt100(&mut harness, 100, 28);
        assert!(screen.contains(slug), "expected slug to be visible; got:\n{screen}");
    }

    #[test]
    fn prompts_editor_backtab_cycles_focus_in_content() {
        use crate::test_helpers::render_chat_widget_to_vt100;

        let mut harness = ChatWidgetHarness::new();
        harness.with_chat(|chat| {
            chat.ensure_settings_overlay_section(SettingsSection::Prompts);
        });

        harness.send_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let screen = render_chat_widget_to_vt100(&mut harness, 100, 28);
        assert!(
            screen.contains("Name (slug) • Enter to save"),
            "expected name field to be focused; got:\n{screen}"
        );

        harness.send_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let screen = render_chat_widget_to_vt100(&mut harness, 100, 28);
        assert!(
            screen.contains("Content (multiline)"),
            "expected body field to be focused; got:\n{screen}"
        );

        harness.send_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        harness.with_chat(|chat| {
            let overlay = chat.settings.overlay.as_ref().expect("overlay");
            assert!(overlay.is_content_focused(), "expected content to keep focus");
        });

        let screen = render_chat_widget_to_vt100(&mut harness, 100, 28);
        assert!(
            screen.contains("Name (slug) • Enter to save"),
            "expected name field to be focused after BackTab; got:\n{screen}"
        );
    }
}
