use super::*;

pub(super) fn debug_subagents_enabled() -> bool {
    match std::env::var("CODE_SUBAGENT_DEBUG") {
        Ok(val) => {
            let lower = val.to_ascii_lowercase();
            matches!(lower.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

pub(super) fn has_debug_flag(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--debug" || arg == "-d")
}

pub(crate) fn maybe_set_gemini_config_dir(env: &mut HashMap<String, String>, orig_home: Option<String>) {
    if env.get("GEMINI_API_KEY").is_some() {
        return;
    }

    let Some(home) = orig_home else { return; };
    let host_gem_cfg = std::path::PathBuf::from(&home).join(".gemini");
    if host_gem_cfg.is_dir() {
        env.insert(
            "GEMINI_CONFIG_DIR".to_string(),
            host_gem_cfg.to_string_lossy().to_string(),
        );
    }
}

pub(super) fn strip_model_flags(args: &mut Vec<String>) {
    let mut i = 0;
    while i < args.len() {
        let lowered = args[i].to_ascii_lowercase();
        if lowered == "--model" || lowered == "-m" {
            args.remove(i);
            if i < args.len() {
                args.remove(i);
            }
            continue;
        }
        if lowered.starts_with("--model=") || lowered.starts_with("-m=") {
            args.remove(i);
            continue;
        }
        i += 1;
    }
}

pub fn split_command_and_args(command: &str) -> (String, Vec<String>) {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return (String::new(), Vec::new());
    }
    if let Some(tokens) = shlex_split(trimmed)
        && let Some((first, rest)) = tokens.split_first() {
            return (first.clone(), rest.to_vec());
        }

    let tokens: Vec<String> = trimmed.split_whitespace().map(std::string::ToString::to_string).collect();
    if tokens.is_empty() {
        (String::new(), Vec::new())
    } else {
        let head = tokens[0].clone();
        (head, tokens[1..].to_vec())
    }
}
