//! Shell presets for the shell selection UI.
//! 
//! This module provides built-in shell definitions that can be extended
//! or overridden by user configuration.

use once_cell::sync::Lazy;

/// Metadata describing a shell option.
#[derive(Debug, Clone)]
pub struct ShellPreset {
    /// Unique identifier for the shell
    pub id: String,
    /// The shell command/binary name (e.g., "zsh", "bash")
    pub command: String,
    /// Display name shown in the UI
    pub display_name: String,
    /// Description of the shell
    pub description: String,
    /// Default arguments to pass to the shell
    pub default_args: Vec<String>,
    /// Preferred shell scripting style for model-generated commands.
    pub script_style: Option<String>,
    /// Whether this shell should be shown in the picker
    pub show_in_picker: bool,
}

/// Built-in shell presets. These cover the most common shells across platforms.
static BUILTIN_PRESETS: Lazy<Vec<ShellPreset>> = Lazy::new(|| {
    vec![
        ShellPreset {
            id: "zsh".to_owned(),
            command: "zsh".to_owned(),
            display_name: "Zsh".to_owned(),
            description: "Z shell - modern shell with powerful features".to_owned(),
            default_args: vec![],
            script_style: Some("zsh".to_owned()),
            show_in_picker: true,
        },
        ShellPreset {
            id: "bash".to_owned(),
            command: "bash".to_owned(),
            display_name: "Bash".to_owned(),
            description: "Bourne Again Shell - most common Unix shell".to_owned(),
            default_args: vec![],
            script_style: Some("bash-zsh-compatible".to_owned()),
            show_in_picker: true,
        },
        ShellPreset {
            id: "fish".to_owned(),
            command: "fish".to_owned(),
            display_name: "Fish".to_owned(),
            description: "Friendly Interactive Shell - modern shell with great defaults".to_owned(),
            default_args: vec!["-c".into()],
            script_style: Some("fish".to_owned()),
            show_in_picker: true,
        },
        ShellPreset {
            id: "nushell".to_owned(),
            command: "nu".to_owned(),
            display_name: "Nushell".to_owned(),
            description: "Nu shell - modern shell with structured data".to_owned(),
            default_args: vec!["-c".into()],
            script_style: Some("nushell".to_owned()),
            show_in_picker: true,
        },
        ShellPreset {
            id: "elvish".to_owned(),
            command: "elvish".to_owned(),
            display_name: "Elvish".to_owned(),
            description: "Elvish - expressive programming language and shell".to_owned(),
            default_args: vec!["-c".into()],
            script_style: Some("elvish".to_owned()),
            show_in_picker: true,
        },
        ShellPreset {
            id: "powershell".to_owned(),
            command: "pwsh".to_owned(),
            display_name: "PowerShell".to_owned(),
            description: "PowerShell Core - cross-platform automation shell".to_owned(),
            default_args: vec![],
            script_style: None,
            show_in_picker: true,
        },
        ShellPreset {
            id: "dash".to_owned(),
            command: "dash".to_owned(),
            display_name: "Dash".to_owned(),
            description: "Debian Almquist Shell - lightweight POSIX shell".to_owned(),
            default_args: vec!["-c".into()],
            script_style: Some("posix-sh".to_owned()),
            show_in_picker: true,
        },
        ShellPreset {
            id: "sh".to_owned(),
            command: "sh".to_owned(),
            display_name: "Sh".to_owned(),
            description: "Bourne Shell - original Unix shell".to_owned(),
            default_args: vec!["-c".into()],
            script_style: Some("posix-sh".to_owned()),
            show_in_picker: true,
        },
        ShellPreset {
            id: "xonsh".to_owned(),
            command: "xonsh".to_owned(),
            display_name: "Xonsh".to_owned(),
            description: "Xonsh - Python-powered shell".to_owned(),
            default_args: vec!["-c".into()],
            script_style: Some("xonsh".to_owned()),
            show_in_picker: true,
        },
        ShellPreset {
            id: "oil".to_owned(),
            command: "osh".to_owned(),
            display_name: "Oil Shell".to_owned(),
            description: "Oil shell - our upgrade path from bash".to_owned(),
            default_args: vec!["-c".into()],
            script_style: Some("oil".to_owned()),
            show_in_picker: true,
        },
    ]
});

/// Returns a reference to the built-in shell presets.
pub fn builtin_shell_presets() -> &'static [ShellPreset] {
    &BUILTIN_PRESETS
}

/// Merges user-defined shell presets with built-in ones.
/// User presets with the same ID override built-in presets.
/// User presets with new IDs are appended.
pub fn merge_shell_presets(user_presets: Vec<ShellPreset>) -> Vec<ShellPreset> {
    let mut result = BUILTIN_PRESETS.clone();
    
    for user_preset in user_presets {
        if let Some(idx) = result.iter().position(|p| p.id == user_preset.id) {
            // Override existing preset
            result[idx] = user_preset;
        } else {
            // Add new preset
            result.push(user_preset);
        }
    }
    
    result
}

/// Filter presets to only those that should be shown in the picker.
pub fn picker_shell_presets(presets: &[ShellPreset]) -> Vec<ShellPreset> {
    presets
        .iter()
        .filter(|p| p.show_in_picker)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_presets_not_empty() {
        let presets = builtin_shell_presets();
        assert!(!presets.is_empty());
    }

    #[test]
    fn test_merge_override() {
        let user = vec![ShellPreset {
            id: "zsh".to_string(),
            command: "zsh".to_string(),
            display_name: "My Custom Zsh".to_string(),
            description: "Custom description".to_string(),
            default_args: vec!["-i".to_string()],
            script_style: Some("zsh".to_string()),
            show_in_picker: true,
        }];
        
        let merged = merge_shell_presets(user);
        let zsh = merged.iter().find(|p| p.id == "zsh").unwrap();
        assert_eq!(zsh.display_name, "My Custom Zsh");
    }

    #[test]
    fn test_merge_add_new() {
        let user = vec![ShellPreset {
            id: "custom".to_string(),
            command: "my-shell".to_string(),
            display_name: "My Shell".to_string(),
            description: "A custom shell".to_string(),
            default_args: vec![],
            script_style: None,
            show_in_picker: true,
        }];
        
        let merged = merge_shell_presets(user);
        assert!(merged.iter().any(|p| p.id == "custom"));
    }
}
