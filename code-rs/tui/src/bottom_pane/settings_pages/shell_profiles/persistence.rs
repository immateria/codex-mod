use super::*;

impl ShellProfilesSettingsView {
    pub(super) fn load_fields_for_style(&mut self, style: ShellScriptStyle) {
        let (summary, references, skill_roots) =
            if let Some(profile) = self.shell_style_profiles.get(&style) {
                (
                    profile.summary.clone().unwrap_or_default(),
                    profile.references.clone(),
                    profile.skill_roots.clone(),
                )
            } else {
                (String::new(), Vec::new(), Vec::new())
            };

        self.summary_field.set_text(summary.as_str());
        self.references_field.set_text(&format_path_list(&references));
        self.skill_roots_field.set_text(&format_path_list(&skill_roots));
    }

    pub(super) fn stage_pending_profile_from_fields(&mut self) {
        let references = parse_path_list(self.references_field.text());
        let skill_roots = parse_path_list(self.skill_roots_field.text());
        let summary = {
            let text = self.summary_field.text();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        if references.is_empty()
            && skill_roots.is_empty()
            && summary.is_none()
            && !self.shell_style_profiles.contains_key(&self.selected_style)
        {
            return;
        }

        let profile = self
            .shell_style_profiles
            .entry(self.selected_style)
            .or_default();
        profile.summary = summary;
        profile.references = references;
        profile.skill_roots = skill_roots;
        if style_profile_is_empty(profile) {
            self.shell_style_profiles.remove(&self.selected_style);
        }
    }

    pub(super) fn apply_settings(&mut self) {
        self.stage_pending_profile_from_fields();

        let was_dirty = self.dirty;
        let mut changed_any = false;

        for style in [
            ShellScriptStyle::PosixSh,
            ShellScriptStyle::BashZshCompatible,
            ShellScriptStyle::Zsh,
            ShellScriptStyle::PowerShell,
            ShellScriptStyle::Cmd,
            ShellScriptStyle::Nushell,
            ShellScriptStyle::Elvish,
        ] {
            let (summary, references, skill_roots, skills, disabled_skills, include, exclude) =
                if let Some(profile) = self.shell_style_profiles.get(&style) {
                    (
                        profile.summary.clone(),
                        profile.references.clone(),
                        profile.skill_roots.clone(),
                        profile.skills.clone(),
                        profile.disabled_skills.clone(),
                        profile.mcp_servers.include.clone(),
                        profile.mcp_servers.exclude.clone(),
                    )
                } else {
                    (
                        None,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    )
                };

            match set_shell_style_profile_paths(&self.code_home, style, &references, &skill_roots)
            {
                Ok(changed) => changed_any |= changed,
                Err(err) => {
                    self.status = Some(format!("Failed to persist style paths: {err}"));
                    return;
                }
            }

            match set_shell_style_profile_summary(&self.code_home, style, summary.as_deref()) {
                Ok(changed) => changed_any |= changed,
                Err(err) => {
                    self.status = Some(format!("Failed to persist summary: {err}"));
                    return;
                }
            }

            match set_shell_style_profile_skills(
                &self.code_home,
                style,
                &skills,
                &disabled_skills,
            ) {
                Ok(changed) => changed_any |= changed,
                Err(err) => {
                    self.status = Some(format!("Failed to persist style skills: {err}"));
                    return;
                }
            }

            match set_shell_style_profile_mcp_servers(&self.code_home, style, &include, &exclude)
            {
                Ok(changed) => changed_any |= changed,
                Err(err) => {
                    self.status = Some(format!("Failed to persist MCP filters: {err}"));
                    return;
                }
            }
        }

        if changed_any || was_dirty {
            self.app_event_tx.send(AppEvent::UpdateShellStyleProfiles {
                shell_style_profiles: self.shell_style_profiles.clone(),
            });
        }

        self.dirty = false;
        if changed_any {
            self.status = Some("Shell style profiles applied.".to_string());
        } else {
            self.status = Some("No changes to apply.".to_string());
        }
    }
}

pub(super) fn format_path_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn normalize_list_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(super) fn parse_path_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

pub(super) fn style_profile_is_empty(profile: &ShellStyleProfileConfig) -> bool {
    profile
        .summary
        .as_ref()
        .map(|value| value.trim())
        .unwrap_or("")
        .is_empty()
        && profile.references.is_empty()
        && profile.prepend_developer_messages.is_empty()
        && profile.skills.is_empty()
        && profile.disabled_skills.is_empty()
        && profile.skill_roots.is_empty()
        && profile.mcp_servers.include.is_empty()
        && profile.mcp_servers.exclude.is_empty()
        && profile.command_safety == code_core::config_types::CommandSafetyProfileConfig::default()
        && profile.dangerous_command_detection.is_none()
}
