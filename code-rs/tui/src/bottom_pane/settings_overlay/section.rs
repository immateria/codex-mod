#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    Model,
    Theme,
    Interface,
    Shell,
    ShellProfiles,
    Updates,
    Accounts,
    Agents,
    Prompts,
    Skills,
    AutoDrive,
    Review,
    Planning,
    Validation,
    Limits,
    Chrome,
    Mcp,
    Network,
    Notifications,
}

impl SettingsSection {
    #[cfg(not(target_os = "android"))]
    pub(crate) const ALL: [SettingsSection; 19] = [
        SettingsSection::Model,
        SettingsSection::Theme,
        SettingsSection::Interface,
        SettingsSection::Shell,
        SettingsSection::ShellProfiles,
        SettingsSection::Updates,
        SettingsSection::Accounts,
        SettingsSection::Agents,
        SettingsSection::Prompts,
        SettingsSection::Skills,
        SettingsSection::AutoDrive,
        SettingsSection::Review,
        SettingsSection::Planning,
        SettingsSection::Validation,
        SettingsSection::Chrome,
        SettingsSection::Mcp,
        SettingsSection::Network,
        SettingsSection::Notifications,
        SettingsSection::Limits,
    ];

    #[cfg(target_os = "android")]
    pub(crate) const ALL: [SettingsSection; 18] = [
        SettingsSection::Model,
        SettingsSection::Theme,
        SettingsSection::Interface,
        SettingsSection::Shell,
        SettingsSection::ShellProfiles,
        SettingsSection::Updates,
        SettingsSection::Accounts,
        SettingsSection::Agents,
        SettingsSection::Prompts,
        SettingsSection::Skills,
        SettingsSection::AutoDrive,
        SettingsSection::Review,
        SettingsSection::Planning,
        SettingsSection::Validation,
        SettingsSection::Mcp,
        SettingsSection::Network,
        SettingsSection::Notifications,
        SettingsSection::Limits,
    ];
}
