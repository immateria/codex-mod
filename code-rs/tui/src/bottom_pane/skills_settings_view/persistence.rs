use std::fs;

use super::*;

impl SkillsSettingsView {
    fn persist_style_profile_mode(
        &mut self,
        code_home: &std::path::Path,
        style: Option<ShellScriptStyle>,
        skill_name: &str,
        aliases: &[String],
    ) -> Result<bool, String> {
        if style.is_none() && self.editor.style_profile_mode != StyleProfileMode::Inherit {
            return Err("Style profile behavior requires a shell style value.".to_string());
        }

        let Some(style) = style else {
            return Ok(false);
        };

        let mut identifiers: Vec<&str> = Vec::new();
        identifiers.push(skill_name);
        for alias in aliases {
            identifiers.push(alias);
        }
        let deduped_identifiers = unique_profile_identifiers(identifiers);

        for identifier in &deduped_identifiers {
            set_shell_style_profile_skill_mode(
                code_home,
                style,
                identifier,
                ShellStyleSkillMode::Inherit,
            )
            .map_err(|err| format!("Failed to update shell_style_profiles: {err}"))?;
        }

        if self.editor.style_profile_mode != StyleProfileMode::Inherit {
            // Only pin the canonical slug. Aliases are cleared first so
            // profile state does not accumulate duplicate names for one skill.
            set_shell_style_profile_skill_mode(
                code_home,
                style,
                skill_name,
                self.editor.style_profile_mode.into_config_mode(),
            )
            .map_err(|err| format!("Failed to update shell_style_profiles: {err}"))?;
        }

        let profile = self.shell_style_profiles.entry(style).or_default();
        for identifier in &deduped_identifiers {
            remove_profile_skill(&mut profile.skills, identifier);
            remove_profile_skill(&mut profile.disabled_skills, identifier);
        }
        if self.editor.style_profile_mode == StyleProfileMode::Enable {
            profile.skills.push(skill_name.trim().to_string());
        }
        if self.editor.style_profile_mode == StyleProfileMode::Disable {
            profile.disabled_skills.push(skill_name.trim().to_string());
        }
        Ok(true)
    }

    fn persist_style_profile_paths(
        &mut self,
        code_home: &std::path::Path,
        style: Option<ShellScriptStyle>,
    ) -> Result<bool, String> {
        if !self.editor.style_resource_paths_dirty() {
            return Ok(false);
        }

        let references = parse_path_list(self.editor.style_references_field.text());
        let skill_roots = parse_path_list(self.editor.style_skill_roots_field.text());

        let Some(style) = style else {
            if references.is_empty() && skill_roots.is_empty() {
                self.editor.style_references_dirty = false;
                self.editor.style_skill_roots_dirty = false;
                return Ok(false);
            }
            return Err("Style references/skill roots require a shell style value.".to_string());
        };

        set_shell_style_profile_paths(code_home, style, &references, &skill_roots)
            .map_err(|err| format!("Failed to update shell_style_profiles paths: {err}"))?;

        let should_remove = {
            let profile = self.shell_style_profiles.entry(style).or_default();
            profile.references = references;
            profile.skill_roots = skill_roots;
            style_profile_is_empty(profile)
        };
        if should_remove {
            self.shell_style_profiles.remove(&style);
        }

        self.editor.style_references_dirty = false;
        self.editor.style_skill_roots_dirty = false;
        Ok(true)
    }

    fn persist_style_profile_mcp_servers(
        &mut self,
        code_home: &std::path::Path,
        style: Option<ShellScriptStyle>,
    ) -> Result<bool, String> {
        if !self.editor.style_mcp_filters_dirty() {
            return Ok(false);
        }

        let include = parse_string_list(self.editor.style_mcp_include_field.text());
        let exclude = parse_string_list(self.editor.style_mcp_exclude_field.text());

        let Some(style) = style else {
            if include.is_empty() && exclude.is_empty() {
                self.editor.style_mcp_include_dirty = false;
                self.editor.style_mcp_exclude_dirty = false;
                return Ok(false);
            }
            return Err("Style MCP include/exclude requires a shell style value.".to_string());
        };

        set_shell_style_profile_mcp_servers(code_home, style, &include, &exclude)
            .map_err(|err| format!("Failed to update shell_style_profiles mcp_servers: {err}"))?;

        let should_remove = {
            let profile = self.shell_style_profiles.entry(style).or_default();
            profile.mcp_servers.include = include;
            profile.mcp_servers.exclude = exclude;
            style_profile_is_empty(profile)
        };
        if should_remove {
            self.shell_style_profiles.remove(&style);
        }

        self.editor.style_mcp_include_dirty = false;
        self.editor.style_mcp_exclude_dirty = false;
        Ok(true)
    }

    fn cleanup_empty_style_profile(&mut self, style: Option<ShellScriptStyle>) -> bool {
        let Some(style) = style else {
            return false;
        };
        if self
            .shell_style_profiles
            .get(&style)
            .is_some_and(style_profile_is_empty)
        {
            self.shell_style_profiles.remove(&style);
            return true;
        }
        false
    }

    fn validate_name(&self, name: &str) -> Result<(), String> {
        let slug = name.trim();
        if slug.is_empty() {
            return Err("Name is required".to_string());
        }
        if !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err("Name must use letters, numbers, '-', '_' or '.'".to_string());
        }

        let dup = self
            .skills
            .iter()
            .enumerate()
            .any(|(idx, skill)| idx != self.selected && skill_slug(skill).eq_ignore_ascii_case(slug));
        if dup {
            return Err("A skill with this name already exists".to_string());
        }
        Ok(())
    }

    fn validate_frontmatter(&self, body: &str) -> Result<(), String> {
        if extract_frontmatter(body).is_none() {
            return Err("SKILL.md must start with YAML frontmatter".to_string());
        }
        if frontmatter_value(body, "name").is_none() {
            return Err("Frontmatter must include name".to_string());
        }
        if frontmatter_value(body, "description").is_none() {
            return Err("Frontmatter must include description".to_string());
        }
        Ok(())
    }

    fn validate_description(&self, description: &str) -> Result<(), String> {
        if description.trim().is_empty() {
            return Err("Description is required".to_string());
        }
        Ok(())
    }

    pub(super) fn generate_draft(&mut self) {
        let name = self.editor.name_field.text().trim().to_string();
        let description = self.editor.description_field.text().trim().to_string();
        if let Err(msg) = self.validate_name(&name) {
            self.status = Some((msg, Style::default().fg(colors::error())));
            return;
        }
        if let Err(msg) = self.validate_description(&description) {
            self.status = Some((msg, Style::default().fg(colors::error())));
            return;
        }

        let shell_style = self.editor.style_field.text().trim();
        let trigger_examples = self.editor.examples_field.text().trim();
        let title = name.replace('-', " ");

        let mut body = format!(
            "# {title}\n\n## Purpose\n\n{description}\n\n## Workflow\n\n1. Describe the first deterministic step.\n2. Describe conditional branches and constraints.\n3. Point to scripts/references/assets when needed.\n"
        );

        if !trigger_examples.is_empty() {
            body.push_str("\n## Trigger Examples\n\n");
            body.push_str(trigger_examples);
            body.push('\n');
        }

        if !shell_style.is_empty() {
            body.push_str("\n## Shell Style Integration\n\n");
            body.push_str(
                "This skill is intended for shell-style-aware loading. Configure it under `shell_style_profiles` when appropriate.\n\n",
            );
            body.push_str(&format!(
                "- Preferred shell style: `{shell_style}`\n- Consider wiring via `shell_style_profiles.{shell_style}.skill_roots`\n"
            ));
        }

        self.editor.body_field.set_text(&body);
        self.status = Some((
            "Draft generated from guided fields. Review and Save.".to_string(),
            Style::default().fg(colors::success()),
        ));
    }

    pub(super) fn save_current(&mut self) {
        if let Some(skill) = self.skills.get(self.selected)
            && skill.scope != SkillScope::User {
                self.status = Some((
                    "Only user skills can be saved".to_string(),
                    Style::default().fg(colors::error()),
                ));
                return;
            }

        let existing_skill = self.skills.get(self.selected).cloned();

        let name = self.editor.name_field.text().trim().to_string();
        let description = self.editor.description_field.text().trim().to_string();
        let shell_style_raw = self.editor.style_field.text().trim().to_string();
        let trigger_examples = self.editor.examples_field.text().trim().to_string();
        let body = self.editor.body_field.text().to_string();
        if let Err(msg) = self.validate_name(&name) {
            self.status = Some((msg, Style::default().fg(colors::error())));
            return;
        }
        if let Err(msg) = self.validate_description(&description) {
            self.status = Some((msg, Style::default().fg(colors::error())));
            return;
        }
        let parsed_shell_style = match self.parse_shell_style(&shell_style_raw) {
            Ok(style) => style,
            Err(msg) => {
                self.status = Some((msg, Style::default().fg(colors::error())));
                return;
            }
        };
        if parsed_shell_style.is_none() && self.editor.style_profile_mode != StyleProfileMode::Inherit {
            self.status = Some((
                "Style profile behavior requires a shell style value.".to_string(),
                Style::default().fg(colors::error()),
            ));
            return;
        }
        if parsed_shell_style.is_none() && self.editor.style_resource_paths_dirty() {
            let references = parse_path_list(self.editor.style_references_field.text());
            let skill_roots = parse_path_list(self.editor.style_skill_roots_field.text());
            if !references.is_empty() || !skill_roots.is_empty() {
                self.status = Some((
                    "Style references/skill roots require a shell style value.".to_string(),
                    Style::default().fg(colors::error()),
                ));
                return;
            }
        }
        if parsed_shell_style.is_none() && self.editor.style_mcp_filters_dirty() {
            let mcp_include = parse_string_list(self.editor.style_mcp_include_field.text());
            let mcp_exclude = parse_string_list(self.editor.style_mcp_exclude_field.text());
            if !mcp_include.is_empty() || !mcp_exclude.is_empty() {
                self.status = Some((
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
            self.validate_frontmatter(&body).is_ok(),
            "compose_skill_document produced invalid frontmatter"
        );

        let code_home = match find_code_home() {
            Ok(path) => path,
            Err(err) => {
                self.status = Some((
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
            self.status = Some((
                format!("Failed to create skill dir: {err}"),
                Style::default().fg(colors::error()),
            ));
            return;
        }
        let mut path = dir;
        path.push("SKILL.md");
        let tmp_path = path.with_extension("tmp");
        if let Err(err) = fs::write(&tmp_path, &body) {
            self.status = Some((
                format!("Failed to save: {err}"),
                Style::default().fg(colors::error()),
            ));
            return;
        }

        self.editor.style_field.set_text(&shell_style);

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
                && let Some(previous_style) = previous_style {
                    let previous_identifiers =
                        unique_profile_identifiers([previous_name.as_str(), previous_skill.name.as_str()]);
                    for identifier in &previous_identifiers {
                        if let Err(err) = set_shell_style_profile_skill_mode(
                            &code_home,
                            previous_style,
                            identifier,
                            ShellStyleSkillMode::Inherit,
                        ) {
                            append_warning(
                                &mut profile_warning,
                                format!(
                                    "Failed to clear previous style profile mapping: {err}"
                                ),
                            );
                            continue;
                        }
                        profiles_changed = true;
                        if let Some(profile) = self.shell_style_profiles.get_mut(&previous_style) {
                            remove_profile_skill(&mut profile.skills, identifier);
                            remove_profile_skill(&mut profile.disabled_skills, identifier);
                        }
                    }
                    profiles_changed |= self.cleanup_empty_style_profile(Some(previous_style));
                }
        }

        match self.persist_style_profile_mode(
            &code_home,
            parsed_shell_style,
            &name,
            &style_profile_aliases,
        ) {
            Ok(changed) => profiles_changed |= changed,
            Err(msg) => append_warning(&mut profile_warning, msg),
        }
        match self.persist_style_profile_paths(&code_home, parsed_shell_style) {
            Ok(changed) => profiles_changed |= changed,
            Err(msg) => append_warning(&mut profile_warning, msg),
        }
        match self.persist_style_profile_mcp_servers(&code_home, parsed_shell_style) {
            Ok(changed) => profiles_changed |= changed,
            Err(msg) => append_warning(&mut profile_warning, msg),
        }
        profiles_changed |= self.cleanup_empty_style_profile(parsed_shell_style);

        if path.exists() {
            let _ = fs::remove_file(&path);
        }
        if let Err(err) = fs::rename(&tmp_path, &path) {
            let _ = fs::remove_file(&tmp_path);
            self.status = Some((
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
        if self.selected < self.skills.len() {
            self.skills[self.selected] = new_entry;
        } else {
            self.skills.push(new_entry);
            self.selected = self.skills.len() - 1;
        }
        self.status = Some(match profile_warning {
            Some(msg) => (format!("Saved skill with warnings: {msg}"), Style::default().fg(colors::warning())),
            None => ("Saved.".to_string(), Style::default().fg(colors::success())),
        });

        self.app_event_tx.send(AppEvent::codex_op(Op::ListSkills));
        if profiles_changed {
            self.app_event_tx.send(AppEvent::UpdateShellStyleProfiles {
                shell_style_profiles: self.shell_style_profiles.clone(),
            });
        }
    }

    pub(super) fn delete_current(&mut self) {
        if self.selected >= self.skills.len() {
            self.status = Some(("Nothing to delete".to_string(), Style::default().fg(colors::warning())));
            self.mode = Mode::List;
            self.editor.focus = Focus::List;
            return;
        }
        let skill = self.skills[self.selected].clone();
        if skill.scope != SkillScope::User {
            self.status = Some((
                "Only user skills can be deleted".to_string(),
                Style::default().fg(colors::error()),
            ));
            return;
        }

        if let Err(err) = fs::remove_file(&skill.path)
            && err.kind() != std::io::ErrorKind::NotFound {
                self.status = Some((
                    format!("Delete failed: {err}"),
                    Style::default().fg(colors::error()),
                ));
                return;
            }

        if let Some(parent) = skill.path.parent() {
            let _ = fs::remove_dir(parent);
        }

        self.skills.remove(self.selected);
        if self.selected >= self.skills.len() && !self.skills.is_empty() {
            self.selected = self.skills.len() - 1;
        }

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
                        if let Some(profile) = self.shell_style_profiles.get_mut(&style) {
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
            profiles_changed |= self.cleanup_empty_style_profile(Some(style));
        }

        self.mode = Mode::List;
        self.editor.focus = Focus::List;
        self.status = Some(match delete_warning {
            Some(msg) => (
                format!("Deleted skill with warnings: {msg}"),
                Style::default().fg(colors::warning()),
            ),
            None => ("Deleted.".to_string(), Style::default().fg(colors::success())),
        });

        self.app_event_tx.send(AppEvent::codex_op(Op::ListSkills));
        if profiles_changed {
            self.app_event_tx.send(AppEvent::UpdateShellStyleProfiles {
                shell_style_profiles: self.shell_style_profiles.clone(),
            });
        }
    }
}


fn style_profile_is_empty(profile: &ShellStyleProfileConfig) -> bool {
    profile.references.is_empty()
        && profile.prepend_developer_messages.is_empty()
        && profile.skills.is_empty()
        && profile.disabled_skills.is_empty()
        && profile.skill_roots.is_empty()
        && profile.mcp_servers.include.is_empty()
        && profile.mcp_servers.exclude.is_empty()
        && profile.command_safety == CommandSafetyProfileConfig::default()
        && profile.dangerous_command_detection.is_none()
}

fn append_warning(current: &mut Option<String>, message: String) {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return;
    }
    match current {
        Some(existing) => {
            if !existing.split("; ").any(|part| part == trimmed) {
                existing.push_str("; ");
                existing.push_str(trimmed);
            }
        }
        None => *current = Some(trimmed.to_string()),
    }
}
