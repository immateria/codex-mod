#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    Model,
    Theme,
    Interface,
    Shell,
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
    pub(crate) const ALL: [SettingsSection; 18] = [
        SettingsSection::Model,
        SettingsSection::Theme,
        SettingsSection::Interface,
        SettingsSection::Shell,
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
}
