use super::PluginManifestPaths;
use super::load_plugin_manifest;
use super::marketplace::MarketplaceError;
use super::marketplace::MarketplacePluginPolicy;
use super::marketplace::MarketplacePluginSource;
use super::marketplace::ResolvedMarketplacePlugin;
use super::marketplace::list_marketplaces;
use super::marketplace::load_marketplace;
use super::marketplace::resolve_marketplace_plugin;
use super::store::PluginId;
use super::store::PluginIdError;
use super::store::PluginInstallResult as StorePluginInstallResult;
use super::store::PluginStore;
use super::store::PluginStoreError;
use crate::config::CONFIG_TOML_FILE;
use crate::config::resolve_code_path_for_read;
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
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tracing::warn;

const DEFAULT_SKILLS_DIR_NAME: &str = "skills";
const DEFAULT_MCP_CONFIG_FILE: &str = ".mcp.json";
const DEFAULT_APP_CONFIG_FILE: &str = ".app.json";

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

#[derive(Clone)]
pub struct PluginsManager {
    code_home: PathBuf,
    store: PluginStore,
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
        config_cwds: &[AbsolutePathBuf],
    ) -> Result<Vec<ConfiguredMarketplace>, MarketplaceError> {
        let configured_plugins = configured_plugins_from_code_home(&self.code_home);

        let mut marketplaces = Vec::new();
        for marketplace in list_marketplaces(config_cwds)? {
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
        Ok(marketplaces)
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

    async fn install_resolved_plugin(
        &self,
        resolved: ResolvedMarketplacePlugin,
    ) -> Result<PluginInstallOutcome, PluginInstallError> {
        let auth_policy = resolved.auth_policy;
        let store = self.store.clone();
        let result: StorePluginInstallResult = tokio::task::spawn_blocking(move || {
            store.install(resolved.source_path, resolved.plugin_id)
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
}

#[derive(Debug, thiserror::Error)]
pub enum PluginInstallError {
    #[error("{0}")]
    Marketplace(#[from] MarketplaceError),

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
        matches!(self, Self::Marketplace(MarketplaceError::PluginNotFound { .. } | MarketplaceError::PluginNotAvailable { .. } | MarketplaceError::InvalidPlugin(_)))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginUninstallError {
    #[error("{0}")]
    InvalidPluginId(#[from] PluginIdError),

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

#[derive(Debug, Default, Deserialize)]
struct PluginsConfigToml {
    #[serde(default)]
    plugins: HashMap<String, PluginConfigToml>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginConfigToml {
    #[serde(default = "default_enabled")]
    enabled: bool,
}

const fn default_enabled() -> bool {
    true
}

fn configured_plugins_from_code_home(code_home: &Path) -> HashMap<String, bool> {
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let contents = match fs::read_to_string(&read_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return HashMap::new(),
        Err(err) => {
            warn!("failed to read config.toml for plugin config: {err}");
            return HashMap::new();
        }
    };

    let parsed: PluginsConfigToml = match toml::from_str(&contents) {
        Ok(parsed) => parsed,
        Err(err) => {
            warn!("failed to parse config.toml plugin config: {err}");
            return HashMap::new();
        }
    };

    parsed.plugins.into_iter().map(|(key, cfg)| (key, cfg.enabled)).collect()
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
