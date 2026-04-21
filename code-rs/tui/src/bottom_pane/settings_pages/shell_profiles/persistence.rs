use super::*;

impl ShellProfilesSettingsView {
    pub(super) fn load_fields_for_style(&mut self, id: &str) {
        let (summary, references, skill_roots) =
            if let Some(entry) = self.shell_style_profiles.get(id) {
                (
                    entry.config.summary.clone().unwrap_or_default(),
                    entry.config.references.clone(),
                    entry.config.skill_roots.clone(),
                )
            } else {
                (String::new(), Vec::new(), Vec::new())
            };

        self.summary_field.set_text(summary.as_str());
        self.references_field.set_text(&crate::text_formatting::format_path_list(&references));
        self.skill_roots_field.set_text(&crate::text_formatting::format_path_list(&skill_roots));
    }

    pub(super) fn stage_pending_profile_from_fields(&mut self) {
        let references = crate::text_formatting::parse_path_list(self.references_field.text());
        let skill_roots = crate::text_formatting::parse_path_list(self.skill_roots_field.text());
        let summary = {
            let text = self.summary_field.text();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            }
        };

        if references.is_empty()
            && skill_roots.is_empty()
            && summary.is_none()
            && !self.shell_style_profiles.contains_key(&self.selected_id)
        {
            return;
        }

        let selected_id = self.selected_id.clone();
        let entry = self
            .shell_style_profiles
            .entry(selected_id.clone())
            .or_insert_with(Default::default);
        entry.config.summary = summary;
        entry.config.references = references;
        entry.config.skill_roots = skill_roots;
        let can_remove = style_profile_is_empty(&entry.config)
            && entry.applicable_shells.is_empty()
            && entry.style.is_none();
        if can_remove {
            self.shell_style_profiles.remove(&selected_id);
        }
    }

    pub(super) fn apply_settings(&mut self) {
        self.stage_pending_profile_from_fields();

        let was_dirty = self.dirty;
        let changed_any = match set_all_shell_style_profiles(&self.code_home, &self.shell_style_profiles) {
            Ok(changed) => changed,
            Err(err) => {
                self.status = Some(format!("Failed to persist shell profiles: {err}"));
                return;
            }
        };

        if changed_any || was_dirty {
            self.app_event_tx.send(AppEvent::UpdateShellStyleProfiles {
                shell_style_profiles: self.shell_style_profiles.clone(),
            });
        }

        self.dirty = false;
        if changed_any {
            self.status = Some("Shell style profiles applied.".to_owned());
        } else {
            self.status = Some("No changes to apply.".to_owned());
        }
    }
}

pub(super) fn normalize_list_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(super) fn style_profile_is_empty(profile: &ShellStyleProfileConfig) -> bool {
    profile
        .summary
        .as_ref()
        .map_or("", |value| value.trim())
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
