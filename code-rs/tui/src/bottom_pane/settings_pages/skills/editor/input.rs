use super::*;

pub(super) fn handle_key_event_direct_inner(view: &mut SkillsSettingsView, key: KeyEvent) -> bool {
    match view.mode {
        Mode::List => match key {
            KeyEvent { code: KeyCode::Esc, .. } => {
                view.complete = true;
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                view.enter_editor();
                true
            }
            KeyEvent {
                code: KeyCode::Char('n'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                view.start_new_skill();
                true
            }
            other => view.handle_list_key(other),
        },
        Mode::Edit => match key {
            KeyEvent { code: KeyCode::Esc, .. } => {
                cancel_edit(view);
                true
            }
            KeyEvent { code: KeyCode::Tab, .. } => {
                cycle_focus(view, true);
                true
            }
            KeyEvent {
                code: KeyCode::BackTab,
                ..
            } => {
                cycle_focus(view, false);
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } if matches!(
                view.editor.focus,
                Focus::StyleProfile | Focus::Generate | Focus::Save | Focus::Delete | Focus::Cancel
            ) =>
            {
                match view.editor.focus {
                    Focus::StyleProfile => style_profile::cycle_style_profile_mode(view, true),
                    Focus::Generate => view.generate_draft(),
                    Focus::Save => view.save_current(),
                    Focus::Delete => view.delete_current(),
                    Focus::Cancel => cancel_edit(view),
                    Focus::List
                    | Focus::Name
                    | Focus::Description
                    | Focus::Style
                    | Focus::StyleReferences
                    | Focus::StyleSkillRoots
                    | Focus::StyleMcpInclude
                    | Focus::StyleMcpExclude
                    | Focus::Examples
                    | Focus::Body => {}
                }
                true
            }
            KeyEvent {
                code: KeyCode::Char('n'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                view.start_new_skill();
                true
            }
            KeyEvent {
                code: KeyCode::Char('g'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                view.generate_draft();
                true
            }
            _ => match view.editor.focus {
                Focus::Name => {
                    view.editor.name_field.handle_key(key);
                    true
                }
                Focus::Description => {
                    view.editor.description_field.handle_key(key);
                    true
                }
                Focus::Style => {
                    let previous_style =
                        ShellScriptStyle::parse(view.editor.style_field.text().trim());
                    view.editor.style_field.handle_key(key);
                    style_profile::sync_style_profile_fields_if_needed(view, previous_style);
                    true
                }
                Focus::StyleProfile => match key.code {
                    KeyCode::Left => {
                        style_profile::cycle_style_profile_mode(view, false);
                        true
                    }
                    KeyCode::Right | KeyCode::Char(' ') => {
                        style_profile::cycle_style_profile_mode(view, true);
                        true
                    }
                    _ => false,
                },
                Focus::StyleReferences => {
                    let before = view.editor.style_references_field.text().to_string();
                    view.editor.style_references_field.handle_key(key);
                    if view.editor.style_references_field.text() != before {
                        view.editor.style_references_dirty = true;
                    }
                    true
                }
                Focus::StyleSkillRoots => {
                    let before = view.editor.style_skill_roots_field.text().to_string();
                    view.editor.style_skill_roots_field.handle_key(key);
                    if view.editor.style_skill_roots_field.text() != before {
                        view.editor.style_skill_roots_dirty = true;
                    }
                    true
                }
                Focus::StyleMcpInclude => {
                    let before = view.editor.style_mcp_include_field.text().to_string();
                    view.editor.style_mcp_include_field.handle_key(key);
                    if view.editor.style_mcp_include_field.text() != before {
                        view.editor.style_mcp_include_dirty = true;
                    }
                    true
                }
                Focus::StyleMcpExclude => {
                    let before = view.editor.style_mcp_exclude_field.text().to_string();
                    view.editor.style_mcp_exclude_field.handle_key(key);
                    if view.editor.style_mcp_exclude_field.text() != before {
                        view.editor.style_mcp_exclude_dirty = true;
                    }
                    true
                }
                Focus::Examples => {
                    view.editor.examples_field.handle_key(key);
                    true
                }
                Focus::Body => {
                    view.editor.body_field.handle_key(key);
                    true
                }
                Focus::Generate | Focus::Save | Focus::Delete | Focus::Cancel => false,
                Focus::List => view.handle_list_key(key),
            },
        },
    }
}

pub(super) fn cancel_edit(view: &mut SkillsSettingsView) {
    view.mode = Mode::List;
    view.editor.focus = Focus::List;
    view.editor.hovered_button = None;
    view.status = None;
    view.ensure_list_selection_visible();
}

fn cycle_focus(view: &mut SkillsSettingsView, forward: bool) {
    let order = [
        Focus::Name,
        Focus::Description,
        Focus::Style,
        Focus::StyleProfile,
        Focus::StyleReferences,
        Focus::StyleSkillRoots,
        Focus::StyleMcpInclude,
        Focus::StyleMcpExclude,
        Focus::Examples,
        Focus::Body,
        Focus::Generate,
        Focus::Save,
        Focus::Delete,
        Focus::Cancel,
    ];
    debug_assert!(
        view.editor.focus != Focus::List,
        "cycle_focus called with Focus::List while in edit mode"
    );
    let mut idx = order
        .iter()
        .position(|f| *f == view.editor.focus)
        .unwrap_or_else(|| if forward { 0 } else { order.len() - 1 });
    if forward {
        idx = (idx + 1) % order.len();
    } else {
        idx = idx.checked_sub(1).unwrap_or(order.len() - 1);
    }
    view.editor.focus = order[idx];
}

