use super::*;

pub(super) async fn load_skills_inventory_and_refresh_session(
    sess: &Arc<Session>,
    config_snapshot: Arc<Config>,
) -> crate::skills::model::SkillLoadOutcome {
    let skills_enabled = config_snapshot.skills_enabled;
    let active_shell_style = sess.user_shell.script_style();
    let active_shell_style_label = active_shell_style.map(|style| style.to_string());

    let mut shell_style_skill_filter: Option<HashSet<String>> = None;
    let mut shell_style_disabled_skills: HashSet<String> = HashSet::new();
    let mut shell_style_skill_roots: Vec<PathBuf> = Vec::new();
    if let Some(style) = active_shell_style
        && let Some(profile) = config_snapshot.shell_style_profiles.get(&style)
    {
        let requested_skills: HashSet<String> = profile
            .skills
            .iter()
            .map(|name| name.trim().to_ascii_lowercase())
            .filter(|name| !name.is_empty())
            .collect();
        if !requested_skills.is_empty() {
            shell_style_skill_filter = Some(requested_skills);
        }

        shell_style_disabled_skills.extend(
            profile
                .disabled_skills
                .iter()
                .map(|name| name.trim().to_ascii_lowercase())
                .filter(|name| !name.is_empty()),
        );

        shell_style_skill_roots.extend(
            profile
                .skill_roots
                .iter()
                .filter(|path| !path.as_os_str().is_empty())
                .cloned(),
        );
    }

    let config_for_load = Arc::clone(&config_snapshot);
    let inventory = match tokio::task::spawn_blocking(move || {
        if !skills_enabled {
            return crate::skills::model::SkillLoadOutcome::default();
        }

        if shell_style_skill_roots.is_empty() {
            crate::skills::loader::load_skills(config_for_load.as_ref())
        } else {
            crate::skills::loader::load_skills_with_additional_roots(
                config_for_load.as_ref(),
                shell_style_skill_roots.into_iter(),
            )
        }
    })
    .await
    {
        Ok(outcome) => outcome,
        Err(err) => {
            warn!("failed to load skills: {err}");
            crate::skills::model::SkillLoadOutcome::default()
        }
    };

    for err in &inventory.errors {
        warn!("invalid skill {}: {}", err.path.display(), err.message);
    }

    if skills_enabled {
        let available_skill_names: HashSet<String> = inventory
            .skills
            .iter()
            .map(|skill| skill.name.trim().to_ascii_lowercase())
            .collect();

        let mut matched_skills: HashSet<String> = HashSet::new();
        let mut active_skills: Vec<crate::skills::model::SkillMetadata> = Vec::new();
        for skill in &inventory.skills {
            let normalized = skill.name.trim().to_ascii_lowercase();
            if let Some(skill_filter) = shell_style_skill_filter.as_ref() {
                if !skill_filter.contains(&normalized) {
                    continue;
                }
                matched_skills.insert(normalized.clone());
            }

            if shell_style_disabled_skills.contains(&normalized) {
                continue;
            }

            active_skills.push(crate::skills::model::SkillMetadata {
                name: skill.name.clone(),
                description: skill.description.clone(),
                path: skill.path.clone(),
                scope: skill.scope,
                content: String::new(),
            });
        }

        if let Some(style_label) = active_shell_style_label.as_deref()
            && let Some(skill_filter) = shell_style_skill_filter.as_ref()
        {
            for requested in skill_filter {
                if !matched_skills.contains(requested) {
                    warn!("shell style profile `{style_label}` requested unknown skill `{requested}`");
                }
            }
        }

        if let Some(style_label) = active_shell_style_label.as_deref() {
            for requested in &shell_style_disabled_skills {
                if !available_skill_names.contains(requested) {
                    warn!(
                        "shell style profile `{style_label}` requested unknown disabled skill `{requested}`"
                    );
                }
            }
        }

        *sess.skills.write().await = active_skills;
    } else {
        sess.skills.write().await.clear();
    }

    inventory
}

pub(super) fn strip_skill_contents(
    skills: &[crate::skills::model::SkillMetadata],
) -> Vec<crate::skills::model::SkillMetadata> {
    let mut out: Vec<crate::skills::model::SkillMetadata> = Vec::with_capacity(skills.len());
    for skill in skills {
        out.push(crate::skills::model::SkillMetadata {
            name: skill.name.clone(),
            description: skill.description.clone(),
            path: skill.path.clone(),
            scope: skill.scope,
            content: String::new(),
        });
    }
    out
}

