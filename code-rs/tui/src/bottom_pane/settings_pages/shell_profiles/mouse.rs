use super::*;

pub(super) fn handle_mouse_event_direct_in_chrome(
    view: &mut ShellProfilesSettingsView,
    chrome: ChromeMode,
    mouse_event: MouseEvent,
    area: Rect,
) -> bool {
    if view.is_complete {
        return false;
    }

    let mut mode_guard = ModeGuard::replace(&mut view.mode, ViewMode::Main, |mode| {
        matches!(mode, ViewMode::Main)
    });
    match mode_guard.mode_mut() {
        ViewMode::Main => match chrome {
            ChromeMode::Framed => view.handle_mouse_event_main(mouse_event, area),
            ChromeMode::ContentOnly => view.handle_mouse_event_main_content(mouse_event, area),
        },
        ViewMode::EditList { target, before } => {
            let layout = view.compute_editor_layout_in_chrome(area, *target, chrome);
            let Some(layout) = layout else {
                return false;
            };

            let handled = match mouse_event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = view.editor_footer_action_at(
                        *target,
                        mouse_event.column,
                        mouse_event.row,
                        &layout,
                    ) {
                        match action {
                            EditorFooterAction::Save => {
                                view.stage_pending_profile_from_fields();
                                view.dirty = true;
                                view.status = Some(
                                    "Changes staged. Select Apply to persist.".to_string(),
                                );
                                mode_guard.disarm();
                                return true;
                            }
                            EditorFooterAction::Generate => {
                                view.request_summary_generation();
                                return true;
                            }
                            EditorFooterAction::Pick => {
                                view.editor_append_picker_path(*target);
                                return true;
                            }
                            EditorFooterAction::Show => {
                                view.editor_show_last_path(*target);
                                return true;
                            }
                            EditorFooterAction::Cancel => {
                                match *target {
                                    ListTarget::Summary => {
                                        view.summary_field.set_text(before.as_str());
                                    }
                                    ListTarget::References => {
                                        view.references_field.set_text(before.as_str());
                                    }
                                    ListTarget::SkillRoots => {
                                        view.skill_roots_field.set_text(before.as_str());
                                    }
                                }
                                mode_guard.disarm();
                                return true;
                            }
                        }
                    }

                    if layout.page.body.contains(ratatui::layout::Position {
                        x: mouse_event.column,
                        y: mouse_event.row,
                    }) {
                        view.editor_field_mut(*target).handle_mouse_click(
                            mouse_event.column,
                            mouse_event.row,
                            layout.sections[0].inner,
                        )
                    } else {
                        false
                    }
                }
                MouseEventKind::ScrollDown => {
                    if layout.page.body.contains(ratatui::layout::Position {
                        x: mouse_event.column,
                        y: mouse_event.row,
                    }) {
                        view.editor_field_mut(*target).handle_mouse_scroll(true)
                    } else {
                        false
                    }
                }
                MouseEventKind::ScrollUp => {
                    if layout.page.body.contains(ratatui::layout::Position {
                        x: mouse_event.column,
                        y: mouse_event.row,
                    }) {
                        view.editor_field_mut(*target).handle_mouse_scroll(false)
                    } else {
                        false
                    }
                }
                _ => false,
            };
            handled
        }
        ViewMode::PickList(state) => {
            let total = state.items.len();
            if total == 0 {
                return false;
            }

            let Some(layout) = view.compute_picker_layout_in_chrome(area, state, chrome) else {
                return false;
            };
            view.pick_viewport_rows.set((layout.body.height as usize).max(1));

            let outcome = route_scroll_state_mouse_in_body(
                mouse_event,
                layout.body,
                &mut state.scroll,
                total,
                SelectableListMouseConfig {
                    hover_select: false,
                    require_pointer_hit_for_scroll: true,
                    scroll_behavior: ScrollSelectionBehavior::Clamp,
                    ..SelectableListMouseConfig::default()
                },
            );

            let mut changed = outcome.changed;
            if matches!(outcome.result, SelectableListMouseResult::Activated) {
                changed |= ShellProfilesSettingsView::toggle_picker_selection(state);
            }

            changed
        }
    }
}

