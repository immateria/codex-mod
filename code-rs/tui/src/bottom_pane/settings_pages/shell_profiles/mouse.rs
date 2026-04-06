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

            match mouse_event.kind {
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
            }
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

            let visible_rows = (layout.body.height as usize).max(1);
            let kind = mouse_event.kind;
            let outcome = crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test(
                mouse_event,
                &mut state.scroll,
                total,
                visible_rows,
                |x, y, scroll_top| {
                    let body = layout.body;
                    if x < body.x || x >= body.x.saturating_add(body.width) {
                        return None;
                    }
                    if y < body.y || y >= body.y.saturating_add(body.height) {
                        return None;
                    }
                    let rel = y.saturating_sub(body.y) as usize;
                    let idx = scroll_top.saturating_add(rel);
                    if matches!(kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) {
                        return Some(idx.min(total.saturating_sub(1)));
                    }
                    if idx >= total {
                        return None;
                    }

                    let item = &state.items[idx];
                    let checked = state.checked.get(idx).copied().unwrap_or(false);
                    let check = if checked { "[x]" } else { "[ ]" };

                    let conflict_label = ShellProfilesSettingsView::picker_conflict_label(state.target);
                    let mut suffix = String::new();
                    let conflict_key = if item.is_no_filter_option {
                        String::new()
                    } else {
                        super::persistence::normalize_list_key(&item.name)
                    };
                    if !item.is_no_filter_option {
                        if item.is_unknown {
                            suffix.push_str(" (unknown)");
                        }
                        if state.other_values.contains(&conflict_key) {
                            suffix.push_str(" (");
                            suffix.push_str(conflict_label);
                            suffix.push(')');
                        }
                    }

                    let line = Line::from(vec![
                        Span::raw(format!("  {check} ")),
                        Span::raw(format!("{}{}", item.name, suffix)),
                    ]);
                    crate::bottom_pane::settings_ui::hit_test::line_has_non_whitespace_at(
                        &line,
                        body.x,
                        body.width,
                        x,
                    )
                    .then_some(idx)
                },
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
