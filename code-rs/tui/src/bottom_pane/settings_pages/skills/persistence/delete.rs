use std::fs;

use super::*;

use super::style_profiles::append_warning;

pub(super) fn delete_current_inner(view: &mut SkillsSettingsView) {
    let selected = view.selected_list_index();
    if selected >= view.skills.len() {
        view.status = Some((
            "Nothing to delete".to_string(),
            Style::default().fg(colors::warning()),
        ));
        view.mode = Mode::List;
        view.editor.focus = Focus::List;
        view.ensure_list_selection_visible();
        return;
    }
    let skill = view.skills[selected].clone();
    if skill.scope != SkillScope::User {
        view.status = Some((
            "Only user skills can be deleted".to_string(),
            Style::default().fg(colors::error()),
        ));
        return;
    }

    if let Err(err) = fs::remove_file(&skill.path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        view.status = Some((
            format!("Delete failed: {err}"),
            Style::default().fg(colors::error()),
        ));
        return;
    }

    if let Some(parent) = skill.path.parent() {
        let _ = fs::remove_dir(parent);
    }

    view.skills.remove(selected);
    if selected >= view.skills.len() && !view.skills.is_empty() {
        view.list_state.selected_idx = Some(view.skills.len().saturating_sub(1));
    } else {
        view.list_state.selected_idx = Some(selected.min(view.skills.len()));
    }
    view.clamp_list_state();

    let mut profiles_changed = false;

    let mut delete_warning: Option<String> = None;
    let deleted_skill_name = skill_slug(&skill);
    if let Some(style) = frontmatter_value(&skill.content, "shell_style")
        .and_then(|value| ShellScriptStyle::parse(&value))
        && let Ok(code_home) = find_code_home()
    {
        let identifiers =
            unique_profile_identifiers([deleted_skill_name.as_str(), skill.name.as_str()]);
        for identifier in &identifiers {
            match set_shell_style_profile_skill_mode(
                &code_home,
                style,
                identifier,
                ShellStyleSkillMode::Inherit,
            ) {
                Ok(_) => {
                    profiles_changed = true;
                    if let Some(profile) = view.shell_style_profiles.get_mut(&style) {
                        remove_profile_skill(&mut profile.skills, identifier);
                        remove_profile_skill(&mut profile.disabled_skills, identifier);
                    }
                }
                Err(err) => append_warning(
                    &mut delete_warning,
                    format!("Failed to clear style profile mapping: {err}"),
                ),
            }
        }
        profiles_changed |= view.cleanup_empty_style_profile(Some(style));
    }

    view.mode = Mode::List;
    view.editor.focus = Focus::List;
    view.ensure_list_selection_visible();
    view.status = Some(match delete_warning {
        Some(msg) => (
            format!("Deleted skill with warnings: {msg}"),
            Style::default().fg(colors::warning()),
        ),
        None => ("Deleted.".to_string(), Style::default().fg(colors::success())),
    });

    view.app_event_tx.send(AppEvent::codex_op(Op::ListSkills));
    if profiles_changed {
        view.app_event_tx.send(AppEvent::UpdateShellStyleProfiles {
            shell_style_profiles: view.shell_style_profiles.clone(),
        });
    }
}

