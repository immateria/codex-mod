use super::PluginManifestPaths;
use super::load_plugin_manifest;
use super::marketplace::MarketplaceError;
use super::marketplace::MarketplaceListError;
use super::marketplace::MarketplacePluginPolicy;
use super::marketplace::MarketplacePluginSource;
use super::marketplace::ResolvedMarketplacePlugin;
use super::marketplace::list_marketplaces_outcome;
use super::marketplace::load_marketplace;
use super::marketplace::resolve_marketplace_plugin;
use super::remote::RemotePluginFetchError;
use super::remote::RemotePluginMutationError;
use super::remote::enable_remote_plugin;
use super::remote::fetch_remote_featured_plugin_ids;
use super::remote::fetch_remote_plugin_status;
use super::remote::uninstall_remote_plugin;
use super::startup_sync::curated_plugins_repo_path;
use super::startup_sync::read_curated_plugins_sha;
use super::startup_sync::sync_curated_plugins_repo;
use super::startup_sync::sync_git_marketplace_repo;
use super::startup_sync::synced_marketplace_repo_path;
use super::store::PluginId;
use super::store::PluginIdError;
use super::store::PluginInstallResult as StorePluginInstallResult;
use super::store::PluginStore;
use super::store::PluginStoreError;
use crate::auth::CodexAuth;
use crate::config::Config;
use crate::config_edit::apply_plugin_config_updates;
use crate::config_edit::clear_plugin_config;
use crate::config_edit::set_plugin_enabled;
use crate::config_types::McpServerConfig;
use crate::skills::loader::SkillRoot;
use crate::skills::loader::load_skills_from_roots;
use crate::skills::model::SkillMetadata;
use crate::skills::model::SkillScope;
use code_protocol::protocol::Product;
use code_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde_json::Map as JsonMap;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::Mutex;
use toml::Value as TomlValue;
use tracing::info;
use tracing::warn;

const DEFAULT_SKILLS_DIR_NAME: &str = "skills";
const DEFAULT_MCP_CONFIG_FILE: &str = ".mcp.json";
const DEFAULT_APP_CONFIG_FILE: &str = ".app.json";
pub const OPENAI_CURATED_MARKETPLACE_NAME: &str = "openai-curated";
const FEATURED_PLUGIN_IDS_CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 3);

#[derive(Clone, PartialEq, Eq)]
struct FeaturedPluginIdsCacheKey {
    chatgpt_base_url: String,
    account_id: Option<String>,
}

#[derive(Clone)]
struct CachedFeaturedPluginIds {
    key: FeaturedPluginIdsCacheKey,
    expires_at: Instant,
    featured_plugin_ids: Vec<String>,
}

fn featured_plugin_ids_cache_key(config: &Config, auth: Option<&CodexAuth>) -> FeaturedPluginIdsCacheKey {
    FeaturedPluginIdsCacheKey {
        chatgpt_base_url: config.chatgpt_base_url.clone(),
        account_id: auth.and_then(CodexAuth::get_account_id),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppConnectorId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInstallRequest {
    pub plugin_name: String,
    pub marketplace_path: AbsolutePathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginReadRequest {
    pub plugin_name: String,
    pub marketplace_path: AbsolutePathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInstallOutcome {
    pub plugin_id: PluginId,
    pub plugin_version: String,
    pub installed_path: AbsolutePathBuf,
    pub auth_policy: super::marketplace::MarketplacePluginAuthPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginReadOutcome {
    pub marketplace_name: String,
    pub marketplace_path: AbsolutePathBuf,
    pub plugin: PluginDetail,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginDetail {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source: MarketplacePluginSource,
    pub policy: super::marketplace::MarketplacePluginPolicy,
    pub interface: Option<super::PluginManifestInterface>,
    pub installed: bool,
    pub enabled: bool,
    pub skills: Vec<SkillMetadata>,
    pub apps: Vec<AppConnectorId>,
    pub mcp_server_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCapabilitySummary {
    pub config_name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub has_skills: bool,
    pub mcp_server_names: Vec<String>,
    pub apps: Vec<AppConnectorId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredMarketplace {
    pub name: String,
    pub path: AbsolutePathBuf,
    pub interface: Option<super::marketplace::MarketplaceInterface>,
    pub plugins: Vec<ConfiguredMarketplacePlugin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredMarketplacePlugin {
    pub id: String,
    pub name: String,
    pub source: MarketplacePluginSource,
    pub policy: MarketplacePluginPolicy,
    pub interface: Option<super::PluginManifestInterface>,
    pub installed: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfiguredMarketplaceListOutcome {
    pub marketplaces: Vec<ConfiguredMarketplace>,
    pub errors: Vec<MarketplaceListError>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemotePluginSyncResult {
    /// Plugin ids newly installed into the local plugin cache.
    pub installed_plugin_ids: Vec<String>,
    /// Plugin ids whose local config was changed to enabled.
    pub enabled_plugin_ids: Vec<String>,
    /// Plugin ids whose local config was changed to disabled.
    /// This is not populated by `sync_plugins_from_remote`.
    pub disabled_plugin_ids: Vec<String>,
    /// Plugin ids removed from local cache or plugin config.
    pub uninstalled_plugin_ids: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PluginRemoteSyncError {
    #[error("chatgpt authentication required to sync remote plugins")]
    AuthRequired,

    #[error(
        "chatgpt authentication required to sync remote plugins; api key auth is not supported"
    )]
    UnsupportedAuthMode,

    #[error("failed to get auth token for remote plugin sync: {0}")]
    AuthToken(#[source] std::io::Error),

    #[error("failed to send remote plugin sync request to {url}: {source}")]
    Request {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("remote plugin sync request to {url} failed with status {status}: {body}")]
    UnexpectedStatus {
        url: String,
        status: reqwest::StatusCode,
        body: String,
    },

    #[error("failed to parse remote plugin sync response from {url}: {source}")]
    Decode {
        url: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("local curated marketplace is not available")]
    LocalMarketplaceNotFound,

    #[error("remote marketplace `{marketplace_name}` is not available locally")]
    UnknownRemoteMarketplace { marketplace_name: String },

    #[error("duplicate remote plugin `{plugin_name}` returned during sync")]
    DuplicateRemotePlugin { plugin_name: String },

    #[error("{0}")]
    InvalidPluginId(#[from] PluginIdError),

    #[error("{0}")]
    Marketplace(#[from] MarketplaceError),

    #[error("{0}")]
    Store(#[from] PluginStoreError),

    #[error("{0}")]
    Config(#[from] anyhow::Error),

    #[error("failed to join remote plugin sync task: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl PluginRemoteSyncError {
    fn join(source: tokio::task::JoinError) -> Self {
        Self::Join(source)
    }
}

impl From<RemotePluginFetchError> for PluginRemoteSyncError {
    fn from(value: RemotePluginFetchError) -> Self {
        match value {
            RemotePluginFetchError::AuthRequired => Self::AuthRequired,
            RemotePluginFetchError::UnsupportedAuthMode => Self::UnsupportedAuthMode,
            RemotePluginFetchError::AuthToken(source) => Self::AuthToken(source),
            RemotePluginFetchError::Request { url, source } => Self::Request { url, source },
            RemotePluginFetchError::UnexpectedStatus { url, status, body } => {
                Self::UnexpectedStatus { url, status, body }
            }
            RemotePluginFetchError::Decode { url, source } => Self::Decode { url, source },
        }
    }
}

pub struct PluginsManager {
    code_home: PathBuf,
    store: PluginStore,
    featured_plugin_ids_cache: RwLock<Option<CachedFeaturedPluginIds>>,
    remote_sync_lock: Mutex<()>,
    restriction_product: Option<Product>,
}

impl PluginsManager {
    pub fn new(code_home: PathBuf) -> Self {
        Self::new_with_restriction_product(code_home, Some(Product::Codex))
    }

    pub fn new_with_restriction_product(
        code_home: PathBuf,
        restriction_product: Option<Product>,
    ) -> Self {
        let store = PluginStore::new(code_home.clone());
        Self {
            code_home,
            store,
            featured_plugin_ids_cache: RwLock::new(None),
            remote_sync_lock: Mutex::new(()),
            restriction_product,
        }
    }

    fn restriction_product_matches(&self, products: Option<&[Product]>) -> bool {
        match products {
            None => true,
            Some([]) => false,
            Some(products) => self
                .restriction_product
                .is_some_and(|product| product.matches_product_restriction(products)),
        }
    }

    pub fn store(&self) -> &PluginStore {
        &self.store
    }

    pub async fn featured_plugin_ids_for_config(
        &self,
        config: &Config,
        auth: Option<&CodexAuth>,
    ) -> Result<Vec<String>, RemotePluginFetchError> {
        let cache_key = featured_plugin_ids_cache_key(config, auth);
        if let Some(featured_plugin_ids) = self.cached_featured_plugin_ids(&cache_key) {
            return Ok(featured_plugin_ids);
        }

        let featured_plugin_ids =
            fetch_remote_featured_plugin_ids(config, auth, self.restriction_product).await?;
        self.write_featured_plugin_ids_cache(cache_key, &featured_plugin_ids);
        Ok(featured_plugin_ids)
    }

    fn cached_featured_plugin_ids(
        &self,
        cache_key: &FeaturedPluginIdsCacheKey,
    ) -> Option<Vec<String>> {
        {
            let cache = match self.featured_plugin_ids_cache.read() {
                Ok(cache) => cache,
                Err(err) => err.into_inner(),
            };
            let now = Instant::now();
            if let Some(cached) = cache.as_ref()
                && now < cached.expires_at
                && cached.key == *cache_key
            {
                return Some(cached.featured_plugin_ids.clone());
            }
        }

        let mut cache = match self.featured_plugin_ids_cache.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        let now = Instant::now();
        if cache
            .as_ref()
            .is_some_and(|cached| now >= cached.expires_at || cached.key != *cache_key)
        {
            *cache = None;
        }
        None
    }

    fn write_featured_plugin_ids_cache(
        &self,
        cache_key: FeaturedPluginIdsCacheKey,
        featured_plugin_ids: &[String],
    ) {
        let mut cache = match self.featured_plugin_ids_cache.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        *cache = Some(CachedFeaturedPluginIds {
            key: cache_key,
            expires_at: Instant::now() + FEATURED_PLUGIN_IDS_CACHE_TTL,
            featured_plugin_ids: featured_plugin_ids.to_vec(),
        });
    }

    fn capability_summary_for_plugin_id(
        &self,
        config_name: &str,
        plugin_id: &PluginId,
    ) -> Option<PluginCapabilitySummary> {
        let plugin_root = self.store.active_plugin_root(plugin_id)?;

        let (manifest, description, display_name) = match load_plugin_manifest(plugin_root.as_path()) {
            Some(manifest) => {
                let description = manifest.description.clone();
                // Match upstream behavior: `display_name` matches the plugin namespace used for
                // skills (derived from the manifest name), not the optional UI display name.
                let display_name = if manifest.name.trim().is_empty() {
                    plugin_id.plugin_name.clone()
                } else {
                    manifest.name.clone()
                };
                (Some(manifest), description, display_name)
            }
            None => (None, None, plugin_id.plugin_name.clone()),
        };

        let apps = load_plugin_apps(plugin_root.as_path());
        let mcp_servers = load_plugin_mcp_servers(plugin_root.as_path());
        let mut mcp_server_names: Vec<String> = mcp_servers.into_keys().collect();
        mcp_server_names.sort_unstable();
        mcp_server_names.dedup();

        let skills = load_plugin_skills(
            plugin_root.as_path(),
            manifest.as_ref().map(|manifest| &manifest.paths),
        );

        Some(PluginCapabilitySummary {
            config_name: config_name.to_string(),
            display_name,
            description,
            has_skills: !skills.is_empty(),
            mcp_server_names,
            apps,
        })
    }

    pub fn capability_summaries(&self) -> Vec<PluginCapabilitySummary> {
        let configured_plugins = configured_plugins_from_code_home(&self.code_home);
        let mut out = Vec::new();

        for (config_name, enabled) in configured_plugins {
            if !enabled {
                continue;
            }
            let plugin_id = match PluginId::parse(&config_name) {
                Ok(plugin_id) => plugin_id,
                Err(err) => {
                    warn!("invalid plugin id in config: {err}");
                    continue;
                }
            };

            if let Some(summary) = self.capability_summary_for_plugin_id(&config_name, &plugin_id) {
                out.push(summary);
            }
        }

        out.sort_unstable_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then_with(|| left.config_name.cmp(&right.config_name))
        });
        out
    }

    /// Return apps exposed by enabled, installed plugins.
    pub fn effective_apps(&self) -> Vec<AppConnectorId> {
        let configured_plugins = configured_plugins_from_code_home(&self.code_home);
        let mut apps = Vec::new();

        for (plugin_key, enabled) in configured_plugins {
            if !enabled {
                continue;
            }
            let plugin_id = match PluginId::parse(&plugin_key) {
                Ok(plugin_id) => plugin_id,
                Err(err) => {
                    warn!("invalid plugin id in config: {err}");
                    continue;
                }
            };
            let Some(plugin_root) = self.store.active_plugin_root(&plugin_id) else {
                continue;
            };
            apps.extend(load_plugin_apps(plugin_root.as_path()));
        }

        apps.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        apps.dedup_by(|left, right| left.0 == right.0);
        apps
    }

    pub fn capability_summary_for_config_name(
        &self,
        config_name: &str,
    ) -> Option<PluginCapabilitySummary> {
        let configured_plugins = configured_plugins_from_code_home(&self.code_home);
        if configured_plugins
            .get(config_name)
            .is_some_and(|enabled| !*enabled)
        {
            return None;
        }

        let plugin_id = PluginId::parse(config_name).ok()?;
        self.capability_summary_for_plugin_id(config_name, &plugin_id)
    }

    /// Returns the plugin namespace for a `SKILL.md` path, if the skill is contained within a
    /// plugin directory.
    ///
    /// This is used to prefix plugin-provided skills as `plugin_name:skill_name` to avoid naming
    /// collisions with user/repo skills.
    pub(crate) fn plugin_namespace_for_skill_path(path: &Path) -> Option<String> {
        for ancestor in path.ancestors() {
            if let Some(manifest) = load_plugin_manifest(ancestor) {
                return Some(manifest.name);
            }
        }

        None
    }

    pub(crate) fn effective_skill_roots(&self) -> Vec<PathBuf> {
        let configured_plugins = configured_plugins_from_code_home(&self.code_home);
        let mut roots = Vec::new();

        for (plugin_key, enabled) in configured_plugins {
            if !enabled {
                continue;
            }
            let plugin_id = match PluginId::parse(&plugin_key) {
                Ok(plugin_id) => plugin_id,
                Err(err) => {
                    warn!("invalid plugin id in config: {err}");
                    continue;
                }
            };
            let Some(plugin_root) = self.store.active_plugin_root(&plugin_id) else {
                continue;
            };

            let root_paths = if let Some(manifest) = load_plugin_manifest(plugin_root.as_path()) {
                plugin_skill_roots(plugin_root.as_path(), &manifest.paths)
            } else {
                default_skill_roots(plugin_root.as_path())
            };
            roots.extend(root_paths.into_iter().filter(|path| path.is_dir()));
        }

        roots.sort();
        roots.dedup();
        roots
    }

    pub fn list_marketplaces_for_roots(
        &self,
        config: &Config,
        config_cwds: &[AbsolutePathBuf],
    ) -> Result<ConfiguredMarketplaceListOutcome, MarketplaceError> {
        let configured_plugins = configured_plugins_from_code_home(&self.code_home);
        let roots = self.marketplace_roots(config, config_cwds);
        let mut seen_plugin_keys = HashSet::new();

        let marketplace_outcome = list_marketplaces_outcome(&roots)?;

        let mut marketplaces = Vec::new();
        for marketplace in marketplace_outcome.marketplaces {
            let marketplace_name = marketplace.name.clone();
            let mut plugins = Vec::with_capacity(marketplace.plugins.len());
            for plugin in marketplace.plugins {
                if !self.restriction_product_matches(plugin.policy.products.as_deref()) {
                    continue;
                }

                let plugin_id =
                    PluginId::new(plugin.name.clone(), marketplace_name.clone()).map_err(|err| {
                        match err {
                            PluginIdError::Invalid(message) => MarketplaceError::InvalidPlugin(message),
                        }
                    })?;
                let plugin_key = plugin_id.as_key();
                if !seen_plugin_keys.insert(plugin_key.clone()) {
                    continue;
                }

                let installed = self.store.is_installed(&plugin_id);
                let enabled = configured_plugins
                    .get(&plugin_key)
                    .copied()
                    .unwrap_or(true);

                plugins.push(ConfiguredMarketplacePlugin {
                    id: plugin_key,
                    name: plugin.name,
                    source: plugin.source,
                    policy: plugin.policy,
                    interface: plugin.interface,
                    installed,
                    enabled,
                });
            }

            plugins.sort_unstable_by(|left, right| left.name.cmp(&right.name).then_with(|| left.id.cmp(&right.id)));

            if !plugins.is_empty() {
                marketplaces.push(ConfiguredMarketplace {
                    name: marketplace.name,
                    path: marketplace.path,
                    interface: marketplace.interface,
                    plugins,
                });
            }
        }

        marketplaces.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        Ok(ConfiguredMarketplaceListOutcome {
            marketplaces,
            errors: marketplace_outcome.errors,
        })
    }

    pub fn read_plugin_for_config(
        &self,
        request: &PluginReadRequest,
    ) -> Result<PluginReadOutcome, MarketplaceError> {
        let configured_plugins = configured_plugins_from_code_home(&self.code_home);
        let marketplace = load_marketplace(&request.marketplace_path)?;
        let marketplace_name = marketplace.name.clone();

        let plugin = marketplace
            .plugins
            .into_iter()
            .find(|plugin| plugin.name == request.plugin_name);
        let Some(plugin) = plugin else {
            return Err(MarketplaceError::PluginNotFound {
                plugin_name: request.plugin_name.clone(),
                marketplace_name,
            });
        };
        if !self.restriction_product_matches(plugin.policy.products.as_deref()) {
            return Err(MarketplaceError::PluginNotFound {
                plugin_name: request.plugin_name.clone(),
                marketplace_name,
            });
        }

        let plugin_id = PluginId::new(plugin.name.clone(), marketplace.name.clone()).map_err(|err| match err {
            PluginIdError::Invalid(message) => MarketplaceError::InvalidPlugin(message),
        })?;
        let plugin_key = plugin_id.as_key();
        let installed_root = self.store.active_plugin_root(&plugin_id);
        let installed = installed_root.is_some();
        let enabled = configured_plugins.get(&plugin_key).copied().unwrap_or(true);

        let source_root = match &plugin.source {
            MarketplacePluginSource::Local { path } => path.as_path(),
        };
        let plugin_root = installed_root
            .as_ref()
            .map(AbsolutePathBuf::as_path)
            .unwrap_or(source_root);

        let (description, manifest_paths) = match load_plugin_manifest(plugin_root) {
            Some(manifest) => (manifest.description, Some(manifest.paths)),
            None => (None, None),
        };

        let apps = load_apps_from_paths(
            plugin_root,
            manifest_paths
                .as_ref()
                .map(|paths| plugin_app_config_paths(plugin_root, paths))
                .unwrap_or_else(|| default_app_config_paths(plugin_root)),
        );

        let mcp_servers = load_plugin_mcp_servers(plugin_root);
        let mut mcp_server_names: Vec<String> = mcp_servers.into_keys().collect();
        mcp_server_names.sort_unstable();
        mcp_server_names.dedup();

        let skills = load_plugin_skills(plugin_root, manifest_paths.as_ref());

        Ok(PluginReadOutcome {
            marketplace_name: marketplace.name,
            marketplace_path: request.marketplace_path.clone(),
            plugin: PluginDetail {
                id: plugin_key,
                name: plugin.name,
                description,
                source: plugin.source,
                policy: plugin.policy,
                interface: plugin.interface,
                installed,
                enabled,
                skills,
                apps,
                mcp_server_names,
            },
        })
    }

    pub async fn install_plugin(
        &self,
        request: PluginInstallRequest,
    ) -> Result<PluginInstallOutcome, PluginInstallError> {
        let resolved = resolve_marketplace_plugin(
            &request.marketplace_path,
            &request.plugin_name,
            self.restriction_product,
        )?;
        self.install_resolved_plugin(resolved).await
    }

    pub async fn install_plugin_with_remote_sync(
        &self,
        config: &Config,
        auth: Option<&CodexAuth>,
        request: PluginInstallRequest,
    ) -> Result<PluginInstallOutcome, PluginInstallError> {
        let resolved = resolve_marketplace_plugin(
            &request.marketplace_path,
            &request.plugin_name,
            self.restriction_product,
        )?;
        let plugin_id = resolved.plugin_id.as_key();
        // Forward the backend mutation before the local install flow. We rely on
        // `plugin/list(forceRemoteSync=true)` to sync local state rather than doing an extra
        // reconcile pass here.
        enable_remote_plugin(config, auth, &plugin_id)
            .await
            .map_err(PluginInstallError::from)?;
        self.install_resolved_plugin(resolved).await
    }

    async fn install_resolved_plugin(
        &self,
        resolved: ResolvedMarketplacePlugin,
    ) -> Result<PluginInstallOutcome, PluginInstallError> {
        let ResolvedMarketplacePlugin {
            plugin_id,
            source_path,
            auth_policy,
        } = resolved;

        let plugin_key = plugin_id.as_key();
        let curated_plugin_version = if plugin_id.marketplace_name == OPENAI_CURATED_MARKETPLACE_NAME
        {
            let version = read_curated_plugins_sha(self.code_home.as_path());
            if version.is_none() {
                warn!(
                    plugin = plugin_key,
                    "curated plugin sha is not available; installing with default plugin version"
                );
            }
            version
        } else {
            None
        };
        let store = self.store.clone();
        let result: StorePluginInstallResult = tokio::task::spawn_blocking(move || {
            if let Some(plugin_version) = curated_plugin_version {
                store.install_with_version(source_path, plugin_id, plugin_version)
            } else {
                store.install(source_path, plugin_id)
            }
        })
        .await
        .map_err(PluginInstallError::join)??;

        set_plugin_enabled(&self.code_home, &result.plugin_id.as_key(), true)
            .await
            .map_err(PluginInstallError::from)?;

        Ok(PluginInstallOutcome {
            plugin_id: result.plugin_id,
            plugin_version: result.plugin_version,
            installed_path: result.installed_path,
            auth_policy,
        })
    }

    pub async fn uninstall_plugin(&self, plugin_id: String) -> Result<(), PluginUninstallError> {
        let plugin_id = PluginId::parse(&plugin_id)?;
        self.uninstall_plugin_id(plugin_id).await
    }

    pub async fn uninstall_plugin_with_remote_sync(
        &self,
        config: &Config,
        auth: Option<&CodexAuth>,
        plugin_id: String,
    ) -> Result<(), PluginUninstallError> {
        let plugin_id = PluginId::parse(&plugin_id)?;
        let plugin_key = plugin_id.as_key();
        // Forward the backend mutation before the local uninstall flow. We rely on
        // `plugin/list(forceRemoteSync=true)` to sync local state rather than doing an extra
        // reconcile pass here.
        uninstall_remote_plugin(config, auth, &plugin_key)
            .await
            .map_err(PluginUninstallError::from)?;
        self.uninstall_plugin_id(plugin_id).await
    }

    async fn uninstall_plugin_id(&self, plugin_id: PluginId) -> Result<(), PluginUninstallError> {
        let store = self.store.clone();
        let plugin_id_for_store = plugin_id.clone();
        tokio::task::spawn_blocking(move || store.uninstall(&plugin_id_for_store))
            .await
            .map_err(PluginUninstallError::join)??;

        clear_plugin_config(&self.code_home, &plugin_id.as_key())
            .await
            .map_err(PluginUninstallError::from)?;

        Ok(())
    }

    pub async fn sync_plugins_from_remote(
        &self,
        config: &Config,
        auth: Option<&CodexAuth>,
        additive_only: bool,
    ) -> Result<RemotePluginSyncResult, PluginRemoteSyncError> {
        let _remote_sync_guard = self.remote_sync_lock.lock().await;

        info!("starting remote plugin sync");
        let configured_plugins = configured_plugins_from_code_home(&self.code_home);

        let curated_marketplace_root = curated_plugins_repo_path(self.code_home.as_path());
        let curated_marketplace_manifest_path =
            curated_marketplace_root.join(".agents/plugins/marketplace.json");
        let curated_sha = read_curated_plugins_sha(self.code_home.as_path());

        if !curated_marketplace_manifest_path.is_file() || curated_sha.is_none() {
            let code_home = self.code_home.clone();
            let plugins_config = config.plugins.clone();
            let sync_result = tokio::task::spawn_blocking(move || {
                sync_curated_plugins_repo(code_home.as_path(), &plugins_config)
            })
            .await
            .map_err(PluginRemoteSyncError::join)?;
            if let Err(err) = sync_result {
                return Err(PluginRemoteSyncError::Store(PluginStoreError::Invalid(
                    format!("failed to sync curated plugin marketplace: {err}"),
                )));
            }
        }

        let curated_marketplace_path = AbsolutePathBuf::try_from(curated_marketplace_manifest_path)
            .map_err(|_| PluginRemoteSyncError::LocalMarketplaceNotFound)?;
        let curated_marketplace = match load_marketplace(&curated_marketplace_path) {
            Ok(marketplace) => marketplace,
            Err(MarketplaceError::MarketplaceNotFound { .. }) => {
                return Err(PluginRemoteSyncError::LocalMarketplaceNotFound);
            }
            Err(err) => return Err(err.into()),
        };

        let marketplace_name = curated_marketplace.name.clone();
        if marketplace_name != OPENAI_CURATED_MARKETPLACE_NAME {
            info!(
                marketplace = %marketplace_name,
                "skipping ChatGPT remote plugin state sync for non-openai curated marketplace"
            );
            return Ok(RemotePluginSyncResult::default());
        }
        let remote_plugins = fetch_remote_plugin_status(config, auth)
            .await
            .map_err(PluginRemoteSyncError::from)?;
        let curated_plugin_version = read_curated_plugins_sha(self.code_home.as_path()).ok_or_else(
            || {
                PluginStoreError::Invalid(
                    "local curated marketplace sha is not available".to_string(),
                )
            },
        )?;

        let mut local_plugins = Vec::<(
            String,
            PluginId,
            AbsolutePathBuf,
            Option<bool>,
            Option<String>,
            bool,
        )>::new();
        let mut local_plugin_names = HashSet::new();

        for plugin in curated_marketplace.plugins {
            let plugin_name = plugin.name;
            if !local_plugin_names.insert(plugin_name.clone()) {
                warn!(
                    plugin = plugin_name,
                    marketplace = %marketplace_name,
                    "ignoring duplicate local plugin entry during remote sync"
                );
                continue;
            }

            let plugin_id = PluginId::new(plugin_name.clone(), marketplace_name.clone())?;
            let plugin_key = plugin_id.as_key();
            let source_path = match plugin.source {
                MarketplacePluginSource::Local { path } => path,
            };
            let current_enabled = configured_plugins.get(&plugin_key).copied();
            let installed_version = self.store.active_plugin_version(&plugin_id);
            let product_allowed = self.restriction_product_matches(plugin.policy.products.as_deref());

            local_plugins.push((
                plugin_name,
                plugin_id,
                source_path,
                current_enabled,
                installed_version,
                product_allowed,
            ));
        }

        let mut remote_installed_plugin_names = HashSet::<String>::new();
        for plugin in remote_plugins {
            if plugin.marketplace_name != marketplace_name {
                return Err(PluginRemoteSyncError::UnknownRemoteMarketplace {
                    marketplace_name: plugin.marketplace_name,
                });
            }
            if !local_plugin_names.contains(&plugin.name) {
                warn!(
                    plugin = plugin.name,
                    marketplace = %marketplace_name,
                    "ignoring remote plugin missing from local marketplace during sync"
                );
                continue;
            }
            // For now, sync treats remote `enabled = false` as uninstall rather than a distinct
            // disabled state.
            // TODO: Switch sync to `plugins/installed` so install and enable states stay distinct.
            if !plugin.enabled {
                continue;
            }
            if !remote_installed_plugin_names.insert(plugin.name.clone()) {
                return Err(PluginRemoteSyncError::DuplicateRemotePlugin {
                    plugin_name: plugin.name,
                });
            }
        }

        let mut enable_keys = Vec::new();
        let mut clear_keys = Vec::new();
        let mut installs = Vec::new();
        let mut uninstalls = Vec::new();
        let mut result = RemotePluginSyncResult::default();
        let remote_plugin_count = remote_installed_plugin_names.len();
        let local_plugin_count = local_plugins.len();

        for (
            plugin_name,
            plugin_id,
            source_path,
            current_enabled,
            installed_version,
            product_allowed,
        ) in local_plugins
        {
            if !product_allowed {
                continue;
            }

            let plugin_key = plugin_id.as_key();
            let is_installed = installed_version.is_some();

            if remote_installed_plugin_names.contains(&plugin_name) {
                if !is_installed {
                    installs.push((
                        source_path,
                        plugin_id.clone(),
                        curated_plugin_version.clone(),
                    ));
                    result.installed_plugin_ids.push(plugin_key.clone());
                }

                if current_enabled != Some(true) {
                    enable_keys.push(plugin_key.clone());
                    result.enabled_plugin_ids.push(plugin_key.clone());
                }
            } else if !additive_only {
                if is_installed {
                    uninstalls.push(plugin_id);
                }
                if is_installed || current_enabled.is_some() {
                    result.uninstalled_plugin_ids.push(plugin_key.clone());
                }
                if current_enabled.is_some() {
                    clear_keys.push(plugin_key);
                }
            }
        }

        let store = self.store.clone();
        let store_result = tokio::task::spawn_blocking(move || {
            for (source_path, plugin_id, plugin_version) in installs {
                store.install_with_version(source_path, plugin_id, plugin_version)?;
            }
            for plugin_id in uninstalls {
                store.uninstall(&plugin_id)?;
            }
            Ok::<(), PluginStoreError>(())
        })
        .await
        .map_err(PluginRemoteSyncError::join)?;
        store_result?;

        if !enable_keys.is_empty() || !clear_keys.is_empty() {
            apply_plugin_config_updates(self.code_home.as_path(), &enable_keys, &clear_keys)
                .await
                .map_err(PluginRemoteSyncError::from)?;
        }

        info!(
            marketplace = %marketplace_name,
            remote_plugin_count,
            local_plugin_count,
            installed_plugin_ids = ?result.installed_plugin_ids,
            enabled_plugin_ids = ?result.enabled_plugin_ids,
            disabled_plugin_ids = ?result.disabled_plugin_ids,
            uninstalled_plugin_ids = ?result.uninstalled_plugin_ids,
            "completed remote plugin sync"
        );

        Ok(result)
    }

    pub async fn sync_marketplace_sources(&self, config: &Config) -> Result<(), String> {
        let code_home = self.code_home.clone();
        let plugins = config.plugins.clone();
        tokio::task::spawn_blocking(move || {
            let mut errors = Vec::new();

            if let Err(err) = sync_curated_plugins_repo(code_home.as_path(), &plugins) {
                errors.push(format!("curated marketplace sync failed: {err}"));
            }

            for repo in &plugins.marketplace_repos {
                if let Err(err) = sync_git_marketplace_repo(code_home.as_path(), repo) {
                    errors.push(format!("marketplace repo sync failed for {}: {err}", repo.url));
                }
            }

            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors.join("; "))
            }
        })
        .await
        .map_err(|err| format!("failed to join plugin marketplace sync task: {err}"))?
    }

    /// Merge enabled, installed plugin MCP servers into a single map.
    ///
    /// Explicit config entries win on name collisions; callers should typically
    /// `entry(name).or_insert(server_cfg)` when merging these into their config map.
    pub fn effective_mcp_servers(&self) -> HashMap<String, McpServerConfig> {
        let configured_plugins = configured_plugins_from_code_home(&self.code_home);
        let mut seen_mcp_server_names = HashMap::<String, String>::new();
        let mut mcp_servers = HashMap::<String, McpServerConfig>::new();

        for (plugin_key, enabled) in configured_plugins {
            if !enabled {
                continue;
            }
            let plugin_id = match PluginId::parse(&plugin_key) {
                Ok(plugin_id) => plugin_id,
                Err(err) => {
                    warn!("invalid plugin id in config: {err}");
                    continue;
                }
            };
            let Some(plugin_root) = self.store.active_plugin_root(&plugin_id) else {
                continue;
            };
            for (name, config) in load_plugin_mcp_servers(plugin_root.as_path()) {
                if let Some(previous_plugin) = seen_mcp_server_names.insert(name.clone(), plugin_key.clone()) {
                    warn!(
                        plugin = plugin_key,
                        previous_plugin,
                        server = name,
                        "skipping duplicate plugin MCP server name"
                    );
                    continue;
                }
                mcp_servers.entry(name).or_insert(config);
            }
        }

        mcp_servers
    }

    fn marketplace_roots(&self, config: &Config, config_cwds: &[AbsolutePathBuf]) -> Vec<AbsolutePathBuf> {
        let mut roots = config_cwds.to_vec();
        let curated_repo_root = curated_plugins_repo_path(self.code_home.as_path());
        if curated_repo_root.is_dir()
            && let Ok(curated_repo_root) = AbsolutePathBuf::try_from(curated_repo_root)
        {
            roots.push(curated_repo_root);
        }
        for repo in &config.plugins.marketplace_repos {
            let synced_repo_root = synced_marketplace_repo_path(self.code_home.as_path(), repo);
            if synced_repo_root.is_dir()
                && let Ok(synced_repo_root) = AbsolutePathBuf::try_from(synced_repo_root)
            {
                roots.push(synced_repo_root);
            }
        }
        roots.sort_unstable_by(|left, right| left.as_path().cmp(right.as_path()));
        roots.dedup_by(|left, right| left.as_path() == right.as_path());
        roots
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginInstallError {
    #[error("{0}")]
    Marketplace(#[from] MarketplaceError),

    #[error("{0}")]
    Remote(#[from] RemotePluginMutationError),

    #[error("{0}")]
    Store(#[from] PluginStoreError),

    #[error("{0}")]
    Config(#[from] anyhow::Error),

    #[error("failed to join plugin install task: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl PluginInstallError {
    fn join(source: tokio::task::JoinError) -> Self {
        Self::Join(source)
    }

    pub fn is_invalid_request(&self) -> bool {
        matches!(
            self,
            Self::Marketplace(
                MarketplaceError::MarketplaceNotFound { .. }
                    | MarketplaceError::InvalidMarketplaceFile { .. }
                    | MarketplaceError::PluginNotFound { .. }
                    | MarketplaceError::PluginNotAvailable { .. }
                    | MarketplaceError::InvalidPlugin(_)
            ) | Self::Store(PluginStoreError::Invalid(_))
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginUninstallError {
    #[error("{0}")]
    InvalidPluginId(#[from] PluginIdError),

    #[error("{0}")]
    Remote(#[from] RemotePluginMutationError),

    #[error("{0}")]
    Store(#[from] PluginStoreError),

    #[error("{0}")]
    Config(#[from] anyhow::Error),

    #[error("failed to join plugin uninstall task: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl PluginUninstallError {
    fn join(source: tokio::task::JoinError) -> Self {
        Self::Join(source)
    }

    pub fn is_invalid_request(&self) -> bool {
        matches!(self, Self::InvalidPluginId(_))
    }
}

const fn default_enabled() -> bool {
    true
}

fn configured_plugins_from_code_home(code_home: &Path) -> HashMap<String, bool> {
    // Plugin entries remain persisted user config only.
    //
    // Load via the config layer stack so any user-config disablement/parsing logic stays
    // consistent with the rest of the config system.
    let stack = match crate::config_loader::load_config_layers_state_blocking(
        code_home,
        &[],
        crate::config_loader::LoaderOverrides::default(),
    ) {
        Ok(stack) => stack,
        Err(err) => {
            warn!("failed to load config layers for plugin config: {err}");
            return HashMap::new();
        }
    };

    let user_layer = stack.layers_high_to_low().find(|layer| {
        matches!(
            layer.name,
            code_app_server_protocol::ConfigLayerSource::User { .. }
        )
    });
    let Some(user_layer) = user_layer else {
        return HashMap::new();
    };
    if user_layer.disabled_reason.is_some() {
        return HashMap::new();
    }

    let parsed: TomlValue = match user_layer.config.clone().try_into() {
        Ok(parsed) => parsed,
        Err(err) => {
            warn!("failed to parse plugins config: {err}");
            return HashMap::new();
        }
    };

    let Some(plugins) = parsed.get("plugins").and_then(TomlValue::as_table) else {
        return HashMap::new();
    };

    let mut configured_plugins = HashMap::new();
    for (key, value) in plugins {
        let Some(table) = value.as_table() else {
            continue;
        };
        let enabled = table
            .get("enabled")
            .and_then(TomlValue::as_bool)
            .unwrap_or_else(default_enabled);
        configured_plugins.insert(key.clone(), enabled);
    }
    configured_plugins
}

fn plugin_skill_roots(plugin_root: &Path, manifest_paths: &PluginManifestPaths) -> Vec<PathBuf> {
    if let Some(path) = &manifest_paths.skills {
        return vec![path.as_path().to_path_buf()];
    }
    default_skill_roots(plugin_root)
}

fn default_skill_roots(plugin_root: &Path) -> Vec<PathBuf> {
    let skills_dir = plugin_root.join(DEFAULT_SKILLS_DIR_NAME);
    if skills_dir.is_dir() {
        vec![skills_dir]
    } else {
        Vec::new()
    }
}

fn load_plugin_skills(plugin_root: &Path, manifest_paths: Option<&PluginManifestPaths>) -> Vec<SkillMetadata> {
    let roots = match manifest_paths {
        Some(paths) => plugin_skill_roots(plugin_root, paths),
        None => default_skill_roots(plugin_root),
    };
    if roots.is_empty() {
        return Vec::new();
    }

    let skill_roots = roots.into_iter().map(|path| SkillRoot { path, scope: SkillScope::User });
    load_skills_from_roots(skill_roots).skills
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginMcpFile {
    #[serde(default)]
    mcp_servers: HashMap<String, JsonValue>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginAppFile {
    #[serde(default)]
    apps: HashMap<String, PluginAppConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct PluginAppConfig {
    id: String,
}

pub fn load_plugin_apps(plugin_root: &Path) -> Vec<AppConnectorId> {
    if let Some(manifest) = load_plugin_manifest(plugin_root) {
        return load_apps_from_paths(
            plugin_root,
            plugin_app_config_paths(plugin_root, &manifest.paths),
        );
    }
    load_apps_from_paths(plugin_root, default_app_config_paths(plugin_root))
}

fn plugin_app_config_paths(
    plugin_root: &Path,
    manifest_paths: &PluginManifestPaths,
) -> Vec<AbsolutePathBuf> {
    if let Some(path) = &manifest_paths.apps {
        return vec![path.clone()];
    }
    default_app_config_paths(plugin_root)
}

fn default_app_config_paths(plugin_root: &Path) -> Vec<AbsolutePathBuf> {
    let mut paths = Vec::new();
    let default_path = plugin_root.join(DEFAULT_APP_CONFIG_FILE);
    if default_path.is_file()
        && let Ok(default_path) = AbsolutePathBuf::try_from(default_path)
    {
        paths.push(default_path);
    }
    paths.sort_unstable_by(|left, right| left.as_path().cmp(right.as_path()));
    paths.dedup_by(|left, right| left.as_path() == right.as_path());
    paths
}

fn load_apps_from_paths(
    plugin_root: &Path,
    app_config_paths: Vec<AbsolutePathBuf>,
) -> Vec<AppConnectorId> {
    let mut connector_ids = Vec::new();
    for app_config_path in app_config_paths {
        let Ok(contents) = fs::read_to_string(app_config_path.as_path()) else {
            continue;
        };
        let parsed = match serde_json::from_str::<PluginAppFile>(&contents) {
            Ok(parsed) => parsed,
            Err(err) => {
                warn!(
                    path = %app_config_path.display(),
                    "failed to parse plugin app config: {err}"
                );
                continue;
            }
        };

        let mut apps: Vec<PluginAppConfig> = parsed.apps.into_values().collect();
        apps.sort_unstable_by(|left, right| left.id.cmp(&right.id));

        connector_ids.extend(apps.into_iter().filter_map(|app| {
            if app.id.trim().is_empty() {
                warn!(
                    plugin = %plugin_root.display(),
                    "plugin app config is missing an app id"
                );
                None
            } else {
                Some(AppConnectorId(app.id))
            }
        }));
    }
    connector_ids.dedup();
    connector_ids
}

pub fn load_plugin_mcp_servers(plugin_root: &Path) -> HashMap<String, McpServerConfig> {
    let Some(manifest) = load_plugin_manifest(plugin_root) else {
        return HashMap::new();
    };

    let mut mcp_servers = HashMap::new();
    for mcp_config_path in plugin_mcp_config_paths(plugin_root, &manifest.paths) {
        let plugin_mcp = load_mcp_servers_from_file(plugin_root, &mcp_config_path);
        for (name, config) in plugin_mcp.mcp_servers {
            mcp_servers.entry(name).or_insert(config);
        }
    }

    mcp_servers
}

fn plugin_mcp_config_paths(
    plugin_root: &Path,
    manifest_paths: &PluginManifestPaths,
) -> Vec<AbsolutePathBuf> {
    if let Some(path) = &manifest_paths.mcp_servers {
        return vec![path.clone()];
    }
    default_mcp_config_paths(plugin_root)
}

fn default_mcp_config_paths(plugin_root: &Path) -> Vec<AbsolutePathBuf> {
    let mut paths = Vec::new();
    let default_path = plugin_root.join(DEFAULT_MCP_CONFIG_FILE);
    if default_path.is_file()
        && let Ok(default_path) = AbsolutePathBuf::try_from(default_path)
    {
        paths.push(default_path);
    }
    paths.sort_unstable_by(|left, right| left.as_path().cmp(right.as_path()));
    paths.dedup_by(|left, right| left.as_path() == right.as_path());
    paths
}

fn load_mcp_servers_from_file(
    plugin_root: &Path,
    mcp_config_path: &AbsolutePathBuf,
) -> PluginMcpDiscovery {
    let Ok(contents) = fs::read_to_string(mcp_config_path.as_path()) else {
        return PluginMcpDiscovery::default();
    };
    let parsed = match serde_json::from_str::<PluginMcpFile>(&contents) {
        Ok(parsed) => parsed,
        Err(err) => {
            warn!(
                path = %mcp_config_path.display(),
                "failed to parse plugin MCP config: {err}"
            );
            return PluginMcpDiscovery::default();
        }
    };
    normalize_plugin_mcp_servers(
        plugin_root,
        parsed.mcp_servers,
        mcp_config_path.to_string_lossy().as_ref(),
    )
}

fn normalize_plugin_mcp_servers(
    plugin_root: &Path,
    plugin_mcp_servers: HashMap<String, JsonValue>,
    source: &str,
) -> PluginMcpDiscovery {
    let mut mcp_servers = HashMap::new();

    for (name, config_value) in plugin_mcp_servers {
        let normalized = normalize_plugin_mcp_server_value(plugin_root, config_value);
        match serde_json::from_value::<McpServerConfig>(JsonValue::Object(normalized)) {
            Ok(config) => {
                mcp_servers.insert(name, config);
            }
            Err(err) => {
                warn!(
                    plugin = %plugin_root.display(),
                    server = name,
                    "failed to parse plugin MCP server from {source}: {err}"
                );
            }
        }
    }

    PluginMcpDiscovery { mcp_servers }
}

fn normalize_plugin_mcp_server_value(
    plugin_root: &Path,
    value: JsonValue,
) -> JsonMap<String, JsonValue> {
    let mut object = match value {
        JsonValue::Object(object) => object,
        _ => return JsonMap::new(),
    };

    if let Some(JsonValue::String(transport_type)) = object.remove("type") {
        match transport_type.as_str() {
            "http" | "streamable_http" | "streamable-http" => {}
            "stdio" => {}
            other => {
                warn!(
                    plugin = %plugin_root.display(),
                    transport = other,
                    "plugin MCP server uses an unknown transport type"
                );
            }
        }
    }

    if let Some(JsonValue::Object(oauth)) = object.remove("oauth")
        && oauth.contains_key("callbackPort")
    {
        warn!(
            plugin = %plugin_root.display(),
            "plugin MCP server OAuth callbackPort is ignored; Code uses global MCP OAuth callback settings"
        );
    }

    if let Some(JsonValue::String(cwd)) = object.get("cwd")
        && !Path::new(cwd).is_absolute()
    {
        object.insert(
            "cwd".to_string(),
            JsonValue::String(plugin_root.join(cwd).display().to_string()),
        );
    }

    object
}

#[derive(Debug, Default)]
struct PluginMcpDiscovery {
    mcp_servers: HashMap<String, McpServerConfig>,
}

#[cfg(test)]
mod tests {
    use super::configured_plugins_from_code_home;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn configured_plugins_ignores_marketplace_source_settings() {
        let code_home = TempDir::new().expect("code home");
        fs::write(
            code_home.path().join("config.toml"),
            r#"
[plugins]
curated_repo_url = "https://example.com/custom/plugins.git"
curated_repo_ref = "stable"

[[plugins.marketplace_repos]]
url = "https://example.com/extra/marketplace.git"
ref = "main"

[plugins."enabled@openai-curated"]

[plugins."disabled@openai-curated"]
enabled = false
"#,
        )
        .expect("write config");

        let configured = configured_plugins_from_code_home(code_home.path());
        assert_eq!(configured.get("enabled@openai-curated"), Some(&true));
        assert_eq!(configured.get("disabled@openai-curated"), Some(&false));
        assert!(!configured.contains_key("curated_repo_url"));
        assert!(!configured.contains_key("marketplace_repos"));
    }
}
