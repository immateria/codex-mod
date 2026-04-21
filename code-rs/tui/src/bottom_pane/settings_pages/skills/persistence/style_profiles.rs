use super::*;

impl SkillsSettingsView {
    pub(super) fn persist_style_profile_mode(
        &mut self,
        code_home: &std::path::Path,
        style: Option<ShellScriptStyle>,
        skill_name: &str,
        aliases: &[String],
    ) -> Result<bool, String> {
        if style.is_none() && self.editor.style_profile_mode != StyleProfileMode::Inherit {
            return Err("Style profile behavior requires a shell style value.".to_owned());
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

        let profile = self
            .shell_style_profiles
            .entry(style.to_string())
            .or_insert_with(Default::default);
        for identifier in &deduped_identifiers {
            remove_profile_skill(&mut profile.config.skills, identifier);
            remove_profile_skill(&mut profile.config.disabled_skills, identifier);
        }
        if self.editor.style_profile_mode == StyleProfileMode::Enable {
            profile.config.skills.push(skill_name.trim().to_owned());
        }
        if self.editor.style_profile_mode == StyleProfileMode::Disable {
            profile.config.disabled_skills.push(skill_name.trim().to_owned());
        }
        Ok(true)
    }

    pub(super) fn persist_style_profile_paths(
        &mut self,
        code_home: &std::path::Path,
        style: Option<ShellScriptStyle>,
    ) -> Result<bool, String> {
        if !self.editor.style_resource_paths_dirty() {
            return Ok(false);
        }

        let references = crate::text_formatting::parse_path_list(self.editor.style_references_field.text());
        let skill_roots = crate::text_formatting::parse_path_list(self.editor.style_skill_roots_field.text());

        let Some(style) = style else {
            if references.is_empty() && skill_roots.is_empty() {
                self.editor.style_references_dirty = false;
                self.editor.style_skill_roots_dirty = false;
                return Ok(false);
            }
            return Err("Style references/skill roots require a shell style value.".to_owned());
        };

        set_shell_style_profile_paths(code_home, style, &references, &skill_roots)
            .map_err(|err| format!("Failed to update shell_style_profiles paths: {err}"))?;

        let key = style.to_string();
        let should_remove = {
            let entry = self.shell_style_profiles.entry(key.clone()).or_insert_with(Default::default);
            entry.config.references = references;
            entry.config.skill_roots = skill_roots;
            style_profile_is_empty(&entry.config)
        };
        if should_remove {
            self.shell_style_profiles.remove(&key);
        }

        self.editor.style_references_dirty = false;
        self.editor.style_skill_roots_dirty = false;
        Ok(true)
    }

    pub(super) fn persist_style_profile_mcp_servers(
        &mut self,
        code_home: &std::path::Path,
        style: Option<ShellScriptStyle>,
    ) -> Result<bool, String> {
        if !self.editor.style_mcp_filters_dirty() {
            return Ok(false);
        }

        let include = crate::text_formatting::parse_string_list(self.editor.style_mcp_include_field.text());
        let exclude = crate::text_formatting::parse_string_list(self.editor.style_mcp_exclude_field.text());

        let Some(style) = style else {
            if include.is_empty() && exclude.is_empty() {
                self.editor.style_mcp_include_dirty = false;
                self.editor.style_mcp_exclude_dirty = false;
                return Ok(false);
            }
            return Err("Style MCP include/exclude requires a shell style value.".to_owned());
        };

        set_shell_style_profile_mcp_servers(code_home, style, &include, &exclude)
            .map_err(|err| format!("Failed to update shell_style_profiles mcp_servers: {err}"))?;

        let key = style.to_string();
        let should_remove = {
            let entry = self.shell_style_profiles.entry(key.clone()).or_insert_with(Default::default);
            entry.config.mcp_servers.include = include;
            entry.config.mcp_servers.exclude = exclude;
            style_profile_is_empty(&entry.config)
        };
        if should_remove {
            self.shell_style_profiles.remove(&key);
        }

        self.editor.style_mcp_include_dirty = false;
        self.editor.style_mcp_exclude_dirty = false;
        Ok(true)
    }

    pub(super) fn cleanup_empty_style_profile(&mut self, style: Option<ShellScriptStyle>) -> bool {
        let Some(style) = style else {
            return false;
        };
        let key = style.to_string();
        if self
            .shell_style_profiles
            .get(&key)
            .is_some_and(|e| style_profile_is_empty(&e.config))
        {
            self.shell_style_profiles.remove(&key);
            return true;
        }
        false
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

pub(super) fn append_warning(current: &mut Option<String>, message: String) {
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
        None => *current = Some(trimmed.to_owned()),
    }
}

