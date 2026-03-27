use std::collections::HashSet;
use std::time::Duration;

use code_app_server_protocol::AppInfo;
use code_connectors::AllConnectorsCacheKey;
use code_connectors::DirectoryListResponse;
use code_core::config::Config;
use code_core::plugins::AppConnectorId;
use code_core::plugins::PluginsManager;
use code_core::token_data::TokenData;

use crate::chatgpt_token::get_chatgpt_token_data;
use crate::chatgpt_token::init_chatgpt_token_from_auth;

const DIRECTORY_CONNECTORS_TIMEOUT: Duration = Duration::from_secs(60);

pub async fn list_all_connectors(config: &Config) -> anyhow::Result<Vec<AppInfo>> {
    list_all_connectors_with_options(config, /*force_refetch*/ false).await
}

pub async fn list_cached_all_connectors(config: &Config) -> Option<Vec<AppInfo>> {
    let token_data = match token_data_for_directory_connectors(config).await {
        Ok(Some(token_data)) => token_data,
        Ok(None) => return Some(Vec::new()),
        Err(_) => return None,
    };

    let cache_key = all_connectors_cache_key(config, &token_data);
    code_connectors::cached_all_connectors(&cache_key)
        .map(|connectors| filter_disallowed_connectors(merge_plugin_apps(connectors, plugin_apps_for_config(config))))
}

pub async fn list_all_connectors_with_options(
    config: &Config,
    force_refetch: bool,
) -> anyhow::Result<Vec<AppInfo>> {
    let Some(token_data) = token_data_for_directory_connectors(config).await? else {
        return Ok(Vec::new());
    };

    let cache_key = all_connectors_cache_key(config, &token_data);
    let connectors = code_connectors::list_all_connectors_with_options(
        cache_key,
        token_data.id_token.is_workspace_account(),
        force_refetch,
        |path| {
            chatgpt_get_request_with_timeout::<DirectoryListResponse>(
                config,
                &token_data,
                path,
                Some(DIRECTORY_CONNECTORS_TIMEOUT),
            )
        },
    )
    .await?;

    let connectors = merge_plugin_apps(connectors, plugin_apps_for_config(config));
    Ok(filter_disallowed_connectors(connectors))
}

async fn token_data_for_directory_connectors(config: &Config) -> anyhow::Result<Option<TokenData>> {
    // Prefer the active auth.json (keeps behavior aligned with other ChatGPT API usage).
    init_chatgpt_token_from_auth(
        &config.code_home,
        config.cli_auth_credentials_store_mode,
        &config.responses_originator_header,
    )
    .await?;
    if let Some(token_data) = get_chatgpt_token_data() {
        return Ok(Some(token_data));
    }

    // Fallback: use the first effective connector-source account (supports pinned-only setups).
    let active_account_id = code_core::auth_accounts::get_active_account_id(&config.code_home)?;
    let effective_source_ids = code_core::apps_sources::effective_source_account_ids(
        &config.apps_sources,
        active_account_id.as_deref(),
    );
    for stored_account_id in effective_source_ids {
        let Some(account) = code_core::auth_accounts::find_account(&config.code_home, &stored_account_id)? else {
            continue;
        };
        if !account.mode.is_chatgpt() {
            continue;
        }

        let auth = code_core::auth::auth_for_stored_account(
            &config.code_home,
            &account,
            "connectors_directory",
        )
        .await?;
        let token_data = match auth.get_token_data().await {
            Ok(token_data) => token_data,
            Err(_) => continue,
        };
        return Ok(Some(token_data));
    }

    Ok(None)
}

fn all_connectors_cache_key(config: &Config, token_data: &TokenData) -> AllConnectorsCacheKey {
    AllConnectorsCacheKey::new(
        config.chatgpt_base_url.clone(),
        token_data.account_id.clone(),
        token_data.id_token.chatgpt_user_id.clone(),
        token_data.id_token.is_workspace_account(),
    )
}

async fn chatgpt_get_request_with_timeout<T: serde::de::DeserializeOwned>(
    config: &Config,
    token_data: &TokenData,
    path: String,
    timeout: Option<Duration>,
) -> anyhow::Result<T> {
    let url = format!("{}{path}", config.chatgpt_base_url);
    let account_id = token_data
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!("ChatGPT account ID not available"))?;

    let client = code_core::http_client::build_http_client();
    let request = client
        .get(&url)
        .bearer_auth(&token_data.access_token)
        .header("chatgpt-account-id", account_id)
        .header("Content-Type", "application/json");

    let response = if let Some(timeout) = timeout {
        tokio::time::timeout(timeout, request.send())
            .await
            .map_err(|_| anyhow::anyhow!("ChatGPT request timed out after {}s", timeout.as_secs()))??
    } else {
        request.send().await?
    };

    if response.status().is_success() {
        Ok(response.json().await?)
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("ChatGPT request failed with status {status}: {body}");
    }
}

fn plugin_apps_for_config(config: &Config) -> Vec<AppConnectorId> {
    PluginsManager::new(config.code_home.clone()).effective_apps()
}

fn merge_plugin_apps(connectors: Vec<AppInfo>, plugin_apps: Vec<AppConnectorId>) -> Vec<AppInfo> {
    let mut merged = connectors;
    let mut connector_ids = merged
        .iter()
        .map(|connector| connector.id.clone())
        .collect::<HashSet<_>>();

    for connector_id in plugin_apps {
        if connector_ids.insert(connector_id.0.clone()) {
            merged.push(plugin_app_to_app_info(connector_id));
        }
    }

    merged.sort_by(|left, right| {
        right
            .is_accessible
            .cmp(&left.is_accessible)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
    merged
}

fn plugin_app_to_app_info(connector_id: AppConnectorId) -> AppInfo {
    AppInfo {
        id: connector_id.0.clone(),
        name: connector_id.0.clone(),
        description: None,
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: Some(connector_install_url(&connector_id.0)),
        is_accessible: true,
        is_enabled: true,
        plugin_display_names: Vec::new(),
    }
}

fn connector_install_url(connector_id: &str) -> String {
    format!("https://chatgpt.com/apps/{connector_id}/{connector_id}")
}

const DISALLOWED_CONNECTOR_IDS: &[&str] = &[
    "asdk_app_6938a94a61d881918ef32cb999ff937c",
    "connector_2b0a9009c9c64bf9933a3dae3f2b1254",
    "connector_3f8d1a79f27c4c7ba1a897ab13bf37dc",
    "connector_68de829bf7648191acd70a907364c67c",
    "connector_68e004f14af881919eb50893d3d9f523",
    "connector_69272cb413a081919685ec3c88d1744e",
];
const DISALLOWED_CONNECTOR_PREFIX: &str = "connector_openai_";

fn filter_disallowed_connectors(connectors: Vec<AppInfo>) -> Vec<AppInfo> {
    connectors
        .into_iter()
        .filter(|connector| {
            let id = connector.id.as_str();
            !id.starts_with(DISALLOWED_CONNECTOR_PREFIX) && !DISALLOWED_CONNECTOR_IDS.contains(&id)
        })
        .collect()
}
