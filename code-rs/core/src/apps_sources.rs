use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::time::Duration;

use crate::config::Config;
use crate::config_types::{AppsSourcesModeToml, AppsSourcesToml, McpServerConfig, McpServerTransportConfig};

pub const CODEX_APPS_SERVER_PREFIX: &str = "codex_apps_";

pub fn codex_apps_server_name_for_source_account_id(account_id: &str) -> String {
    format!("{CODEX_APPS_SERVER_PREFIX}{account_id}")
}

pub fn source_account_id_from_codex_apps_server_name(server_name: &str) -> Option<&str> {
    server_name.strip_prefix(CODEX_APPS_SERVER_PREFIX)
}

pub fn active_chatgpt_account_id(code_home: &Path) -> io::Result<Option<String>> {
    let Some(active_id) = crate::auth_accounts::get_active_account_id(code_home)? else {
        return Ok(None);
    };
    let Some(account) = crate::auth_accounts::find_account(code_home, &active_id)? else {
        return Ok(None);
    };
    Ok(account.mode.is_chatgpt().then_some(active_id))
}

pub fn effective_source_account_ids(
    sources: &AppsSourcesToml,
    active_account_id: Option<&str>,
) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();

    let include_active = match sources.mode {
        AppsSourcesModeToml::ActiveOnly | AppsSourcesModeToml::ActivePlusPinned => true,
        AppsSourcesModeToml::PinnedOnly => false,
    };
    if include_active {
        if let Some(active) = active_account_id.map(str::trim).filter(|id| !id.is_empty()) {
            ids.push(active.to_string());
        }
    }

    if !matches!(sources.mode, AppsSourcesModeToml::ActiveOnly) {
        for id in &sources.pinned_account_ids {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                continue;
            }
            if ids.iter().any(|existing| existing == trimmed) {
                continue;
            }
            ids.push(trimmed.to_string());
        }
    }

    ids
}

fn normalize_codex_apps_base_url(base_url: &str) -> String {
    let mut base_url = base_url.trim_end_matches('/').to_string();
    if (base_url.starts_with("https://chatgpt.com") || base_url.starts_with("https://chat.openai.com"))
        && !base_url.contains("/backend-api")
    {
        base_url = format!("{base_url}/backend-api");
    }
    base_url
}

fn codex_apps_mcp_url_for_base_url(base_url: &str) -> String {
    let base_url = normalize_codex_apps_base_url(base_url);
    if base_url.contains("/backend-api") {
        format!("{base_url}/wham/apps")
    } else if base_url.contains("/api/codex") {
        format!("{base_url}/apps")
    } else {
        format!("{base_url}/api/codex/apps")
    }
}

fn codex_apps_mcp_url(config: &Config) -> String {
    codex_apps_mcp_url_for_base_url(&config.chatgpt_base_url)
}

pub async fn build_codex_apps_source_servers(
    config: &Config,
    active_account_id: Option<&str>,
) -> (HashMap<String, McpServerConfig>, Vec<String>) {
    let mut servers: HashMap<String, McpServerConfig> = HashMap::new();
    let mut warnings: Vec<String> = Vec::new();

    let effective_source_ids = effective_source_account_ids(&config.apps_sources, active_account_id);
    if effective_source_ids.is_empty() {
        return (servers, warnings);
    }

    let url = codex_apps_mcp_url(config);

    for stored_account_id in effective_source_ids {
        let server_name = codex_apps_server_name_for_source_account_id(&stored_account_id);

        let stored = match crate::auth_accounts::find_account(&config.code_home, &stored_account_id) {
            Ok(Some(account)) => account,
            Ok(None) => {
                warnings.push(format!(
                    "Apps sources: account '{stored_account_id}' not found; skipping connector source."
                ));
                continue;
            }
            Err(err) => {
                warnings.push(format!(
                    "Apps sources: failed to load account '{stored_account_id}': {err}; skipping connector source."
                ));
                continue;
            }
        };

        if !stored.mode.is_chatgpt() {
            warnings.push(format!(
                "Apps sources: account '{stored_account_id}' is not a ChatGPT account; skipping connector source."
            ));
            continue;
        }

        let auth = match crate::auth::auth_for_stored_account(
            &config.code_home,
            &stored,
            "apps_sources",
        )
        .await
        {
            Ok(auth) => auth,
            Err(err) => {
                warnings.push(format!(
                    "Apps sources: failed to load auth for account '{stored_account_id}': {err}; skipping connector source."
                ));
                continue;
            }
        };

        let bearer_token = match auth.get_token().await {
            Ok(token) if !token.trim().is_empty() => Some(token),
            Ok(_) => {
                warnings.push(format!(
                    "Apps sources: empty access token for account '{stored_account_id}'; skipping connector source."
                ));
                None
            }
            Err(err) => {
                warnings.push(format!(
                    "Apps sources: failed to read access token for account '{stored_account_id}': {err}; skipping connector source."
                ));
                None
            }
        };
        let Some(bearer_token) = bearer_token else {
            continue;
        };

        let mut http_headers: HashMap<String, String> = HashMap::new();
        if let Some(chatgpt_account_id) = auth.get_account_id() {
            http_headers.insert("ChatGPT-Account-ID".to_string(), chatgpt_account_id);
        } else {
            warnings.push(format!(
                "Apps sources: ChatGPT account id is missing for stored account '{stored_account_id}'; continuing without ChatGPT-Account-ID header."
            ));
        }

        let cfg = McpServerConfig {
            transport: McpServerTransportConfig::StreamableHttp {
                url: url.clone(),
                bearer_token: Some(bearer_token),
                bearer_token_env_var: None,
                http_headers: if http_headers.is_empty() { None } else { Some(http_headers) },
                env_http_headers: None,
                oauth_resource: None,
            },
            startup_timeout_sec: Some(Duration::from_secs(30)),
            tool_timeout_sec: None,
            scheduling: crate::config_types::McpServerSchedulingToml::default(),
            tool_scheduling: Default::default(),
            disabled_tools: Vec::new(),
        };

        servers.insert(server_name, cfg);
    }

    (servers, warnings)
}
