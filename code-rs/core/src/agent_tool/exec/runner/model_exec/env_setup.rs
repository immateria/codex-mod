use super::*;

pub(super) fn build_agent_env(
    config: Option<&AgentConfig>,
    debug_subagent: bool,
    child_log_tag: Option<&str>,
    use_current_exe: bool,
    family: &str,
    source_kind: Option<&AgentSourceKind>,
) -> std::collections::HashMap<String, String> {
    // Build env from current process then overlay any config-provided vars.
    let mut env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let orig_home: Option<String> = env.get("HOME").cloned();
    if let Some(cfg) = config
        && let Some(cfg_env) = cfg.env.as_ref()
    {
        for (key, value) in cfg_env {
            env.insert(key.clone(), value.clone());
        }
    }

    if debug_subagent {
        env.entry("CODE_SUBAGENT_DEBUG".to_string())
            .or_insert_with(|| "1".to_string());
        if let Some(tag) = child_log_tag {
            env.insert("CODE_DEBUG_LOG_TAG".to_string(), tag.to_string());
        }
    }

    // Tag OpenAI requests originating from agent runs so server-side telemetry
    // can distinguish subagent traffic.
    if use_current_exe || family == "codex" || family == "code" {
        let subagent = match source_kind {
            Some(AgentSourceKind::AutoReview) => "review",
            _ => "agent",
        };
        env.entry("CODE_OPENAI_SUBAGENT".to_string())
            .or_insert_with(|| subagent.to_string());
    }

    // Convenience: map common key names so external CLIs "just work".
    if let Some(google_key) = env.get("GOOGLE_API_KEY").cloned() {
        env.entry("GEMINI_API_KEY".to_string()).or_insert(google_key);
    }
    if let Some(claude_key) = env.get("CLAUDE_API_KEY").cloned() {
        env.entry("ANTHROPIC_API_KEY".to_string())
            .or_insert(claude_key);
    }
    if let Some(anthropic_key) = env.get("ANTHROPIC_API_KEY").cloned() {
        env.entry("CLAUDE_API_KEY".to_string())
            .or_insert(anthropic_key);
    }
    if let Some(anthropic_base) = env.get("ANTHROPIC_BASE_URL").cloned() {
        env.entry("CLAUDE_BASE_URL".to_string())
            .or_insert(anthropic_base);
    }
    // Qwen/DashScope convenience: mirror API keys and base URLs both ways so
    // either variable name works across tools.
    if let Some(qwen_key) = env.get("QWEN_API_KEY").cloned() {
        env.entry("DASHSCOPE_API_KEY".to_string()).or_insert(qwen_key);
    }
    if let Some(dashscope_key) = env.get("DASHSCOPE_API_KEY").cloned() {
        env.entry("QWEN_API_KEY".to_string())
            .or_insert(dashscope_key);
    }
    if let Some(qwen_base) = env.get("QWEN_BASE_URL").cloned() {
        env.entry("DASHSCOPE_BASE_URL".to_string())
            .or_insert(qwen_base);
    }
    if let Some(ds_base) = env.get("DASHSCOPE_BASE_URL").cloned() {
        env.entry("QWEN_BASE_URL".to_string()).or_insert(ds_base);
    }
    if family == "qwen" {
        env.insert("OPENAI_API_KEY".to_string(), String::new());
    }

    // Reduce startup overhead for Claude CLI: disable auto-updater/telemetry.
    env.entry("DISABLE_AUTOUPDATER".to_string())
        .or_insert("1".to_string());
    env.entry("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string())
        .or_insert("1".to_string());
    env.entry("DISABLE_ERROR_REPORTING".to_string())
        .or_insert("1".to_string());

    // If GEMINI_API_KEY not provided, try pointing to host config for read-only
    // discovery (Gemini CLI supports GEMINI_CONFIG_DIR). We keep HOME as-is so
    // CLIs that require ~/.gemini and ~/.claude continue to work with your
    // existing config.
    maybe_set_gemini_config_dir(&mut env, orig_home);

    env
}
