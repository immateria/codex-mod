use std::io;
use std::path::Path;

use code_common::model_presets::{
    all_model_presets,
    model_preset_available_for_auth,
    ModelPreset,
    HIDE_GPT5_1_MIGRATION_PROMPT_CONFIG,
    HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG,
    HIDE_GPT_5_2_MIGRATION_PROMPT_CONFIG,
};
use code_core::config::Config;
use code_core::config_edit::{self, CONFIG_KEY_EFFORT, CONFIG_KEY_MODEL};
use code_core::config_types::Notice;
use code_core::config_types::ReasoningEffort;
use code_login::AuthMode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StartupModelMigrationNotice {
    pub current_model_label: String,
    pub target_model_label: String,
    pub target_model: String,
    pub hide_key: String,
    pub new_effort: Option<ReasoningEffort>,
}

impl StartupModelMigrationNotice {
    pub(crate) fn banner_message(&self) -> String {
        format!(
            "Recommended model: {} (current: {})",
            self.target_model_label, self.current_model_label
        )
    }
}

#[derive(Clone, Copy)]
struct MigrationPlan {
    current: &'static ModelPreset,
    target: &'static ModelPreset,
    hide_key: &'static str,
    new_effort: Option<ReasoningEffort>,
}

pub(crate) fn determine_startup_model_migration_notice(
    config: &Config,
    auth_mode: AuthMode,
) -> Option<StartupModelMigrationNotice> {
    let plan = determine_migration_plan(config, auth_mode)?;
    Some(StartupModelMigrationNotice {
        current_model_label: plan.current.display_name.clone(),
        target_model_label: plan.target.display_name.clone(),
        target_model: plan.target.model.clone(),
        hide_key: plan.hide_key.to_string(),
        new_effort: plan.new_effort,
    })
}

pub(crate) async fn persist_startup_model_migration_acceptance(
    code_home: &Path,
    profile: Option<&str>,
    notice: &StartupModelMigrationNotice,
) -> io::Result<()> {
    let mut pending: Vec<(Vec<&str>, String)> = Vec::new();
    pending.push((vec![CONFIG_KEY_MODEL], notice.target_model.clone()));

    if let Some(effort) = notice.new_effort {
        pending.push((
            vec![CONFIG_KEY_EFFORT],
            reasoning_effort_to_str(effort).to_string(),
        ));
    }

    pending.push((vec!["notice", notice.hide_key.as_str()], "true".to_string()));

    let overrides: Vec<(&[&str], &str)> = pending
        .iter()
        .map(|(path, value)| (path.as_slice(), value.as_str()))
        .collect();

    config_edit::persist_overrides(code_home, profile, &overrides)
        .await
        .map_err(|err| io::Error::other(err.to_string()))
}

pub(crate) async fn persist_startup_model_migration_dismissal(
    code_home: &Path,
    profile: Option<&str>,
    hide_key: &str,
) -> io::Result<()> {
    let notice_path = ["notice", hide_key];
    let overrides = [(&notice_path[..], "true")];
    config_edit::persist_overrides(code_home, profile, &overrides)
        .await
        .map_err(|err| io::Error::other(err.to_string()))
}

pub(crate) fn set_notice_flag(notices: &mut Notice, key: &str) {
    if key == HIDE_GPT5_1_MIGRATION_PROMPT_CONFIG {
        notices.hide_gpt5_1_migration_prompt = Some(true);
    } else if key == HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG {
        notices.hide_gpt_5_1_codex_max_migration_prompt = Some(true);
    } else if key == HIDE_GPT_5_2_MIGRATION_PROMPT_CONFIG {
        notices.hide_gpt5_2_migration_prompt = Some(true);
    } else if key == code_common::model_presets::HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG {
        notices.hide_gpt5_2_codex_migration_prompt = Some(true);
    }
}

fn determine_migration_plan(config: &Config, auth_mode: AuthMode) -> Option<MigrationPlan> {
    let current_slug = config.model.to_ascii_lowercase();
    let presets = all_model_presets();
    let current = find_migration_preset(presets, &current_slug)?;
    let upgrade = current.upgrade.as_ref()?;
    if notice_hidden(&config.notices, upgrade.migration_config_key.as_str()) {
        return None;
    }
    let target = presets
        .iter()
        .find(|preset| preset.id.eq_ignore_ascii_case(&upgrade.id))?;
    if !auth_allows_target(auth_mode, target) {
        return None;
    }
    let new_effort = None;
    Some(MigrationPlan {
        current,
        target,
        hide_key: upgrade.migration_config_key.as_str(),
        new_effort,
    })
}

fn find_migration_preset<'a>(
    presets: &'a [ModelPreset],
    slug_lower: &str,
) -> Option<&'a ModelPreset> {
    let slug_no_prefix = slug_lower
        .rsplit_once(':')
        .map(|(_, rest)| rest)
        .unwrap_or(slug_lower);
    let slug_no_prefix = slug_no_prefix
        .rsplit_once('/')
        .map(|(_, rest)| rest)
        .unwrap_or(slug_no_prefix);
    let slug_no_test = slug_no_prefix.strip_prefix("test-").unwrap_or(slug_no_prefix);

    if let Some(preset) = presets.iter().find(|preset| {
        preset.id.eq_ignore_ascii_case(slug_no_test)
            || preset.model.eq_ignore_ascii_case(slug_no_test)
            || preset.display_name.eq_ignore_ascii_case(slug_no_test)
    }) {
        return Some(preset);
    }

    let mut best: Option<&ModelPreset> = None;
    let mut best_len = 0usize;
    for preset in presets {
        for candidate in [&preset.id, &preset.model, &preset.display_name] {
            let candidate_lower = candidate.to_ascii_lowercase();
            if slug_no_test.starts_with(candidate_lower.as_str()) {
                let candidate_len = candidate.len();
                if candidate_len > best_len {
                    best = Some(preset);
                    best_len = candidate_len;
                }
                break;
            }
        }
    }

    best
}

fn notice_hidden(notices: &Notice, key: &str) -> bool {
    if key == HIDE_GPT5_1_MIGRATION_PROMPT_CONFIG {
        notices.hide_gpt5_1_migration_prompt.unwrap_or(false)
    } else if key == HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG {
        notices.hide_gpt_5_1_codex_max_migration_prompt.unwrap_or(false)
    } else if key == HIDE_GPT_5_2_MIGRATION_PROMPT_CONFIG {
        notices.hide_gpt5_2_migration_prompt.unwrap_or(false)
    } else if key == code_common::model_presets::HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG {
        notices.hide_gpt5_2_codex_migration_prompt.unwrap_or(false)
    } else {
        false
    }
}

fn auth_allows_target(auth_mode: AuthMode, target: &ModelPreset) -> bool {
    // Startup migration runs before remote model discovery, so we do not know
    // whether a ChatGPT account can access pro-only models. Use the shared
    // availability policy with a conservative non-pro assumption here.
    model_preset_available_for_auth(target, Some(auth_mode), false)
}

fn reasoning_effort_to_str(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::None => "none",
        ReasoningEffort::Minimal => "minimal",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::XHigh => "xhigh",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_core::config::ConfigOverrides;
    use code_core::config::ConfigToml;

    fn config_for_model(model: &str) -> Config {
        let cfg = ConfigToml {
            model: Some(model.to_string()),
            ..Default::default()
        };
        Config::load_from_base_config_with_overrides(
            cfg,
            ConfigOverrides::default(),
            std::env::temp_dir(),
        )
        .unwrap_or_else(|err| panic!("failed to build config: {err}"))
    }

    #[test]
    fn api_key_auth_hides_unavailable_migration_target() {
        let config = config_for_model("gpt-5.2-codex");
        assert!(determine_startup_model_migration_notice(&config, AuthMode::ApiKey).is_none());
    }

    #[test]
    fn chatgpt_auth_still_shows_migration_target() {
        let config = config_for_model("gpt-5.2-codex");
        let notice = determine_startup_model_migration_notice(&config, AuthMode::Chatgpt)
            .unwrap_or_else(|| panic!("expected migration notice"));
        assert_eq!(notice.target_model, "gpt-5.3-codex");
    }
}
