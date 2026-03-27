use super::SettingsSection;

impl SettingsSection {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            SettingsSection::Model => "Model",
            SettingsSection::Theme => "Theme",
            SettingsSection::Interface => "Interface",
            SettingsSection::Experimental => "Experimental",
            SettingsSection::Shell => "Shell",
            SettingsSection::ShellProfiles => "Shell profiles",
            SettingsSection::ExecLimits => "Exec limits",
            SettingsSection::Planning => "Planning",
            SettingsSection::Updates => "Updates",
            SettingsSection::Accounts => "Accounts",
            SettingsSection::Apps => "Apps",
            SettingsSection::Agents => "Agents",
            SettingsSection::Memories => "Memories",
            SettingsSection::AutoDrive => "Auto Drive",
            SettingsSection::Review => "Review",
            SettingsSection::Validation => "Validation",
            SettingsSection::Limits => "Limits",
            SettingsSection::Chrome => "Chrome",
            SettingsSection::Mcp => "MCP",
            SettingsSection::JsRepl => "JS REPL",
            SettingsSection::Network => "Network",
            SettingsSection::Notifications => "Notifications",
            SettingsSection::Prompts => "Prompts",
            SettingsSection::Skills => "Skills",
            SettingsSection::Plugins => "Plugins",
        }
    }

    pub(crate) const fn help_line(self) -> &'static str {
        match self {
            SettingsSection::Model => "Choose the language model used for new completions.",
            SettingsSection::Theme => "Switch between preset color palettes and adjust contrast.",
            SettingsSection::Interface => "Control Settings UI routing and other layout preferences.",
            SettingsSection::Experimental => {
                "Toggle experimental features (saved to config.toml and applied after session reconfigure)."
            }
            SettingsSection::Shell => "Select the shell used for tool execution.",
            SettingsSection::ShellProfiles => {
                "Configure shell-style profiles (skills, references, MCP filters)."
            }
            SettingsSection::ExecLimits => {
                "Set process and memory limits for tool-spawned commands (Linux cgroups)."
            }
            SettingsSection::Planning => "Choose the model used in Plan Mode (Read Only).",
            SettingsSection::Updates => "Control CLI auto-update cadence and release channels.",
            SettingsSection::Accounts => {
                "Configure account switching behavior under rate and usage limits."
            }
            SettingsSection::Apps => {
                "Pin connector-source accounts and view connected apps (multi-account connectors)."
            }
            SettingsSection::Agents => "Configure linked agents and default task permissions.",
            SettingsSection::Memories => {
                "Manage scoped memory policies, generated artifacts, and prompt injection."
            }
            SettingsSection::AutoDrive => "Manage Auto Drive defaults for review and cadence.",
            SettingsSection::Review => "Adjust Auto Review and Auto Resolve automation for /review.",
            SettingsSection::Validation => "Toggle validation groups and tool availability.",
            SettingsSection::Limits => "Inspect API usage, rate limits, and reset windows.",
            SettingsSection::Chrome => "Connect to Chrome or switch browser integrations.",
            SettingsSection::Mcp => "Enable and manage local MCP servers for tooling.",
            SettingsSection::JsRepl => "Configure the optional js_repl tool runtime and paths.",
            SettingsSection::Network => "Configure managed network mediation and approvals.",
            SettingsSection::Notifications => {
                "Adjust desktop and terminal notification preferences."
            }
            SettingsSection::Prompts => "Create and edit custom prompt snippets.",
            SettingsSection::Skills => "Manage project-scoped and global skills.",
            SettingsSection::Plugins => "Browse and manage installed plugins and marketplaces.",
        }
    }

    pub(crate) const fn placeholder(self) -> &'static str {
        match self {
            SettingsSection::Model => "Model settings coming soon.",
            SettingsSection::Theme => "Theme settings coming soon.",
            SettingsSection::Interface => "Control Settings UI routing (overlay vs bottom pane).",
            SettingsSection::Experimental => "Toggle experimental features.",
            SettingsSection::Shell => "Select the shell used for tool execution.",
            SettingsSection::ShellProfiles => {
                "Configure shell-style profiles (skills, references, MCP filters)."
            }
            SettingsSection::ExecLimits => "Configure execution resource limits for tool commands.",
            SettingsSection::Planning => "Planning settings coming soon.",
            SettingsSection::Updates => "Upgrade Codex and manage automatic updates.",
            SettingsSection::Accounts => "Account switching settings coming soon.",
            SettingsSection::Apps => "Manage connector-source accounts and connected apps.",
            SettingsSection::Agents => "Agents configuration coming soon.",
            SettingsSection::Memories => {
                "Configure scoped Memories generation, pollution rules, and artifact controls."
            }
            SettingsSection::AutoDrive => "Auto Drive controls coming soon.",
            SettingsSection::Review => "Adjust Auto Review and Auto Resolve automation for /review.",
            SettingsSection::Validation => "Toggle validation groups and tools.",
            SettingsSection::Limits => "Limits usage visualization coming soon.",
            SettingsSection::Chrome => "Chrome integration settings coming soon.",
            SettingsSection::Mcp => "MCP server management coming soon.",
            SettingsSection::JsRepl => "Configure the js_repl tool runtime and paths.",
            SettingsSection::Network => "Configure managed network mediation for tool execution.",
            SettingsSection::Notifications => "Notification preferences coming soon.",
            SettingsSection::Prompts => "Manage custom prompts.",
            SettingsSection::Skills => "Manage skills.",
            SettingsSection::Plugins => "Manage plugins and marketplaces.",
        }
    }
}
