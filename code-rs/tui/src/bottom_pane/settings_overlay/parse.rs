use super::SettingsSection;

impl SettingsSection {
    pub(crate) fn from_hint(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "model" | "models" => Some(SettingsSection::Model),
            "skill" | "skills" => Some(SettingsSection::Skills),
            "theme" | "themes" => Some(SettingsSection::Theme),
            "planning" | "plan" => Some(SettingsSection::Planning),
            "update" | "updates" => Some(SettingsSection::Updates),
            "account" | "accounts" | "auth" => Some(SettingsSection::Accounts),
            "agent" | "agents" => Some(SettingsSection::Agents),
            "auto" | "autodrive" | "drive" => Some(SettingsSection::AutoDrive),
            "review" | "reviews" => Some(SettingsSection::Review),
            "validation" | "validate" => Some(SettingsSection::Validation),
            "limit" | "limits" | "usage" => Some(SettingsSection::Limits),
            "chrome" | "browser" => Some(SettingsSection::Chrome),
            "mcp" => Some(SettingsSection::Mcp),
            "network" | "net" | "proxy" => Some(SettingsSection::Network),
            "notification" | "notifications" | "notify" | "notif" => {
                Some(SettingsSection::Notifications)
            }
            _ => None,
        }
    }
}
