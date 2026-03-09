use std::path::Path;
use std::path::PathBuf;

use anyhow::bail;

use crate::config::Config;
use crate::config::ConfigToml;
use crate::config::set_windows_sandbox_mode;
use crate::config_profile::ConfigProfile;
use crate::config::FeaturesToml;
use crate::config_types::WindowsSandboxModeToml;
use crate::protocol::SandboxPolicy;
#[cfg(target_os = "windows")]
use crate::sandboxing::protocol_policy_from_local;
use code_protocol::config_types::WindowsSandboxLevel;

pub trait WindowsSandboxLevelExt {
    fn from_config(config: &Config) -> WindowsSandboxLevel;
}

impl WindowsSandboxLevelExt for WindowsSandboxLevel {
    fn from_config(config: &Config) -> WindowsSandboxLevel {
        config.windows_sandbox_level
    }
}

pub fn windows_sandbox_level_from_config(config: &Config) -> WindowsSandboxLevel {
    WindowsSandboxLevel::from_config(config)
}

pub fn resolve_windows_sandbox_mode(
    cfg: &ConfigToml,
    profile: &ConfigProfile,
) -> Option<WindowsSandboxModeToml> {
    profile
        .windows
        .as_ref()
        .and_then(|windows| windows.sandbox)
        .or_else(|| cfg.windows.as_ref().and_then(|windows| windows.sandbox))
        .or_else(|| legacy_windows_sandbox_mode(cfg.features.as_ref()))
}

pub fn legacy_windows_sandbox_mode(
    features: Option<&FeaturesToml>,
) -> Option<WindowsSandboxModeToml> {
    let features = features?;
    if features.elevated_windows_sandbox.unwrap_or(false) {
        return Some(WindowsSandboxModeToml::Elevated);
    }
    if features.experimental_windows_sandbox.unwrap_or(false)
        || features.enable_experimental_windows_sandbox.unwrap_or(false)
    {
        return Some(WindowsSandboxModeToml::Unelevated);
    }
    None
}

#[cfg(target_os = "windows")]
pub fn sandbox_setup_is_complete(codex_home: &Path) -> bool {
    code_windows_sandbox::sandbox_setup_is_complete(codex_home)
}

#[cfg(not(target_os = "windows"))]
pub fn sandbox_setup_is_complete(_codex_home: &Path) -> bool {
    false
}

#[cfg(target_os = "windows")]
pub fn run_elevated_setup(
    policy: &SandboxPolicy,
    policy_cwd: &Path,
    command_cwd: &Path,
    env_map: &std::collections::HashMap<String, String>,
    codex_home: &Path,
) -> anyhow::Result<()> {
    let policy = protocol_policy_from_local(policy);
    code_windows_sandbox::run_elevated_setup(
        &policy,
        policy_cwd,
        command_cwd,
        env_map,
        codex_home,
        None,
        None,
    )
}

#[cfg(not(target_os = "windows"))]
pub fn run_elevated_setup(
    _policy: &SandboxPolicy,
    _policy_cwd: &Path,
    _command_cwd: &Path,
    _env_map: &std::collections::HashMap<String, String>,
    _codex_home: &Path,
) -> anyhow::Result<()> {
    bail!("elevated Windows sandbox setup is only supported on Windows")
}

#[cfg(target_os = "windows")]
pub fn run_legacy_setup_preflight(
    policy: &SandboxPolicy,
    policy_cwd: &Path,
    command_cwd: &Path,
    env_map: &std::collections::HashMap<String, String>,
    codex_home: &Path,
) -> anyhow::Result<()> {
    let policy = protocol_policy_from_local(policy);
    code_windows_sandbox::run_windows_sandbox_legacy_preflight(
        &policy,
        policy_cwd,
        codex_home,
        command_cwd,
        env_map.clone(),
    )
}

#[cfg(not(target_os = "windows"))]
pub fn run_legacy_setup_preflight(
    _policy: &SandboxPolicy,
    _policy_cwd: &Path,
    _command_cwd: &Path,
    _env_map: &std::collections::HashMap<String, String>,
    _codex_home: &Path,
) -> anyhow::Result<()> {
    bail!("legacy Windows sandbox setup is only supported on Windows")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsSandboxSetupMode {
    Elevated,
    Unelevated,
}

#[derive(Debug, Clone)]
pub struct WindowsSandboxSetupRequest {
    pub mode: WindowsSandboxSetupMode,
    pub policy: SandboxPolicy,
    pub policy_cwd: PathBuf,
    pub command_cwd: PathBuf,
    pub env_map: std::collections::HashMap<String, String>,
    pub codex_home: PathBuf,
    pub active_profile: Option<String>,
}

pub async fn run_windows_sandbox_setup(request: WindowsSandboxSetupRequest) -> anyhow::Result<()> {
    let mode = request.mode;
    let policy = request.policy;
    let policy_cwd = request.policy_cwd;
    let command_cwd = request.command_cwd;
    let env_map = request.env_map;
    let codex_home = request.codex_home;
    let active_profile = request.active_profile;
    let setup_codex_home = codex_home.clone();

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        match mode {
            WindowsSandboxSetupMode::Elevated => {
                if !sandbox_setup_is_complete(setup_codex_home.as_path()) {
                    run_elevated_setup(
                        &policy,
                        policy_cwd.as_path(),
                        command_cwd.as_path(),
                        &env_map,
                        setup_codex_home.as_path(),
                    )?;
                }
            }
            WindowsSandboxSetupMode::Unelevated => {
                run_legacy_setup_preflight(
                    &policy,
                    policy_cwd.as_path(),
                    command_cwd.as_path(),
                    &env_map,
                    setup_codex_home.as_path(),
                )?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|join_err| anyhow::anyhow!("windows sandbox setup task failed: {join_err}"))??;

    let mode = match mode {
        WindowsSandboxSetupMode::Elevated => WindowsSandboxModeToml::Elevated,
        WindowsSandboxSetupMode::Unelevated => WindowsSandboxModeToml::Unelevated,
    };

    set_windows_sandbox_mode(&codex_home, active_profile.as_deref(), Some(mode))
        .map_err(|err| anyhow::anyhow!("failed to persist windows sandbox mode: {err}"))
}
