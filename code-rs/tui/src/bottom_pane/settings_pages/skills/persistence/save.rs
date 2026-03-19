use std::fs;

use super::*;

use super::style_profiles::append_warning;

pub(super) fn save_current_inner(view: &mut SkillsSettingsView) {
    let selected = view.selected_list_index();
    if let Some(skill) = view.skills.get(selected)
        && skill.scope != SkillScope::User
    {
        view.status = Some((
            "Only user skills can be saved".to_string(),
            Style::default().fg(colors::error()),
        ));
        return;
    }

    let existing_skill = view.skills.get(selected).cloned();

    let name = view.editor.name_field.text().trim().to_string();
    let description = view.editor.description_field.text().trim().to_string();
    let shell_style_raw = view.editor.style_field.text().trim().to_string();
    let trigger_examples = view.editor.examples_field.text().trim().to_string();
    let body = view.editor.body_field.text().to_string();
    if let Err(msg) = view.validate_name(&name) {
        view.status = Some((msg, Style::default().fg(colors::error())));
        return;
    }
    if let Err(msg) = view.validate_description(&description) {
        view.status = Some((msg, Style::default().fg(colors::error())));
        return;
    }
    let parsed_shell_style = match view.parse_shell_style(&shell_style_raw) {
        Ok(style) => style,
        Err(msg) => {
            view.status = Some((msg, Style::default().fg(colors::error())));
            return;
        }
    };
    if parsed_shell_style.is_none() && view.editor.style_profile_mode != StyleProfileMode::Inherit {
        view.status = Some((
            "Style profile behavior requires a shell style value.".to_string(),
            Style::default().fg(colors::error()),
        ));
        return;
    }
    if parsed_shell_style.is_none() && view.editor.style_resource_paths_dirty() {
        let references = parse_path_list(view.editor.style_references_field.text());
        let skill_roots = parse_path_list(view.editor.style_skill_roots_field.text());
        if !references.is_empty() || !skill_roots.is_empty() {
            view.status = Some((
                "Style references/skill roots require a shell style value.".to_string(),
                Style::default().fg(colors::error()),
            ));
            return;
        }
    }
    if parsed_shell_style.is_none() && view.editor.style_mcp_filters_dirty() {
        let mcp_include = parse_string_list(view.editor.style_mcp_include_field.text());
        let mcp_exclude = parse_string_list(view.editor.style_mcp_exclude_field.text());
        if !mcp_include.is_empty() || !mcp_exclude.is_empty() {
            view.status = Some((
                "Style MCP include/exclude requires a shell style value.".to_string(),
                Style::default().fg(colors::error()),
            ));
            return;
        }
    }
    let shell_style = parsed_shell_style
        .map(|style| style.to_string())
        .unwrap_or_default();

    let mut document_body = strip_frontmatter(&body);
    let extra_frontmatter = extract_frontmatter(&body)
        .map(|frontmatter| {
            filter_frontmatter_excluding_keys(
                frontmatter.as_str(),
                &["name", "description", "shell_style"],
            )
        })
        .unwrap_or_default();
    let has_trigger_examples_section = document_body
        .lines()
        .any(|line| line.trim() == "## Trigger Examples");
    if !trigger_examples.is_empty() && !has_trigger_examples_section {
        document_body.push_str("\n\n## Trigger Examples\n\n");
        document_body.push_str(&trigger_examples);
        document_body.push('\n');
    }

    let body = compose_skill_document(
        &name,
        &description,
        &shell_style,
        &extra_frontmatter,
        &document_body,
    );
    debug_assert!(
        view.validate_frontmatter(&body).is_ok(),
        "compose_skill_document produced invalid frontmatter"
    );

    let code_home = match find_code_home() {
        Ok(path) => path,
        Err(err) => {
            view.status = Some((
                format!("CODE_HOME unavailable: {err}"),
                Style::default().fg(colors::error()),
            ));
            return;
        }
    };
    let mut dir = code_home.clone();
    dir.push("skills");
    dir.push(&name);
    if let Err(err) = fs::create_dir_all(&dir) {
        view.status = Some((
            format!("Failed to create skill dir: {err}"),
            Style::default().fg(colors::error()),
        ));
        return;
    }
    let mut path = dir;
    path.push("SKILL.md");
    let tmp_path = path.with_extension("tmp");
    if let Err(err) = fs::write(&tmp_path, &body) {
        view.status = Some((
            format!("Failed to save: {err}"),
            Style::default().fg(colors::error()),
        ));
        return;
    }

    view.editor.style_field.set_text(&shell_style);

    let mut profiles_changed = false;

    let mut profile_warning: Option<String> = None;
    let mut style_profile_aliases: Vec<String> = Vec::new();
    if let Some(previous_skill) = existing_skill.as_ref() {
        let previous_name = skill_slug(previous_skill);
        style_profile_aliases.push(previous_name.clone());
        style_profile_aliases.push(previous_skill.name.clone());
        let previous_style = frontmatter_value(&previous_skill.content, "shell_style")
            .and_then(|value| ShellScriptStyle::parse(&value));
        let changed_identity = previous_name != name || previous_style != parsed_shell_style;
        if changed_identity
            && let Some(previous_style) = previous_style
        {
            let previous_identifiers = unique_profile_identifiers([
                previous_name.as_str(),
                previous_skill.name.as_str(),
            ]);
            for identifier in &previous_identifiers {
                if let Err(err) = set_shell_style_profile_skill_mode(
                    &code_home,
                    previous_style,
                    identifier,
                    ShellStyleSkillMode::Inherit,
                ) {
                    append_warning(
                        &mut profile_warning,
                        format!("Failed to clear previous style profile mapping: {err}"),
                    );
                    continue;
                }
                profiles_changed = true;
                if let Some(profile) = view.shell_style_profiles.get_mut(&previous_style) {
                    remove_profile_skill(&mut profile.skills, identifier);
                    remove_profile_skill(&mut profile.disabled_skills, identifier);
                }
            }
            profiles_changed |= view.cleanup_empty_style_profile(Some(previous_style));
        }
    }

    match view.persist_style_profile_mode(
        &code_home,
        parsed_shell_style,
        &name,
        &style_profile_aliases,
    ) {
        Ok(changed) => profiles_changed |= changed,
        Err(msg) => append_warning(&mut profile_warning, msg),
    }
    match view.persist_style_profile_paths(&code_home, parsed_shell_style) {
        Ok(changed) => profiles_changed |= changed,
        Err(msg) => append_warning(&mut profile_warning, msg),
    }
    match view.persist_style_profile_mcp_servers(&code_home, parsed_shell_style) {
        Ok(changed) => profiles_changed |= changed,
        Err(msg) => append_warning(&mut profile_warning, msg),
    }
    profiles_changed |= view.cleanup_empty_style_profile(parsed_shell_style);

    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    if let Err(err) = fs::rename(&tmp_path, &path) {
        let _ = fs::remove_file(&tmp_path);
        view.status = Some((
            format!("Failed to finalize save: {err}"),
            Style::default().fg(colors::error()),
        ));
        return;
    }

    if let Some(previous_skill) = existing_skill.as_ref()
        && previous_skill.path != path
        && previous_skill.scope == SkillScope::User
    {
        if let Err(err) = fs::remove_file(&previous_skill.path)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            append_warning(
                &mut profile_warning,
                format!("Failed to remove previous file: {err}"),
            );
        }
        if let Some(parent) = previous_skill.path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    let display_name = name.clone();

    let new_entry = Skill {
        name: display_name,
        path,
        description,
        scope: SkillScope::User,
        content: body,
    };
    if selected < view.skills.len() {
        view.skills[selected] = new_entry;
    } else {
        view.skills.push(new_entry);
        view.list_state.selected_idx = Some(view.skills.len().saturating_sub(1));
    }
    view.clamp_list_state();
    view.status = Some(match profile_warning {
        Some(msg) => (
            format!("Saved skill with warnings: {msg}"),
            Style::default().fg(colors::warning()),
        ),
        None => ("Saved.".to_string(), Style::default().fg(colors::success())),
    });

    view.app_event_tx.send(AppEvent::codex_op(Op::ListSkills));
    if profiles_changed {
        view.app_event_tx.send(AppEvent::UpdateShellStyleProfiles {
            shell_style_profiles: view.shell_style_profiles.clone(),
        });
    }
}

