use crate::config::resolve_code_path_for_read;
use crate::config_types::{
    AppsSourcesModeToml,
    AppsSourcesToml,
    PluginMarketplaceRepoToml,
    PluginsToml,
    SubagentCommandConfig,
};
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;
use tempfile::NamedTempFile;
use toml_edit::ArrayOfTables;
use toml_edit::DocumentMut;
use toml_edit::Item as TomlItem;
use toml_edit::Table as TomlTable;
use toml_edit::value;

pub const CONFIG_KEY_MODEL: &str = "model";
pub const CONFIG_KEY_EFFORT: &str = "model_reasoning_effort";
const CONFIG_TOML_FILE: &str = "config.toml";

#[derive(Copy, Clone)]
enum NoneBehavior {
    Skip,
    Remove,
}

/// Persist overrides into `config.toml` using explicit key segments per
/// override. This avoids ambiguity with keys that contain dots or spaces.
pub async fn persist_overrides(
    code_home: &Path,
    profile: Option<&str>,
    overrides: &[(&[&str], &str)],
) -> Result<()> {
    let with_options: Vec<(&[&str], Option<&str>)> = overrides
        .iter()
        .map(|(segments, value)| (*segments, Some(*value)))
        .collect();

    persist_overrides_with_behavior(code_home, profile, &with_options, NoneBehavior::Skip).await
}

/// Persist overrides into `config.toml` at the document root, ignoring any active
/// profile. This is intended for settings that must remain global even when a
/// profile is selected (for example: auth credential storage backend).
pub async fn persist_root_overrides(code_home: &Path, overrides: &[(&[&str], &str)]) -> Result<()> {
    let with_options: Vec<(&[&str], Option<&str>)> = overrides
        .iter()
        .map(|(segments, value)| (*segments, Some(*value)))
        .collect();

    persist_root_overrides_with_behavior(code_home, &with_options, NoneBehavior::Skip).await
}

/// Persist overrides where values may be optional. Any entries with `None`
/// values are skipped. If all values are `None`, this becomes a no-op and
/// returns `Ok(())` without touching the file.
pub async fn persist_non_null_overrides(
    code_home: &Path,
    profile: Option<&str>,
    overrides: &[(&[&str], Option<&str>)],
) -> Result<()> {
    persist_overrides_with_behavior(code_home, profile, overrides, NoneBehavior::Skip).await
}

/// Persist overrides where `None` values clear any existing values from the
/// configuration file.
pub async fn persist_overrides_and_clear_if_none(
    code_home: &Path,
    profile: Option<&str>,
    overrides: &[(&[&str], Option<&str>)],
) -> Result<()> {
    persist_overrides_with_behavior(code_home, profile, overrides, NoneBehavior::Remove).await
}

pub async fn set_session_context_settings(
    code_home: &Path,
    profile: Option<&str>,
    context_window: Option<u64>,
    auto_compact_token_limit: Option<i64>,
) -> Result<()> {
    let context_window_text = context_window.map(|value| value.to_string());
    let auto_compact_text = auto_compact_token_limit.map(|value| value.to_string());
    persist_overrides_and_clear_if_none(
        code_home,
        profile,
        &[
            (&["model_context_window"], context_window_text.as_deref()),
            (
                &["model_auto_compact_token_limit"],
                auto_compact_text.as_deref(),
            ),
        ],
    )
    .await
}

pub async fn set_feature_flags(
    code_home: &Path,
    profile: Option<&str>,
    updates: &BTreeMap<String, bool>,
) -> Result<bool> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    let mut mutated = false;

    let target: &mut TomlTable = if let Some(profile) = profile {
        let root = doc.as_table_mut();

        let profiles_item = match root.get_mut("profiles") {
            Some(item) => item,
            None => {
                if updates.is_empty() {
                    return Ok(false);
                }
                root.insert("profiles", TomlItem::Table(new_implicit_table()));
                root.get_mut("profiles")
                    .ok_or_else(|| anyhow::anyhow!("missing profiles table"))?
            }
        };
        if profiles_item.as_table_mut().is_none() {
            if updates.is_empty() {
                return Ok(false);
            }
            *profiles_item = TomlItem::Table(new_implicit_table());
            mutated = true;
        }

        let profiles_table = profiles_item
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("profiles item is not a table"))?;

        let profile_item = match profiles_table.get_mut(profile) {
            Some(item) => item,
            None => {
                if updates.is_empty() {
                    return Ok(false);
                }
                profiles_table.insert(profile, TomlItem::Table(new_implicit_table()));
                mutated = true;
                profiles_table
                    .get_mut(profile)
                    .ok_or_else(|| anyhow::anyhow!("missing profile table"))?
            }
        };

        if profile_item.as_table_mut().is_none() {
            if updates.is_empty() {
                return Ok(false);
            }
            *profile_item = TomlItem::Table(new_implicit_table());
            mutated = true;
        }

        profile_item
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("profile item is not a table"))?
    } else {
        doc.as_table_mut()
    };

    if updates.is_empty() {
        if target.remove("features").is_some() {
            mutated = true;
        }
    } else {
        let features_item = match target.get_mut("features") {
            Some(item) => item,
            None => {
                target.insert("features", TomlItem::Table(new_implicit_table()));
                mutated = true;
                target
                    .get_mut("features")
                    .ok_or_else(|| anyhow::anyhow!("missing features table"))?
            }
        };

        if features_item.as_table_mut().is_none() {
            *features_item = TomlItem::Table(new_implicit_table());
            mutated = true;
        }

        let features_table = features_item
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("features item is not a table"))?;

        for (key, value_bool) in updates {
            let previous = features_table
                .get(key)
                .and_then(toml_edit::Item::as_bool);
            if previous != Some(*value_bool) {
                mutated = true;
            }
            features_table[key] = value(*value_bool);
        }
    }

    if !mutated {
        return Ok(false);
    }

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;

    Ok(true)
}

pub async fn set_shell_escalation_paths(
    code_home: &Path,
    zsh_path: Option<&str>,
    main_execve_wrapper_exe: Option<&str>,
) -> Result<bool> {
    fn normalize(value: Option<&str>) -> Option<&str> {
        value.map(str::trim).filter(|v| !v.is_empty())
    }

    let zsh_path = normalize(zsh_path);
    let main_execve_wrapper_exe = normalize(main_execve_wrapper_exe);

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let read_result = tokio::fs::read_to_string(&read_path).await;
    let mut doc = match read_result {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if zsh_path.is_none() && main_execve_wrapper_exe.is_none() {
                return Ok(false);
            }
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    let root = doc.as_table_mut();
    let mut mutated = false;

    match zsh_path {
        Some(value_text) => {
            let previous = root.get("zsh_path").and_then(|item| item.as_str());
            if previous != Some(value_text) {
                root["zsh_path"] = value(value_text);
                mutated = true;
            }
        }
        None => {
            if root.remove("zsh_path").is_some() {
                mutated = true;
            }
        }
    }

    match main_execve_wrapper_exe {
        Some(value_text) => {
            let previous = root
                .get("main_execve_wrapper_exe")
                .and_then(|item| item.as_str());
            if previous != Some(value_text) {
                root["main_execve_wrapper_exe"] = value(value_text);
                mutated = true;
            }
        }
        None => {
            if root.remove("main_execve_wrapper_exe").is_some() {
                mutated = true;
            }
        }
    }

    if !mutated {
        return Ok(false);
    }

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;

    Ok(true)
}

pub async fn set_shell_escalation_settings(
    code_home: &Path,
    profile: Option<&str>,
    enabled: bool,
    zsh_path: Option<&str>,
    main_execve_wrapper_exe: Option<&str>,
) -> Result<bool> {
    fn normalize(value: Option<&str>) -> Option<&str> {
        value.map(str::trim).filter(|v| !v.is_empty())
    }

    let zsh_path = normalize(zsh_path);
    let main_execve_wrapper_exe = normalize(main_execve_wrapper_exe);

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let read_result = tokio::fs::read_to_string(&read_path).await;
    let mut doc = match read_result {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    let root = doc.as_table_mut();
    let mut mutated = false;

    {
        let target: &mut TomlTable = if let Some(profile) = profile {
            let profiles_item = match root.get_mut("profiles") {
                Some(item) => item,
                None => {
                    root.insert("profiles", TomlItem::Table(new_implicit_table()));
                    mutated = true;
                    root.get_mut("profiles")
                        .ok_or_else(|| anyhow::anyhow!("missing profiles table"))?
                }
            };
            if profiles_item.as_table_mut().is_none() {
                *profiles_item = TomlItem::Table(new_implicit_table());
                mutated = true;
            }

            let profiles_table = profiles_item
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("profiles item is not a table"))?;

            let profile_item = match profiles_table.get_mut(profile) {
                Some(item) => item,
                None => {
                    profiles_table.insert(profile, TomlItem::Table(new_implicit_table()));
                    mutated = true;
                    profiles_table
                        .get_mut(profile)
                        .ok_or_else(|| anyhow::anyhow!("missing profile table"))?
                }
            };

            if profile_item.as_table_mut().is_none() {
                *profile_item = TomlItem::Table(new_implicit_table());
                mutated = true;
            }

            profile_item
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("profile item is not a table"))?
        } else {
            root
        };

        let features_item = match target.get_mut("features") {
            Some(item) => item,
            None => {
                target.insert("features", TomlItem::Table(new_implicit_table()));
                mutated = true;
                target
                    .get_mut("features")
                    .ok_or_else(|| anyhow::anyhow!("missing features table"))?
            }
        };
        if features_item.as_table_mut().is_none() {
            *features_item = TomlItem::Table(new_implicit_table());
            mutated = true;
        }

        let features_table = features_item
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("features item is not a table"))?;

        let previous = features_table
            .get("shell_zsh_fork")
            .and_then(toml_edit::Item::as_bool);
        if previous != Some(enabled) {
            mutated = true;
        }
        features_table["shell_zsh_fork"] = value(enabled);
    }

    match zsh_path {
        Some(value_text) => {
            let previous = root.get("zsh_path").and_then(|item| item.as_str());
            if previous != Some(value_text) {
                root["zsh_path"] = value(value_text);
                mutated = true;
            }
        }
        None => {
            if root.remove("zsh_path").is_some() {
                mutated = true;
            }
        }
    }

    match main_execve_wrapper_exe {
        Some(value_text) => {
            let previous = root
                .get("main_execve_wrapper_exe")
                .and_then(|item| item.as_str());
            if previous != Some(value_text) {
                root["main_execve_wrapper_exe"] = value(value_text);
                mutated = true;
            }
        }
        None => {
            if root.remove("main_execve_wrapper_exe").is_some() {
                mutated = true;
            }
        }
    }

    if !mutated {
        return Ok(false);
    }

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;

    Ok(true)
}

/// Apply a single override onto a `toml_edit` document while preserving
/// existing formatting/comments.
/// The key is expressed as explicit segments to correctly handle keys that
/// contain dots or spaces.
fn apply_toml_edit_override_segments(
    doc: &mut DocumentMut,
    segments: &[&str],
    value: toml_edit::Item,
) {
    use toml_edit::Item;

    if segments.is_empty() {
        return;
    }

    let mut current = doc.as_table_mut();
    for seg in &segments[..segments.len() - 1] {
        if !current.contains_key(seg) {
            current[*seg] = Item::Table(toml_edit::Table::new());
            if let Some(t) = current[*seg].as_table_mut() {
                t.set_implicit(true);
            }
        }

        let maybe_item = current.get_mut(seg);
        let Some(item) = maybe_item else { return };

        if !item.is_table() {
            *item = Item::Table(toml_edit::Table::new());
            if let Some(t) = item.as_table_mut() {
                t.set_implicit(true);
            }
        }

        let Some(tbl) = item.as_table_mut() else {
            return;
        };
        current = tbl;
    }

    let last = segments[segments.len() - 1];
    current[last] = value;
}

fn new_implicit_table() -> TomlTable {
    let mut table = TomlTable::new();
    table.set_implicit(true);
    table
}

fn normalize_skill_config_path(path: &Path) -> String {
    dunce::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// Set or clear a skill override entry in `config.toml`.
///
/// Semantics follow upstream codex-rs:
/// - `enabled=true` removes any matching override entry.
/// - `enabled=false` creates/updates an override entry with `enabled=false`.
pub async fn set_skill_config(code_home: &Path, skill_path: &Path, enabled: bool) -> Result<bool> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    let normalized_path = normalize_skill_config_path(skill_path);
    let mut remove_skills_table = false;
    let mut mutated = false;

    {
        let root = doc.as_table_mut();
        let skills_item = match root.get_mut("skills") {
            Some(item) => item,
            None => {
                if enabled {
                    return Ok(false);
                }
                root.insert("skills", TomlItem::Table(new_implicit_table()));
                root.get_mut("skills").ok_or_else(|| anyhow::anyhow!("missing skills table"))?
            }
        };

        if skills_item.as_table_mut().is_none() {
            if enabled {
                return Ok(false);
            }
            *skills_item = TomlItem::Table(new_implicit_table());
        }

        let skills_table = skills_item
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("skills item is not a table"))?;

        let config_item = match skills_table.get_mut("config") {
            Some(item) => item,
            None => {
                if enabled {
                    return Ok(false);
                }
                skills_table.insert("config", TomlItem::ArrayOfTables(ArrayOfTables::new()));
                skills_table
                    .get_mut("config")
                    .ok_or_else(|| anyhow::anyhow!("missing skills.config"))?
            }
        };

        if !matches!(config_item, TomlItem::ArrayOfTables(_)) {
            if enabled {
                return Ok(false);
            }
            *config_item = TomlItem::ArrayOfTables(ArrayOfTables::new());
        }

        let TomlItem::ArrayOfTables(overrides) = config_item else {
            return Ok(false);
        };

        let existing_index = overrides.iter().enumerate().find_map(|(idx, table)| {
            table
                .get("path")
                .and_then(|item| item.as_str())
                .map(Path::new)
                .map(normalize_skill_config_path)
                .filter(|value| *value == normalized_path)
                .map(|_| idx)
        });

        if enabled {
            if let Some(index) = existing_index {
                overrides.remove(index);
                mutated = true;
                if overrides.is_empty() {
                    skills_table.remove("config");
                    if skills_table.is_empty() {
                        remove_skills_table = true;
                    }
                }
            }
        } else if let Some(index) = existing_index {
            for (idx, table) in overrides.iter_mut().enumerate() {
                if idx == index {
                    table["path"] = value(normalized_path);
                    table["enabled"] = value(false);
                    mutated = true;
                    break;
                }
            }
        } else {
            let mut entry = TomlTable::new();
            entry.set_implicit(false);
            entry["path"] = value(normalized_path);
            entry["enabled"] = value(false);
            overrides.push(entry);
            mutated = true;
        }
    }

    if remove_skills_table {
        let root = doc.as_table_mut();
        root.remove("skills");
    }

    if !mutated {
        return Ok(false);
    }

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;

    Ok(true)
}

/// Ensure `[plugins."<plugin_key>"].enabled = <enabled>` exists in `config.toml`.
///
/// Semantics follow upstream codex-rs plugin install/uninstall flows:
/// - install writes `enabled=true`
/// - uninstall clears the entire `plugins."<plugin_key>"` entry
pub async fn set_plugin_enabled(code_home: &Path, plugin_key: &str, enabled: bool) -> Result<bool> {
    if plugin_key.trim().is_empty() {
        anyhow::bail!("plugin key must not be empty");
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    let mut mutated = false;
    {
        let root = doc.as_table_mut();
        let plugins_item = match root.get_mut("plugins") {
            Some(item) => item,
            None => {
                root.insert("plugins", TomlItem::Table(new_implicit_table()));
                root.get_mut("plugins")
                    .ok_or_else(|| anyhow::anyhow!("missing plugins table"))?
            }
        };

        if plugins_item.as_table_mut().is_none() {
            *plugins_item = TomlItem::Table(new_implicit_table());
        }

        let plugins_table = plugins_item
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("plugins item is not a table"))?;

        match plugins_table.get_mut(plugin_key) {
            Some(item) => {
                if let Some(entry) = item.as_table_mut() {
                    let previous = entry
                        .get("enabled")
                        .and_then(toml_edit::Item::as_bool);
                    if previous != Some(enabled) {
                        mutated = true;
                    }
                    entry["enabled"] = value(enabled);
                } else {
                    let mut entry = TomlTable::new();
                    entry.set_implicit(false);
                    entry["enabled"] = value(enabled);
                    *item = TomlItem::Table(entry);
                    mutated = true;
                }
            }
            None => {
                let mut entry = TomlTable::new();
                entry.set_implicit(false);
                entry["enabled"] = value(enabled);
                plugins_table.insert(plugin_key, TomlItem::Table(entry));
                mutated = true;
            }
        }
    }

    if !mutated {
        return Ok(false);
    }

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;

    Ok(true)
}

/// Apply a batch of plugin config edits to `config.toml` in a single write.
///
/// - Every `plugin_key` in `set_enabled_keys` is ensured to have `enabled=true`.
/// - Every `plugin_key` in `clear_keys` is removed from `[plugins]`.
pub async fn apply_plugin_config_updates(
    code_home: &Path,
    set_enabled_keys: &[String],
    clear_keys: &[String],
) -> Result<bool> {
    if set_enabled_keys.is_empty() && clear_keys.is_empty() {
        return Ok(false);
    }

    for plugin_key in set_enabled_keys.iter().chain(clear_keys) {
        if plugin_key.trim().is_empty() {
            anyhow::bail!("plugin key must not be empty");
        }
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if set_enabled_keys.is_empty() {
                return Ok(false);
            }
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    let mut mutated = false;
    {
        let root = doc.as_table_mut();
        let plugins_item = match root.get_mut("plugins") {
            Some(item) => item,
            None => {
                if set_enabled_keys.is_empty() {
                    return Ok(false);
                }
                root.insert("plugins", TomlItem::Table(new_implicit_table()));
                root.get_mut("plugins")
                    .ok_or_else(|| anyhow::anyhow!("missing plugins table"))?
            }
        };

        if plugins_item.as_table_mut().is_none() {
            *plugins_item = TomlItem::Table(new_implicit_table());
            mutated = true;
        }

        let plugins_table = plugins_item
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("plugins item is not a table"))?;

        for plugin_key in clear_keys {
            if plugins_table.remove(plugin_key).is_some() {
                mutated = true;
            }
        }

        for plugin_key in set_enabled_keys {
            match plugins_table.get_mut(plugin_key) {
                Some(item) => {
                    if let Some(entry) = item.as_table_mut() {
                        let previous = entry
                            .get("enabled")
                            .and_then(toml_edit::Item::as_bool);
                        if previous != Some(true) {
                            mutated = true;
                        }
                        entry["enabled"] = value(true);
                    } else {
                        let mut entry = TomlTable::new();
                        entry.set_implicit(false);
                        entry["enabled"] = value(true);
                        *item = TomlItem::Table(entry);
                        mutated = true;
                    }
                }
                None => {
                    let mut entry = TomlTable::new();
                    entry.set_implicit(false);
                    entry["enabled"] = value(true);
                    plugins_table.insert(plugin_key, TomlItem::Table(entry));
                    mutated = true;
                }
            }
        }

        if mutated && plugins_table.is_empty() {
            root.remove("plugins");
        }
    }

    if !mutated {
        return Ok(false);
    }

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;

    Ok(true)
}

/// Remove `plugins."<plugin_key>"` from `config.toml`.
pub async fn clear_plugin_config(code_home: &Path, plugin_key: &str) -> Result<bool> {
    if plugin_key.trim().is_empty() {
        anyhow::bail!("plugin key must not be empty");
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };

    let mut mutated = false;
    {
        let root = doc.as_table_mut();
        let Some(plugins_item) = root.get_mut("plugins") else {
            return Ok(false);
        };
        let Some(plugins_table) = plugins_item.as_table_mut() else {
            return Ok(false);
        };

        if plugins_table.remove(plugin_key).is_some() {
            mutated = true;
        }

        if mutated && plugins_table.is_empty() {
            root.remove("plugins");
        }
    }

    if !mutated {
        return Ok(false);
    }

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;

    Ok(true)
}

fn normalize_marketplace_repo_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn normalize_marketplace_repo_ref(git_ref: Option<&str>) -> Option<String> {
    git_ref
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(std::string::ToString::to_string)
}

fn parse_marketplace_repos_from_item(item: Option<&TomlItem>) -> Vec<PluginMarketplaceRepoToml> {
    let Some(TomlItem::ArrayOfTables(tables)) = item else {
        return Vec::new();
    };

    let mut repos = Vec::new();
    for table in tables.iter() {
        let Some(url) = table.get("url").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(url) = normalize_marketplace_repo_url(url) else {
            continue;
        };
        let git_ref = table
            .get("ref")
            .and_then(|value| value.as_str())
            .and_then(|value| normalize_marketplace_repo_ref(Some(value)));
        repos.push(PluginMarketplaceRepoToml { url, git_ref });
    }
    repos
}

fn build_marketplace_repos_item(repos: &[PluginMarketplaceRepoToml]) -> Option<TomlItem> {
    if repos.is_empty() {
        return None;
    }

    let mut overrides = ArrayOfTables::new();
    for repo in repos {
        let Some(url) = normalize_marketplace_repo_url(&repo.url) else {
            continue;
        };
        let mut entry = TomlTable::new();
        entry.set_implicit(false);
        entry["url"] = value(url);
        if let Some(git_ref) = normalize_marketplace_repo_ref(repo.git_ref.as_deref()) {
            entry["ref"] = value(git_ref);
        }
        overrides.push(entry);
    }

    (!overrides.is_empty()).then_some(TomlItem::ArrayOfTables(overrides))
}

/// Persist plugin marketplace sources into `config.toml` at the document root.
///
/// - Writes `plugins.curated_repo_url` / `plugins.curated_repo_ref` (clearing both if URL is missing).
/// - Writes `[[plugins.marketplace_repos]]` entries from `sources.marketplace_repos`.
/// - Preserves any existing `[plugins."<plugin_key>"]` subtables.
pub async fn set_plugin_marketplace_sources(code_home: &Path, sources: &PluginsToml) -> Result<bool> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let has_sources = sources
                .curated_repo_url
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
                || !sources.marketplace_repos.is_empty();
            if !has_sources {
                return Ok(false);
            }
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    let curated_url = sources
        .curated_repo_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(std::string::ToString::to_string);
    let curated_ref = if curated_url.is_some() {
        normalize_marketplace_repo_ref(sources.curated_repo_ref.as_deref())
    } else {
        None
    };

    let desired_marketplace_repos = sources
        .marketplace_repos
        .iter()
        .filter_map(|repo| {
            normalize_marketplace_repo_url(&repo.url).map(|url| PluginMarketplaceRepoToml {
                url,
                git_ref: normalize_marketplace_repo_ref(repo.git_ref.as_deref()),
            })
        })
        .collect::<Vec<_>>();

    let mut mutated = false;
    {
        let root = doc.as_table_mut();
        let plugins_item = match root.get_mut("plugins") {
            Some(item) => item,
            None => {
                if curated_url.is_none() && desired_marketplace_repos.is_empty() {
                    return Ok(false);
                }
                root.insert("plugins", TomlItem::Table(new_implicit_table()));
                root.get_mut("plugins")
                    .ok_or_else(|| anyhow::anyhow!("missing plugins table"))?
            }
        };

        if plugins_item.as_table_mut().is_none() {
            *plugins_item = TomlItem::Table(new_implicit_table());
            mutated = true;
        }

        let plugins_table = plugins_item
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("plugins item is not a table"))?;

        match curated_url.as_deref() {
            None => {
                if plugins_table.remove("curated_repo_url").is_some() {
                    mutated = true;
                }
                if plugins_table.remove("curated_repo_ref").is_some() {
                    mutated = true;
                }
            }
            Some(url) => {
                let previous = plugins_table
                    .get("curated_repo_url")
                    .and_then(|item| item.as_str());
                if previous != Some(url) {
                    plugins_table["curated_repo_url"] = value(url);
                    mutated = true;
                }

                match curated_ref.as_deref() {
                    None => {
                        if plugins_table.remove("curated_repo_ref").is_some() {
                            mutated = true;
                        }
                    }
                    Some(git_ref) => {
                        let previous = plugins_table
                            .get("curated_repo_ref")
                            .and_then(|item| item.as_str());
                        if previous != Some(git_ref) {
                            plugins_table["curated_repo_ref"] = value(git_ref);
                            mutated = true;
                        }
                    }
                }
            }
        }

        let existing_repos =
            parse_marketplace_repos_from_item(plugins_table.get("marketplace_repos"));
        if existing_repos != desired_marketplace_repos {
            match build_marketplace_repos_item(&desired_marketplace_repos) {
                Some(item) => {
                    plugins_table["marketplace_repos"] = item;
                }
                None => {
                    let _ = plugins_table.remove("marketplace_repos");
                }
            }
            mutated = true;
        }
    }

    if !mutated {
        return Ok(false);
    }

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;

    Ok(true)
}

fn normalize_account_id_list(ids: &[String]) -> Vec<String> {
    ids.iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

fn apps_sources_is_default(sources: &AppsSourcesToml) -> bool {
    sources.mode == AppsSourcesModeToml::default() && sources.pinned_account_ids.is_empty()
}

/// Persist `[apps._sources]` into `config.toml`.
///
/// - Writes under `profiles.<active_profile>.apps._sources` when profile is set; otherwise root.
/// - Removes `apps._sources` entirely when it matches defaults.
/// - Preserves any existing `[apps.<app_id>]` subtables.
pub async fn set_apps_sources(
    code_home: &Path,
    profile: Option<&str>,
    sources: &AppsSourcesToml,
) -> Result<bool> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if apps_sources_is_default(sources) {
                return Ok(false);
            }
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    let effective_profile = match profile {
        Some(profile) => Some(profile.to_string()),
        None => doc
            .get("profile")
            .and_then(|item| item.as_str())
            .map(std::string::ToString::to_string),
    };

    let normalized_pins = normalize_account_id_list(&sources.pinned_account_ids);
    let mut desired = sources.clone();
    desired.pinned_account_ids = normalized_pins;

    let mut mutated = false;
    {
        let root = doc.as_table_mut();
        let base: &mut TomlTable = if let Some(profile) = effective_profile.as_deref() {
            if !root.contains_key("profiles") {
                root.insert("profiles", TomlItem::Table(new_implicit_table()));
                mutated = true;
            }
            let profiles_item = root
                .get_mut("profiles")
                .ok_or_else(|| anyhow::anyhow!("missing profiles table"))?;
            if profiles_item.as_table_mut().is_none() {
                *profiles_item = TomlItem::Table(new_implicit_table());
                mutated = true;
            }
            let profiles_table = profiles_item
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("profiles item is not a table"))?;

            if !profiles_table.contains_key(profile) {
                profiles_table.insert(profile, TomlItem::Table(new_implicit_table()));
                mutated = true;
            }
            let profile_item = profiles_table
                .get_mut(profile)
                .ok_or_else(|| anyhow::anyhow!("missing profile table"))?;
            if profile_item.as_table_mut().is_none() {
                *profile_item = TomlItem::Table(new_implicit_table());
                mutated = true;
            }
            profile_item
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("profile item is not a table"))?
        } else {
            root
        };

        let apps_item = match base.get_mut("apps") {
            Some(item) => item,
            None => {
                if apps_sources_is_default(&desired) {
                    return Ok(false);
                }
                base.insert("apps", TomlItem::Table(new_implicit_table()));
                mutated = true;
                base.get_mut("apps")
                    .ok_or_else(|| anyhow::anyhow!("missing apps table"))?
            }
        };

        if apps_item.as_table_mut().is_none() {
            *apps_item = TomlItem::Table(new_implicit_table());
            mutated = true;
        }

        let apps_table = apps_item
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("apps item is not a table"))?;

        if apps_sources_is_default(&desired) {
            if apps_table.remove("_sources").is_some() {
                mutated = true;
            }
        } else {
            let sources_item = match apps_table.get_mut("_sources") {
                Some(item) => item,
                None => {
                    apps_table.insert("_sources", TomlItem::Table(new_implicit_table()));
                    mutated = true;
                    apps_table
                        .get_mut("_sources")
                        .ok_or_else(|| anyhow::anyhow!("missing apps._sources"))?
                }
            };

            if sources_item.as_table_mut().is_none() {
                *sources_item = TomlItem::Table(new_implicit_table());
                mutated = true;
            }

            let sources_table = sources_item
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("apps._sources is not a table"))?;

            if desired.mode == AppsSourcesModeToml::default() {
                if sources_table.remove("mode").is_some() {
                    mutated = true;
                }
            } else {
                let desired_mode = match desired.mode {
                    AppsSourcesModeToml::ActiveOnly => "active_only",
                    AppsSourcesModeToml::ActivePlusPinned => "active_plus_pinned",
                    AppsSourcesModeToml::PinnedOnly => "pinned_only",
                };
                let previous = sources_table.get("mode").and_then(|item| item.as_str());
                if previous != Some(desired_mode) {
                    sources_table["mode"] = value(desired_mode);
                    mutated = true;
                }
            }

            if desired.pinned_account_ids.is_empty() {
                if sources_table.remove("pinned_account_ids").is_some() {
                    mutated = true;
                }
            } else {
                let previous = sources_table
                    .get("pinned_account_ids")
                    .and_then(|item| item.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| item.as_str())
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if previous != desired.pinned_account_ids {
                    let mut arr = toml_edit::Array::new();
                    arr.set_trailing_comma(true);
                    for id in &desired.pinned_account_ids {
                        arr.push(id.as_str());
                    }
                    sources_table["pinned_account_ids"] = TomlItem::Value(arr.into());
                    mutated = true;
                }
            }
        }
    }

    if !mutated {
        return Ok(false);
    }

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;

    Ok(true)
}

/// Upsert a `[[subagents.commands]]` entry by `name`.
/// If an entry with the same (case-insensitive) name exists, it is updated; otherwise a new entry is appended.
pub async fn upsert_subagent_command(code_home: &Path, cmd: &SubagentCommandConfig) -> Result<()> {
    const CONFIG_TOML_FILE: &str = "config.toml";
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    // Ensure [subagents] exists
    if !doc.as_table().contains_key("subagents") {
        doc["subagents"] = toml_edit::table();
        if let Some(t) = doc["subagents"].as_table_mut() { t.set_implicit(false); }
    }

    // Search for existing by name (case-insensitive) and rebuild commands array
    let mut updated = false;
    let mut new_commands = toml_edit::ArrayOfTables::new();
    if let Some(arr) = doc["subagents"].get("commands").and_then(|i| i.as_array_of_tables()) {
        for tbl_ref in arr.iter() {
            let mut tbl = tbl_ref.clone();
            let same = tbl
                .get("name")
                .and_then(|i| i.as_str())
                .map(|s| s.eq_ignore_ascii_case(&cmd.name))
                .unwrap_or(false);
            if same {
                // Update fields
                tbl["name"] = toml_edit::value(cmd.name.clone());
                tbl["read-only"] = toml_edit::value(cmd.read_only);
                let agents = toml_edit::Array::from_iter(cmd.agents.iter().cloned());
                tbl["agents"] = toml_edit::Item::Value(toml_edit::Value::Array(agents));
                if let Some(s) = &cmd.orchestrator_instructions { tbl["orchestrator-instructions"] = toml_edit::value(s.clone()); } else { tbl.remove("orchestrator-instructions"); }
                if let Some(s) = &cmd.agent_instructions { tbl["agent-instructions"] = toml_edit::value(s.clone()); } else { tbl.remove("agent-instructions"); }
                updated = true;
            }
            new_commands.push(tbl);
        }
    }
    if !updated {
        let mut t = toml_edit::Table::new();
        t.set_implicit(true);
        t["name"] = toml_edit::value(cmd.name.clone());
        t["read-only"] = toml_edit::value(cmd.read_only);
        let agents = toml_edit::Array::from_iter(cmd.agents.iter().cloned());
        t["agents"] = toml_edit::Item::Value(toml_edit::Value::Array(agents));
        if let Some(s) = &cmd.orchestrator_instructions {
            t["orchestrator-instructions"] = toml_edit::value(s.clone());
        }
        if let Some(s) = &cmd.agent_instructions {
            t["agent-instructions"] = toml_edit::value(s.clone());
        }
        new_commands.push(t);
    }

    doc["subagents"]["commands"] = toml_edit::Item::ArrayOfTables(new_commands);

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;
    Ok(())
}

/// Delete a `[[subagents.commands]]` entry by name. Returns true if removed.
pub async fn delete_subagent_command(code_home: &Path, name: &str) -> Result<bool> {
    const CONFIG_TOML_FILE: &str = "config.toml";
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };

    let Some(arr) = doc["subagents"].get_mut("commands").and_then(|i| i.as_array_of_tables_mut()) else {
        return Ok(false);
    };

    let before = arr.len();
    arr.retain(|t| {
        !t.get("name")
            .and_then(|i| i.as_str())
            .map(|s| s.eq_ignore_ascii_case(name))
            .unwrap_or(false)
    });
    let removed = arr.len() != before;
    if removed {
        let tmp_file = NamedTempFile::new_in(code_home)?;
        tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
        tmp_file.persist(config_path)?;
    }
    Ok(removed)
}

/// Upsert an `[[agents]]` entry by `name`. If an entry with the same
/// (case-insensitive) name exists, update selected fields; otherwise append a
/// new entry with the provided values. Fields not managed by the editor are
/// preserved when updating.
pub struct AgentConfigPatch<'a> {
    pub name: &'a str,
    pub enabled: Option<bool>,
    pub args: Option<&'a [String]>,
    pub args_read_only: Option<&'a [String]>,
    pub args_write: Option<&'a [String]>,
    pub instructions: Option<&'a str>,
    pub description: Option<&'a str>,
    pub command: Option<&'a str>,
}

pub async fn upsert_agent_config(
    code_home: &Path,
    patch: AgentConfigPatch<'_>,
) -> Result<()> {
    let AgentConfigPatch {
        name,
        enabled,
        args,
        args_read_only,
        args_write,
        instructions,
        description,
        command,
    } = patch;
    let config_path = code_home.join(CONFIG_TOML_FILE);

    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    // Search existing [[agents]] for a case‑insensitive name match
    let mut found = false;
    if let Some(item) = doc.as_table().get("agents").cloned() {
        let Some(arr) = item.as_array_of_tables() else {
            /* not an array, treat as missing */
            return write_new_or_append(
                doc,
                code_home,
                config_path,
                AgentConfigPatch {
                    name,
                    enabled,
                    args,
                    args_read_only,
                    args_write,
                    instructions,
                    description,
                    command,
                },
            )
            .await;
        };
        let mut new_arr = toml_edit::ArrayOfTables::new();
        for tbl_ref in arr.iter() {
            let mut tbl = tbl_ref.clone();
            let same = tbl
                .get("name")
                .and_then(|i| i.as_str())
                .map(|s| s.eq_ignore_ascii_case(name))
                .unwrap_or(false);
            if same {
                if let Some(val) = enabled { tbl["enabled"] = toml_edit::value(val); }
                if let Some(a) = args { tbl["args"] = toml_edit::value(toml_edit::Array::from_iter(a.iter().cloned())); }
                if let Some(ro) = args_read_only {
                    tbl["args-read-only"] = toml_edit::value(toml_edit::Array::from_iter(ro.iter().cloned()));
                }
                if let Some(w) = args_write {
                    tbl["args-write"] = toml_edit::value(toml_edit::Array::from_iter(w.iter().cloned()));
                }
                if let Some(instr) = instructions {
                    if instr.trim().is_empty() { tbl.remove("instructions"); }
                    else { tbl["instructions"] = toml_edit::value(instr.to_string()); }
                }
                if let Some(desc) = description {
                    if desc.trim().is_empty() {
                        tbl.remove("description");
                    } else {
                        tbl["description"] = toml_edit::value(desc.to_string());
                    }
                }
                if let Some(cmd) = command {
                    if cmd.trim().is_empty() {
                        tbl.remove("command");
                    } else {
                        tbl["command"] = toml_edit::value(cmd.to_string());
                    }
                }
                found = true;
            }
            new_arr.push(tbl);
        }
        doc["agents"] = toml_edit::Item::ArrayOfTables(new_arr);
    }

    if !found {
        // Append a new entry safely
        append_agent_entry(
            &mut doc,
            AgentConfigPatch {
                name,
                enabled,
                args,
                args_read_only,
                args_write,
                instructions,
                description,
                command,
            },
        );
    }

    // Write back atomically
    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;
    Ok(())
}

// Helper: append a single [[agents]] entry (no-alloc fallible path wrapper above)
fn append_agent_entry(
    doc: &mut DocumentMut,
    patch: AgentConfigPatch<'_>,
) {
    let AgentConfigPatch {
        name,
        enabled,
        args,
        args_read_only,
        args_write,
        instructions,
        description,
        command,
    } = patch;
    let mut t = toml_edit::Table::new();
    t.set_implicit(true);
    t["name"] = toml_edit::value(name.to_string());
    if let Some(val) = enabled { t["enabled"] = toml_edit::value(val); }
    if let Some(a) = args { t["args"] = toml_edit::value(toml_edit::Array::from_iter(a.iter().cloned())); }
    if let Some(ro) = args_read_only { t["args-read-only"] = toml_edit::value(toml_edit::Array::from_iter(ro.iter().cloned())); }
    if let Some(w) = args_write { t["args-write"] = toml_edit::value(toml_edit::Array::from_iter(w.iter().cloned())); }
    if let Some(instr) = instructions && !instr.trim().is_empty() { t["instructions"] = toml_edit::value(instr.to_string()); }
    if let Some(desc) = description && !desc.trim().is_empty() { t["description"] = toml_edit::value(desc.to_string()); }
    if let Some(cmd) = command
        && !cmd.trim().is_empty() {
            t["command"] = toml_edit::value(cmd.to_string());
        }

    let mut arr = doc
        .as_table()
        .get("agents")
        .and_then(|i| i.as_array_of_tables().cloned())
        .unwrap_or_default();
    arr.push(t);
    doc["agents"] = toml_edit::Item::ArrayOfTables(arr);
}

async fn write_new_or_append(
    mut doc: DocumentMut,
    code_home: &Path,
    config_path: std::path::PathBuf,
    patch: AgentConfigPatch<'_>,
) -> Result<()> {
    append_agent_entry(&mut doc, patch);
    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;
    Ok(())
}


// Internal helper to support persist_* variants above.
async fn persist_overrides_with_behavior(
    code_home: &Path,
    profile: Option<&str>,
    overrides: &[(&[&str], Option<&str>)],
    none_behavior: NoneBehavior,
) -> Result<()> {
    if overrides.is_empty() {
        return Ok(());
    }

    let should_skip = match none_behavior {
        NoneBehavior::Skip => overrides.iter().all(|(_, value)| value.is_none()),
        NoneBehavior::Remove => false,
    };

    if should_skip {
        return Ok(());
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let read_result = tokio::fs::read_to_string(&read_path).await;
    let mut doc = match read_result {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if overrides.iter().all(|(_, value)| value.is_none() && matches!(none_behavior, NoneBehavior::Remove)) {
                return Ok(());
            }
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    let effective_profile = if let Some(p) = profile { Some(p.to_owned()) } else { doc.get("profile").and_then(|i| i.as_str()).map(std::string::ToString::to_string) };

    let mut mutated = false;
    for (segments, value) in overrides.iter().copied() {
        let mut seg_buf: Vec<&str> = Vec::new();
        let segments_to_apply: &[&str];
        if let Some(ref name) = effective_profile {
            if segments.first().copied() == Some("profiles") {
                segments_to_apply = segments;
            } else {
                seg_buf.reserve(2 + segments.len());
                seg_buf.push("profiles");
                seg_buf.push(name.as_str());
                seg_buf.extend_from_slice(segments);
                segments_to_apply = seg_buf.as_slice();
            }
        } else {
            segments_to_apply = segments;
        }
        match value {
            Some(v) => {
                let trimmed = v.trim();
                let item_value = match trimmed.parse::<bool>() {
                    Ok(parsed_bool) => toml_edit::value(parsed_bool),
                    Err(_) => toml_edit::value(v),
                };
                apply_toml_edit_override_segments(&mut doc, segments_to_apply, item_value);
                mutated = true;
            }
            None => {
                if matches!(none_behavior, NoneBehavior::Remove) && remove_toml_edit_segments(&mut doc, segments_to_apply) {
                    mutated = true;
                }
            }
        }
    }
    if !mutated { return Ok(()); }
    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;
    Ok(())
}

async fn persist_root_overrides_with_behavior(
    code_home: &Path,
    overrides: &[(&[&str], Option<&str>)],
    none_behavior: NoneBehavior,
) -> Result<()> {
    if overrides.is_empty() {
        return Ok(());
    }

    let should_skip = match none_behavior {
        NoneBehavior::Skip => overrides.iter().all(|(_, value)| value.is_none()),
        NoneBehavior::Remove => false,
    };

    if should_skip {
        return Ok(());
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let read_result = tokio::fs::read_to_string(&read_path).await;
    let mut doc = match read_result {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if overrides.iter().all(|(_, value)| value.is_none() && matches!(none_behavior, NoneBehavior::Remove)) {
                return Ok(());
            }
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    let mut mutated = false;
    for (segments, value) in overrides.iter().copied() {
        match value {
            Some(v) => {
                let trimmed = v.trim();
                let item_value = match trimmed.parse::<bool>() {
                    Ok(parsed_bool) => toml_edit::value(parsed_bool),
                    Err(_) => toml_edit::value(v),
                };
                apply_toml_edit_override_segments(&mut doc, segments, item_value);
                mutated = true;
            }
            None => {
                if matches!(none_behavior, NoneBehavior::Remove) && remove_toml_edit_segments(&mut doc, segments) {
                    mutated = true;
                }
            }
        }
    }
    if !mutated {
        return Ok(());
    }

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;
    Ok(())
}

fn remove_toml_edit_segments(doc: &mut DocumentMut, segments: &[&str]) -> bool {
    use toml_edit::Item;
    if segments.is_empty() { return false; }
    let mut current = doc.as_table_mut();
    for seg in &segments[..segments.len() - 1] {
        let Some(item) = current.get_mut(seg) else { return false }; 
        match item { Item::Table(table) => { current = table; } _ => { return false; } }
    }
    current.remove(segments[segments.len() - 1]).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    /// Verifies model and effort are written at top-level when no profile is set.
    #[tokio::test]
    async fn set_default_model_and_effort_top_level_when_no_profile() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_overrides(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], "gpt-5.1-codex"),
                (&[CONFIG_KEY_EFFORT], "high"),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let table = parsed.as_table().expect("root table");
        assert_eq!(
            table
                .get(CONFIG_KEY_MODEL)
                .and_then(|value| value.as_str()),
            Some("gpt-5.1-codex")
        );
        assert_eq!(
            table
                .get(CONFIG_KEY_EFFORT)
                .and_then(|value| value.as_str()),
            Some("high")
        );
    }

    #[tokio::test]
    async fn set_shell_escalation_paths_sets_and_clears_keys_preserving_other_tables() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let config_path = code_home.join("config.toml");
        tokio::fs::write(
            &config_path,
            r#"
[plugins."some@plugin"]
enabled = true

[profiles.default]
model = "gpt-5.4"
"#,
        )
        .await
        .expect("write config");

        assert!(
            set_shell_escalation_paths(code_home, Some("/opt/zsh-patched"), Some("/opt/codex-execve-wrapper"))
                .await
                .expect("set paths")
        );

        let after_set = tokio::fs::read_to_string(&config_path)
            .await
            .expect("read after set");
        let doc = after_set.parse::<DocumentMut>().expect("parse");
        let root = doc.as_table();
        assert_eq!(root.get("zsh_path").and_then(|item| item.as_str()), Some("/opt/zsh-patched"));
        assert_eq!(
            root.get("main_execve_wrapper_exe")
                .and_then(|item| item.as_str()),
            Some("/opt/codex-execve-wrapper")
        );
        assert!(root.get("plugins").is_some(), "expected plugins table preserved");
        assert!(root.get("profiles").is_some(), "expected profiles table preserved");

        assert!(
            set_shell_escalation_paths(code_home, Some(""), Some("   "))
                .await
                .expect("clear paths")
        );

        let after_clear = tokio::fs::read_to_string(&config_path)
            .await
            .expect("read after clear");
        let doc = after_clear.parse::<DocumentMut>().expect("parse");
        let root = doc.as_table();
        assert!(root.get("zsh_path").is_none());
        assert!(root.get("main_execve_wrapper_exe").is_none());
        assert!(root.get("plugins").is_some(), "expected plugins table preserved");
        assert!(root.get("profiles").is_some(), "expected profiles table preserved");
    }

    #[tokio::test]
    async fn set_shell_escalation_settings_updates_profile_features_and_root_paths() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let config_path = code_home.join("config.toml");
        tokio::fs::write(
            &config_path,
            r#"
[plugins."some@plugin"]
enabled = true

[profiles.default]
model = "gpt-5.4"
"#,
        )
        .await
        .expect("write config");

        assert!(
            set_shell_escalation_settings(
                code_home,
                Some("default"),
                true,
                Some("/opt/zsh-patched"),
                Some("/opt/codex-execve-wrapper"),
            )
            .await
            .expect("set settings")
        );

        let after_set = tokio::fs::read_to_string(&config_path)
            .await
            .expect("read after set");
        let doc = after_set.parse::<DocumentMut>().expect("parse");
        let root = doc.as_table();
        assert_eq!(
            root.get("zsh_path").and_then(|item| item.as_str()),
            Some("/opt/zsh-patched")
        );
        assert_eq!(
            root.get("main_execve_wrapper_exe")
                .and_then(|item| item.as_str()),
            Some("/opt/codex-execve-wrapper")
        );

        let profiles = root
            .get("profiles")
            .and_then(|item| item.as_table())
            .expect("profiles table");
        let profile = profiles
            .get("default")
            .and_then(|item| item.as_table())
            .expect("profile table");
        let features = profile
            .get("features")
            .and_then(|item| item.as_table())
            .expect("features table");
        assert_eq!(
            features
                .get("shell_zsh_fork")
                .and_then(|item| item.as_bool()),
            Some(true)
        );

        assert!(root.get("plugins").is_some(), "expected plugins table preserved");

        assert!(
            set_shell_escalation_settings(code_home, Some("default"), false, Some(""), Some("   "))
                .await
                .expect("clear settings")
        );

        let after_clear = tokio::fs::read_to_string(&config_path)
            .await
            .expect("read after clear");
        let doc = after_clear.parse::<DocumentMut>().expect("parse");
        let root = doc.as_table();
        assert!(root.get("zsh_path").is_none());
        assert!(root.get("main_execve_wrapper_exe").is_none());

        let profiles = root
            .get("profiles")
            .and_then(|item| item.as_table())
            .expect("profiles table");
        let profile = profiles
            .get("default")
            .and_then(|item| item.as_table())
            .expect("profile table");
        let features = profile
            .get("features")
            .and_then(|item| item.as_table())
            .expect("features table");
        assert_eq!(
            features
                .get("shell_zsh_fork")
                .and_then(|item| item.as_bool()),
            Some(false)
        );

        assert!(root.get("plugins").is_some(), "expected plugins table preserved");
    }

    /// Verifies values are written under the active profile when `profile` is set.
    #[tokio::test]
    async fn set_defaults_update_profile_when_profile_set() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed config with a profile selection but without profiles table
        let seed = "profile = \"o3\"\n";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], "o3"),
                (&[CONFIG_KEY_EFFORT], "minimal"),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"profile = "o3"

[profiles.o3]
model = "o3"
model_reasoning_effort = "minimal"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies profile names with dots/spaces are preserved via explicit segments.
    #[tokio::test]
    async fn set_defaults_update_profile_with_dot_and_space() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed config with a profile name that contains a dot and a space
        let seed = "profile = \"my.team name\"\n";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], "o3"),
                (&[CONFIG_KEY_EFFORT], "minimal"),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"profile = "my.team name"

[profiles."my.team name"]
model = "o3"
model_reasoning_effort = "minimal"
"#;
        assert_eq!(contents, expected);
    }

    #[tokio::test]
    async fn set_feature_flags_updates_root_table() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let mut updates = BTreeMap::new();
        updates.insert("apps".to_string(), false);
        set_feature_flags(code_home, None, &updates)
            .await
            .expect("set features");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let table = parsed.as_table().expect("root table");
        assert_eq!(
            table
                .get("features")
                .and_then(|value| value.as_table())
                .and_then(|t| t.get("apps"))
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    }

    #[tokio::test]
    async fn set_feature_flags_updates_profile_table() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let mut updates = BTreeMap::new();
        updates.insert("apps".to_string(), false);
        set_feature_flags(code_home, Some("work"), &updates)
            .await
            .expect("set features");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let apps_flag = parsed
            .get("profiles")
            .and_then(|value| value.as_table())
            .and_then(|t| t.get("work"))
            .and_then(|value| value.as_table())
            .and_then(|t| t.get("features"))
            .and_then(|value| value.as_table())
            .and_then(|t| t.get("apps"))
            .and_then(|value| value.as_bool());
        assert_eq!(apps_flag, Some(false));
    }

    #[tokio::test]
    async fn set_feature_flags_preserves_unrelated_keys() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let seed = r#"[features]
apps = true
other = true

[shell]
program = "/bin/zsh"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        let mut updates = BTreeMap::new();
        updates.insert("apps".to_string(), false);
        set_feature_flags(code_home, None, &updates)
            .await
            .expect("set features");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        assert_eq!(
            parsed
                .get("features")
                .and_then(|value| value.as_table())
                .and_then(|t| t.get("other"))
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            parsed
                .get("shell")
                .and_then(|value| value.as_table())
                .and_then(|t| t.get("program"))
                .and_then(|value| value.as_str()),
            Some("/bin/zsh")
        );
    }

    #[tokio::test]
    async fn set_feature_flags_removes_table_when_empty() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let seed = r#"[features]
apps = false

[shell]
program = "/bin/zsh"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        let updates = BTreeMap::new();
        set_feature_flags(code_home, None, &updates)
            .await
            .expect("set features");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        assert_eq!(
            parsed.get("features").and_then(|value| value.as_table()),
            None
        );
        assert_eq!(
            parsed
                .get("shell")
                .and_then(|value| value.as_table())
                .and_then(|t| t.get("program"))
                .and_then(|value| value.as_str()),
            Some("/bin/zsh")
        );
    }

    /// Verifies explicit profile override writes under that profile even without active profile.
    #[tokio::test]
    async fn set_defaults_update_when_profile_override_supplied() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // No profile key in config.toml
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), "")
            .await
            .expect("seed write");

        // Persist with an explicit profile override
        persist_overrides(
            code_home,
            Some("o3"),
            &[(&[CONFIG_KEY_MODEL], "o3"), (&[CONFIG_KEY_EFFORT], "high")],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"[profiles.o3]
model = "o3"
model_reasoning_effort = "high"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies nested tables are created as needed when applying overrides.
    #[tokio::test]
    async fn persist_overrides_creates_nested_tables() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_overrides(
            code_home,
            None,
            &[
                (&["a", "b", "c"], "v"),
                (&["x"], "y"),
                (&["profiles", "p1", CONFIG_KEY_MODEL], "gpt-5.1-codex"),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let table = parsed.as_table().expect("root table");
        assert_eq!(
            table.get("x").and_then(toml::Value::as_str),
            Some("y")
        );
        let a_table = table
            .get("a")
            .and_then(toml::Value::as_table)
            .expect("a table");
        let b_table = a_table
            .get("b")
            .and_then(toml::Value::as_table)
            .expect("b table");
        assert_eq!(b_table.get("c").and_then(toml::Value::as_str), Some("v"));
        let profiles = table
            .get("profiles")
            .and_then(toml::Value::as_table)
            .expect("profiles table");
        let p1 = profiles
            .get("p1")
            .and_then(toml::Value::as_table)
            .expect("profile p1");
        assert_eq!(
            p1.get(CONFIG_KEY_MODEL).and_then(toml::Value::as_str),
            Some("gpt-5.1-codex")
        );
    }

    #[tokio::test]
    async fn persist_overrides_writes_boolean_literals() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_overrides(code_home, None, &[(&["auto_upgrade_enabled"], "true")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        assert!(contents.contains("auto_upgrade_enabled = true"));

        persist_overrides(code_home, None, &[(&["auto_upgrade_enabled"], "false")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        assert!(contents.contains("auto_upgrade_enabled = false"));
    }

    /// Verifies a scalar key becomes a table when nested keys are written.
    #[tokio::test]
    async fn persist_overrides_replaces_scalar_with_table() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();
        let seed = "foo = \"bar\"\n";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides(code_home, None, &[(&["foo", "bar", "baz"], "ok")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"[foo.bar]
baz = "ok"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies comments and spacing are preserved when writing under active profile.
    #[tokio::test]
    async fn set_defaults_preserve_comments() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed a config with comments and spacing we expect to preserve
        let seed = r#"# Global comment
# Another line

profile = "o3"

# Profile settings
[profiles.o3]
# keep me
existing = "keep"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        // Apply defaults; since profile is set, it should write under [profiles.o3]
        persist_overrides(
            code_home,
            None,
            &[(&[CONFIG_KEY_MODEL], "o3"), (&[CONFIG_KEY_EFFORT], "high")],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"# Global comment
# Another line

profile = "o3"

# Profile settings
[profiles.o3]
# keep me
existing = "keep"
model = "o3"
model_reasoning_effort = "high"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies comments and spacing are preserved when writing at top level.
    #[tokio::test]
    async fn set_defaults_preserve_global_comments() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed a config WITHOUT a profile, containing comments and spacing
        let seed = r#"# Top-level comments
# should be preserved

existing = "keep"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        // Since there is no profile, the defaults should be written at top-level
        persist_overrides(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], "gpt-5.1-codex"),
                (&[CONFIG_KEY_EFFORT], "minimal"),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"# Top-level comments
# should be preserved

existing = "keep"
model = "gpt-5.1-codex"
model_reasoning_effort = "minimal"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies errors on invalid TOML propagate and file is not clobbered.
    #[tokio::test]
    async fn persist_overrides_errors_on_parse_failure() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Write an intentionally invalid TOML file
        let invalid = "invalid = [unclosed";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), invalid)
            .await
            .expect("seed write");

        // Attempting to persist should return an error and must not clobber the file.
        let res = persist_overrides(code_home, None, &[(&["x"], "y")]).await;
        assert!(res.is_err(), "expected parse error to propagate");

        // File should be unchanged
        let contents = read_config(code_home).await;
        assert_eq!(contents, invalid);
    }

    /// Verifies changing model only preserves existing effort at top-level.
    #[tokio::test]
    async fn changing_only_model_preserves_existing_effort_top_level() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed with an effort value only
        let seed = "model_reasoning_effort = \"minimal\"\n";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        // Change only the model
        persist_overrides(code_home, None, &[(&[CONFIG_KEY_MODEL], "o3")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"model_reasoning_effort = "minimal"
model = "o3"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies changing effort only preserves existing model at top-level.
    #[tokio::test]
    async fn changing_only_effort_preserves_existing_model_top_level() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed with a model value only
        let seed = "model = \"gpt-5.1-codex\"\n";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        // Change only the effort
        persist_overrides(code_home, None, &[(&[CONFIG_KEY_EFFORT], "high")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"model = "gpt-5.1-codex"
model_reasoning_effort = "high"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies changing model only preserves existing effort in active profile.
    #[tokio::test]
    async fn changing_only_model_preserves_effort_in_active_profile() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed with an active profile and an existing effort under that profile
        let seed = r#"profile = "p1"

[profiles.p1]
model_reasoning_effort = "low"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides(code_home, None, &[(&[CONFIG_KEY_MODEL], "o4-mini")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"profile = "p1"

[profiles.p1]
model_reasoning_effort = "low"
model = "o4-mini"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies changing effort only preserves existing model in a profile override.
    #[tokio::test]
    async fn changing_only_effort_preserves_model_in_profile_override() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // No active profile key; we'll target an explicit override
        let seed = r#"[profiles.team]
model = "gpt-5.1-codex"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides(
            code_home,
            Some("team"),
            &[(&[CONFIG_KEY_EFFORT], "minimal")],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"[profiles.team]
model = "gpt-5.1-codex"
model_reasoning_effort = "minimal"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies `persist_non_null_overrides` skips `None` entries and writes only present values at top-level.
    #[tokio::test]
    async fn persist_non_null_skips_none_top_level() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_non_null_overrides(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], Some("gpt-5.1-codex")),
                (&[CONFIG_KEY_EFFORT], None),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let table = parsed.as_table().expect("root table");
        assert_eq!(
            table
                .get(CONFIG_KEY_MODEL)
                .and_then(|value| value.as_str()),
            Some("gpt-5.1-codex")
        );
        assert!(table.get(CONFIG_KEY_EFFORT).is_none());
    }

    /// Verifies no-op behavior when all provided overrides are `None` (no file created/modified).
    #[tokio::test]
    async fn persist_non_null_noop_when_all_none() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_non_null_overrides(
            code_home,
            None,
            &[(&["a"], None), (&["profiles", "p", "x"], None)],
        )
        .await
        .expect("persist");

        // Should not create config.toml on a pure no-op
        assert!(!code_home.join(CONFIG_TOML_FILE).exists());
    }

    #[tokio::test]
    async fn persist_root_overrides_writes_top_level_even_when_profile_set() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let seed = "profile = \"team\"\n";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_root_overrides(code_home, &[(&["cli_auth_credentials_store"], "keyring")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let table = parsed.as_table().expect("root table");
        assert_eq!(
            table
                .get("cli_auth_credentials_store")
                .and_then(toml::Value::as_str),
            Some("keyring")
        );
        let profiles = table.get("profiles").and_then(toml::Value::as_table);
        assert!(
            profiles
                .and_then(|p| p.get("team"))
                .and_then(toml::Value::as_table)
                .and_then(|t| t.get("cli_auth_credentials_store"))
                .is_none(),
            "expected root override to not be nested under profiles"
        );
    }

    #[tokio::test]
    async fn persist_root_overrides_creates_file_when_missing() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_root_overrides(code_home, &[(&["cli_auth_credentials_store"], "file")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let table = parsed.as_table().expect("root table");
        assert_eq!(
            table
                .get("cli_auth_credentials_store")
                .and_then(toml::Value::as_str),
            Some("file")
        );
    }

    /// Verifies entries are written under the specified profile and `None` entries are skipped.
    #[tokio::test]
    async fn persist_non_null_respects_profile_override() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_non_null_overrides(
            code_home,
            Some("team"),
            &[
                (&[CONFIG_KEY_MODEL], Some("o3")),
                (&[CONFIG_KEY_EFFORT], None),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let table = parsed.as_table().expect("root table");
        let profiles = table
            .get("profiles")
            .and_then(|value| value.as_table())
            .expect("profiles table");
        let team = profiles
            .get("team")
            .and_then(|value| value.as_table())
            .expect("team profile");
        assert_eq!(
            team
                .get(CONFIG_KEY_MODEL)
                .and_then(|value| value.as_str()),
            Some("o3")
        );
        assert!(team.get(CONFIG_KEY_EFFORT).is_none());
    }

    #[tokio::test]
    async fn persist_clear_none_removes_top_level_value() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let seed = r#"model = "gpt-5.1-codex"
model_reasoning_effort = "medium"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides_and_clear_if_none(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], None),
                (&[CONFIG_KEY_EFFORT], Some("high")),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = "model_reasoning_effort = \"high\"\n";
        assert_eq!(contents, expected);
    }

    #[tokio::test]
    async fn persist_clear_none_respects_active_profile() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let seed = r#"profile = "team"

[profiles.team]
model = "gpt-4"
model_reasoning_effort = "minimal"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides_and_clear_if_none(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], None),
                (&[CONFIG_KEY_EFFORT], Some("high")),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"profile = "team"

[profiles.team]
model_reasoning_effort = "high"
"#;
        assert_eq!(contents, expected);
    }

    #[tokio::test]
    async fn persist_clear_none_noop_when_file_missing() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_overrides_and_clear_if_none(code_home, None, &[(&[CONFIG_KEY_MODEL], None)])
            .await
            .expect("persist");

        assert!(!code_home.join(CONFIG_TOML_FILE).exists());
    }

    #[tokio::test]
    async fn set_skill_config_disable_writes_entry_and_enable_removes_entry() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let skill_path = code_home.join("skills").join("my-skill").join("SKILL.md");
        tokio::fs::create_dir_all(skill_path.parent().expect("parent"))
            .await
            .expect("mkdir");
        tokio::fs::write(&skill_path, "---\nname: my-skill\ndescription: ok\n---\n")
            .await
            .expect("write");

        let mutated = set_skill_config(code_home, &skill_path, false)
            .await
            .expect("disable");
        assert!(mutated);

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("toml");
        let cfg = parsed
            .get("skills")
            .and_then(|value| value.as_table())
            .and_then(|skills| skills.get("config"))
            .and_then(|value| value.as_array())
            .expect("skills.config");
        assert_eq!(cfg.len(), 1);
        let entry = cfg[0].as_table().expect("entry table");
        let normalized_path = normalize_skill_config_path(&skill_path);
        assert_eq!(
            entry.get("path").and_then(|value| value.as_str()),
            Some(normalized_path.as_str())
        );
        assert_eq!(entry.get("enabled").and_then(|value| value.as_bool()), Some(false));

        let mutated = set_skill_config(code_home, &skill_path, true)
            .await
            .expect("enable");
        assert!(mutated);

        let contents = read_config(code_home).await;
        assert!(contents.trim().is_empty());
    }

    #[tokio::test]
    async fn set_skill_config_enable_is_noop_when_file_missing() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let skill_path = code_home.join("skills").join("my-skill").join("SKILL.md");
        let mutated = set_skill_config(code_home, &skill_path, true)
            .await
            .expect("enable");
        assert!(!mutated);
        assert!(!code_home.join(CONFIG_TOML_FILE).exists());
    }

    #[tokio::test]
    async fn set_plugin_marketplace_sources_updates_sources_and_preserves_plugin_entries() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let seed = r#"[plugins]
curated_repo_url = "https://old.example.com/curated.git"
curated_repo_ref = "old"

[[plugins.marketplace_repos]]
url = "https://old.example.com/marketplace.git"
ref = "main"

[plugins."some@plugin"]
enabled = true
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        let sources = PluginsToml {
            curated_repo_url: Some("https://example.com/custom/plugins.git".to_string()),
            curated_repo_ref: Some("stable".to_string()),
            marketplace_repos: vec![
                PluginMarketplaceRepoToml {
                    url: "https://github.com/acme/marketplace.git".to_string(),
                    git_ref: Some("main".to_string()),
                },
                PluginMarketplaceRepoToml {
                    url: "https://git.example.com/more.git".to_string(),
                    git_ref: None,
                },
            ],
        };

        let mutated = set_plugin_marketplace_sources(code_home, &sources)
            .await
            .expect("persist sources");
        assert!(mutated);

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let root = parsed.as_table().expect("root table");
        let plugins = root
            .get("plugins")
            .and_then(toml::Value::as_table)
            .expect("plugins table");

        assert_eq!(
            plugins
                .get("curated_repo_url")
                .and_then(toml::Value::as_str),
            Some("https://example.com/custom/plugins.git")
        );
        assert_eq!(
            plugins
                .get("curated_repo_ref")
                .and_then(toml::Value::as_str),
            Some("stable")
        );

        let marketplace_repos = plugins
            .get("marketplace_repos")
            .and_then(toml::Value::as_array)
            .expect("marketplace_repos");
        assert_eq!(marketplace_repos.len(), 2);
        let first = marketplace_repos[0].as_table().expect("repo 0");
        assert_eq!(
            first.get("url").and_then(toml::Value::as_str),
            Some("https://github.com/acme/marketplace.git")
        );
        assert_eq!(
            first.get("ref").and_then(toml::Value::as_str),
            Some("main")
        );

        let second = marketplace_repos[1].as_table().expect("repo 1");
        assert_eq!(
            second.get("url").and_then(toml::Value::as_str),
            Some("https://git.example.com/more.git")
        );
        assert!(second.get("ref").is_none());

        let plugin_entry = plugins
            .get("some@plugin")
            .and_then(toml::Value::as_table)
            .expect("plugin entry");
        assert_eq!(
            plugin_entry.get("enabled").and_then(toml::Value::as_bool),
            Some(true)
        );
    }

    #[tokio::test]
    async fn set_apps_sources_writes_under_profile_when_profile_is_provided() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), "profile = \"work\"\n")
            .await
            .expect("seed");

        let sources = AppsSourcesToml {
            mode: AppsSourcesModeToml::PinnedOnly,
            pinned_account_ids: vec!["acc1".to_string(), "acc2".to_string()],
        };

        let mutated = set_apps_sources(code_home, Some("work"), &sources)
            .await
            .expect("set apps sources");
        assert!(mutated);

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let root = parsed.as_table().expect("root table");
        assert_eq!(
            root.get("profile").and_then(toml::Value::as_str),
            Some("work")
        );

        let profiles = root
            .get("profiles")
            .and_then(toml::Value::as_table)
            .expect("profiles");
        let work = profiles
            .get("work")
            .and_then(toml::Value::as_table)
            .expect("work profile");
        let apps = work
            .get("apps")
            .and_then(toml::Value::as_table)
            .expect("apps table");
        let sources_table = apps
            .get("_sources")
            .and_then(toml::Value::as_table)
            .expect("_sources table");
        assert_eq!(
            sources_table.get("mode").and_then(toml::Value::as_str),
            Some("pinned_only")
        );
        let pins = sources_table
            .get("pinned_account_ids")
            .and_then(toml::Value::as_array)
            .expect("pins");
        assert_eq!(
            pins.iter()
                .filter_map(toml::Value::as_str)
                .collect::<Vec<_>>(),
            vec!["acc1", "acc2"]
        );
    }

    #[tokio::test]
    async fn set_apps_sources_removes_sources_table_when_defaults_are_provided() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let seed = r#"[apps._sources]
mode = "pinned_only"
pinned_account_ids = ["acc1"]
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed");

        let sources = AppsSourcesToml::default();
        let mutated = set_apps_sources(code_home, None, &sources)
            .await
            .expect("set apps sources");
        assert!(mutated);

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let root = parsed.as_table().expect("root table");
        let apps = root
            .get("apps")
            .and_then(toml::Value::as_table)
            .expect("apps table");
        assert!(apps.get("_sources").is_none(), "_sources should be removed");
    }

    #[tokio::test]
    async fn set_apps_sources_preserves_existing_per_app_tables() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let seed = r#"[apps._sources]
mode = "active_only"

[apps."some_app"]
enabled = true
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed");

        let sources = AppsSourcesToml {
            mode: AppsSourcesModeToml::ActivePlusPinned,
            pinned_account_ids: vec!["acc1".to_string()],
        };

        let mutated = set_apps_sources(code_home, None, &sources)
            .await
            .expect("set apps sources");
        assert!(mutated);

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let root = parsed.as_table().expect("root table");
        let apps = root
            .get("apps")
            .and_then(toml::Value::as_table)
            .expect("apps table");
        let some_app = apps
            .get("some_app")
            .and_then(toml::Value::as_table)
            .expect("some_app table");
        assert_eq!(
            some_app.get("enabled").and_then(toml::Value::as_bool),
            Some(true)
        );
    }

    // Test helper moved to bottom per review guidance.
    async fn read_config(code_home: &Path) -> String {
        let p = code_home.join(CONFIG_TOML_FILE);
        tokio::fs::read_to_string(p).await.unwrap_or_default()
    }
}
