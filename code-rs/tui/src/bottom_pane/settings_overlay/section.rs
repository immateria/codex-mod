#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    Model,
    Theme,
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
    Notifications,
}

impl SettingsSection {
    pub(crate) const ALL: [SettingsSection; 15] = [
        SettingsSection::Model,
        SettingsSection::Theme,
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
        SettingsSection::Notifications,
        SettingsSection::Limits,
    ];
}
