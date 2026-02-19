use super::diagnostics::config_error_from_toml;
use super::diagnostics::io_error_from_config_error;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;
use toml::Value as TomlValue;

#[cfg(unix)]
const CODE_MANAGED_CONFIG_SYSTEM_PATH: &str = "/etc/code/managed_config.toml";

#[cfg(unix)]
const CODE_REQUIREMENTS_SYSTEM_PATH: &str = "/etc/code/requirements.toml";

#[cfg(unix)]
const CODE_SYSTEM_CONFIG_SYSTEM_PATH: &str = "/etc/code/config.toml";

pub(super) fn managed_config_default_path(code_home: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        let _ = code_home;
        PathBuf::from(CODE_MANAGED_CONFIG_SYSTEM_PATH)
    }

    #[cfg(not(unix))]
    {
        code_home.join("managed_config.toml")
    }
}

pub(super) fn requirements_default_path(code_home: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        let _ = code_home;
        PathBuf::from(CODE_REQUIREMENTS_SYSTEM_PATH)
    }

    #[cfg(not(unix))]
    {
        code_home.join("requirements.toml")
    }
}

pub(super) fn system_config_default_path(code_home: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        let _ = code_home;
        PathBuf::from(CODE_SYSTEM_CONFIG_SYSTEM_PATH)
    }

    #[cfg(not(unix))]
    {
        code_home.join("system_config.toml")
    }
}

pub(super) async fn read_config_from_path(
    path: &Path,
    log_missing_as_info: bool,
) -> io::Result<Option<TomlValue>> {
    match fs::read_to_string(path).await {
        Ok(contents) => match toml::from_str::<TomlValue>(&contents) {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                tracing::error!("Failed to parse {}: {err}", path.display());
                let config_error = config_error_from_toml(path, &contents, err.clone());
                Err(io_error_from_config_error(
                    io::ErrorKind::InvalidData,
                    config_error,
                    Some(err),
                ))
            }
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            if log_missing_as_info {
                tracing::info!("{} not found, using defaults", path.display());
            } else {
                tracing::debug!("{} not found", path.display());
            }
            Ok(None)
        }
        Err(err) => {
            tracing::error!("Failed to read {}: {err}", path.display());
            Err(err)
        }
    }
}

pub(super) async fn load_legacy_managed_config(
    code_home: &Path,
    managed_config_path: Option<PathBuf>,
) -> io::Result<(PathBuf, Option<TomlValue>)> {
    let path = managed_config_path.unwrap_or_else(|| managed_config_default_path(code_home));
    let config = read_config_from_path(&path, false).await?;
    Ok((path, config))
}
