use crate::config::Config;
use crate::config::ConfigBuilder;
use crate::config::ConfigOverrides;
use crate::config_loader::LoaderOverrides;
use crate::protocol::AskForApproval as CoreAskForApproval;
use code_app_server_protocol::AskForApproval as V2AskForApproval;
use code_app_server_protocol::Config as V2Config;
use code_app_server_protocol::ConfigBatchWriteParams;
use code_app_server_protocol::ConfigEdit;
use code_app_server_protocol::ConfigLayer;
use code_app_server_protocol::ConfigReadParams;
use code_app_server_protocol::ConfigReadResponse;
use code_app_server_protocol::ConfigRequirements;
use code_app_server_protocol::ConfigRequirementsReadResponse;
use code_app_server_protocol::ConfigValueWriteParams;
use code_app_server_protocol::ConfigWriteErrorCode;
use code_app_server_protocol::ConfigWriteResponse;
use code_app_server_protocol::MergeStrategy;
use code_app_server_protocol::OverriddenMetadata;
use code_app_server_protocol::ToolsV2;
use code_app_server_protocol::WriteStatus;
use code_protocol::config_types::Verbosity;
use code_protocol::config_types::WebSearchMode;
use code_utils_absolute_path::AbsolutePathBuf;
use code_utils_json_to_toml::json_to_toml;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use thiserror::Error;
use toml::Value as TomlValue;

#[derive(Debug, Error)]
pub enum ConfigServiceError {
    #[error("{message}")]
    Write {
        code: ConfigWriteErrorCode,
        message: String,
    },

    #[error("{context}: {source}")]
    Io {
        context: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("{context}: {source}")]
    Json {
        context: &'static str,
        #[source]
        source: serde_json::Error,
    },

    #[error("{context}: {source}")]
    Toml {
        context: &'static str,
        #[source]
        source: toml::de::Error,
    },

    #[error("{context}: {source}")]
    Anyhow {
        context: &'static str,
        #[source]
        source: anyhow::Error,
    },
}

impl ConfigServiceError {
    pub fn write_error_code(&self) -> Option<ConfigWriteErrorCode> {
        match self {
            Self::Write { code, .. } => Some(code.clone()),
            _ => None,
        }
    }

    fn write(code: ConfigWriteErrorCode, message: impl Into<String>) -> Self {
        Self::Write {
            code,
            message: message.into(),
        }
    }

    fn io(context: &'static str, source: std::io::Error) -> Self {
        Self::Io { context, source }
    }

    fn json(context: &'static str, source: serde_json::Error) -> Self {
        Self::Json { context, source }
    }

    // Prefer returning `ConfigServiceError::Write` for user-facing validation failures.
}

#[derive(Clone)]
pub struct ConfigService {
    code_home: PathBuf,
    default_cwd: PathBuf,
    code_linux_sandbox_exe: Option<PathBuf>,
    cli_overrides: Vec<(String, TomlValue)>,
    loader_overrides: LoaderOverrides,
}

impl ConfigService {
    pub fn new(
        code_home: PathBuf,
        default_cwd: PathBuf,
        code_linux_sandbox_exe: Option<PathBuf>,
        cli_overrides: Vec<(String, TomlValue)>,
        loader_overrides: LoaderOverrides,
    ) -> Self {
        Self {
            code_home,
            default_cwd,
            code_linux_sandbox_exe,
            cli_overrides,
            loader_overrides,
        }
    }

    pub fn new_with_defaults(code_home: PathBuf, default_cwd: PathBuf) -> Self {
        Self::new(
            code_home,
            default_cwd,
            None,
            Vec::new(),
            LoaderOverrides::default(),
        )
    }

    pub fn read(&self, params: ConfigReadParams) -> Result<ConfigReadResponse, ConfigServiceError> {
        let layers_cwd = params
            .cwd
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.default_cwd.clone());

        let config = self
            .load_effective_config(Some(layers_cwd.clone()))
            .map_err(|err| ConfigServiceError::io("failed to load effective config", err))?;

        let layers_state = crate::config_loader::load_config_layers_state_blocking_with_cwd(
            &self.code_home,
            Some(layers_cwd.as_path()),
            &self.cli_overrides,
            self.loader_overrides.clone(),
        )
        .map_err(|err| ConfigServiceError::io("failed to read configuration layers", err))?;

        let origins = layers_state.origins();
        let layers = if params.include_layers {
            let mut layers = Vec::<ConfigLayer>::new();
            for layer in layers_state.layers_high_to_low() {
                let config_json = serde_json::to_value(&layer.config)
                    .map_err(|err| ConfigServiceError::json("failed to serialize config layer", err))?;
                layers.push(ConfigLayer {
                    name: layer.name.clone(),
                    version: layer.version.clone(),
                    config: config_json,
                    disabled_reason: layer.disabled_reason.clone(),
                });
            }
            Some(layers)
        } else {
            None
        };

        Ok(ConfigReadResponse {
            config: v2_config_snapshot_from(&config),
            origins,
            layers,
        })
    }

    pub fn read_requirements(&self) -> Result<ConfigRequirementsReadResponse, ConfigServiceError> {
        let requirements = crate::config::load_allowed_approval_policies(&self.code_home)
            .map_err(|err| ConfigServiceError::io("failed to read config requirements", err))?;

        let requirements = match requirements {
            Some(allowed_approval_policies) => Some(ConfigRequirements {
                allowed_approval_policies: Some(
                    allowed_approval_policies
                        .into_iter()
                        .map(map_approval_policy_to_v2)
                        .collect(),
                ),
                allowed_sandbox_modes: None,
                allowed_web_search_modes: None,
                enforce_residency: None,
                network: None,
            }),
            None => None,
        };

        Ok(ConfigRequirementsReadResponse { requirements })
    }

    pub fn write_value(
        &self,
        params: ConfigValueWriteParams,
    ) -> Result<ConfigWriteResponse, ConfigServiceError> {
        let ConfigValueWriteParams {
            key_path,
            value,
            merge_strategy,
            file_path,
            expected_version,
        } = params;

        self.apply_config_edits(
            vec![ConfigEdit {
                key_path,
                value,
                merge_strategy,
            }],
            file_path,
            expected_version,
        )
    }

    pub fn batch_write(
        &self,
        params: ConfigBatchWriteParams,
    ) -> Result<ConfigWriteResponse, ConfigServiceError> {
        self.apply_config_edits(params.edits, params.file_path, params.expected_version)
    }

    fn load_effective_config(&self, cwd: Option<PathBuf>) -> std::io::Result<Config> {
        let overrides = ConfigOverrides {
            code_linux_sandbox_exe: self.code_linux_sandbox_exe.clone(),
            cwd,
            ..Default::default()
        };

        ConfigBuilder::new()
            .with_code_home(self.code_home.clone())
            .with_cli_overrides(self.cli_overrides.clone())
            .with_overrides(overrides)
            .with_loader_overrides(self.loader_overrides.clone())
            .load()
    }

    fn apply_config_edits(
        &self,
        edits: Vec<ConfigEdit>,
        file_path: Option<String>,
        expected_version: Option<String>,
    ) -> Result<ConfigWriteResponse, ConfigServiceError> {
        let allowed_file_path = self.code_home.join(crate::config::CONFIG_TOML_FILE);
        let file_path = resolve_config_file_path(file_path, &allowed_file_path)?;

        let current_contents = match std::fs::read_to_string(&file_path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return Err(ConfigServiceError::write(
                    ConfigWriteErrorCode::ConfigValidationError,
                    format!("Unable to read config file: {err}"),
                ));
            }
        };

        let mut root = if current_contents.trim().is_empty() {
            TomlValue::Table(Default::default())
        } else {
            current_contents.parse::<TomlValue>().map_err(|err| {
                ConfigServiceError::write(
                    ConfigWriteErrorCode::ConfigValidationError,
                    format!("Invalid TOML in config file: {err}"),
                )
            })?
        };

        let current_version = crate::config_loader::version_for_toml(&root);
        if let Some(expected_version) = expected_version
            && expected_version != current_version
        {
            return Err(ConfigServiceError::write(
                ConfigWriteErrorCode::ConfigVersionConflict,
                "Config version conflict",
            ));
        }

        let mut edited_paths = Vec::<String>::new();
        for edit in edits {
            edited_paths.push(edit.key_path.clone());
            apply_toml_edit(
                &mut root,
                edit.key_path.as_str(),
                json_to_toml(edit.value),
                edit.merge_strategy,
            )?;
        }

        let serialized = toml::to_string_pretty(&root).map_err(|err| {
            ConfigServiceError::write(
                ConfigWriteErrorCode::ConfigValidationError,
                format!("Unable to serialize config TOML: {err}"),
            )
        })?;

        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                ConfigServiceError::write(
                    ConfigWriteErrorCode::UserLayerNotFound,
                    format!("Unable to create config directory: {err}"),
                )
            })?;
        }

        write_atomically(&file_path, serialized.as_bytes())
            .map_err(|err| ConfigServiceError::io("failed to write config file", err))?;

        let absolute_file_path = AbsolutePathBuf::from_absolute_path(&file_path)
            .map_err(|err| ConfigServiceError::io("failed to resolve config file path", err))?;

        let mut status = WriteStatus::Ok;
        let mut overridden_metadata: Option<OverriddenMetadata> = None;

        // Best-effort: if the value the user wrote is overridden by a higher-precedence
        // config layer (session flags, managed config), report that in the response so
        // callers can explain why the write didn't take effect.
        if !edited_paths.is_empty()
            && let Ok(state) = crate::config_loader::load_config_layers_state_blocking_with_cwd(
                &self.code_home,
                Some(self.default_cwd.as_path()),
                &self.cli_overrides,
                self.loader_overrides.clone(),
            )
        {
            let effective = state.effective_config();
            let origins = state.origins();

            for key_path in edited_paths {
                let user_value = toml_value_at_path(&root, &key_path);
                let effective_value = toml_value_at_path(&effective, &key_path);
                if user_value == effective_value {
                    continue;
                }

                let Some(origin) = origins.get(&key_path) else {
                    continue;
                };

                if matches!(&origin.name, code_app_server_protocol::ConfigLayerSource::User { .. }) {
                    continue;
                }

                let overriding_layer = format_layer_source_for_override(&origin.name);
                status = WriteStatus::OkOverridden;
                overridden_metadata = Some(OverriddenMetadata {
                    message: format!("Value is overridden by {overriding_layer}."),
                    overriding_layer: origin.clone(),
                    effective_value: match effective_value {
                        Some(value) => serde_json::to_value(value)
                            .unwrap_or_else(|_| serde_json::Value::Null),
                        None => serde_json::Value::Null,
                    },
                });
                break;
            }
        }

        Ok(ConfigWriteResponse {
            status,
            version: crate::config_loader::version_for_toml(&root),
            file_path: absolute_file_path,
            overridden_metadata,
        })
    }
}

fn resolve_config_file_path(
    file_path: Option<String>,
    allowed_file_path: &Path,
) -> Result<PathBuf, ConfigServiceError> {
    let path = match file_path {
        Some(path) => {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err(ConfigServiceError::write(
                    ConfigWriteErrorCode::ConfigValidationError,
                    "filePath must be an absolute path",
                ));
            }
            if !paths_match(allowed_file_path, &path) {
                return Err(ConfigServiceError::write(
                    ConfigWriteErrorCode::ConfigLayerReadonly,
                    "Only writes to the user config are allowed",
                ));
            }
            path
        }
        None => allowed_file_path.to_path_buf(),
    };

    Ok(path)
}

fn paths_match(expected: &Path, provided: &Path) -> bool {
    let expected = expected
        .canonicalize()
        .unwrap_or_else(|_| expected.to_path_buf());
    let provided = provided
        .canonicalize()
        .unwrap_or_else(|_| provided.to_path_buf());
    expected == provided
}

fn map_approval_policy_to_v2(policy: CoreAskForApproval) -> V2AskForApproval {
    match policy {
        CoreAskForApproval::UnlessTrusted => V2AskForApproval::UnlessTrusted,
        CoreAskForApproval::OnFailure => V2AskForApproval::OnFailure,
        CoreAskForApproval::OnRequest => V2AskForApproval::OnRequest,
        CoreAskForApproval::Never => V2AskForApproval::Never,
    }
}

fn v2_config_snapshot_from(config: &Config) -> V2Config {
    V2Config {
        model: Some(config.model.clone()),
        review_model: Some(config.review_model.clone()),
        model_context_window: config
            .model_context_window
            .and_then(|value| i64::try_from(value).ok()),
        model_auto_compact_token_limit: config.model_auto_compact_token_limit,
        model_provider: Some(config.model_provider_id.clone()),
        approval_policy: Some(map_approval_policy_to_v2(config.approval_policy)),
        sandbox_mode: None,
        sandbox_workspace_write: None,
        forced_chatgpt_workspace_id: None,
        forced_login_method: None,
        web_search: Some(if config.tools_web_search_request {
            WebSearchMode::Cached
        } else {
            WebSearchMode::Disabled
        }),
        tools: Some(ToolsV2 {
            web_search: Some(config.tools_web_search_request),
            view_image: Some(config.include_view_image_tool),
        }),
        profile: config.active_profile.clone(),
        profiles: HashMap::new(),
        instructions: config.base_instructions.clone(),
        developer_instructions: None,
        compact_prompt: config.compact_prompt_override.clone(),
        model_reasoning_effort: None,
        model_reasoning_summary: None,
        model_verbosity: Some(match config.model_text_verbosity {
            crate::config_types::TextVerbosity::Low => Verbosity::Low,
            crate::config_types::TextVerbosity::Medium => Verbosity::Medium,
            crate::config_types::TextVerbosity::High => Verbosity::High,
        }),
        analytics: None,
        apps: None,
        additional: HashMap::new(),
    }
}

fn apply_toml_edit(
    root: &mut TomlValue,
    key_path: &str,
    value: TomlValue,
    merge_strategy: MergeStrategy,
) -> Result<(), ConfigServiceError> {
    match merge_strategy {
        MergeStrategy::Replace => set_toml_path(root, key_path, value),
        MergeStrategy::Upsert => upsert_toml_path(root, key_path, value),
    }
}

fn set_toml_path(
    root: &mut TomlValue,
    key_path: &str,
    value: TomlValue,
) -> Result<(), ConfigServiceError> {
    let segments: Vec<&str> = key_path.split('.').filter(|segment| !segment.is_empty()).collect();
    if segments.is_empty() {
        return Err(ConfigServiceError::write(
            ConfigWriteErrorCode::ConfigPathNotFound,
            "Config key path must not be empty",
        ));
    }

    let mut current = root;
    for segment in &segments[..segments.len() - 1] {
        if !current.is_table() {
            *current = TomlValue::Table(Default::default());
        }
        let Some(table) = current.as_table_mut() else {
            return Err(ConfigServiceError::write(
                ConfigWriteErrorCode::ConfigValidationError,
                format!("Failed to apply config edit: expected table for '{key_path}'"),
            ));
        };
        current = table
            .entry((*segment).to_string())
            .or_insert_with(|| TomlValue::Table(Default::default()));
    }

    if !current.is_table() {
        *current = TomlValue::Table(Default::default());
    }
    let Some(table) = current.as_table_mut() else {
        return Err(ConfigServiceError::write(
            ConfigWriteErrorCode::ConfigValidationError,
            format!("Failed to apply config edit: expected table for '{key_path}'"),
        ));
    };
    let Some(key) = segments.last() else {
        return Err(ConfigServiceError::write(
            ConfigWriteErrorCode::ConfigPathNotFound,
            "Config key path must not be empty",
        ));
    };
    table.insert((*key).to_string(), value);

    Ok(())
}

fn upsert_toml_path(
    root: &mut TomlValue,
    key_path: &str,
    value: TomlValue,
) -> Result<(), ConfigServiceError> {
    let segments: Vec<&str> = key_path.split('.').filter(|segment| !segment.is_empty()).collect();
    if segments.is_empty() {
        return Err(ConfigServiceError::write(
            ConfigWriteErrorCode::ConfigPathNotFound,
            "Config key path must not be empty",
        ));
    }

    let mut current = root;
    for segment in &segments[..segments.len() - 1] {
        if !current.is_table() {
            *current = TomlValue::Table(Default::default());
        }
        let Some(table) = current.as_table_mut() else {
            return Err(ConfigServiceError::write(
                ConfigWriteErrorCode::ConfigValidationError,
                format!("Failed to apply config edit: expected table for '{key_path}'"),
            ));
        };
        current = table
            .entry((*segment).to_string())
            .or_insert_with(|| TomlValue::Table(Default::default()));
    }

    if !current.is_table() {
        *current = TomlValue::Table(Default::default());
    }

    let Some(table) = current.as_table_mut() else {
        return Err(ConfigServiceError::write(
            ConfigWriteErrorCode::ConfigValidationError,
            format!("Failed to apply config edit: expected table for '{key_path}'"),
        ));
    };
    let Some(key) = segments.last() else {
        return Err(ConfigServiceError::write(
            ConfigWriteErrorCode::ConfigPathNotFound,
            "Config key path must not be empty",
        ));
    };
    let key = (*key).to_string();
    if let Some(existing) = table.get_mut(&key) {
        merge_toml_values(existing, value);
    } else {
        table.insert(key, value);
    }
    Ok(())
}

fn merge_toml_values(target: &mut TomlValue, incoming: TomlValue) {
    match (target, incoming) {
        (TomlValue::Table(target_table), TomlValue::Table(incoming_table)) => {
            for (key, incoming_value) in incoming_table {
                if let Some(existing) = target_table.get_mut(&key) {
                    merge_toml_values(existing, incoming_value);
                } else {
                    target_table.insert(key, incoming_value);
                }
            }
        }
        (target_value, incoming_value) => {
            *target_value = incoming_value;
        }
    }
}

fn toml_value_at_path<'a>(root: &'a TomlValue, key_path: &str) -> Option<&'a TomlValue> {
    let mut current = root;
    for segment in key_path.split('.').filter(|segment| !segment.is_empty()) {
        let table = current.as_table()?;
        current = table.get(segment)?;
    }
    Some(current)
}

fn format_layer_source_for_override(source: &code_app_server_protocol::ConfigLayerSource) -> String {
    match source {
        code_app_server_protocol::ConfigLayerSource::Mdm { domain, key } => {
            format!("MDM ({domain}:{key})")
        }
        code_app_server_protocol::ConfigLayerSource::System { file } => {
            let path = file.as_ref().display();
            format!("system config ({path})")
        }
        code_app_server_protocol::ConfigLayerSource::User { file } => {
            let path = file.as_ref().display();
            format!("user config ({path})")
        }
        code_app_server_protocol::ConfigLayerSource::Project { dot_codex_folder } => {
            let path = dot_codex_folder.as_ref().display();
            format!("project config ({path})")
        }
        code_app_server_protocol::ConfigLayerSource::SessionFlags => "session overrides".to_string(),
        code_app_server_protocol::ConfigLayerSource::LegacyManagedConfigTomlFromFile { file } => {
            let path = file.as_ref().display();
            format!("managed config ({path})")
        }
        code_app_server_protocol::ConfigLayerSource::LegacyManagedConfigTomlFromMdm => {
            "MDM managed config".to_string()
        }
    }
}

fn write_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no parent directory",
        ));
    };
    std::fs::create_dir_all(parent)?;
    let tmp = NamedTempFile::new_in(parent)?;
    std::fs::write(tmp.path(), contents)?;
    tmp.persist(path)?;
    Ok(())
}
