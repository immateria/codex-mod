use super::types::{ReplRuntimeConfig, ResolvedRuntime};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Minimum Node.js version required for `--experimental-vm-modules`.
const MIN_NODE_VERSION: (u64, u64, u64) = (18, 0, 0);

/// Resolve a user-facing `ReplRuntimeConfig` into a `ResolvedRuntime` by
/// probing the binary for its version and injecting any required default
/// flags (e.g. `--experimental-vm-modules` for Node).
pub(super) async fn resolve_runtime(cfg: ReplRuntimeConfig) -> Result<ResolvedRuntime, String> {
    let executable = cfg
        .runtime_path
        .unwrap_or_else(|| PathBuf::from(cfg.kind.default_executable()));

    let version = detect_runtime_version(cfg.kind, &executable).await?;
    if matches!(cfg.kind, crate::config::ReplRuntimeKindToml::Node) {
        let parsed = parse_version_triplet(&version).ok_or_else(|| {
            format!("failed to parse Node version `{version}` (expected like `18.0.0`)")
        })?;
        if !version_at_least(parsed, MIN_NODE_VERSION) {
            return Err(format!(
                "Node version {version} is too old for repl (need >= {min_major}.{min_minor}.{min_patch}). Consider setting `[tools].repl_runtime = \"deno\"`.",
                min_major = MIN_NODE_VERSION.0,
                min_minor = MIN_NODE_VERSION.1,
                min_patch = MIN_NODE_VERSION.2,
            ));
        }
    }

    let mut args = Vec::with_capacity(cfg.runtime_args.len() + 1);
    if matches!(cfg.kind, crate::config::ReplRuntimeKindToml::Node)
        && !cfg
            .runtime_args
            .iter()
            .any(|arg| arg == "--experimental-vm-modules")
    {
        args.push("--experimental-vm-modules".to_owned());
    }
    args.extend(cfg.runtime_args);

    Ok(ResolvedRuntime {
        kind: cfg.kind,
        executable,
        args,
        version,
        node_module_dirs: cfg.node_module_dirs,
    })
}

/// Run `<exe> --version` and extract the version string.
pub(super) async fn detect_runtime_version(
    kind: crate::config::ReplRuntimeKindToml,
    executable: &Path,
) -> Result<String, String> {
    let output = Command::new(executable)
        .arg("--version")
        .output()
        .await
        .map_err(|err| {
            format!(
                "failed to run `{executable}`: {err}",
                executable = executable.display()
            )
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let text = if stdout.is_empty() { stderr } else { stdout };
    if text.is_empty() {
        return Err(format!(
            "`{executable}` produced no version output",
            executable = executable.display()
        ));
    }

    match kind {
        crate::config::ReplRuntimeKindToml::Node => {
            Ok(text.trim().trim_start_matches('v').to_owned())
        }
        crate::config::ReplRuntimeKindToml::Deno => {
            for line in text.lines() {
                let l = line.trim();
                if let Some(rest) = l.strip_prefix("deno ") {
                    let version = rest.split_whitespace().next().unwrap_or_default().trim();
                    if !version.is_empty() {
                        return Ok(version.to_owned());
                    }
                }
            }
            // Fallback to first token of the first line.
            Ok(text.lines().next().unwrap_or_default().trim().to_owned())
        }
    }
}

/// Build the OS command to launch the kernel process, including sandbox
/// wrapping (seatbelt on macOS, Deno's built-in permissions, or none).
pub(super) fn build_runtime_command(
    runtime: &ResolvedRuntime,
    kernel_path: &Path,
    tmp_dir: &Path,
    sess: &crate::codex::Session,
    cwd: &Path,
) -> Result<Command, String> {
    use std::collections::HashMap;

    let sandbox_policy = sess.get_sandbox_policy();
    let sandbox_policy_cwd = sess.get_cwd();
    let enforce_managed_network = sess.managed_network_proxy().is_some();
    let caps = runtime.kind.capabilities();

    let mut env_overrides = HashMap::<String, String>::new();
    if let Some(proxy) = sess.managed_network_proxy() {
        proxy.apply_to_env(&mut env_overrides);
    }

    let seatbelt_enabled = cfg!(target_os = "macos")
        && caps.supports_seatbelt
        && !matches!(
            sandbox_policy,
            crate::protocol::SandboxPolicy::DangerFullAccess
        );

    if enforce_managed_network
        && !caps.can_enforce_network_without_seatbelt
        && !seatbelt_enabled
        && !matches!(
            sandbox_policy,
            crate::protocol::SandboxPolicy::DangerFullAccess
        )
    {
        return Err(format!(
            "repl {} runtime cannot enforce network mediation on this platform. \
             Set `[tools].repl_runtime = \"deno\"` (recommended) or disable \
             network mediation.",
            runtime.kind
        ));
    }

    let mut command = if seatbelt_enabled {
        if enforce_managed_network
            && !crate::seatbelt::has_loopback_proxy_endpoints(&env_overrides)
        {
            return Err(
                "managed network enforcement active but no usable proxy endpoints".to_owned(),
            );
        }

        let mut child_command: Vec<String> = Vec::with_capacity(2 + runtime.args.len());
        child_command.push(runtime.executable.to_string_lossy().into_owned());
        child_command.extend(runtime.args.iter().cloned());
        child_command.push(kernel_path.to_string_lossy().into_owned());

        let seatbelt_args = crate::seatbelt::build_seatbelt_args(
            child_command,
            sandbox_policy,
            sandbox_policy_cwd,
            enforce_managed_network,
            &env_overrides,
        );
        let mut cmd = Command::new(crate::seatbelt::seatbelt_exec_path());
        cmd.args(seatbelt_args);
        cmd.env(crate::spawn::CODEX_SANDBOX_ENV_VAR, "seatbelt");
        cmd
    } else if matches!(
        caps.sandbox,
        crate::config::RuntimeSandboxKind::BuiltinPermissions
    ) {
        // Runtime has its own permission sandbox (Deno).
        let mut cmd = Command::new(&runtime.executable);
        let allow_env = caps.sandbox_env_passthrough.join(",");
        let tmp_dir_display = tmp_dir.display();
        cmd.arg("run");
        cmd.arg("--quiet");
        cmd.arg("--no-prompt");
        cmd.arg(format!("--allow-env={allow_env}"));
        cmd.arg(format!("--allow-read={tmp_dir_display}"));
        cmd.args(&runtime.args);
        cmd.arg(kernel_path);
        cmd
    } else {
        // No sandbox available — run directly.
        let mut cmd = Command::new(&runtime.executable);
        cmd.args(&runtime.args);
        cmd.arg(kernel_path);
        cmd
    };

    command.current_dir(cwd);
    command.kill_on_drop(true);

    command.env("CODEX_REPL_TMP_DIR", tmp_dir);
    command.env("CODEX_REPL_RUNTIME", runtime.kind.label());
    command.env("CODEX_REPL_RUNTIME_VERSION", runtime.version.clone());

    if caps.uses_node_module_dirs && !runtime.node_module_dirs.is_empty() {
        let joined = std::env::join_paths(runtime.node_module_dirs.iter().map(|p| p.as_os_str()))
            .map_err(|err| format!("failed to join repl_node_module_dirs: {err}"))?;
        command.env("CODEX_REPL_NODE_MODULE_DIRS", joined);
    }

    for (key, value) in env_overrides {
        command.env(key, value);
    }

    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    Ok(command)
}

// ── Version parsing helpers ─────────────────────────────────────────────

pub(super) fn parse_version_triplet(version: &str) -> Option<(u64, u64, u64)> {
    let cleaned = version.trim().trim_start_matches('v');
    let mut parts = cleaned.split('.');
    let major = take_leading_u64(parts.next()?)?;
    let minor = take_leading_u64(parts.next()?)?;
    let patch = take_leading_u64(parts.next()?)?;
    Some((major, minor, patch))
}

fn take_leading_u64(input: &str) -> Option<u64> {
    let mut end = 0;
    for (idx, ch) in input.char_indices() {
        if ch.is_ascii_digit() {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    input[..end].parse().ok()
}

pub(super) fn version_at_least(found: (u64, u64, u64), min: (u64, u64, u64)) -> bool {
    if found.0 != min.0 {
        return found.0 > min.0;
    }
    if found.1 != min.1 {
        return found.1 > min.1;
    }
    found.2 >= min.2
}
