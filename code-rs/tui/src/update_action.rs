use code_core::config::Config;
use shlex::try_join;

/// Update action the CLI should perform for a managed install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpdateAction {
    /// Update via a user-provided command override.
    Custom(Vec<String>),
}

impl UpdateAction {
    pub(crate) fn command_and_display(&self) -> (Vec<String>, String) {
        match self {
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
    detect_update_action_impl(config.tui.upgrade_command.clone())
}

fn detect_update_action_impl(custom_upgrade_command: Vec<String>) -> Option<UpdateAction> {
    if custom_upgrade_command.is_empty() {
        return None;
    }
    Some(UpdateAction::Custom(custom_upgrade_command))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_override_enables_update_action() {
        let custom = vec!["nix".to_string(), "profile".to_string(), "upgrade".to_string()];
        assert_eq!(
            detect_update_action_impl(custom.clone()),
            Some(UpdateAction::Custom(custom))
        );
    }

    #[test]
    fn empty_upgrade_command_disables_update_action() {
        assert_eq!(detect_update_action_impl(Vec::new()), None);
    }
}
