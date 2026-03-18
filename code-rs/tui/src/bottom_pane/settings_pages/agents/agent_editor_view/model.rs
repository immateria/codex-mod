use crate::app_event_sender::AppEventSender;
use crate::components::form_text_field::{FormTextField, InputFilter};

#[cfg(target_os = "macos")]
use crate::agent_install_helpers::macos_brew_formula_for_command;

#[derive(Debug)]
pub(crate) struct AgentEditorView {
    pub(super) name: String,
    pub(super) name_field: FormTextField,
    pub(super) name_editable: bool,
    pub(super) enabled: bool,
    pub(super) command: String,
    pub(super) command_field: FormTextField,
    pub(super) params_ro: FormTextField,
    pub(super) params_wr: FormTextField,
    pub(super) description_field: FormTextField,
    pub(super) instr: FormTextField,
    pub(super) field: usize, // see FIELD_* constants below
    pub(super) complete: bool,
    pub(super) app_event_tx: AppEventSender,
    pub(super) installed: bool,
    pub(super) install_hint: String,
    pub(super) description_error: Option<String>,
    pub(super) name_error: Option<String>,
}

pub(crate) struct AgentEditorInit {
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) args_read_only: Option<Vec<String>>,
    pub(crate) args_write: Option<Vec<String>>,
    pub(crate) instructions: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) command: String,
    pub(crate) builtin: bool,
    pub(crate) app_event_tx: AppEventSender,
}

// These indices define the deterministic focus order for arrow/tab navigation.
// Keep them aligned with the visual layout: ID -> Command -> Status -> params...
pub(super) const FIELD_NAME: usize = 0;
pub(super) const FIELD_COMMAND: usize = 1;
pub(super) const FIELD_TOGGLE: usize = 2;
pub(super) const FIELD_READ_ONLY: usize = 3;
pub(super) const FIELD_WRITE: usize = 4;
pub(super) const FIELD_DESCRIPTION: usize = 5;
pub(super) const FIELD_INSTRUCTIONS: usize = 6;
pub(super) const FIELD_SAVE: usize = 7;
pub(super) const FIELD_CANCEL: usize = 8;

impl AgentEditorView {
    pub fn new(init: AgentEditorInit) -> Self {
        let AgentEditorInit {
            name,
            enabled,
            args_read_only,
            args_write,
            instructions,
            description,
            command,
            builtin,
            app_event_tx,
        } = init;
        // Simple PATH check similar to the core executor’s logic
        fn command_exists(cmd: &str) -> bool {
            if cmd.contains(std::path::MAIN_SEPARATOR) || cmd.contains('/') || cmd.contains('\\') {
                return std::fs::metadata(cmd).map(|m| m.is_file()).unwrap_or(false);
            }
            #[cfg(target_os = "windows")]
            {
                if let Ok(p) = which::which(cmd) {
                    if !p.is_file() {
                        return false;
                    }
                    match p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                        Some(ext) if matches!(ext.as_str(), "exe" | "com" | "cmd" | "bat") => true,
                        _ => false,
                    }
                } else {
                    false
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let Some(path_os) = std::env::var_os("PATH") else {
                    return false;
                };
                for dir in std::env::split_paths(&path_os) {
                    if dir.as_os_str().is_empty() {
                        continue;
                    }
                    let candidate = dir.join(cmd);
                    if let Ok(meta) = std::fs::metadata(&candidate)
                        && meta.is_file()
                        && meta.permissions().mode() & 0o111 != 0
                    {
                        return true;
                    }
                }
                false
            }
        }

        let name_editable = name.is_empty();
        let mut name_field = FormTextField::new_single_line();
        name_field.set_text(&name);
        name_field.set_filter(InputFilter::Id);
        let mut command_field = FormTextField::new_single_line();
        command_field.set_text(&command);
        let command_exists_flag = builtin || (!command.trim().is_empty() && command_exists(&command));
        let mut description_field = FormTextField::new_multi_line();
        if let Some(desc) = description
            .as_ref()
            .map(|d| d.trim())
            .filter(|value| !value.is_empty())
        {
            description_field.set_text(desc);
            description_field.move_cursor_to_start();
        }
        let mut v = Self {
            name,
            name_field,
            name_editable,
            enabled,
            command: command.clone(),
            command_field,
            params_ro: FormTextField::new_multi_line(),
            params_wr: FormTextField::new_multi_line(),
            description_field,
            instr: FormTextField::new_multi_line(),
            field: if name_editable { FIELD_NAME } else { FIELD_TOGGLE },
            complete: false,
            app_event_tx,
            installed: command_exists_flag,
            install_hint: String::new(),
            description_error: None,
            name_error: None,
        };

        if let Some(ro) = args_read_only {
            v.params_ro.set_text(&ro.join(" "));
        }
        if let Some(wr) = args_write {
            v.params_wr.set_text(&wr.join(" "));
        }
        if let Some(s) = instructions {
            v.instr.set_text(&s);
            v.instr.move_cursor_to_start();
        }

        // OS-specific short hint
        if !builtin && !v.command.trim().is_empty() {
            #[cfg(target_os = "macos")]
            {
                let brew_formula = macos_brew_formula_for_command(&v.command);
                v.install_hint = format!(
                    "'{}' not found. On macOS, try Homebrew (brew install {brew_formula}) or consult the agent's docs.",
                    v.command
                );
            }
            #[cfg(target_os = "linux")]
            {
                v.install_hint = format!(
                    "'{}' not found. On Linux, install via your package manager or consult the agent's docs.",
                    v.command
                );
            }
            #[cfg(target_os = "windows")]
            {
                v.install_hint = format!(
                    "'{}' not found. On Windows, install the CLI from the vendor site and ensure it’s on PATH.",
                    v.command
                );
            }
        }

        v
    }
}

