//! Wrapper parsing and invocation classification utilities.
//!
//! This module centralizes:
//! - prefix wrapper peeling (`env`, `sudo`)
//! - script wrapper detection (`<shell> -c/-lc`, `nu -c/-lc`, `elvish -c`, `cmd /c`, `pwsh -Command`)
//! - conservative parsing of "word-only" command sequences from script text
//!
//! Consumers (safety checks, canonicalization, summaries) should prefer
//! `classify()` and match on `Invocation` rather than duplicating wrapper
//! detection.

mod cmd_script;
mod prefix;
mod word_only;
mod wrappers;

pub(crate) use cmd_script::split_cmd_script_into_segments;
pub(crate) use word_only::parse_word_only_commands;
pub(crate) use word_only::parse_word_only_commands_with_fallback;
pub(crate) use wrappers::extract_script_wrapper;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ScriptWrapperFamily {
    PosixLike,
    Nushell,
    Elvish,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptWrapper {
    pub(crate) family: ScriptWrapperFamily,
    pub(crate) mode_flag: String,
    pub(crate) script: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrefixWrappers {
    pub(crate) sudo: bool,
    pub(crate) env: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Invocation {
    Argv(Vec<String>),

    ScriptWrapper {
        family: ScriptWrapperFamily,
        mode_flag: String,
        script: String,
    },

    PowerShellScript { script: String },

    CmdScript { mode: String, script: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClassifiedInvocation {
    pub(crate) prefix: PrefixWrappers,
    pub(crate) peeled_argv: Vec<String>,
    pub(crate) invocation: Invocation,
}

pub(crate) fn classify(command: &[String]) -> ClassifiedInvocation {
    let (prefix, peeled) = prefix::peel_prefix_wrappers(command);

    let invocation = if let Some(wrapper) = wrappers::extract_script_wrapper(&peeled) {
        Invocation::ScriptWrapper {
            family: wrapper.family,
            mode_flag: wrapper.mode_flag,
            script: wrapper.script,
        }
    } else if let Some(script) = wrappers::extract_powershell_script(&peeled) {
        Invocation::PowerShellScript { script }
    } else if let Some((mode, script)) = wrappers::extract_cmd_wrapper(&peeled) {
        Invocation::CmdScript { mode, script }
    } else {
        Invocation::Argv(peeled.clone())
    };

    ClassifiedInvocation {
        prefix,
        peeled_argv: peeled,
        invocation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_str(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn classify_peels_env_and_sudo() {
        let classified = classify(&vec_str(&["env", "-i", "FOO=bar", "sudo", "ls"]));
        assert!(classified.prefix.env);
        assert!(classified.prefix.sudo);
        assert_eq!(classified.peeled_argv, vec_str(&["ls"]));
        assert_eq!(classified.invocation, Invocation::Argv(vec_str(&["ls"])));
    }

    #[test]
    fn classify_detects_nushell_script_wrapper() {
        let classified = classify(&vec_str(&["nu", "-c", "ls"]));
        assert_eq!(
            classified.invocation,
            Invocation::ScriptWrapper {
                family: ScriptWrapperFamily::Nushell,
                mode_flag: "-c".to_string(),
                script: "ls".to_string(),
            }
        );
    }

    #[test]
    fn classify_detects_cmd_wrapper() {
        let classified = classify(&vec_str(&["cmd", "/c", "dir"]));
        assert_eq!(
            classified.invocation,
            Invocation::CmdScript {
                mode: "/c".to_string(),
                script: "dir".to_string(),
            }
        );
    }

    #[test]
    fn classify_detects_powershell_wrapper() {
        let classified = classify(&vec_str(&["pwsh", "-Command", "Get-ChildItem"]));
        assert_eq!(
            classified.invocation,
            Invocation::PowerShellScript {
                script: "Get-ChildItem".to_string(),
            }
        );
    }
}
