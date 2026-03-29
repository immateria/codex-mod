use code_core::config::Config;
use shlex::try_join;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

/// Update action the CLI should perform for a managed install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpdateAction {
    /// Update via `npm install -g @just-every/code@latest`.
    NpmGlobalLatest,
    /// Update via `bun install -g @just-every/code@latest`.
    BunGlobalLatest,
    /// Update via `brew upgrade code`.
    BrewUpgrade,
    /// Update via a user-provided command override.
    Custom(Vec<String>),
}

impl UpdateAction {
    pub(crate) fn command_and_display(&self) -> (Vec<String>, String) {
        match self {
            UpdateAction::NpmGlobalLatest => (
                vec![
                    "npm".to_string(),
                    "install".to_string(),
                    "-g".to_string(),
                    "@just-every/code@latest".to_string(),
                ],
                "npm install -g @just-every/code@latest".to_string(),
            ),
            UpdateAction::BunGlobalLatest => (
                vec![
                    "bun".to_string(),
                    "install".to_string(),
                    "-g".to_string(),
                    "@just-every/code@latest".to_string(),
                ],
                "bun install -g @just-every/code@latest".to_string(),
            ),
            UpdateAction::BrewUpgrade => (
                vec!["brew".to_string(), "upgrade".to_string(), "code".to_string()],
                "brew upgrade code".to_string(),
            ),
            UpdateAction::Custom(command) => {
                let display = try_join(command.iter().map(String::as_str))
                    .ok()
                    .unwrap_or_else(|| command.join(" "));
                (command.clone(), display)
            }
        }
    }
}

pub(crate) fn detect_update_action(config: &Config) -> Option<UpdateAction> {
    let managed_by_npm = std::env::var_os("CODEX_MANAGED_BY_NPM").is_some();
    let managed_by_bun = std::env::var_os("CODEX_MANAGED_BY_BUN").is_some();
    let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::new());

    detect_update_action_impl(
        cfg!(target_os = "macos"),
        &current_exe,
        managed_by_npm,
        managed_by_bun,
        has_on_path("brew"),
        config.tui.upgrade_command.clone(),
    )
}

fn detect_update_action_impl(
    is_macos: bool,
    current_exe: &Path,
    managed_by_npm: bool,
    managed_by_bun: bool,
    has_brew: bool,
    custom_upgrade_command: Vec<String>,
) -> Option<UpdateAction> {
    if !custom_upgrade_command.is_empty() {
        return Some(UpdateAction::Custom(custom_upgrade_command));
    }

    if managed_by_npm {
        return Some(UpdateAction::NpmGlobalLatest);
    }

    if managed_by_bun {
        return Some(UpdateAction::BunGlobalLatest);
    }

    if is_macos
        && has_brew
        && (current_exe.starts_with("/opt/homebrew") || current_exe.starts_with("/usr/local"))
    {
        return Some(UpdateAction::BrewUpgrade);
    }

    None
}

fn has_on_path(exe: &str) -> bool {
    let name = OsStr::new(exe);
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_override_wins_over_env_markers() {
        let custom = vec!["nix".to_string(), "profile".to_string(), "upgrade".to_string()];
        assert_eq!(
            detect_update_action_impl(
                false,
                Path::new("/any/path"),
                true,
                true,
                true,
                custom.clone()
            ),
            Some(UpdateAction::Custom(custom))
        );
    }

    #[test]
    fn npm_marker_precedes_bun_marker() {
        assert_eq!(
            detect_update_action_impl(false, Path::new("/any/path"), true, true, true, Vec::new()),
            Some(UpdateAction::NpmGlobalLatest)
        );
    }

    #[test]
    fn bun_marker_selects_bun_action() {
        assert_eq!(
            detect_update_action_impl(false, Path::new("/any/path"), false, true, true, Vec::new()),
            Some(UpdateAction::BunGlobalLatest)
        );
    }

    #[test]
    fn brew_upgrade_requires_macos_brew_and_homebrew_prefix() {
        assert_eq!(
            detect_update_action_impl(
                true,
                Path::new("/opt/homebrew/bin/code"),
                false,
                false,
                true,
                Vec::new()
            ),
            Some(UpdateAction::BrewUpgrade)
        );
        assert_eq!(
            detect_update_action_impl(
                true,
                Path::new("/usr/local/bin/code"),
                false,
                false,
                true,
                Vec::new()
            ),
            Some(UpdateAction::BrewUpgrade)
        );

        assert_eq!(
            detect_update_action_impl(
                true,
                Path::new("/opt/homebrew/bin/code"),
                false,
                false,
                false,
                Vec::new()
            ),
            None
        );
        assert_eq!(
            detect_update_action_impl(
                false,
                Path::new("/opt/homebrew/bin/code"),
                false,
                false,
                true,
                Vec::new()
            ),
            None
        );
    }
}

