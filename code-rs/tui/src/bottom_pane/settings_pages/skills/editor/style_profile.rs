use super::*;

pub(super) fn cycle_style_profile_mode(view: &mut SkillsSettingsView, forward: bool) {
    view.editor.style_profile_mode = if forward {
        view.editor.style_profile_mode.next()
    } else {
        view.editor.style_profile_mode.previous()
    };
}

pub(super) fn sync_style_profile_fields_if_needed(
    view: &mut SkillsSettingsView,
    previous_style: Option<ShellScriptStyle>,
) {
    let next_style = ShellScriptStyle::parse(view.editor.style_field.text().trim());
    if next_style != previous_style && next_style.is_some() && !view.editor.style_profile_fields_dirty() {
        view.set_style_resource_fields_from_profile(next_style);
    }
}

pub(super) fn set_style_resource_fields_from_profile_inner(
    view: &mut SkillsSettingsView,
    style: Option<ShellScriptStyle>,
) {
    let profile = style.and_then(|shell_style| view.shell_style_profiles.get(&shell_style));
    let (references, skill_roots, mcp_include, mcp_exclude) = match profile {
        Some(profile) => (
            profile.references.clone(),
            profile.skill_roots.clone(),
            profile.mcp_servers.include.clone(),
            profile.mcp_servers.exclude.clone(),
        ),
        None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
    };

    view.editor
        .style_references_field
        .set_text(&format_path_list(&references));
    view.editor
        .style_skill_roots_field
        .set_text(&format_path_list(&skill_roots));
    view.editor
        .style_mcp_include_field
        .set_text(&format_string_list(&mcp_include));
    view.editor
        .style_mcp_exclude_field
        .set_text(&format_string_list(&mcp_exclude));
    view.editor.style_references_dirty = false;
    view.editor.style_skill_roots_dirty = false;
    view.editor.style_mcp_include_dirty = false;
    view.editor.style_mcp_exclude_dirty = false;
}

pub(super) fn infer_style_profile_mode_inner(
    view: &SkillsSettingsView,
    shell_style: &str,
    slug: &str,
    display_name: &str,
) -> StyleProfileMode {
    let Some(style) = ShellScriptStyle::parse(shell_style) else {
        return StyleProfileMode::Inherit;
    };

    let Some(profile) = view.shell_style_profiles.get(&style) else {
        return StyleProfileMode::Inherit;
    };

    let identifiers = [slug, display_name];
    if profile_list_contains_any(&profile.disabled_skills, &identifiers) {
        return StyleProfileMode::Disable;
    }
    if profile_list_contains_any(&profile.skills, &identifiers) {
        return StyleProfileMode::Enable;
    }
    StyleProfileMode::Inherit
}

pub(super) fn parse_shell_style_inner(
    shell_style_raw: &str,
) -> Result<Option<ShellScriptStyle>, String> {
    let trimmed = shell_style_raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    ShellScriptStyle::parse(trimmed)
        .ok_or_else(|| "Invalid shell style. Use: posix-sh, bash-zsh-compatible, or zsh.".to_string())
        .map(Some)
}

