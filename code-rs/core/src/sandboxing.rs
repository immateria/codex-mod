use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use code_protocol::config_types::WindowsSandboxLevel;
use code_protocol::models::SandboxPermissions;
use code_protocol::protocol::NetworkAccess;
use code_protocol::protocol::SandboxPolicy as ProtocolSandboxPolicy;

use crate::error::CodexErr;
use crate::error::Result;
use crate::exec::DEFAULT_EXEC_COMMAND_TIMEOUT_MS;
use crate::exec::ExecExpiration;
use crate::exec::ExecToolCallOutput;
use crate::exec::SandboxType;
use crate::protocol::SandboxPolicy as LocalSandboxPolicy;

#[derive(Debug)]
pub struct ExecRequest {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub network: Option<crate::managed_network_proxy_api::ManagedNetworkProxy>,
    pub expiration: ExecExpiration,
    pub sandbox: SandboxType,
    pub windows_sandbox_level: WindowsSandboxLevel,
    pub sandbox_permissions: SandboxPermissions,
    pub sandbox_policy: ProtocolSandboxPolicy,
    pub justification: Option<String>,
    pub arg0: Option<String>,
}

#[derive(Debug)]
pub struct BuildExecRequestParams {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub network: Option<crate::managed_network_proxy_api::ManagedNetworkProxy>,
    pub expiration: ExecExpiration,
    pub sandbox_permissions: SandboxPermissions,
    pub windows_sandbox_level: WindowsSandboxLevel,
    pub justification: Option<String>,
    pub sandbox_policy: ProtocolSandboxPolicy,
    pub sandbox_policy_cwd: PathBuf,
    pub code_linux_sandbox_exe: Option<PathBuf>,
}

pub fn protocol_policy_from_local(policy: &LocalSandboxPolicy) -> ProtocolSandboxPolicy {
    match policy {
        LocalSandboxPolicy::DangerFullAccess => ProtocolSandboxPolicy::DangerFullAccess,
        LocalSandboxPolicy::ReadOnly => ProtocolSandboxPolicy::ReadOnly,
        LocalSandboxPolicy::WorkspaceWrite {
            writable_roots,
            network_access,
            exclude_tmpdir_env_var,
            exclude_slash_tmp,
            allow_git_writes,
        } => ProtocolSandboxPolicy::WorkspaceWrite {
            writable_roots: writable_roots.clone().into_iter().filter_map(|p| p.try_into().ok()).collect(),
            network_access: *network_access,
            exclude_tmpdir_env_var: *exclude_tmpdir_env_var,
            exclude_slash_tmp: *exclude_slash_tmp,
            allow_git_writes: *allow_git_writes,
        },
    }
}

pub fn local_policy_from_protocol(policy: &ProtocolSandboxPolicy) -> LocalSandboxPolicy {
    match policy {
        ProtocolSandboxPolicy::DangerFullAccess => LocalSandboxPolicy::DangerFullAccess,
        ProtocolSandboxPolicy::ReadOnly => LocalSandboxPolicy::ReadOnly,
        ProtocolSandboxPolicy::ExternalSandbox { network_access } => match network_access {
            NetworkAccess::Enabled => LocalSandboxPolicy::DangerFullAccess,
            NetworkAccess::Restricted => LocalSandboxPolicy::ReadOnly,
        },
        ProtocolSandboxPolicy::WorkspaceWrite {
            writable_roots,
            network_access,
            exclude_tmpdir_env_var,
            exclude_slash_tmp,
            allow_git_writes,
        } => LocalSandboxPolicy::WorkspaceWrite {
            writable_roots: writable_roots
                .iter()
                .map(|path| path.as_ref().to_path_buf())
                .collect(),
            network_access: *network_access,
            exclude_tmpdir_env_var: *exclude_tmpdir_env_var,
            exclude_slash_tmp: *exclude_slash_tmp,
            allow_git_writes: *allow_git_writes,
        },
    }
}

fn should_require_platform_sandbox(
    policy: &ProtocolSandboxPolicy,
    has_managed_network_requirements: bool,
) -> bool {
    if has_managed_network_requirements {
        return !matches!(policy, ProtocolSandboxPolicy::ExternalSandbox { .. });
    }

    if !policy.has_full_network_access() {
        return !matches!(policy, ProtocolSandboxPolicy::ExternalSandbox { .. });
    }

    !policy.has_full_disk_write_access()
}

fn select_process_exec_tool_sandbox_type(
    policy: &ProtocolSandboxPolicy,
    windows_sandbox_level: WindowsSandboxLevel,
    has_managed_network_requirements: bool,
) -> SandboxType {
    if !should_require_platform_sandbox(policy, has_managed_network_requirements) {
        return SandboxType::None;
    }

    if cfg!(target_os = "windows") {
        return if windows_sandbox_level == WindowsSandboxLevel::Disabled {
            SandboxType::None
        } else {
            SandboxType::WindowsRestrictedToken
        };
    }

    if cfg!(target_os = "macos") {
        return SandboxType::MacosSeatbelt;
    }

    if cfg!(target_os = "linux") {
        return SandboxType::LinuxSeccomp;
    }

    SandboxType::None
}

pub fn build_exec_request(params: BuildExecRequestParams) -> Result<ExecRequest> {
    let BuildExecRequestParams {
        command,
        cwd,
        mut env,
        network,
        expiration,
        sandbox_permissions,
        windows_sandbox_level,
        justification,
        sandbox_policy,
        sandbox_policy_cwd,
        code_linux_sandbox_exe,
    } = params;

    if let Some(network) = network.as_ref() {
        network.apply_to_env(&mut env);
    }

    if command.is_empty() {
        return Err(CodexErr::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "command args are empty",
        )));
    }

    let sandbox = select_process_exec_tool_sandbox_type(
        &sandbox_policy,
        windows_sandbox_level,
        network.is_some(),
    );
    let local_policy = local_policy_from_protocol(&sandbox_policy);

    let (command, arg0) = match sandbox {
        SandboxType::None => (command, None),
        SandboxType::MacosSeatbelt => (
            vec![crate::seatbelt::seatbelt_exec_path().to_string()]
                .into_iter()
                .chain(crate::seatbelt::build_seatbelt_args(
                    command,
                    &local_policy,
                    sandbox_policy_cwd.as_path(),
                    network.is_some(),
                    &env,
                ))
                .collect(),
            None,
        ),
        SandboxType::LinuxSeccomp => {
            let code_linux_sandbox_exe = code_linux_sandbox_exe.ok_or_else(|| {
                CodexErr::Io(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "codex-linux-sandbox executable not configured",
                ))
            })?;
            let sandbox_policy_json = serde_json::to_string(&local_policy).map_err(|err| {
                CodexErr::Io(io::Error::other(format!(
                    "failed to serialize sandbox policy: {err}"
                )))
            })?;
            let sandbox_cwd = sandbox_policy_cwd.display().to_string();
            let mut wrapped = vec![
                code_linux_sandbox_exe.display().to_string(),
                sandbox_cwd,
                sandbox_policy_json,
                "--".to_string(),
            ];
            wrapped.extend(command);
            (wrapped, Some("codex-linux-sandbox".to_string()))
        }
        SandboxType::WindowsRestrictedToken => (command, None),
    };

    Ok(ExecRequest {
        command,
        cwd,
        env,
        network,
        expiration,
        sandbox,
        windows_sandbox_level,
        sandbox_permissions,
        sandbox_policy,
        justification,
        arg0,
    })
}

pub async fn execute_env(
    exec_request: ExecRequest,
    _stdout_stream: Option<crate::exec::StdoutStream>,
) -> Result<ExecToolCallOutput> {
    let ExecRequest {
        command,
        cwd,
        mut env,
        network,
        expiration,
        sandbox,
        windows_sandbox_level,
        sandbox_permissions: _sandbox_permissions,
        sandbox_policy,
        justification: _justification,
        arg0: _arg0,
    } = exec_request;

    #[cfg(not(target_os = "windows"))]
    let _ = windows_sandbox_level;

    if sandbox != SandboxType::WindowsRestrictedToken {
        return Err(CodexErr::UnsupportedOperation(
            "sandboxing::execute_env only supports windows restricted-token exec in this path"
                .to_string(),
        ));
    }

    if let Some(network) = network.as_ref() {
        network.apply_to_env(&mut env);
    }

    let timeout_ms = match expiration {
        ExecExpiration::DefaultTimeout => Some(DEFAULT_EXEC_COMMAND_TIMEOUT_MS),
        ExecExpiration::Timeout(duration) => Some(duration.as_millis().min(u64::MAX as u128) as u64),
        ExecExpiration::Cancellation(_) => None,
    };

    let policy_str = serde_json::to_string(&sandbox_policy).map_err(|err| {
        CodexErr::Io(io::Error::other(format!(
            "failed to serialize Windows sandbox policy: {err}"
        )))
    })?;
    let sandbox_cwd = cwd.clone();
    let codex_home = crate::config::find_code_home().map_err(|err| {
        CodexErr::Io(io::Error::other(format!(
            "windows sandbox: failed to resolve code_home: {err}"
        )))
    })?;

    #[cfg(target_os = "windows")]
    let capture = tokio::task::spawn_blocking(move || {
        if matches!(windows_sandbox_level, WindowsSandboxLevel::Elevated) {
            code_windows_sandbox::run_windows_sandbox_capture_elevated(
                policy_str.as_str(),
                sandbox_cwd.as_path(),
                codex_home.as_path(),
                command,
                cwd.as_path(),
                env,
                timeout_ms,
            )
        } else {
            code_windows_sandbox::run_windows_sandbox_capture(
                policy_str.as_str(),
                sandbox_cwd.as_path(),
                codex_home.as_path(),
                command,
                cwd.as_path(),
                env,
                timeout_ms,
            )
        }
    })
    .await
    .map_err(|err| CodexErr::Io(io::Error::other(format!(
        "windows sandbox task failed: {err}"
    ))))?
    .map_err(|err| CodexErr::Io(io::Error::other(format!("windows sandbox: {err}"))))?;

    #[cfg(not(target_os = "windows"))]
    let capture = tokio::task::spawn_blocking(move || {
        code_windows_sandbox::run_windows_sandbox_capture(
            policy_str.as_str(),
            sandbox_cwd.as_path(),
            codex_home.as_path(),
            command,
            cwd.as_path(),
            env,
            timeout_ms,
        )
    })
    .await
    .map_err(|err| CodexErr::Io(io::Error::other(format!(
        "windows sandbox task failed: {err}"
    ))))?
    .map_err(|err| CodexErr::Io(io::Error::other(format!("windows sandbox: {err}"))))?;

    Ok(ExecToolCallOutput {
        exit_code: capture.exit_code,
        stdout: crate::exec::StreamOutput::new(crate::bytes_to_string_smart(&capture.stdout)),
        stderr: crate::exec::StreamOutput::new(crate::bytes_to_string_smart(&capture.stderr)),
        aggregated_output: crate::exec::StreamOutput::new(String::new()),
        duration: Duration::default(),
        timed_out: capture.timed_out,
    })
}
