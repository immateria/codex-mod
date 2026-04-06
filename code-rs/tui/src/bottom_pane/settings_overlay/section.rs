#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    Model,
    Theme,
    Interface,
    Experimental,
    Shell,
    ShellEscalation,
    ShellProfiles,
    ExecLimits,
    Updates,
    Accounts,
    Secrets,
    Apps,
    Agents,
    Memories,
    Prompts,
    Skills,
    Plugins,
    AutoDrive,
    Review,
    Planning,
    Validation,
    Limits,
    #[cfg(feature = "browser-automation")]
    Chrome,
    Mcp,
    JsRepl,
    #[cfg(feature = "managed-network-proxy")]
    Network,
    Notifications,
}

impl SettingsSection {
    #[cfg(not(target_os = "android"))]
    pub(crate) const ALL: &[SettingsSection] = &[
        SettingsSection::Model,
        SettingsSection::Theme,
        SettingsSection::Interface,
        SettingsSection::Experimental,
        SettingsSection::Shell,
        SettingsSection::ShellEscalation,
        SettingsSection::ShellProfiles,
        SettingsSection::ExecLimits,
        SettingsSection::Updates,
        SettingsSection::Accounts,
        SettingsSection::Secrets,
        SettingsSection::Apps,
        SettingsSection::Agents,
        SettingsSection::Memories,
        SettingsSection::Prompts,
        SettingsSection::Skills,
        SettingsSection::Plugins,
        SettingsSection::AutoDrive,
        SettingsSection::Review,
        SettingsSection::Planning,
        SettingsSection::Validation,
        #[cfg(feature = "browser-automation")]
        SettingsSection::Chrome,
        SettingsSection::Mcp,
        SettingsSection::JsRepl,
        #[cfg(feature = "managed-network-proxy")]
        SettingsSection::Network,
        SettingsSection::Notifications,
        SettingsSection::Limits,
    ];

    #[cfg(target_os = "android")]
    pub(crate) const ALL: &[SettingsSection] = &[
        SettingsSection::Model,
        SettingsSection::Theme,
        SettingsSection::Interface,
        SettingsSection::Experimental,
        SettingsSection::Shell,
        SettingsSection::ShellEscalation,
        SettingsSection::ShellProfiles,
        SettingsSection::ExecLimits,
        SettingsSection::Updates,
        SettingsSection::Accounts,
        SettingsSection::Secrets,
        SettingsSection::Apps,
        SettingsSection::Agents,
        SettingsSection::Memories,
        SettingsSection::Prompts,
        SettingsSection::Skills,
        SettingsSection::Plugins,
        SettingsSection::AutoDrive,
        SettingsSection::Review,
        SettingsSection::Planning,
        SettingsSection::Validation,
        SettingsSection::Mcp,
        SettingsSection::JsRepl,
        #[cfg(feature = "managed-network-proxy")]
        SettingsSection::Network,
        SettingsSection::Notifications,
        SettingsSection::Limits,
    ];

    /// Whether this section should appear in the settings sidebar given the
    /// current feature flags. Sections tied to experimental features are hidden
    /// when the feature is disabled — the user can enable them via the
    /// Experimental settings page, after which the section appears.
    pub(crate) fn is_visible(&self, features: &code_core::config_types::FeaturesToml) -> bool {
        match self {
            SettingsSection::JsRepl => features.enabled("js_repl"),
            SettingsSection::Apps => features.enabled("apps"),
            _ => true,
        }
    }
}
