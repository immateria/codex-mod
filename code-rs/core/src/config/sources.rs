use crate::config_loader::{load_config_as_toml_blocking, LoaderOverrides};
use crate::config_types::{
    AutoDriveContinueMode,
    AutoDriveSettings,
    CachedTerminalBackground,
    LimitsLayoutMode,
    MemoriesConfig,
    MemoriesToml,
    McpDispatchMode,
    McpServerConfig,
    McpServerSchedulingToml,
    McpServerTransportConfig,
    McpToolSchedulingOverrideToml,
    ReasoningEffort,
    SettingsMenuConfig,
    SettingsMenuOpenMode,
    ShellConfig,
    ShellScriptStyle,
    StatusLineLane,
    ThemeColors,
    ThemeName,
    WindowsSandboxModeToml,
};
use crate::protocol::{ApprovedCommandMatchKind, AskForApproval};
use code_protocol::config_types::SandboxMode;
use dirs::home_dir;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::NamedTempFile;
use toml::Value as TomlValue;
use toml_edit::Array as TomlArray;
use toml_edit::ArrayOfTables as TomlArrayOfTables;
use toml_edit::DocumentMut;
use toml_edit::Item as TomlItem;
use toml_edit::Table as TomlTable;
use which::which;

use super::CONFIG_TOML_FILE;

pub fn load_config_as_toml(code_home: &Path) -> std::io::Result<TomlValue> {
    load_config_as_toml_blocking(code_home, LoaderOverrides::default())
}

pub fn load_global_mcp_servers(
    code_home: &Path,
) -> std::io::Result<BTreeMap<String, McpServerConfig>> {
    let root_value = load_config_as_toml(code_home)?;
    let Some(servers_value) = root_value.get("mcp_servers") else {
        return Ok(BTreeMap::new());
    };

    let servers: BTreeMap<String, McpServerConfig> = servers_value
        .clone()
        .try_into()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    for (name, cfg) in &servers {
        if let McpServerTransportConfig::Stdio { command, .. } = &cfg.transport {
            let command_looks_like_path = {
                let path = Path::new(command);
                path.components().count() > 1 || path.is_absolute()
            };
            if !command_looks_like_path && which(command).is_err() {
                let msg = format!(
                    "MCP server `{name}` command `{command}` not found on PATH. If the server is an npm package, set command = \"npx\" and keep the package name in args."
                );
                return Err(std::io::Error::new(ErrorKind::NotFound, msg));
            }
        }
    }

    Ok(servers)
}

pub fn write_global_mcp_servers(
    code_home: &Path,
    servers: &BTreeMap<String, McpServerConfig>,
) -> std::io::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents
            .parse::<DocumentMut>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e),
    };

    doc.as_table_mut().remove("mcp_servers");

    if !servers.is_empty() {
        let mut table = TomlTable::new();
        table.set_implicit(true);
        doc["mcp_servers"] = TomlItem::Table(table);

        for (name, config) in servers {
            let mut entry = TomlTable::new();
            entry.set_implicit(false);
            match &config.transport {
                McpServerTransportConfig::Stdio { command, args, env } => {
                    entry["command"] = toml_edit::value(command.clone());

                    if !args.is_empty() {
                        let mut args_array = TomlArray::new();
                        for arg in args {
                            args_array.push(arg.clone());
                        }
                        entry["args"] = TomlItem::Value(args_array.into());
                    }

                    if let Some(env) = env
                        && !env.is_empty()
                    {
                        let mut env_table = TomlTable::new();
                        env_table.set_implicit(false);
                        let mut pairs: Vec<_> = env.iter().collect();
                        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (key, value) in pairs {
                            env_table.insert(key, toml_edit::value(value.clone()));
                        }
                        entry["env"] = TomlItem::Table(env_table);
                    }
                }
                McpServerTransportConfig::StreamableHttp {
                    url,
                    bearer_token,
                    oauth_resource,
                    bearer_token_env_var,
                    http_headers,
                    env_http_headers,
                } => {
                    entry["url"] = toml_edit::value(url.clone());
                    if let Some(token) = bearer_token {
                        entry["bearer_token"] = toml_edit::value(token.clone());
                    }
                    if let Some(resource) = oauth_resource {
                        entry["oauth_resource"] = toml_edit::value(resource.clone());
                    }
                    if let Some(env_var) = bearer_token_env_var {
                        entry["bearer_token_env_var"] = toml_edit::value(env_var.clone());
                    }

                    if let Some(http_headers) = http_headers
                        && !http_headers.is_empty()
                    {
                        let mut headers_table = TomlTable::new();
                        headers_table.set_implicit(false);
                        let mut pairs: Vec<_> = http_headers.iter().collect();
                        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (key, value) in pairs {
                            headers_table.insert(key, toml_edit::value(value.clone()));
                        }
                        entry["http_headers"] = TomlItem::Table(headers_table);
                    }

                    if let Some(env_http_headers) = env_http_headers
                        && !env_http_headers.is_empty()
                    {
                        let mut headers_table = TomlTable::new();
                        headers_table.set_implicit(false);
                        let mut pairs: Vec<_> = env_http_headers.iter().collect();
                        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (key, value) in pairs {
                            headers_table.insert(key, toml_edit::value(value.clone()));
                        }
                        entry["env_http_headers"] = TomlItem::Table(headers_table);
                    }
                }
            }

            if let Some(timeout) = config.startup_timeout_sec {
                entry["startup_timeout_sec"] = toml_edit::value(timeout.as_secs_f64());
            }

            if let Some(timeout) = config.tool_timeout_sec {
                entry["tool_timeout_sec"] = toml_edit::value(timeout.as_secs_f64());
            }

            if !config.disabled_tools.is_empty() {
                let mut disabled = config.disabled_tools.clone();
                disabled.sort();
                disabled.dedup();

                let mut arr = TomlArray::new();
                for tool in disabled {
                    arr.push(toml_edit::Value::from(tool));
                }
                entry["disabled_tools"] = TomlItem::Value(toml_edit::Value::Array(arr));
            }

            let default_scheduling = McpServerSchedulingToml::default();
            if config.scheduling != default_scheduling {
                let mut sched_table = TomlTable::new();
                sched_table.set_implicit(false);
                if config.scheduling.dispatch != default_scheduling.dispatch {
                    sched_table["dispatch"] = toml_edit::value(config.scheduling.dispatch.to_string());
                }
                if config.scheduling.max_concurrent != default_scheduling.max_concurrent {
                    sched_table["max_concurrent"] = toml_edit::value(config.scheduling.max_concurrent as i64);
                }
                if let Some(duration) = config.scheduling.min_interval_sec {
                    sched_table["min_interval_sec"] = toml_edit::value(duration.as_secs_f64());
                }
                if let Some(duration) = config.scheduling.queue_timeout_sec {
                    sched_table["queue_timeout_sec"] = toml_edit::value(duration.as_secs_f64());
                }
                if let Some(depth) = config.scheduling.max_queue_depth {
                    sched_table["max_queue_depth"] = toml_edit::value(depth as i64);
                }
                entry["scheduling"] = TomlItem::Table(sched_table);
            }

            if !config.tool_scheduling.is_empty() {
                let mut tool_sched_table = TomlTable::new();
                tool_sched_table.set_implicit(false);
                for (tool, override_cfg) in &config.tool_scheduling {
                    if override_cfg.is_empty() {
                        continue;
                    }
                    let mut override_tbl = TomlTable::new();
                    override_tbl.set_implicit(false);
                    if let Some(max) = override_cfg.max_concurrent {
                        override_tbl["max_concurrent"] = toml_edit::value(max as i64);
                    }
                    if let Some(duration) = override_cfg.min_interval_sec {
                        override_tbl["min_interval_sec"] = toml_edit::value(duration.as_secs_f64());
                    }
                    tool_sched_table[tool.as_str()] = TomlItem::Table(override_tbl);
                }
                if !tool_sched_table.is_empty() {
                    entry["tool_scheduling"] = TomlItem::Table(tool_sched_table);
                }
            }

            doc["mcp_servers"][name.as_str()] = TomlItem::Table(entry);
        }
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path).map_err(|err| err.error)?;

    Ok(())
}

/// Persist the currently active model selection back to `config.toml` so that it
/// becomes the default for future sessions.
pub async fn persist_model_selection(
    code_home: &Path,
    profile: Option<&str>,
    model: &str,
    effort: Option<ReasoningEffort>,
    preferred_effort: Option<ReasoningEffort>,
) -> anyhow::Result<()> {
    use tokio::fs;

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let existing = match fs::read_to_string(&read_path).await {
        Ok(raw) => Some(raw),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => return Err(err.into()),
    };

    let mut doc = match existing {
        Some(raw) if raw.trim().is_empty() => DocumentMut::new(),
        Some(raw) => raw
            .parse::<DocumentMut>()
            .map_err(|e| anyhow::anyhow!("failed to parse config.toml: {e}"))?,
        None => DocumentMut::new(),
    };

    {
        let root = doc.as_table_mut();
        if let Some(profile_name) = profile {
            let profiles_item = root
                .entry("profiles")
                .or_insert_with(|| {
                    let mut table = TomlTable::new();
                    table.set_implicit(true);
                    TomlItem::Table(table)
                });

            let Some(profiles_table) = profiles_item.as_table_mut() else {
                return Err(anyhow::anyhow!("profiles table should be a table"));
            };

            let profile_item = profiles_table
                .entry(profile_name)
                .or_insert_with(|| {
                    let mut table = TomlTable::new();
                    table.set_implicit(false);
                    TomlItem::Table(table)
                });

            let Some(profile_table) = profile_item.as_table_mut() else {
                return Err(anyhow::anyhow!("profile entry should be a table"));
            };

            profile_table["model"] = toml_edit::value(model.to_string());

            if let Some(effort) = effort {
                profile_table["model_reasoning_effort"] =
                    toml_edit::value(effort.to_string());
            } else {
                profile_table.remove("model_reasoning_effort");
            }

            if let Some(preferred) = preferred_effort {
                profile_table["preferred_model_reasoning_effort"] =
                    toml_edit::value(preferred.to_string());
            } else {
                profile_table.remove("preferred_model_reasoning_effort");
            }
        } else {
            root["model"] = toml_edit::value(model.to_string());
            match effort {
                Some(effort) => {
                    root["model_reasoning_effort"] =
                        toml_edit::value(effort.to_string());
                }
                None => {
                    root.remove("model_reasoning_effort");
                }
            }

            match preferred_effort {
                Some(preferred) => {
                    root["preferred_model_reasoning_effort"] =
                        toml_edit::value(preferred.to_string());
                }
                None => {
                    root.remove("preferred_model_reasoning_effort");
                }
            }
        }
    }

    fs::create_dir_all(code_home).await?;
    let tmp_path = config_path.with_extension("tmp");
    fs::write(&tmp_path, doc.to_string()).await?;
    fs::rename(&tmp_path, &config_path).await?;

    Ok(())
}

/// Persist the shell setting back to `config.toml`.
pub async fn persist_shell(code_home: &Path, shell: Option<&ShellConfig>) -> anyhow::Result<()> {
    use tokio::fs;

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let existing = match fs::read_to_string(&read_path).await {
        Ok(raw) => Some(raw),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => return Err(err.into()),
    };

    let mut doc = match existing {
        Some(raw) if raw.trim().is_empty() => DocumentMut::new(),
        Some(raw) => raw
            .parse::<DocumentMut>()
            .map_err(|e| anyhow::anyhow!("failed to parse config.toml: {e}"))?,
        None => DocumentMut::new(),
    };

    {
        let root = doc.as_table_mut();
        if let Some(shell_config) = shell {
            let shell_table = root
                .entry("shell")
                .or_insert_with(|| {
                    let mut table = TomlTable::new();
                    table.set_implicit(false);
                    TomlItem::Table(table)
                });

            let Some(shell_table) = shell_table.as_table_mut() else {
                return Err(anyhow::anyhow!("shell entry should be a table"));
            };

            shell_table["path"] = toml_edit::value(shell_config.path.clone());
            if !shell_config.args.is_empty() {
                let mut args_array = toml_edit::Array::new();
                for arg in &shell_config.args {
                    args_array.push(arg.as_str());
                }
                shell_table["args"] = toml_edit::value(args_array);
            } else {
                shell_table.remove("args");
            }
            if let Some(style) = shell_config.script_style {
                shell_table["script_style"] = toml_edit::value(style.to_string());
            } else {
                shell_table.remove("script_style");
            }
        } else {
            root.remove("shell");
        }
    }

    fs::create_dir_all(code_home).await?;
    let tmp_path = config_path.with_extension("tmp");
    fs::write(&tmp_path, doc.to_string()).await?;
    fs::rename(&tmp_path, &config_path).await?;

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellStyleSkillMode {
    Inherit,
    Enabled,
    Disabled,
}

/// Update the membership of `skill_name` in a shell style profile.
///
/// - `Enabled`: add to `shell_style_profiles.<style>.skills` and remove from
///   `disabled_skills`.
/// - `Disabled`: add to `disabled_skills` and remove from `skills`.
/// - `Inherit`: remove from both lists.
///
/// Returns `true` when `config.toml` changed.
pub fn set_shell_style_profile_skill_mode(
    code_home: &Path,
    style: ShellScriptStyle,
    skill_name: &str,
    mode: ShellStyleSkillMode,
) -> anyhow::Result<bool> {
    let normalized_skill = skill_name.trim();
    if normalized_skill.is_empty() {
        return Err(anyhow::anyhow!("skill name cannot be empty"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let style_key = style.to_string();
    let mut changed = false;

    {
        let root = doc.as_table_mut();
        match root.get("shell_style_profiles") {
            Some(item) => {
                if item.as_table().is_none() {
                    return Err(anyhow::anyhow!(
                        "`shell_style_profiles` must be a TOML table"
                    ));
                }
            }
            None => {
                if matches!(mode, ShellStyleSkillMode::Inherit) {
                    return Ok(false);
                }
                let mut table = TomlTable::new();
                table.set_implicit(true);
                root.insert("shell_style_profiles", TomlItem::Table(table));
                changed = true;
            }
        }
    }

    {
        let root = doc.as_table_mut();
        let profiles_table = root
            .get_mut("shell_style_profiles")
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("failed to prepare shell_style_profiles table"))?;

        let mut resolved_style_key = find_shell_style_profile_key(profiles_table, style)?;
        match resolved_style_key.as_deref() {
            Some(existing_key) => {
                if profiles_table
                    .get(existing_key)
                    .and_then(|item| item.as_table())
                    .is_none()
                {
                    return Err(anyhow::anyhow!(
                        "`shell_style_profiles.{existing_key}` must be a TOML table"
                    ));
                }
            }
            None => {
                if matches!(mode, ShellStyleSkillMode::Inherit) {
                    return Ok(changed);
                }
                let mut style_table = TomlTable::new();
                style_table.set_implicit(false);
                profiles_table.insert(style_key.as_str(), TomlItem::Table(style_table));
                resolved_style_key = Some(style_key.clone());
                changed = true;
            }
        }
        let resolved_style_key = resolved_style_key
            .ok_or_else(|| anyhow::anyhow!("failed to resolve shell style profile key"))?;

        let style_table = profiles_table
            .get_mut(resolved_style_key.as_str())
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("failed to prepare shell style profile table"))?;

        let mut skills = read_string_array(style_table, "skills")?;
        let mut disabled = read_string_array(style_table, "disabled_skills")?;
        let normalized_target = normalize_skill_name(normalized_skill);

        let removed_from_skills = remove_skill_name(&mut skills, &normalized_target);
        let removed_from_disabled = remove_skill_name(&mut disabled, &normalized_target);
        changed |= removed_from_skills || removed_from_disabled;

        match mode {
            ShellStyleSkillMode::Inherit => {}
            ShellStyleSkillMode::Enabled => {
                if push_unique_skill_name(&mut skills, normalized_skill) {
                    changed = true;
                }
            }
            ShellStyleSkillMode::Disabled => {
                if push_unique_skill_name(&mut disabled, normalized_skill) {
                    changed = true;
                }
            }
        }

        changed |= write_string_array(style_table, "skills", &skills)?;
        changed |= write_string_array(style_table, "disabled_skills", &disabled)?;

        if style_table.is_empty() {
            profiles_table.remove(resolved_style_key.as_str());
            changed = true;
        }

        if profiles_table.is_empty() {
            root.remove("shell_style_profiles");
            changed = true;
        }
    }

    if changed {
        std::fs::create_dir_all(code_home)?;
        let tmp_path = config_path.with_extension("tmp");
        std::fs::write(&tmp_path, doc.to_string())?;
        std::fs::rename(&tmp_path, &config_path)?;
    }

    Ok(changed)
}

/// Update shell-style profile skill lists (`skills` allow-list + `disabled_skills` overrides).
///
/// Empty lists remove their corresponding keys. If the resulting style profile
/// table is empty it is removed.
pub fn set_shell_style_profile_skills(
    code_home: &Path,
    style: ShellScriptStyle,
    skills: &[String],
    disabled_skills: &[String],
) -> anyhow::Result<bool> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let style_key = style.to_string();
    let mut changed = false;

    {
        let root = doc.as_table_mut();
        match root.get("shell_style_profiles") {
            Some(item) => {
                if item.as_table().is_none() {
                    return Err(anyhow::anyhow!(
                        "`shell_style_profiles` must be a TOML table"
                    ));
                }
            }
            None => {
                if skills.is_empty() && disabled_skills.is_empty() {
                    return Ok(false);
                }
                let mut table = TomlTable::new();
                table.set_implicit(true);
                root.insert("shell_style_profiles", TomlItem::Table(table));
                changed = true;
            }
        }
    }

    {
        let root = doc.as_table_mut();
        let profiles_table = root
            .get_mut("shell_style_profiles")
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("failed to prepare shell_style_profiles table"))?;

        let mut resolved_style_key = find_shell_style_profile_key(profiles_table, style)?;
        match resolved_style_key.as_deref() {
            Some(existing_key) => {
                if profiles_table
                    .get(existing_key)
                    .and_then(|item| item.as_table())
                    .is_none()
                {
                    return Err(anyhow::anyhow!(
                        "`shell_style_profiles.{existing_key}` must be a TOML table"
                    ));
                }
            }
            None => {
                if skills.is_empty() && disabled_skills.is_empty() {
                    return Ok(changed);
                }
                let mut style_table = TomlTable::new();
                style_table.set_implicit(false);
                profiles_table.insert(style_key.as_str(), TomlItem::Table(style_table));
                resolved_style_key = Some(style_key.clone());
                changed = true;
            }
        }
        let resolved_style_key = resolved_style_key
            .ok_or_else(|| anyhow::anyhow!("failed to resolve shell style profile key"))?;

        let style_table = profiles_table
            .get_mut(resolved_style_key.as_str())
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("failed to prepare shell style profile table"))?;

        changed |= write_string_array(style_table, "skills", skills)?;
        changed |= write_string_array(style_table, "disabled_skills", disabled_skills)?;

        if style_table.is_empty() {
            profiles_table.remove(resolved_style_key.as_str());
            changed = true;
        }

        if profiles_table.is_empty() {
            root.remove("shell_style_profiles");
            changed = true;
        }
    }

    if changed {
        std::fs::create_dir_all(code_home)?;
        let tmp_path = config_path.with_extension("tmp");
        std::fs::write(&tmp_path, doc.to_string())?;
        std::fs::rename(&tmp_path, &config_path)?;
    }

    Ok(changed)
}

/// Update shell-style profile path lists for `references` and `skill_roots`.
///
/// Empty lists remove their corresponding keys. If the resulting style profile
/// table is empty it is removed.
pub fn set_shell_style_profile_paths(
    code_home: &Path,
    style: ShellScriptStyle,
    references: &[PathBuf],
    skill_roots: &[PathBuf],
) -> anyhow::Result<bool> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let style_key = style.to_string();
    let mut changed = false;

    {
        let root = doc.as_table_mut();
        match root.get("shell_style_profiles") {
            Some(item) => {
                if item.as_table().is_none() {
                    return Err(anyhow::anyhow!(
                        "`shell_style_profiles` must be a TOML table"
                    ));
                }
            }
            None => {
                if references.is_empty() && skill_roots.is_empty() {
                    return Ok(false);
                }
                let mut table = TomlTable::new();
                table.set_implicit(true);
                root.insert("shell_style_profiles", TomlItem::Table(table));
                changed = true;
            }
        }
    }

    {
        let root = doc.as_table_mut();
        let profiles_table = root
            .get_mut("shell_style_profiles")
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("failed to prepare shell_style_profiles table"))?;

        let mut resolved_style_key = find_shell_style_profile_key(profiles_table, style)?;
        match resolved_style_key.as_deref() {
            Some(existing_key) => {
                if profiles_table
                    .get(existing_key)
                    .and_then(|item| item.as_table())
                    .is_none()
                {
                    return Err(anyhow::anyhow!(
                        "`shell_style_profiles.{existing_key}` must be a TOML table"
                    ));
                }
            }
            None => {
                if references.is_empty() && skill_roots.is_empty() {
                    return Ok(changed);
                }
                let mut style_table = TomlTable::new();
                style_table.set_implicit(false);
                profiles_table.insert(style_key.as_str(), TomlItem::Table(style_table));
                resolved_style_key = Some(style_key.clone());
                changed = true;
            }
        }
        let resolved_style_key = resolved_style_key
            .ok_or_else(|| anyhow::anyhow!("failed to resolve shell style profile key"))?;

        let style_table = profiles_table
            .get_mut(resolved_style_key.as_str())
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("failed to prepare shell style profile table"))?;

        changed |= write_path_array(style_table, "references", references)?;
        changed |= write_path_array(style_table, "skill_roots", skill_roots)?;

        if style_table.is_empty() {
            profiles_table.remove(resolved_style_key.as_str());
            changed = true;
        }

        if profiles_table.is_empty() {
            root.remove("shell_style_profiles");
            changed = true;
        }
    }

    if changed {
        std::fs::create_dir_all(code_home)?;
        let tmp_path = config_path.with_extension("tmp");
        std::fs::write(&tmp_path, doc.to_string())?;
        std::fs::rename(&tmp_path, &config_path)?;
    }

    Ok(changed)
}

/// Update the optional shell-style profile summary.
///
/// When `summary` is `None` (or empty after trimming), the key is removed. If
/// the resulting style profile table is empty it is removed.
pub fn set_shell_style_profile_summary(
    code_home: &Path,
    style: ShellScriptStyle,
    summary: Option<&str>,
) -> anyhow::Result<bool> {
    let summary = summary
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let style_key = style.to_string();
    let mut changed = false;

    {
        let root = doc.as_table_mut();
        match root.get("shell_style_profiles") {
            Some(item) => {
                if item.as_table().is_none() {
                    return Err(anyhow::anyhow!(
                        "`shell_style_profiles` must be a TOML table"
                    ));
                }
            }
            None => {
                if summary.is_none() {
                    return Ok(false);
                }
                let mut table = TomlTable::new();
                table.set_implicit(true);
                root.insert("shell_style_profiles", TomlItem::Table(table));
                changed = true;
            }
        }
    }

    {
        let root = doc.as_table_mut();
        let profiles_table = root
            .get_mut("shell_style_profiles")
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("failed to prepare shell_style_profiles table"))?;

        let mut resolved_style_key = find_shell_style_profile_key(profiles_table, style)?;
        match resolved_style_key.as_deref() {
            Some(existing_key) => {
                if profiles_table
                    .get(existing_key)
                    .and_then(|item| item.as_table())
                    .is_none()
                {
                    return Err(anyhow::anyhow!(
                        "`shell_style_profiles.{existing_key}` must be a TOML table"
                    ));
                }
            }
            None => {
                if summary.is_none() {
                    return Ok(changed);
                }
                let mut style_table = TomlTable::new();
                style_table.set_implicit(false);
                profiles_table.insert(style_key.as_str(), TomlItem::Table(style_table));
                resolved_style_key = Some(style_key.clone());
                changed = true;
            }
        }
        let resolved_style_key = resolved_style_key
            .ok_or_else(|| anyhow::anyhow!("failed to resolve shell style profile key"))?;

        let style_table = profiles_table
            .get_mut(resolved_style_key.as_str())
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("failed to prepare shell style profile table"))?;

        let existing_summary = style_table
            .get("summary")
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if existing_summary != summary {
            match summary {
                Some(value) => style_table["summary"] = toml_edit::value(value),
                None => {
                    style_table.remove("summary");
                }
            }
            changed = true;
        }

        if style_table.is_empty() {
            profiles_table.remove(resolved_style_key.as_str());
            changed = true;
        }

        if profiles_table.is_empty() {
            root.remove("shell_style_profiles");
            changed = true;
        }
    }

    if changed {
        std::fs::create_dir_all(code_home)?;
        let tmp_path = config_path.with_extension("tmp");
        std::fs::write(&tmp_path, doc.to_string())?;
        std::fs::rename(&tmp_path, &config_path)?;
    }

    Ok(changed)
}

/// Update shell-style profile MCP server include/exclude filters.
///
/// Empty lists remove their corresponding keys. If the resulting `mcp_servers`
/// table becomes empty it is removed.
pub fn set_shell_style_profile_mcp_servers(
    code_home: &Path,
    style: ShellScriptStyle,
    include: &[String],
    exclude: &[String],
) -> anyhow::Result<bool> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let style_key = style.to_string();
    let mut changed = false;

    {
        let root = doc.as_table_mut();
        match root.get("shell_style_profiles") {
            Some(item) => {
                if item.as_table().is_none() {
                    return Err(anyhow::anyhow!(
                        "`shell_style_profiles` must be a TOML table"
                    ));
                }
            }
            None => {
                if include.is_empty() && exclude.is_empty() {
                    return Ok(false);
                }
                let mut table = TomlTable::new();
                table.set_implicit(true);
                root.insert("shell_style_profiles", TomlItem::Table(table));
                changed = true;
            }
        }
    }

    {
        let root = doc.as_table_mut();
        let profiles_table = root
            .get_mut("shell_style_profiles")
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("failed to prepare shell_style_profiles table"))?;

        let mut resolved_style_key = find_shell_style_profile_key(profiles_table, style)?;
        match resolved_style_key.as_deref() {
            Some(existing_key) => {
                if profiles_table
                    .get(existing_key)
                    .and_then(|item| item.as_table())
                    .is_none()
                {
                    return Err(anyhow::anyhow!(
                        "`shell_style_profiles.{existing_key}` must be a TOML table"
                    ));
                }
            }
            None => {
                if include.is_empty() && exclude.is_empty() {
                    return Ok(changed);
                }
                let mut style_table = TomlTable::new();
                style_table.set_implicit(false);
                profiles_table.insert(style_key.as_str(), TomlItem::Table(style_table));
                resolved_style_key = Some(style_key.clone());
                changed = true;
            }
        }
        let resolved_style_key = resolved_style_key
            .ok_or_else(|| anyhow::anyhow!("failed to resolve shell style profile key"))?;

        let style_table = profiles_table
            .get_mut(resolved_style_key.as_str())
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("failed to prepare shell style profile table"))?;

        let mcp_key = "mcp_servers";
        let mut has_mcp_table = false;
        match style_table.get(mcp_key) {
            Some(item) => {
                if item.as_table().is_none() {
                    return Err(anyhow::anyhow!(
                        "`shell_style_profiles.{resolved_style_key}.{mcp_key}` must be a TOML table"
                    ));
                }
                has_mcp_table = true;
            }
            None => {
                if !include.is_empty() || !exclude.is_empty() {
                    let mut mcp_table = TomlTable::new();
                    mcp_table.set_implicit(false);
                    style_table.insert(mcp_key, TomlItem::Table(mcp_table));
                    changed = true;
                    has_mcp_table = true;
                }
            }
        }

        if has_mcp_table {
            let mcp_table = style_table
                .get_mut(mcp_key)
                .and_then(|item| item.as_table_mut())
                .ok_or_else(|| anyhow::anyhow!("failed to prepare mcp_servers table"))?;
            changed |= write_string_array(mcp_table, "include", include)?;
            changed |= write_string_array(mcp_table, "exclude", exclude)?;

            if mcp_table.is_empty() {
                style_table.remove(mcp_key);
                changed = true;
            }
        }

        if style_table.is_empty() {
            profiles_table.remove(resolved_style_key.as_str());
            changed = true;
        }

        if profiles_table.is_empty() {
            root.remove("shell_style_profiles");
            changed = true;
        }
    }

    if changed {
        std::fs::create_dir_all(code_home)?;
        let tmp_path = config_path.with_extension("tmp");
        std::fs::write(&tmp_path, doc.to_string())?;
        std::fs::rename(&tmp_path, &config_path)?;
    }

    Ok(changed)
}

fn normalize_skill_name(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn find_shell_style_profile_key(
    table: &TomlTable,
    style: ShellScriptStyle,
) -> anyhow::Result<Option<String>> {
    let mut match_key: Option<String> = None;
    for (key, _) in table.iter() {
        if ShellScriptStyle::parse(key) == Some(style) {
            if let Some(existing) = &match_key {
                return Err(anyhow::anyhow!(
                    "multiple shell_style_profiles entries map to `{style}` (`{existing}` and `{key}`); keep only one"
                ));
            }
            match_key = Some(key.to_string());
        }
    }
    Ok(match_key)
}

fn read_string_array(table: &TomlTable, key: &str) -> anyhow::Result<Vec<String>> {
    let Some(item) = table.get(key) else {
        return Ok(Vec::new());
    };
    let array = item
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("`{key}` must be a TOML array"))?;
    let mut out: Vec<String> = Vec::new();
    for value in array.iter() {
        let as_str = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("`{key}` entries must be TOML strings"))?;
        let trimmed = as_str.trim();
        if !trimmed.is_empty() {
            out.push(trimmed.to_string());
        }
    }
    Ok(out)
}

fn remove_skill_name(values: &mut Vec<String>, normalized_target: &str) -> bool {
    let original_len = values.len();
    values.retain(|entry| normalize_skill_name(entry) != normalized_target);
    original_len != values.len()
}

fn push_unique_skill_name(values: &mut Vec<String>, skill_name: &str) -> bool {
    let normalized_target = normalize_skill_name(skill_name);
    if values
        .iter()
        .any(|entry| normalize_skill_name(entry) == normalized_target)
    {
        return false;
    }
    values.push(skill_name.trim().to_string());
    true
}

fn write_string_array(table: &mut TomlTable, key: &str, values: &[String]) -> anyhow::Result<bool> {
    if values.is_empty() {
        return Ok(table.remove(key).is_some());
    }

    let mut deduped: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = normalize_skill_name(trimmed);
        if seen.insert(normalized) {
            deduped.push(trimmed.to_string());
        }
    }

    if deduped.is_empty() {
        return Ok(table.remove(key).is_some());
    }

    let existing = read_string_array(table, key)?;
    if existing == deduped {
        return Ok(false);
    }

    let mut array = TomlArray::new();
    for value in &deduped {
        array.push(value.as_str());
    }
    table[key] = toml_edit::value(array);
    Ok(true)
}

fn write_exact_string_array(
    table: &mut TomlTable,
    key: &str,
    values: &[String],
) -> anyhow::Result<bool> {
    if values.is_empty() {
        return Ok(table.remove(key).is_some());
    }

    let mut deduped: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            deduped.push(trimmed.to_string());
        }
    }

    if deduped.is_empty() {
        return Ok(table.remove(key).is_some());
    }

    let existing = read_string_array(table, key)?;
    if existing == deduped {
        return Ok(false);
    }

    let mut array = TomlArray::new();
    for value in &deduped {
        array.push(value.as_str());
    }
    table[key] = toml_edit::value(array);
    Ok(true)
}

fn read_path_array(table: &TomlTable, key: &str) -> anyhow::Result<Vec<PathBuf>> {
    let Some(item) = table.get(key) else {
        return Ok(Vec::new());
    };
    let array = item
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("`{key}` must be a TOML array"))?;
    let mut out: Vec<PathBuf> = Vec::new();
    for value in array.iter() {
        let as_str = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("`{key}` entries must be TOML strings"))?;
        let trimmed = as_str.trim();
        if !trimmed.is_empty() {
            out.push(PathBuf::from(trimmed));
        }
    }
    Ok(out)
}

fn write_path_array(table: &mut TomlTable, key: &str, values: &[PathBuf]) -> anyhow::Result<bool> {
    if values.is_empty() {
        return Ok(table.remove(key).is_some());
    }

    let mut deduped: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for value in values {
        let rendered = value.to_string_lossy().trim().to_string();
        if rendered.is_empty() {
            continue;
        }
        if seen.insert(rendered.clone()) {
            deduped.push(rendered);
        }
    }

    if deduped.is_empty() {
        return Ok(table.remove(key).is_some());
    }

    let existing = read_path_array(table, key)?
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    if existing == deduped {
        return Ok(false);
    }

    let mut array = TomlArray::new();
    for value in &deduped {
        array.push(value.as_str());
    }
    table[key] = toml_edit::value(array);
    Ok(true)
}

/// Patch `CODEX_HOME/config.toml` project state.
/// Use with caution.
pub fn set_project_trusted(code_home: &Path, project_path: &Path) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    set_project_trusted_inner(&mut doc, project_path)?;

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

fn set_project_trusted_inner(doc: &mut DocumentMut, project_path: &Path) -> anyhow::Result<()> {
    // Ensure we render a human-friendly structure:
    //
    // [projects]
    // [projects."/path/to/project"]
    // trust_level = "trusted"
    //
    // rather than inline tables like:
    //
    // [projects]
    // "/path/to/project" = { trust_level = "trusted" }
    let project_key = project_path.to_string_lossy().to_string();

    // Ensure top-level `projects` exists as a non-inline, explicit table. If it
    // exists but was previously represented as a non-table (e.g., inline),
    // replace it with an explicit table.
    let mut created_projects_table = false;
    {
        let root = doc.as_table_mut();
        let needs_table = !root.contains_key("projects")
            || root.get("projects").and_then(|i| i.as_table()).is_none();
        if needs_table {
            root.insert("projects", toml_edit::table());
            created_projects_table = true;
        }
    }
    let Some(projects_tbl) = doc["projects"].as_table_mut() else {
        return Err(anyhow::anyhow!(
            "projects table missing after initialization"
        ));
    };

    // If we created the `projects` table ourselves, keep it implicit so we
    // don't render a standalone `[projects]` header.
    if created_projects_table {
        projects_tbl.set_implicit(true);
    }

    // Ensure the per-project entry is its own explicit table. If it exists but
    // is not a table (e.g., an inline table), replace it with an explicit table.
    let needs_proj_table = !projects_tbl.contains_key(project_key.as_str())
        || projects_tbl
            .get(project_key.as_str())
            .and_then(|i| i.as_table())
            .is_none();
    if needs_proj_table {
        projects_tbl.insert(project_key.as_str(), toml_edit::table());
    }
    let Some(proj_tbl) = projects_tbl
        .get_mut(project_key.as_str())
        .and_then(|i| i.as_table_mut())
    else {
        return Err(anyhow::anyhow!("project table missing for {project_key}"));
    };
    proj_tbl.set_implicit(false);
    proj_tbl["trust_level"] = toml_edit::value("trusted");

    Ok(())
}

/// Persist the selected TUI theme into `CODEX_HOME/config.toml` at `[tui.theme].name`.
pub fn set_tui_theme_name(code_home: &Path, theme: ThemeName) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Map enum to kebab-case string used in config
    let theme_str = match theme {
        ThemeName::LightPhoton => "light-photon",
        ThemeName::LightPhotonAnsi16 => "light-photon-ansi16",
        ThemeName::LightPrismRainbow => "light-prism-rainbow",
        ThemeName::LightVividTriad => "light-vivid-triad",
        ThemeName::LightPorcelain => "light-porcelain",
        ThemeName::LightSandbar => "light-sandbar",
        ThemeName::LightGlacier => "light-glacier",
        ThemeName::DarkCarbonNight => "dark-carbon-night",
        ThemeName::DarkCarbonAnsi16 => "dark-carbon-ansi16",
        ThemeName::DarkShinobiDusk => "dark-shinobi-dusk",
        ThemeName::DarkOledBlackPro => "dark-oled-black-pro",
        ThemeName::DarkAmberTerminal => "dark-amber-terminal",
        ThemeName::DarkAuroraFlux => "dark-aurora-flux",
        ThemeName::DarkCharcoalRainbow => "dark-charcoal-rainbow",
        ThemeName::DarkZenGarden => "dark-zen-garden",
        ThemeName::DarkPaperLightPro => "dark-paper-light-pro",
        ThemeName::Custom => "custom",
    };

    // Ensure `[tui.theme]` is a table before writing to it. Older configs may
    // store `tui.theme = "…"`.
    {
        use toml_edit::Item as It;
        if !doc["tui"].is_table() {
            doc["tui"] = It::Table(toml_edit::Table::new());
        }
        if !doc["tui"]["theme"].is_table() {
            doc["tui"]["theme"] = It::Table(toml_edit::Table::new());
        }
    }

    // Write `[tui.theme].name = "…"`
    doc["tui"]["theme"]["name"] = toml_edit::value(theme_str);
    // When switching away from the Custom theme, clear any lingering custom
    // overrides so built-in themes render true to spec on next startup.
    if theme != ThemeName::Custom
        && let Some(tbl) = doc["tui"]["theme"].as_table_mut() {
            tbl.remove("label");
            tbl.remove("colors");
        }

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Record the most recent terminal background autodetect result under `[tui.cached_terminal_background]`.
pub fn set_cached_terminal_background(
    code_home: &Path,
    cache: &CachedTerminalBackground,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let mut tbl = toml_edit::Table::new();
    tbl.set_implicit(false);
    tbl.insert("is_dark", toml_edit::value(cache.is_dark));
    if let Some(term) = &cache.term {
        tbl.insert("term", toml_edit::value(term.as_str()));
    }
    if let Some(term_program) = &cache.term_program {
        tbl.insert("term_program", toml_edit::value(term_program.as_str()));
    }
    if let Some(term_program_version) = &cache.term_program_version {
        tbl.insert(
            "term_program_version",
            toml_edit::value(term_program_version.as_str()),
        );
    }
    if let Some(colorfgbg) = &cache.colorfgbg {
        tbl.insert("colorfgbg", toml_edit::value(colorfgbg.as_str()));
    }
    if let Some(source) = &cache.source {
        tbl.insert("source", toml_edit::value(source.as_str()));
    }
    if let Some(rgb) = &cache.rgb {
        tbl.insert("rgb", toml_edit::value(rgb.as_str()));
    }

    doc["tui"]["cached_terminal_background"] = toml_edit::Item::Table(tbl);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;
    Ok(())
}

/// Persist the selected spinner into `CODEX_HOME/config.toml` at `[tui.spinner].name`.
pub fn set_tui_spinner_name(code_home: &Path, spinner_name: &str) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Write `[tui.spinner].name = "…"`
    doc["tui"]["spinner"]["name"] = toml_edit::value(spinner_name);

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Save or update a custom spinner under `[tui.spinner.custom.<id>]` with a display `label`,
/// and set it active by writing `[tui.spinner].name = <id>`.
pub fn set_custom_spinner(
    code_home: &Path,
    id: &str,
    label: &str,
    interval: u64,
    frames: &[String],
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };
    // Write custom spinner
    let node = &mut doc["tui"]["spinner"]["custom"][id];
    node["interval"] = toml_edit::value(interval as i64);
    let mut arr = toml_edit::Array::default();
    for s in frames { arr.push(s.as_str()); }
    node["frames"] = toml_edit::value(arr);
    node["label"] = toml_edit::value(label);

    // Set as active
    doc["tui"]["spinner"]["name"] = toml_edit::value(id);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Save or update a custom theme with a display `label` and color overrides
/// under `[tui.theme]`, and set it active by writing `[tui.theme].name = "custom"`.
pub fn set_custom_theme(
    code_home: &Path,
    label: &str,
    colors: &ThemeColors,
    set_active: bool,
    is_dark: Option<bool>,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Optionally activate custom theme and persist label
    if set_active {
        doc["tui"]["theme"]["name"] = toml_edit::value("custom");
    }
    doc["tui"]["theme"]["label"] = toml_edit::value(label);
    if let Some(d) = is_dark { doc["tui"]["theme"]["is_dark"] = toml_edit::value(d); }

    // Ensure colors table exists and write provided keys
    {
        use toml_edit::Item as It;
        if !doc["tui"]["theme"].is_table() {
            doc["tui"]["theme"] = It::Table(toml_edit::Table::new());
        }
        let Some(theme_tbl) = doc["tui"]["theme"].as_table_mut() else {
            return Err(anyhow::anyhow!("tui.theme must be a table"));
        };
        if !theme_tbl.contains_key("colors") {
            theme_tbl.insert("colors", It::Table(toml_edit::Table::new()));
        }
        let Some(colors_tbl) = theme_tbl["colors"].as_table_mut() else {
            return Err(anyhow::anyhow!("tui.theme.colors must be a table"));
        };
        macro_rules! set_opt {
            ($key:ident) => {
                if let Some(ref v) = colors.$key { colors_tbl.insert(stringify!($key), toml_edit::value(v.clone())); }
            };
        }
        set_opt!(primary);
        set_opt!(secondary);
        set_opt!(background);
        set_opt!(foreground);
        set_opt!(border);
        set_opt!(border_focused);
        set_opt!(selection);
        set_opt!(cursor);
        set_opt!(success);
        set_opt!(warning);
        set_opt!(error);
        set_opt!(info);
        set_opt!(text);
        set_opt!(text_dim);
        set_opt!(text_bright);
        set_opt!(keyword);
        set_opt!(string);
        set_opt!(comment);
        set_opt!(function);
        set_opt!(spinner);
        set_opt!(progress);
    }

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist the alternate screen preference into `CODEX_HOME/config.toml` at `[tui].alternate_screen`.
pub fn set_tui_alternate_screen(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Write `[tui].alternate_screen = true/false`
    doc["tui"]["alternate_screen"] = toml_edit::value(enabled);

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the lower header-line visibility flag into
/// `CODEX_HOME/config.toml` at `[tui.header].show_bottom_line`.
pub fn set_tui_header_show_bottom_line(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["tui"]["header"]["show_bottom_line"] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;
    Ok(())
}

/// Persist the limits layout mode into `CODEX_HOME/config.toml` at `[tui.limits].layout_mode`.
pub fn set_tui_limits_layout_mode(
    code_home: &Path,
    layout_mode: LimitsLayoutMode,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let mode = match layout_mode {
        LimitsLayoutMode::Auto => "auto",
        LimitsLayoutMode::SingleColumn => "single-column",
    };
    doc["tui"]["limits"]["layout_mode"] = toml_edit::value(mode);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist Settings UI routing preferences into `CODEX_HOME/config.toml` at
/// `[tui.settings_menu]`.
pub fn set_tui_settings_menu(
    code_home: &Path,
    settings_menu: &SettingsMenuConfig,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let open_mode = match settings_menu.open_mode {
        SettingsMenuOpenMode::Auto => "auto",
        SettingsMenuOpenMode::Overlay => "overlay",
        SettingsMenuOpenMode::Bottom => "bottom",
    };

    doc["tui"]["settings_menu"]["open_mode"] = toml_edit::value(open_mode);
    doc["tui"]["settings_menu"]["overlay_min_width"] =
        toml_edit::value(settings_menu.overlay_min_width as i64);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist TUI hotkey preferences into `CODEX_HOME/config.toml` at
/// `[tui.hotkeys]`.
pub fn set_tui_hotkeys(
    code_home: &Path,
    hotkeys: &crate::config_types::TuiHotkeysConfig,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let tui_table = doc["tui"]
        .or_insert(TomlItem::Table(TomlTable::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("`tui` must be a TOML table"))?;

    let hotkeys_table = tui_table["hotkeys"]
        .or_insert(TomlItem::Table(TomlTable::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("`tui.hotkeys` must be a TOML table"))?;

    hotkeys_table["model_selector"] =
        toml_edit::value(hotkeys.model_selector.toml_value().as_ref());
    hotkeys_table["reasoning_effort"] =
        toml_edit::value(hotkeys.reasoning_effort.toml_value().as_ref());
    hotkeys_table["shell_selector"] =
        toml_edit::value(hotkeys.shell_selector.toml_value().as_ref());
    hotkeys_table["network_settings"] =
        toml_edit::value(hotkeys.network_settings.toml_value().as_ref());
    hotkeys_table["exec_output_fold"] =
        toml_edit::value(hotkeys.exec_output_fold.toml_value().as_ref());
    hotkeys_table["js_repl_code_fold"] =
        toml_edit::value(hotkeys.js_repl_code_fold.toml_value().as_ref());
    hotkeys_table["jump_to_parent_call"] =
        toml_edit::value(hotkeys.jump_to_parent_call.toml_value().as_ref());
    hotkeys_table["jump_to_latest_child_call"] =
        toml_edit::value(hotkeys.jump_to_latest_child_call.toml_value().as_ref());

    fn write_hotkey_override_field(
        table: &mut TomlTable,
        key: &str,
        value: Option<crate::config_types::TuiHotkey>,
    ) {
        match value {
            Some(value) => {
                table[key] = toml_edit::value(value.toml_value().as_ref());
            }
            None => {
                table.remove(key);
            }
        }
    }

    fn write_hotkey_overrides_table(
        hotkeys_table: &mut TomlTable,
        platform_key: &str,
        overrides: Option<&crate::config_types::TuiHotkeysOverrides>,
    ) -> anyhow::Result<()> {
        match overrides {
            Some(overrides) => {
                let platform_table = hotkeys_table[platform_key]
                    .or_insert(TomlItem::Table(TomlTable::new()))
                    .as_table_mut()
                    .ok_or_else(|| {
                        anyhow::anyhow!("`tui.hotkeys.{platform_key}` must be a TOML table")
                    })?;

                write_hotkey_override_field(platform_table, "model_selector", overrides.model_selector);
                write_hotkey_override_field(
                    platform_table,
                    "reasoning_effort",
                    overrides.reasoning_effort,
                );
                write_hotkey_override_field(platform_table, "shell_selector", overrides.shell_selector);
                write_hotkey_override_field(
                    platform_table,
                    "network_settings",
                    overrides.network_settings,
                );
                write_hotkey_override_field(
                    platform_table,
                    "exec_output_fold",
                    overrides.exec_output_fold,
                );
                write_hotkey_override_field(
                    platform_table,
                    "js_repl_code_fold",
                    overrides.js_repl_code_fold,
                );
                write_hotkey_override_field(
                    platform_table,
                    "jump_to_parent_call",
                    overrides.jump_to_parent_call,
                );
                write_hotkey_override_field(
                    platform_table,
                    "jump_to_latest_child_call",
                    overrides.jump_to_latest_child_call,
                );

                if platform_table.is_empty() {
                    hotkeys_table.remove(platform_key);
                }
            }
            None => {
                let Some(item) = hotkeys_table.get_mut(platform_key) else {
                    return Ok(());
                };
                let platform_table = item.as_table_mut().ok_or_else(|| {
                    anyhow::anyhow!("`tui.hotkeys.{platform_key}` must be a TOML table")
                })?;

                platform_table.remove("model_selector");
                platform_table.remove("reasoning_effort");
                platform_table.remove("shell_selector");
                platform_table.remove("network_settings");
                platform_table.remove("exec_output_fold");
                platform_table.remove("js_repl_code_fold");
                platform_table.remove("jump_to_parent_call");
                platform_table.remove("jump_to_latest_child_call");

                if platform_table.is_empty() {
                    hotkeys_table.remove(platform_key);
                }
            }
        }
        Ok(())
    }

    write_hotkey_overrides_table(hotkeys_table, "macos", hotkeys.macos.as_ref())?;
    write_hotkey_overrides_table(hotkeys_table, "windows", hotkeys.windows.as_ref())?;
    write_hotkey_overrides_table(hotkeys_table, "linux", hotkeys.linux.as_ref())?;
    write_hotkey_overrides_table(hotkeys_table, "android", hotkeys.android.as_ref())?;
    write_hotkey_overrides_table(hotkeys_table, "termux", hotkeys.termux.as_ref())?;
    write_hotkey_overrides_table(hotkeys_table, "freebsd", hotkeys.freebsd.as_ref())?;
    write_hotkey_overrides_table(hotkeys_table, "openbsd", hotkeys.openbsd.as_ref())?;
    write_hotkey_overrides_table(hotkeys_table, "netbsd", hotkeys.netbsd.as_ref())?;
    write_hotkey_overrides_table(hotkeys_table, "dragonfly", hotkeys.dragonfly.as_ref())?;

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the TUI notifications preference into `CODEX_HOME/config.toml` at `[tui].notifications`.
pub fn set_tui_notifications(
    code_home: &Path,
    notifications: crate::config_types::Notifications,
) -> anyhow::Result<()> {
    use crate::config_types::Notifications;

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    match notifications {
        Notifications::Enabled(value) => {
            doc["tui"]["notifications"] = toml_edit::value(value);
        }
        Notifications::Custom(values) => {
            let mut array = TomlArray::default();
            for value in values {
                array.push(value);
            }
            doc["tui"]["notifications"] = TomlItem::Value(array.into());
        }
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist top status-line item ids into `CODEX_HOME/config.toml` at
/// `[tui].status_line_top`.
///
/// Item order is preserved. Empty or whitespace-only ids are dropped.
/// Passing an empty list removes `[tui].status_line_top`, reverting to the
/// built-in dynamic top-line layout.
pub fn set_tui_status_line(code_home: &Path, item_ids: &[String]) -> anyhow::Result<()> {
    set_tui_status_line_layout(code_home, item_ids, &[], StatusLineLane::Top)
}

/// Persist split status-line layout into `CODEX_HOME/config.toml`.
///
/// - top lane: `[tui].status_line_top`
/// - bottom lane: `[tui].status_line_bottom`
/// - default `/statusline` lane: `[tui].status_line_primary`
pub fn set_tui_status_line_layout(
    code_home: &Path,
    top_item_ids: &[String],
    bottom_item_ids: &[String],
    primary_lane: StatusLineLane,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let normalized_top = top_item_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let normalized_bottom = bottom_item_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    if normalized_top.is_empty() {
        if let Some(tui_table) = doc["tui"].as_table_mut() {
            tui_table.remove("status_line_top");
            tui_table.remove("status_line");
        }
    } else {
        let mut array = TomlArray::default();
        for id in normalized_top {
            array.push(id);
        }
        doc["tui"]["status_line_top"] = TomlItem::Value(array.into());
    }

    if normalized_bottom.is_empty() {
        if let Some(tui_table) = doc["tui"].as_table_mut() {
            tui_table.remove("status_line_bottom");
        }
    } else {
        let mut array = TomlArray::default();
        for id in normalized_bottom {
            array.push(id);
        }
        doc["tui"]["status_line_bottom"] = TomlItem::Value(array.into());
    }

    doc["tui"]["status_line_primary"] = toml_edit::value(match primary_lane {
        StatusLineLane::Top => "top",
        StatusLineLane::Bottom => "bottom",
    });

    if let Some(tui_table) = doc["tui"].as_table_mut() {
        tui_table.remove("status_line");
        if tui_table.is_empty() {
            doc.as_table_mut().remove("tui");
        }
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;
    Ok(())
}

/// Persist account-store path preferences into `CODEX_HOME/config.toml` at `[accounts]`.
///
/// - `read_paths` is an ordered list of candidate files to read from.
/// - `write_path` is the file used for writes/updates.
/// - If both are empty/unset, the `[accounts]` table is removed so defaults apply.
pub fn set_account_store_paths(
    code_home: &Path,
    read_paths: &[String],
    write_path: Option<&str>,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let normalized_read_paths = read_paths
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let normalized_write_path = write_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    if normalized_read_paths.is_empty() && normalized_write_path.is_none() {
        doc.as_table_mut().remove("accounts");
    } else {
        if !doc["accounts"].is_table() {
            doc["accounts"] = TomlItem::Table(TomlTable::new());
        }
        let Some(accounts_table) = doc["accounts"].as_table_mut() else {
            return Err(anyhow::anyhow!(
                "failed to configure account store paths: [accounts] is not a table"
            ));
        };

        if normalized_read_paths.is_empty() {
            accounts_table.remove("read_paths");
        } else {
            let mut array = TomlArray::new();
            for path in normalized_read_paths {
                array.push(path);
            }
            accounts_table["read_paths"] = TomlItem::Value(array.into());
        }

        if let Some(path) = normalized_write_path {
            accounts_table["write_path"] = toml_edit::value(path);
        } else {
            accounts_table.remove("write_path");
        }
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the review auto-resolve preference into `CODEX_HOME/config.toml` at `[tui].review_auto_resolve`.
pub fn set_tui_review_auto_resolve(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["tui"]["review_auto_resolve"] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the auto review preference into `CODEX_HOME/config.toml` at `[tui].auto_review_enabled`.
pub fn set_tui_auto_review_enabled(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["tui"]["auto_review_enabled"] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the review model + reasoning effort into `CODEX_HOME/config.toml`.
pub fn set_review_model(
    code_home: &Path,
    model: &str,
    effort: ReasoningEffort,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("review model cannot be empty"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["review_use_chat_model"] = toml_edit::value(use_chat_model);
    if !use_chat_model {
        doc["review_model"] = toml_edit::value(trimmed);
        doc["review_model_reasoning_effort"] =
            toml_edit::value(effort.to_string().to_ascii_lowercase());
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the resolve model + reasoning effort for `/review` auto-resolve flows.
pub fn set_review_resolve_model(
    code_home: &Path,
    model: &str,
    effort: ReasoningEffort,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let trimmed = model.trim();
    if !use_chat_model && trimmed.is_empty() {
        return Err(anyhow::anyhow!("review resolve model cannot be empty"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["review_resolve_use_chat_model"] = toml_edit::value(use_chat_model);
    if !use_chat_model {
        doc["review_resolve_model"] = toml_edit::value(trimmed);
        doc["review_resolve_model_reasoning_effort"] =
            toml_edit::value(effort.to_string().to_ascii_lowercase());
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the planning model + reasoning effort into `CODEX_HOME/config.toml`.
pub fn set_planning_model(
    code_home: &Path,
    model: &str,
    effort: ReasoningEffort,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["planning_use_chat_model"] = toml_edit::value(use_chat_model);
    if !use_chat_model {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return Err(anyhow::anyhow!("planning model cannot be empty"));
        }
        doc["planning_model"] = toml_edit::value(trimmed);
        doc["planning_model_reasoning_effort"] =
            toml_edit::value(effort.to_string().to_ascii_lowercase());
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the Auto Review review model + reasoning effort.
pub fn set_auto_review_model(
    code_home: &Path,
    model: &str,
    effort: ReasoningEffort,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let trimmed = model.trim();
    if !use_chat_model && trimmed.is_empty() {
        return Err(anyhow::anyhow!("auto review model cannot be empty"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["auto_review_use_chat_model"] = toml_edit::value(use_chat_model);
    if !use_chat_model {
        doc["auto_review_model"] = toml_edit::value(trimmed);
        doc["auto_review_model_reasoning_effort"] =
            toml_edit::value(effort.to_string().to_ascii_lowercase());
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the Auto Review resolve model + reasoning effort.
pub fn set_auto_review_resolve_model(
    code_home: &Path,
    model: &str,
    effort: ReasoningEffort,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let trimmed = model.trim();
    if !use_chat_model && trimmed.is_empty() {
        return Err(anyhow::anyhow!("auto review resolve model cannot be empty"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["auto_review_resolve_use_chat_model"] = toml_edit::value(use_chat_model);
    if !use_chat_model {
        doc["auto_review_resolve_model"] = toml_edit::value(trimmed);
        doc["auto_review_resolve_model_reasoning_effort"] =
            toml_edit::value(effort.to_string().to_ascii_lowercase());
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist Auto Drive defaults under `[auto_drive]`.
pub fn set_auto_drive_settings(
    code_home: &Path,
    settings: &AutoDriveSettings,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    if let Some(tui_tbl) = doc["tui"].as_table_mut() {
        tui_tbl.remove("auto_drive");
    }

    if !doc.as_table().contains_key("auto_drive") || !doc["auto_drive"].is_table() {
        doc["auto_drive"] = TomlItem::Table(TomlTable::new());
    }

    doc["auto_drive_use_chat_model"] = toml_edit::value(use_chat_model);

    doc["auto_drive"]["review_enabled"] = toml_edit::value(settings.review_enabled);
    doc["auto_drive"]["agents_enabled"] = toml_edit::value(settings.agents_enabled);
    doc["auto_drive"]["qa_automation_enabled"] =
        toml_edit::value(settings.qa_automation_enabled);
    doc["auto_drive"]["cross_check_enabled"] =
        toml_edit::value(settings.cross_check_enabled);
    doc["auto_drive"]["observer_enabled"] =
        toml_edit::value(settings.observer_enabled);
    doc["auto_drive"]["coordinator_routing"] =
        toml_edit::value(settings.coordinator_routing);
    doc["auto_drive"]["model_routing_enabled"] =
        toml_edit::value(settings.model_routing_enabled);
    if settings.model_routing_entries.is_empty() {
        if let Some(auto_drive_tbl) = doc["auto_drive"].as_table_mut() {
            auto_drive_tbl.remove("model_routing_entries");
        }
    } else {
        let mut routing_entries = TomlArrayOfTables::new();
        for entry in &settings.model_routing_entries {
            let mut table = TomlTable::new();
            table.insert("model", TomlItem::Value(entry.model.trim().into()));
            table.insert("enabled", TomlItem::Value(entry.enabled.into()));

            let mut reasoning_levels = TomlArray::new();
            for level in &entry.reasoning_levels {
                reasoning_levels.push(level.to_string().to_ascii_lowercase());
            }
            table.insert(
                "reasoning_levels",
                TomlItem::Value(toml_edit::Value::Array(reasoning_levels)),
            );

            table.insert(
                "description",
                TomlItem::Value(entry.description.trim().into()),
            );

            routing_entries.push(table);
        }
        doc["auto_drive"]["model_routing_entries"] = TomlItem::ArrayOfTables(routing_entries);
    }
    doc["auto_drive"]["model"] = toml_edit::value(settings.model.trim());
    doc["auto_drive"]["model_reasoning_effort"] = toml_edit::value(
        settings
            .model_reasoning_effort
            .to_string()
            .to_ascii_lowercase(),
    );
    doc["auto_drive"]["auto_resolve_review_attempts"] =
        toml_edit::value(settings.auto_resolve_review_attempts.get() as i64);
    doc["auto_drive"]["auto_review_followup_attempts"] =
        toml_edit::value(settings.auto_review_followup_attempts.get() as i64);
    doc["auto_drive"]["coordinator_turn_cap"] =
        toml_edit::value(settings.coordinator_turn_cap as i64);

    let mode_str = match settings.continue_mode {
        AutoDriveContinueMode::Immediate => "immediate",
        AutoDriveContinueMode::TenSeconds => "ten-seconds",
        AutoDriveContinueMode::SixtySeconds => "sixty-seconds",
        AutoDriveContinueMode::Manual => "manual",
    };
    doc["auto_drive"]["continue_mode"] = toml_edit::value(mode_str);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Legacy helper: persist Auto Drive defaults under `[auto_drive]` while
/// accepting the former API surface.
#[deprecated(note = "use set_auto_drive_settings instead")]
pub fn set_tui_auto_drive_settings(
    code_home: &Path,
    settings: &AutoDriveSettings,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    set_auto_drive_settings(code_home, settings, use_chat_model)
}

/// Persist the GitHub workflow check preference under `[github].check_workflows_on_push`.
pub fn set_github_check_on_push(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Write `[github].check_workflows_on_push = <enabled>`
    doc["github"]["check_workflows_on_push"] = toml_edit::value(enabled);

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist `github.actionlint_on_patch = <enabled>`.
pub fn set_github_actionlint_on_patch(
    code_home: &Path,
    enabled: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["github"]["actionlint_on_patch"] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist `[validation.groups.<group>] = <enabled>`.
pub fn set_validation_group_enabled(
    code_home: &Path,
    group: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["validation"]["groups"][group] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist `[validation.tools.<tool>] = <enabled>`.
pub fn set_validation_tool_enabled(
    code_home: &Path,
    tool: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["validation"]["tools"][tool] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist per-project access mode under `[projects."<path>"]` with
/// `approval_policy` and `sandbox_mode`.
pub fn set_project_access_mode(
    code_home: &Path,
    project_path: &Path,
    approval: AskForApproval,
    sandbox_mode: SandboxMode,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Ensure projects table and the per-project table exist
    let project_key = project_path.to_string_lossy().to_string();
    // Ensure `projects` is a table; if key exists but is not a table, replace it.
    let has_projects_table = doc
        .as_table()
        .get("projects")
        .and_then(|i| i.as_table())
        .is_some();
    if !has_projects_table {
        doc["projects"] = TomlItem::Table(toml_edit::Table::new());
    }
    let Some(projects_tbl) = doc["projects"].as_table_mut() else {
        return Err(anyhow::anyhow!("failed to prepare projects table"));
    };
    // Ensure per-project entry exists and is a table; replace if wrong type.
    let needs_proj_table = projects_tbl
        .get(project_key.as_str())
        .and_then(|i| i.as_table())
        .is_none();
    if needs_proj_table {
        projects_tbl.insert(project_key.as_str(), TomlItem::Table(toml_edit::Table::new()));
    }
    let proj_tbl = projects_tbl
        .get_mut(project_key.as_str())
        .and_then(|i| i.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!(format!("failed to create projects.{project_key} table")))?;

    // Write fields
    proj_tbl.insert(
        "approval_policy",
        TomlItem::Value(toml_edit::Value::from(format!("{approval}"))),
    );
    proj_tbl.insert(
        "sandbox_mode",
        TomlItem::Value(toml_edit::Value::from(format!("{sandbox_mode}"))),
    );

    // Harmonize trust_level with selected access mode:
    // - Full Access (Never + DangerFullAccess): set trust_level = "trusted" so future runs
    //   default to non-interactive behavior when no overrides are present.
    // - Other modes: remove trust_level to avoid conflicting with per-project policy.
    let full_access = matches!(
        (approval, sandbox_mode),
        (AskForApproval::Never, SandboxMode::DangerFullAccess)
    );
    if full_access {
        proj_tbl.insert(
            "trust_level",
            TomlItem::Value(toml_edit::Value::from("trusted")),
        );
    } else {
        proj_tbl.remove("trust_level");
    }

    // Ensure home exists; write atomically
    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;

    Ok(())
}

/// Append a command pattern to `[projects."<path>"].always_allow_commands`.
pub fn add_project_allowed_command(
    code_home: &Path,
    project_path: &Path,
    command: &[String],
    match_kind: ApprovedCommandMatchKind,
) -> anyhow::Result<()> {
    let command = crate::command_canonicalization::normalize_command_for_persistence(command);
    if command.is_empty() {
        return Ok(());
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let project_key = project_path.to_string_lossy().to_string();
    if doc
        .as_table()
        .get("projects")
        .and_then(|i| i.as_table())
        .is_none()
    {
        doc["projects"] = TomlItem::Table(TomlTable::new());
    }

    let Some(projects_tbl) = doc["projects"].as_table_mut() else {
        return Err(anyhow::anyhow!("failed to prepare projects table"));
    };

    if projects_tbl
        .get(project_key.as_str())
        .and_then(|i| i.as_table())
        .is_none()
    {
        projects_tbl.insert(project_key.as_str(), TomlItem::Table(TomlTable::new()));
    }

    let project_tbl = projects_tbl
        .get_mut(project_key.as_str())
        .and_then(|i| i.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!(format!("failed to create projects.{project_key} table")))?;

    let mut argv_array = TomlArray::new();
    for arg in &command {
        argv_array.push(arg.clone());
    }

    let mut table = TomlTable::new();
    table.insert("argv", TomlItem::Value(toml_edit::Value::Array(argv_array)));
    let match_str = match match_kind {
        ApprovedCommandMatchKind::Exact => "exact",
        ApprovedCommandMatchKind::Prefix => "prefix",
    };
    table.insert(
        "match_kind",
        TomlItem::Value(toml_edit::Value::from(match_str)),
    );

    if let Some(existing) = project_tbl
        .get_mut("always_allow_commands")
        .and_then(|item| item.as_array_of_tables_mut())
    {
        let exists = existing.iter().any(|tbl| {
            let argv_match = tbl
                .get("argv")
                .and_then(|item| item.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let match_kind = tbl
                .get("match_kind")
                .and_then(|item| item.as_str())
                .unwrap_or("exact");
            argv_match == command && match_kind.eq_ignore_ascii_case(match_str)
        });
        if !exists {
            existing.push(table);
        }
    } else {
        let mut arr = TomlArrayOfTables::new();
        arr.push(table);
        project_tbl.insert("always_allow_commands", TomlItem::ArrayOfTables(arr));
    }

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;

    Ok(())
}

/// List MCP servers from `CODEX_HOME/config.toml`.
/// Returns `(enabled, disabled)` lists of `(name, McpServerConfig)`.
type NamedMcpServer = (String, McpServerConfig);
type McpServerListPair = (Vec<NamedMcpServer>, Vec<NamedMcpServer>);

pub fn list_mcp_servers(code_home: &Path) -> anyhow::Result<McpServerListPair> {
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let doc_str = std::fs::read_to_string(&read_path).unwrap_or_default();
    let doc = doc_str.parse::<DocumentMut>().unwrap_or_else(|_| DocumentMut::new());

    fn parse_duration_field(
        table: &toml_edit::Table,
        key: &str,
    ) -> anyhow::Result<Option<Duration>> {
        let Some(item) = table.get(key) else {
            return Ok(None);
        };
        if let Some(f) = item.as_float() {
            return Ok(Some(Duration::try_from_secs_f64(f)?));
        }
        if let Some(i) = item.as_integer() {
            if i < 0 {
                return Err(anyhow::anyhow!("{key} must be non-negative"));
            }
            return Ok(Some(Duration::from_secs(i as u64)));
        }
        Err(anyhow::anyhow!("{key} must be a number (seconds)"))
    }

    fn parse_u32_field(
        table: &toml_edit::Table,
        key: &str,
    ) -> anyhow::Result<Option<u32>> {
        let Some(item) = table.get(key) else {
            return Ok(None);
        };
        let Some(i) = item.as_integer() else {
            return Err(anyhow::anyhow!("{key} must be an integer"));
        };
        if i < 0 {
            return Err(anyhow::anyhow!("{key} must be non-negative"));
        }
        if i > i64::from(u32::MAX) {
            return Err(anyhow::anyhow!("{key} is too large"));
        }
        Ok(Some(i as u32))
    }

    fn parse_duration_value(value: &toml_edit::Value, key: &str) -> anyhow::Result<Duration> {
        if let Some(f) = value.as_float() {
            return Ok(Duration::try_from_secs_f64(f)?);
        }
        if let Some(i) = value.as_integer() {
            if i < 0 {
                return Err(anyhow::anyhow!("{key} must be non-negative"));
            }
            return Ok(Duration::from_secs(i as u64));
        }
        Err(anyhow::anyhow!("{key} must be a number (seconds)"))
    }

    fn parse_duration_inline_field(
        table: &toml_edit::InlineTable,
        key: &str,
    ) -> anyhow::Result<Option<Duration>> {
        let Some(value) = table.get(key) else {
            return Ok(None);
        };
        parse_duration_value(value, key).map(Some)
    }

    fn parse_u32_inline_field(
        table: &toml_edit::InlineTable,
        key: &str,
    ) -> anyhow::Result<Option<u32>> {
        let Some(value) = table.get(key) else {
            return Ok(None);
        };
        let Some(i) = value.as_integer() else {
            return Err(anyhow::anyhow!("{key} must be an integer"));
        };
        if i < 0 {
            return Err(anyhow::anyhow!("{key} must be non-negative"));
        }
        if i > i64::from(u32::MAX) {
            return Err(anyhow::anyhow!("{key} is too large"));
        }
        Ok(Some(i as u32))
    }

    fn parse_mcp_scheduling(item: &toml_edit::Item) -> anyhow::Result<McpServerSchedulingToml> {
        let mut scheduling = McpServerSchedulingToml::default();

        if let Some(tbl) = item.as_table() {
            if let Some(dispatch) = tbl.get("dispatch").and_then(toml_edit::Item::as_str) {
                scheduling.dispatch = match dispatch.trim().to_ascii_lowercase().as_str() {
                    "exclusive" => McpDispatchMode::Exclusive,
                    "parallel" => McpDispatchMode::Parallel,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "scheduling.dispatch must be 'exclusive' or 'parallel'",
                        ));
                    }
                };
            }

            if let Some(max) = parse_u32_field(tbl, "max_concurrent")? {
                if max == 0 {
                    return Err(anyhow::anyhow!("scheduling.max_concurrent must be >= 1"));
                }
                scheduling.max_concurrent = max;
            }

            scheduling.min_interval_sec = parse_duration_field(tbl, "min_interval_sec")?;
            scheduling.queue_timeout_sec = parse_duration_field(tbl, "queue_timeout_sec")?;

            if let Some(depth) = parse_u32_field(tbl, "max_queue_depth")? {
                if depth == 0 {
                    return Err(anyhow::anyhow!("scheduling.max_queue_depth must be >= 1"));
                }
                scheduling.max_queue_depth = Some(depth);
            }

            return Ok(scheduling);
        }

        if let Some(tbl) = item.as_inline_table() {
            if let Some(dispatch) = tbl.get("dispatch").and_then(toml_edit::Value::as_str) {
                scheduling.dispatch = match dispatch.trim().to_ascii_lowercase().as_str() {
                    "exclusive" => McpDispatchMode::Exclusive,
                    "parallel" => McpDispatchMode::Parallel,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "scheduling.dispatch must be 'exclusive' or 'parallel'",
                        ));
                    }
                };
            }

            if let Some(max) = parse_u32_inline_field(tbl, "max_concurrent")? {
                if max == 0 {
                    return Err(anyhow::anyhow!("scheduling.max_concurrent must be >= 1"));
                }
                scheduling.max_concurrent = max;
            }

            scheduling.min_interval_sec = parse_duration_inline_field(tbl, "min_interval_sec")?;
            scheduling.queue_timeout_sec =
                parse_duration_inline_field(tbl, "queue_timeout_sec")?;

            if let Some(depth) = parse_u32_inline_field(tbl, "max_queue_depth")? {
                if depth == 0 {
                    return Err(anyhow::anyhow!("scheduling.max_queue_depth must be >= 1"));
                }
                scheduling.max_queue_depth = Some(depth);
            }

            return Ok(scheduling);
        }

        Err(anyhow::anyhow!(
            "scheduling for an MCP server must be a table",
        ))
    }

    fn table_to_list(tbl: &toml_edit::Table) -> anyhow::Result<Vec<(String, McpServerConfig)>> {
        let mut out = Vec::new();
        for (name, item) in tbl.iter() {
            if let Some(t) = item.as_table() {
                let transport = if let Some(command) = t.get("command").and_then(|v| v.as_str()) {
                    let args: Vec<String> = t
                        .get("args")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|i| i.as_str().map(ToString::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    let env = t
                        .get("env")
                        .and_then(|v| {
                            if let Some(tbl) = v.as_inline_table() {
                                Some(
                                    tbl.iter()
                                        .filter_map(|(k, v)| {
                                            v.as_str().map(|s| (k.to_string(), s.to_string()))
                                        })
                                        .collect::<HashMap<_, _>>(),
                                )
                            } else { v.as_table().map(|table| table
                                        .iter()
                                        .filter_map(|(k, v)| {
                                            v.as_str().map(|s| (k.to_string(), s.to_string()))
                                        })
                                        .collect::<HashMap<_, _>>()) }
                        });

                    McpServerTransportConfig::Stdio {
                        command: command.to_string(),
                        args,
                        env,
                    }
                } else if let Some(url) = t.get("url").and_then(|v| v.as_str()) {
                    let bearer_token = t
                        .get("bearer_token")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string);
                    let oauth_resource = t
                        .get("oauth_resource")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string);

                    let bearer_token_env_var = t
                        .get("bearer_token_env_var")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string);

                    let table_string_map = |item: &toml_edit::Item| {
                        item.as_inline_table()
                            .map(|tbl| {
                                tbl.iter()
                                    .filter_map(|(k, v)| {
                                        v.as_str().map(|s| (k.to_string(), s.to_string()))
                                    })
                                    .collect::<HashMap<_, _>>()
                            })
                            .or_else(|| {
                                item.as_table().map(|table| {
                                    table
                                        .iter()
                                        .filter_map(|(k, v)| {
                                            v.as_str().map(|s| (k.to_string(), s.to_string()))
                                        })
                                        .collect::<HashMap<_, _>>()
                                })
                            })
                    };

                    let http_headers = t.get("http_headers").and_then(table_string_map);
                    let env_http_headers = t.get("env_http_headers").and_then(table_string_map);

                    McpServerTransportConfig::StreamableHttp {
                        url: url.to_string(),
                        bearer_token,
                        oauth_resource,
                        bearer_token_env_var,
                        http_headers,
                        env_http_headers,
                    }
                } else {
                    continue;
                };

                let startup_timeout_sec = match parse_duration_field(t, "startup_timeout_sec")? {
                    Some(duration) => Some(duration),
                    None => t
                        .get("startup_timeout_ms")
                        .and_then(toml_edit::Item::as_integer)
                        .map(|ms| {
                            if ms < 0 {
                                Err(anyhow::anyhow!(
                                    "startup_timeout_ms must be non-negative",
                                ))
                            } else {
                                Ok(Duration::from_millis(ms as u64))
                            }
                        })
                        .transpose()?,
                };

                let tool_timeout_sec = parse_duration_field(t, "tool_timeout_sec")?;

                let scheduling = match t.get("scheduling") {
                    Some(item) => parse_mcp_scheduling(item)?,
                    None => McpServerSchedulingToml::default(),
                };

                let mut tool_scheduling: BTreeMap<String, McpToolSchedulingOverrideToml> = BTreeMap::new();
                if let Some(tool_sched_item) = t.get("tool_scheduling") {
                    let Some(tool_sched_tbl) = tool_sched_item.as_table() else {
                        return Err(anyhow::anyhow!(
                            "tool_scheduling for MCP server '{name}' must be a table",
                        ));
                    };
                    for (tool_name_raw, override_item) in tool_sched_tbl.iter() {
                        let tool_name = tool_name_raw.trim();
                        if tool_name.is_empty() {
                            return Err(anyhow::anyhow!(
                                "tool_scheduling keys cannot be empty",
                            ));
                        }
                        if tool_scheduling.contains_key(tool_name) {
                            return Err(anyhow::anyhow!(
                                "duplicated tool_scheduling entry for '{tool_name}'",
                            ));
                        }

                        let override_cfg = if let Some(override_tbl) = override_item.as_table() {
                            let max_concurrent = parse_u32_field(override_tbl, "max_concurrent")?;
                            if max_concurrent == Some(0) {
                                return Err(anyhow::anyhow!(
                                    "tool_scheduling.{tool_name}.max_concurrent must be >= 1",
                                ));
                            }
                            let min_interval_sec =
                                parse_duration_field(override_tbl, "min_interval_sec")?;
                            McpToolSchedulingOverrideToml {
                                max_concurrent,
                                min_interval_sec,
                            }
                        } else if let Some(override_tbl) = override_item.as_inline_table() {
                            let max_concurrent = parse_u32_inline_field(override_tbl, "max_concurrent")?;
                            if max_concurrent == Some(0) {
                                return Err(anyhow::anyhow!(
                                    "tool_scheduling.{tool_name}.max_concurrent must be >= 1",
                                ));
                            }
                            let min_interval_sec = parse_duration_inline_field(override_tbl, "min_interval_sec")?;
                            McpToolSchedulingOverrideToml {
                                max_concurrent,
                                min_interval_sec,
                            }
                        } else {
                            return Err(anyhow::anyhow!(
                                "tool_scheduling.{tool_name} must be a table",
                            ));
                        };

                        if override_cfg.is_empty() {
                            continue;
                        }

                        tool_scheduling.insert(tool_name.to_string(), override_cfg);
                    }
                }

                let mut disabled_tools: Vec<String> = t
                    .get("disabled_tools")
                    .and_then(toml_edit::Item::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| item.as_str().map(str::trim))
                            .filter(|name| !name.is_empty())
                            .map(ToString::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                disabled_tools.sort();
                disabled_tools.dedup();

                out.push((
                    name.to_string(),
                    McpServerConfig {
                        transport,
                        startup_timeout_sec,
                        tool_timeout_sec,
                        scheduling,
                        tool_scheduling,
                        disabled_tools,
                    },
                ));
            }
        }
        Ok(out)
    }

    let enabled = match doc
        .as_table()
        .get("mcp_servers")
        .and_then(|i| i.as_table())
    {
        Some(table) => table_to_list(table)?,
        None => Vec::new(),
    };

    let disabled = match doc
        .as_table()
        .get("mcp_servers_disabled")
        .and_then(|i| i.as_table())
    {
        Some(table) => table_to_list(table)?,
        None => Vec::new(),
    };

    // Merge in enabled plugin-provided MCP servers.
    // Explicit config entries win on name collisions, including disabled entries.
    let mut server_names: std::collections::HashSet<String> = enabled
        .iter()
        .map(|(name, _cfg)| name.clone())
        .collect();
    server_names.extend(disabled.iter().map(|(name, _cfg)| name.clone()));

    let plugin_manager = crate::plugins::PluginsManager::new(code_home.to_path_buf());
    let mut plugin_servers: Vec<(String, McpServerConfig)> = plugin_manager
        .effective_mcp_servers()
        .into_iter()
        .filter(|(name, _cfg)| !server_names.contains(name))
        .collect();
    plugin_servers.sort_by(|a, b| a.0.cmp(&b.0));

    let mut enabled = enabled;
    enabled.extend(plugin_servers);

    Ok((enabled, disabled))
}

/// Add or update an MCP server under `[mcp_servers.<name>]`. If the same
/// server exists under `mcp_servers_disabled`, it will be removed from there.
pub fn add_mcp_server(
    code_home: &Path,
    name: &str,
    cfg: McpServerConfig,
) -> anyhow::Result<()> {
    // Validate server name for safety and compatibility with MCP tool naming.
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(anyhow::anyhow!(
            "invalid server name '{name}': must match ^[a-zA-Z0-9_-]+$"
        ));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Ensure target tables exist
    if !doc.as_table().contains_key("mcp_servers") {
        doc["mcp_servers"] = TomlItem::Table(toml_edit::Table::new());
    }
    let Some(tbl) = doc["mcp_servers"].as_table_mut() else {
        return Err(anyhow::anyhow!("mcp_servers must be a table"));
    };

    let McpServerConfig {
        transport,
        startup_timeout_sec,
        tool_timeout_sec,
        scheduling,
        tool_scheduling,
        disabled_tools,
    } = cfg;

    // Build table for this server
    let mut server_tbl = toml_edit::Table::new();
    match transport {
        McpServerTransportConfig::Stdio { command, args, env } => {
            server_tbl.insert("command", toml_edit::value(command));
            if !args.is_empty() {
                let mut arr = toml_edit::Array::new();
                for a in args {
                    arr.push(toml_edit::Value::from(a));
                }
                server_tbl.insert("args", TomlItem::Value(toml_edit::Value::Array(arr)));
            }
            if let Some(env) = env {
                let mut it = toml_edit::InlineTable::new();
                for (k, v) in env {
                    it.insert(&k, toml_edit::Value::from(v));
                }
                server_tbl.insert("env", TomlItem::Value(toml_edit::Value::InlineTable(it)));
            }
        }
        McpServerTransportConfig::StreamableHttp {
            url,
            bearer_token,
            oauth_resource,
            bearer_token_env_var,
            http_headers,
            env_http_headers,
        } => {
            server_tbl.insert("url", toml_edit::value(url));
            if let Some(token) = bearer_token {
                server_tbl.insert("bearer_token", toml_edit::value(token));
            }
            if let Some(resource) = oauth_resource {
                server_tbl.insert("oauth_resource", toml_edit::value(resource));
            }
            if let Some(env_var) = bearer_token_env_var {
                server_tbl.insert("bearer_token_env_var", toml_edit::value(env_var));
            }
            if let Some(http_headers) = http_headers
                && !http_headers.is_empty()
            {
                let mut it = toml_edit::InlineTable::new();
                for (k, v) in http_headers {
                    it.insert(&k, toml_edit::Value::from(v));
                }
                server_tbl.insert(
                    "http_headers",
                    TomlItem::Value(toml_edit::Value::InlineTable(it)),
                );
            }
            if let Some(env_http_headers) = env_http_headers
                && !env_http_headers.is_empty()
            {
                let mut it = toml_edit::InlineTable::new();
                for (k, v) in env_http_headers {
                    it.insert(&k, toml_edit::Value::from(v));
                }
                server_tbl.insert(
                    "env_http_headers",
                    TomlItem::Value(toml_edit::Value::InlineTable(it)),
                );
            }
        }
    }

    if let Some(duration) = startup_timeout_sec {
        server_tbl.insert("startup_timeout_sec", toml_edit::value(duration.as_secs_f64()));
    }
    if let Some(duration) = tool_timeout_sec {
        server_tbl.insert("tool_timeout_sec", toml_edit::value(duration.as_secs_f64()));
    }
    if !disabled_tools.is_empty() {
        let mut arr = toml_edit::Array::new();
        for tool in disabled_tools {
            arr.push(toml_edit::Value::from(tool));
        }
        server_tbl.insert(
            "disabled_tools",
            TomlItem::Value(toml_edit::Value::Array(arr)),
        );
    }

    let default_scheduling = McpServerSchedulingToml::default();
    if scheduling != default_scheduling {
        let mut sched_table = toml_edit::Table::new();
        sched_table.set_implicit(false);
        if scheduling.dispatch != default_scheduling.dispatch {
            sched_table["dispatch"] = toml_edit::value(scheduling.dispatch.to_string());
        }
        if scheduling.max_concurrent != default_scheduling.max_concurrent {
            sched_table["max_concurrent"] = toml_edit::value(scheduling.max_concurrent as i64);
        }
        if let Some(duration) = scheduling.min_interval_sec {
            sched_table["min_interval_sec"] = toml_edit::value(duration.as_secs_f64());
        }
        if let Some(duration) = scheduling.queue_timeout_sec {
            sched_table["queue_timeout_sec"] = toml_edit::value(duration.as_secs_f64());
        }
        if let Some(depth) = scheduling.max_queue_depth {
            sched_table["max_queue_depth"] = toml_edit::value(depth as i64);
        }
        server_tbl.insert("scheduling", TomlItem::Table(sched_table));
    }

    if !tool_scheduling.is_empty() {
        let mut tool_sched_tbl = toml_edit::Table::new();
        tool_sched_tbl.set_implicit(false);
        for (tool, override_cfg) in tool_scheduling {
            if override_cfg.is_empty() {
                continue;
            }
            let mut override_tbl = toml_edit::Table::new();
            override_tbl.set_implicit(false);
            if let Some(max) = override_cfg.max_concurrent {
                override_tbl["max_concurrent"] = toml_edit::value(max as i64);
            }
            if let Some(duration) = override_cfg.min_interval_sec {
                override_tbl["min_interval_sec"] = toml_edit::value(duration.as_secs_f64());
            }
            tool_sched_tbl[tool.as_str()] = TomlItem::Table(override_tbl);
        }
        if !tool_sched_tbl.is_empty() {
            server_tbl.insert("tool_scheduling", TomlItem::Table(tool_sched_tbl));
        }
    }

    // Write into enabled table
    tbl.insert(name, TomlItem::Table(server_tbl));

    // Remove from disabled if present
    if let Some(disabled_tbl) = doc["mcp_servers_disabled"].as_table_mut() {
        disabled_tbl.remove(name);
    }

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Enable/disable an MCP server by moving it between `[mcp_servers]` and
/// `[mcp_servers_disabled]`. Returns `true` if a change was made.
pub fn set_mcp_server_enabled(
    code_home: &Path,
    name: &str,
    enabled: bool,
) -> anyhow::Result<bool> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Helper to ensure table exists
    fn ensure_table<'a>(doc: &'a mut DocumentMut, key: &'a str) -> &'a mut toml_edit::Table {
        if !doc.as_table().contains_key(key) {
            doc[key] = TomlItem::Table(toml_edit::Table::new());
        }
        match doc[key].as_table_mut() {
            Some(table) => table,
            None => panic!("table key '{key}' should be a table"),
        }
    }

    let mut changed = false;
    if enabled {
        // Move from disabled -> enabled
        let moved = {
            let disabled_tbl = ensure_table(&mut doc, "mcp_servers_disabled");
            disabled_tbl.remove(name)
        };
        if let Some(item) = moved {
            let enabled_tbl = ensure_table(&mut doc, "mcp_servers");
            enabled_tbl.insert(name, item);
            changed = true;
        }
    } else {
        // Move from enabled -> disabled
        let moved = {
            let enabled_tbl = ensure_table(&mut doc, "mcp_servers");
            enabled_tbl.remove(name)
        };
        if let Some(item) = moved {
            let disabled_tbl = ensure_table(&mut doc, "mcp_servers_disabled");
            disabled_tbl.insert(name, item);
            changed = true;
        }
    }

    if changed {
        std::fs::create_dir_all(code_home)?;
        let tmp = NamedTempFile::new_in(code_home)?;
        std::fs::write(tmp.path(), doc.to_string())?;
        tmp.persist(config_path)?;
    }

    Ok(changed)
}

/// Enable/disable a specific MCP tool for a named server.
/// Returns `true` when the persisted config changed.
pub fn set_mcp_server_tool_enabled(
    code_home: &Path,
    server_name: &str,
    tool_name: &str,
    enabled: bool,
) -> anyhow::Result<bool> {
    let normalized_tool = tool_name.trim();
    if normalized_tool.is_empty() {
        return Err(anyhow::anyhow!("tool name cannot be empty"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    fn find_server_table_mut<'a>(
        doc: &'a mut DocumentMut,
        server_name: &str,
    ) -> Option<&'a mut toml_edit::Table> {
        let section_key = if doc
            .as_table()
            .get("mcp_servers")
            .and_then(toml_edit::Item::as_table)
            .and_then(|table| table.get(server_name))
            .is_some()
        {
            Some("mcp_servers")
        } else if doc
            .as_table()
            .get("mcp_servers_disabled")
            .and_then(toml_edit::Item::as_table)
            .and_then(|table| table.get(server_name))
            .is_some()
        {
            Some("mcp_servers_disabled")
        } else {
            None
        }?;

        doc.as_table_mut()
            .get_mut(section_key)
            .and_then(toml_edit::Item::as_table_mut)
            .and_then(|section| section.get_mut(server_name))
            .and_then(toml_edit::Item::as_table_mut)
    }

    let Some(server_table) = find_server_table_mut(&mut doc, server_name) else {
        return Err(anyhow::anyhow!("MCP server '{server_name}' not found"));
    };

    let mut disabled_tools: Vec<String> = server_table
        .get("disabled_tools")
        .and_then(toml_edit::Item::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(str::trim))
                .filter(|name| !name.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    let mut changed = false;
    if enabled {
        let prev_len = disabled_tools.len();
        disabled_tools.retain(|name| name != normalized_tool);
        changed = prev_len != disabled_tools.len();
    } else if !disabled_tools
        .iter()
        .any(|name| name == normalized_tool)
    {
        disabled_tools.push(normalized_tool.to_string());
        changed = true;
    }

    if !changed {
        return Ok(false);
    }

    disabled_tools.sort();
    disabled_tools.dedup();
    if disabled_tools.is_empty() {
        server_table.remove("disabled_tools");
    } else {
        let mut arr = toml_edit::Array::new();
        for tool in disabled_tools {
            arr.push(toml_edit::Value::from(tool));
        }
        server_table["disabled_tools"] = TomlItem::Value(toml_edit::Value::Array(arr));
    }

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;

    Ok(true)
}

pub fn set_mcp_server_scheduling(
    code_home: &Path,
    server_name: &str,
    scheduling: McpServerSchedulingToml,
) -> anyhow::Result<()> {
    if scheduling.max_concurrent == 0 {
        return Err(anyhow::anyhow!("scheduling.max_concurrent must be >= 1"));
    }
    if let Some(depth) = scheduling.max_queue_depth
        && depth == 0
    {
        return Err(anyhow::anyhow!("scheduling.max_queue_depth must be >= 1"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    fn find_server_table_mut<'a>(
        doc: &'a mut DocumentMut,
        server_name: &str,
    ) -> Option<&'a mut toml_edit::Table> {
        let section_key = if doc
            .as_table()
            .get("mcp_servers")
            .and_then(toml_edit::Item::as_table)
            .and_then(|table| table.get(server_name))
            .is_some()
        {
            Some("mcp_servers")
        } else if doc
            .as_table()
            .get("mcp_servers_disabled")
            .and_then(toml_edit::Item::as_table)
            .and_then(|table| table.get(server_name))
            .is_some()
        {
            Some("mcp_servers_disabled")
        } else {
            None
        }?;

        doc.as_table_mut()
            .get_mut(section_key)
            .and_then(toml_edit::Item::as_table_mut)
            .and_then(|section| section.get_mut(server_name))
            .and_then(toml_edit::Item::as_table_mut)
    }

    let Some(server_table) = find_server_table_mut(&mut doc, server_name) else {
        return Err(anyhow::anyhow!("MCP server '{server_name}' not found"));
    };

    let default_scheduling = McpServerSchedulingToml::default();
    if scheduling == default_scheduling {
        server_table.remove("scheduling");
    } else {
        let mut sched_table = toml_edit::Table::new();
        sched_table.set_implicit(false);
        if scheduling.dispatch != default_scheduling.dispatch {
            sched_table["dispatch"] = toml_edit::value(scheduling.dispatch.to_string());
        }
        if scheduling.max_concurrent != default_scheduling.max_concurrent {
            sched_table["max_concurrent"] =
                toml_edit::value(scheduling.max_concurrent as i64);
        }
        if let Some(duration) = scheduling.min_interval_sec {
            sched_table["min_interval_sec"] = toml_edit::value(duration.as_secs_f64());
        }
        if let Some(duration) = scheduling.queue_timeout_sec {
            sched_table["queue_timeout_sec"] = toml_edit::value(duration.as_secs_f64());
        }
        if let Some(depth) = scheduling.max_queue_depth {
            sched_table["max_queue_depth"] = toml_edit::value(depth as i64);
        }
        server_table["scheduling"] = TomlItem::Table(sched_table);
    }

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;

    Ok(())
}

pub fn set_mcp_tool_scheduling_override(
    code_home: &Path,
    server_name: &str,
    tool_name: &str,
    override_cfg: Option<McpToolSchedulingOverrideToml>,
) -> anyhow::Result<()> {
    let normalized_tool = tool_name.trim();
    if normalized_tool.is_empty() {
        return Err(anyhow::anyhow!("tool name cannot be empty"));
    }

    if let Some(cfg) = override_cfg.as_ref()
        && cfg.max_concurrent == Some(0)
    {
        return Err(anyhow::anyhow!(
            "tool_scheduling.{normalized_tool}.max_concurrent must be >= 1",
        ));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    fn find_server_table_mut<'a>(
        doc: &'a mut DocumentMut,
        server_name: &str,
    ) -> Option<&'a mut toml_edit::Table> {
        let section_key = if doc
            .as_table()
            .get("mcp_servers")
            .and_then(toml_edit::Item::as_table)
            .and_then(|table| table.get(server_name))
            .is_some()
        {
            Some("mcp_servers")
        } else if doc
            .as_table()
            .get("mcp_servers_disabled")
            .and_then(toml_edit::Item::as_table)
            .and_then(|table| table.get(server_name))
            .is_some()
        {
            Some("mcp_servers_disabled")
        } else {
            None
        }?;

        doc.as_table_mut()
            .get_mut(section_key)
            .and_then(toml_edit::Item::as_table_mut)
            .and_then(|section| section.get_mut(server_name))
            .and_then(toml_edit::Item::as_table_mut)
    }

    let Some(server_table) = find_server_table_mut(&mut doc, server_name) else {
        return Err(anyhow::anyhow!("MCP server '{server_name}' not found"));
    };

    let override_cfg = override_cfg.filter(|cfg| !cfg.is_empty());

    if let Some(cfg) = override_cfg {
        let tool_sched_item = server_table
            .entry("tool_scheduling")
            .or_insert_with(|| {
                let mut t = toml_edit::Table::new();
                t.set_implicit(false);
                TomlItem::Table(t)
            });
        let Some(tool_sched_table) = tool_sched_item.as_table_mut() else {
            return Err(anyhow::anyhow!(
                "tool_scheduling for MCP server '{server_name}' must be a table",
            ));
        };

        let mut override_tbl = toml_edit::Table::new();
        override_tbl.set_implicit(false);
        if let Some(max) = cfg.max_concurrent {
            override_tbl["max_concurrent"] = toml_edit::value(max as i64);
        }
        if let Some(duration) = cfg.min_interval_sec {
            override_tbl["min_interval_sec"] = toml_edit::value(duration.as_secs_f64());
        }
        tool_sched_table[normalized_tool] = TomlItem::Table(override_tbl);
    } else {
        let should_remove = server_table
            .get_mut("tool_scheduling")
            .and_then(toml_edit::Item::as_table_mut)
            .and_then(|tool_sched| tool_sched.remove(normalized_tool))
            .is_some();
        if should_remove {
            let remove_whole_table = server_table
                .get("tool_scheduling")
                .and_then(toml_edit::Item::as_table)
                .map(toml_edit::Table::is_empty)
                .unwrap_or(false);
            if remove_whole_table {
                server_table.remove("tool_scheduling");
            }
        } else {
            // No-op: nothing to clear.
            return Ok(());
        }
    }

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;

    Ok(())
}

/// Apply a single dotted-path override onto a TOML value.
fn env_path(var: &str) -> std::io::Result<Option<PathBuf>> {
    match std::env::var(var) {
        Ok(val) if !val.trim().is_empty() => {
            let canonical = PathBuf::from(val).canonicalize()?;
            Ok(Some(canonical))
        }
        _ => Ok(None),
    }
}

fn env_overrides_present() -> bool {
    matches!(std::env::var("CODE_HOME"), Ok(ref v) if !v.trim().is_empty())
        || matches!(std::env::var("CODEX_HOME"), Ok(ref v) if !v.trim().is_empty())
}

fn default_code_home_dir() -> Option<PathBuf> {
    let mut path = home_dir()?;
    path.push(".code");
    Some(path)
}

fn compute_legacy_code_home_dir() -> Option<PathBuf> {
    if env_overrides_present() {
        return None;
    }
    let home = home_dir()?;
    let candidate = home.join(".codex");
    if path_exists(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

fn legacy_code_home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    {
        compute_legacy_code_home_dir()
    }

    #[cfg(not(test))]
    {
        static LEGACY: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
        LEGACY
            .get_or_init(compute_legacy_code_home_dir)
            .clone()
    }
}

fn path_exists(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
}

fn find_repo_dev_code_home() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    for ancestor in cwd.ancestors() {
        // Limit this fallback to local source checkouts so regular users are
        // unaffected unless they intentionally run from the codebase.
        let repo_marker = ancestor.join("code-rs/Cargo.toml");
        if !path_exists(&repo_marker) {
            continue;
        }

        let dev_code_home = ancestor.join(".code");
        let dev_config = dev_code_home.join(CONFIG_TOML_FILE);
        if path_exists(&dev_config) {
            return Some(dev_code_home);
        }
    }
    None
}

/// Resolve the filesystem path used for *reading* Codex state that may live in
/// a legacy `~/.codex` directory. Writes should continue targeting `code_home`.
pub fn resolve_code_path_for_read(code_home: &Path, relative: &Path) -> PathBuf {
    let default_path = code_home.join(relative);

    if env_overrides_present() {
        return default_path;
    }

    if path_exists(&default_path) {
        return default_path;
    }

    if let Some(default_home) = default_code_home_dir()
        && default_home != code_home {
            return default_path;
        }

    if let Some(legacy) = legacy_code_home_dir() {
        let candidate = legacy.join(relative);
        if path_exists(&candidate) {
            return candidate;
        }
    }

    default_path
}

/// Returns the path to the Code/Codex configuration directory, which can be
/// specified by the `CODE_HOME` or `CODEX_HOME` environment variables. If not set,
/// defaults to `~/.code` for the fork.
///
/// - If `CODE_HOME` or `CODEX_HOME` is set, the value will be canonicalized and this
///   function will Err if the path does not exist.
/// - If neither is set, this function does not verify that the directory exists.
pub fn find_code_home() -> std::io::Result<PathBuf> {
    if let Some(path) = env_path("CODE_HOME")? {
        return Ok(path);
    }

    if let Some(path) = env_path("CODEX_HOME")? {
        return Ok(path);
    }

    if let Some(dev_code_home) = find_repo_dev_code_home() {
        return Ok(dev_code_home);
    }

    let home = home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find home directory",
        )
    })?;

    let mut write_path = home;
    write_path.push(".code");
    Ok(write_path)
}

pub(crate) fn load_instructions(code_dir: Option<&Path>) -> Option<String> {
    let code_home = code_dir?;
    let read_path = resolve_code_path_for_read(code_home, Path::new("AGENTS.md"));

    let contents = match std::fs::read_to_string(&read_path) {
        Ok(s) => s,
        Err(_) => {
            if env_overrides_present() {
                return None;
            }
            let legacy_home = legacy_code_home_dir()?;
            let legacy_path = legacy_home.join("AGENTS.md");
            match std::fs::read_to_string(&legacy_path) {
                Ok(s) => s,
                Err(_) => return None,
            }
        }
    };

    let trimmed = contents.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn read_override_file(
    path: Option<&Path>,
    cwd: &Path,
    description: &str,
) -> std::io::Result<Option<String>> {
    let p = match path {
        None => return Ok(None),
        Some(p) => p,
    };

    // Resolve relative paths against the provided cwd to make CLI
    // overrides consistent regardless of where the process was launched
    // from.
    let full_path = if p.is_relative() {
        cwd.join(p)
    } else {
        p.to_path_buf()
    };

    let contents = std::fs::read_to_string(&full_path).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("failed to read {description} {}: {e}", full_path.display()),
        )
    })?;

    let s = contents.trim().to_string();
    if s.is_empty() {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{description} is empty: {}", full_path.display()),
        ))
    } else {
        Ok(Some(s))
    }
}

pub(crate) fn get_base_instructions(
    path: Option<&Path>,
    cwd: &Path,
) -> std::io::Result<Option<String>> {
    read_override_file(path, cwd, "experimental instructions file")
}

pub(crate) fn get_compact_prompt_override(
    path: Option<&Path>,
    cwd: &Path,
) -> std::io::Result<Option<String>> {
    read_override_file(path, cwd, "compact prompt override file")
}

pub fn set_network_proxy_settings(
    code_home: &Path,
    settings: &super::NetworkProxySettingsToml,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let network_table = doc["network"]
        .or_insert(TomlItem::Table(TomlTable::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("`network` must be a TOML table"))?;

    network_table["enabled"] = toml_edit::value(settings.enabled);
    network_table["proxy_url"] = toml_edit::value(settings.proxy_url.clone());
    network_table["admin_url"] = toml_edit::value(settings.admin_url.clone());
    network_table["enable_socks5"] = toml_edit::value(settings.enable_socks5);
    network_table["socks_url"] = toml_edit::value(settings.socks_url.clone());
    network_table["enable_socks5_udp"] = toml_edit::value(settings.enable_socks5_udp);
    network_table["allow_upstream_proxy"] = toml_edit::value(settings.allow_upstream_proxy);
    network_table["dangerously_allow_non_loopback_proxy"] =
        toml_edit::value(settings.dangerously_allow_non_loopback_proxy);
    network_table["dangerously_allow_non_loopback_admin"] =
        toml_edit::value(settings.dangerously_allow_non_loopback_admin);

    let mode_label = match settings.mode {
        super::NetworkModeToml::Limited => "limited",
        super::NetworkModeToml::Full => "full",
    };
    network_table["mode"] = toml_edit::value(mode_label);

    let _ = write_string_array(network_table, "allowed_domains", &settings.allowed_domains)?;
    let _ = write_string_array(network_table, "denied_domains", &settings.denied_domains)?;
    if settings.allow_unix_sockets.is_empty() {
        network_table.remove("allow_unix_sockets");
    } else {
        let mut deduped: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for value in &settings.allow_unix_sockets {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen.insert(trimmed.to_string()) {
                deduped.push(trimmed.to_string());
            }
        }

        if deduped.is_empty() {
            network_table.remove("allow_unix_sockets");
        } else {
            let mut array = TomlArray::new();
            for value in &deduped {
                array.push(value.as_str());
            }
            network_table["allow_unix_sockets"] = toml_edit::value(array);
        }
    }
    network_table["allow_local_binding"] = toml_edit::value(settings.allow_local_binding);

    if network_table.is_empty() {
        doc.as_table_mut().remove("network");
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;
    Ok(())
}

/// Persist memories settings into `CODEX_HOME/config.toml` at `[memories]`.
pub fn set_memories_settings(code_home: &Path, settings: &MemoriesConfig) -> anyhow::Result<()> {
    set_global_memories_settings(code_home, Some(&settings.to_toml()))
}

fn load_config_doc(code_home: &Path) -> anyhow::Result<(DocumentMut, PathBuf)> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };
    Ok((doc, config_path))
}

fn persist_config_doc(code_home: &Path, config_path: &Path, doc: &DocumentMut) -> anyhow::Result<()> {
    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;
    Ok(())
}

fn apply_memories_table(table: &mut TomlTable, settings: &MemoriesToml) {
    write_optional_bool(table, "no_memories_if_mcp_or_web_search", settings.no_memories_if_mcp_or_web_search);
    write_optional_bool(table, "generate_memories", settings.generate_memories);
    write_optional_bool(table, "use_memories", settings.use_memories);
    write_optional_usize(
        table,
        "max_raw_memories_for_consolidation",
        settings.max_raw_memories_for_consolidation,
    );
    write_optional_i64(table, "max_rollout_age_days", settings.max_rollout_age_days);
    write_optional_usize(
        table,
        "max_rollouts_per_startup",
        settings.max_rollouts_per_startup,
    );
    write_optional_i64(table, "min_rollout_idle_hours", settings.min_rollout_idle_hours);

    table.remove("max_raw_memories_for_global");
    table.remove("max_unused_days");
    table.remove("phase_1_model");
    table.remove("phase_2_model");
    table.remove("extract_model");
    table.remove("consolidation_model");
}

fn write_optional_bool(table: &mut TomlTable, key: &str, value: Option<bool>) {
    match value {
        Some(value) => table[key] = toml_edit::value(value),
        None => {
            table.remove(key);
        }
    }
}

fn write_optional_usize(table: &mut TomlTable, key: &str, value: Option<usize>) {
    match value {
        Some(value) => table[key] = toml_edit::value(value as i64),
        None => {
            table.remove(key);
        }
    }
}

fn write_optional_i64(table: &mut TomlTable, key: &str, value: Option<i64>) {
    match value {
        Some(value) => table[key] = toml_edit::value(value),
        None => {
            table.remove(key);
        }
    }
}

pub fn set_global_memories_settings(
    code_home: &Path,
    settings: Option<&MemoriesToml>,
) -> anyhow::Result<()> {
    let (mut doc, config_path) = load_config_doc(code_home)?;

    match settings.filter(|settings| !settings.is_empty()) {
        Some(settings) => {
            let memories_table = doc["memories"]
                .or_insert(TomlItem::Table(TomlTable::new()))
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("`memories` must be a TOML table"))?;
            apply_memories_table(memories_table, settings);
            if memories_table.is_empty() {
                doc.as_table_mut().remove("memories");
            }
        }
        None => {
            doc.as_table_mut().remove("memories");
        }
    }

    persist_config_doc(code_home, &config_path, &doc)?;
    Ok(())
}

pub fn set_profile_memories_settings(
    code_home: &Path,
    profile_name: &str,
    settings: Option<&MemoriesToml>,
) -> anyhow::Result<()> {
    let (mut doc, config_path) = load_config_doc(code_home)?;
    let profiles_table = doc["profiles"]
        .or_insert(TomlItem::Table(TomlTable::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("`profiles` must be a TOML table"))?;
    let profile_table = profiles_table[profile_name]
        .or_insert(TomlItem::Table(TomlTable::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("`profiles.{profile_name}` must be a TOML table"))?;

    match settings.filter(|settings| !settings.is_empty()) {
        Some(settings) => {
            let memories_table = profile_table["memories"]
                .or_insert(TomlItem::Table(TomlTable::new()))
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("`profiles.{profile_name}.memories` must be a TOML table"))?;
            apply_memories_table(memories_table, settings);
            if memories_table.is_empty() {
                profile_table.remove("memories");
            }
        }
        None => {
            profile_table.remove("memories");
        }
    }

    persist_config_doc(code_home, &config_path, &doc)?;
    Ok(())
}

pub fn set_project_memories_settings(
    code_home: &Path,
    project_path: &Path,
    settings: Option<&MemoriesToml>,
) -> anyhow::Result<()> {
    let (mut doc, config_path) = load_config_doc(code_home)?;
    let project_key = project_path.to_string_lossy().to_string();
    let projects_table = doc["projects"]
        .or_insert(TomlItem::Table(TomlTable::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("`projects` must be a TOML table"))?;
    let project_table = projects_table[project_key.as_str()]
        .or_insert(TomlItem::Table(TomlTable::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("`projects.{project_key}` must be a TOML table"))?;

    match settings.filter(|settings| !settings.is_empty()) {
        Some(settings) => {
            let memories_table = project_table["memories"]
                .or_insert(TomlItem::Table(TomlTable::new()))
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("`projects.{project_key}.memories` must be a TOML table"))?;
            apply_memories_table(memories_table, settings);
            if memories_table.is_empty() {
                project_table.remove("memories");
            }
        }
        None => {
            project_table.remove("memories");
        }
    }

    persist_config_doc(code_home, &config_path, &doc)?;
    Ok(())
}

pub fn set_windows_sandbox_mode(
    code_home: &Path,
    profile_name: Option<&str>,
    mode: Option<WindowsSandboxModeToml>,
) -> anyhow::Result<()> {
    let (mut doc, config_path) = load_config_doc(code_home)?;
    let windows_table = if let Some(profile_name) = profile_name {
        let profiles_table = doc["profiles"]
            .or_insert(TomlItem::Table(TomlTable::new()))
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("`profiles` must be a TOML table"))?;
        let profile_table = profiles_table[profile_name]
            .or_insert(TomlItem::Table(TomlTable::new()))
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("`profiles.{profile_name}` must be a TOML table"))?;
        profile_table["windows"]
            .or_insert(TomlItem::Table(TomlTable::new()))
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("`profiles.{profile_name}.windows` must be a TOML table"))?
    } else {
        doc["windows"]
            .or_insert(TomlItem::Table(TomlTable::new()))
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("`windows` must be a TOML table"))?
    };

    match mode {
        Some(mode) => {
            let value = match mode {
                WindowsSandboxModeToml::Unelevated => "unelevated",
                WindowsSandboxModeToml::Elevated => "elevated",
            };
            windows_table["sandbox"] = toml_edit::value(value);
        }
        None => {
            windows_table.remove("sandbox");
        }
    }

    if windows_table.is_empty() {
        if let Some(profile_name) = profile_name {
            if let Some(profile_table) = doc["profiles"][profile_name].as_table_mut() {
                profile_table.remove("windows");
            }
        } else {
            doc.as_table_mut().remove("windows");
        }
    }

    persist_config_doc(code_home, &config_path, &doc)?;
    Ok(())
}

fn write_exec_limit_value(
    table: &mut TomlTable,
    key: &str,
    value: super::ExecLimitToml,
) -> anyhow::Result<()> {
    match value {
        super::ExecLimitToml::Mode(super::ExecLimitModeToml::Auto) => {
            table.remove(key);
        }
        super::ExecLimitToml::Mode(super::ExecLimitModeToml::Disabled) => {
            table[key] = toml_edit::value("disabled");
        }
        super::ExecLimitToml::Value(v) => {
            let value_i64: i64 = v
                .try_into()
                .map_err(|_| anyhow::anyhow!("{key} is too large"))?;
            table[key] = toml_edit::value(value_i64);
        }
    }
    Ok(())
}

/// Persist execution limits into `CODEX_HOME/config.toml` at `[exec_limits]`.
pub fn set_exec_limits_settings(
    code_home: &Path,
    settings: &super::ExecLimitsToml,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let exec_table = doc["exec_limits"]
        .or_insert(TomlItem::Table(TomlTable::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("`exec_limits` must be a TOML table"))?;

    write_exec_limit_value(exec_table, "pids_max", settings.pids_max)?;
    write_exec_limit_value(exec_table, "memory_max_mb", settings.memory_max_mb)?;

    if exec_table.is_empty() {
        doc.as_table_mut().remove("exec_limits");
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;
    Ok(())
}

/// Persist `js_repl` runtime settings into `CODEX_HOME/config.toml` at `[tools]`.
pub fn set_js_repl_settings(
    code_home: &Path,
    settings: &super::JsReplSettingsToml,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let tools_table = doc["tools"]
        .or_insert(TomlItem::Table(TomlTable::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("`tools` must be a TOML table"))?;

    tools_table["js_repl"] = toml_edit::value(settings.enabled);

    let runtime = match settings.runtime {
        super::JsReplRuntimeKindToml::Node => "node",
        super::JsReplRuntimeKindToml::Deno => "deno",
    };
    tools_table["js_repl_runtime"] = toml_edit::value(runtime);

    match settings.runtime_path.as_ref() {
        Some(path) => {
            tools_table["js_repl_runtime_path"] =
                toml_edit::value(path.to_string_lossy().trim().to_string());
        }
        None => {
            tools_table.remove("js_repl_runtime_path");
        }
    }

    let _ = write_exact_string_array(
        tools_table,
        "js_repl_runtime_args",
        &settings.runtime_args,
    )?;
    let _ = write_path_array(
        tools_table,
        "js_repl_node_module_dirs",
        &settings.node_module_dirs,
    )?;

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;
    Ok(())
}
