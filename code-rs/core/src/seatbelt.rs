use code_network_proxy::ALLOW_LOCAL_BINDING_ENV_KEY;
use code_network_proxy::PROXY_URL_ENV_KEYS;
use code_network_proxy::has_proxy_url_env_vars;
use code_network_proxy::proxy_url_env_value;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use tokio::process::Child;
use url::Url;

use crate::protocol::SandboxPolicy;
use crate::spawn::CODEX_SANDBOX_ENV_VAR;
use crate::spawn::StdioPolicy;
use crate::spawn::spawn_child_async;

const MACOS_SEATBELT_BASE_POLICY: &str = include_str!("seatbelt_base_policy.sbpl");
const MACOS_SEATBELT_NETWORK_POLICY: &str = include_str!("seatbelt_network_policy.sbpl");

/// When working with `sandbox-exec`, only consider `sandbox-exec` in `/usr/bin`
/// to defend against an attacker trying to inject a malicious version on the
/// PATH. If /usr/bin/sandbox-exec has been tampered with, then the attacker
/// already has root access.
const MACOS_PATH_TO_SEATBELT_EXECUTABLE: &str = "/usr/bin/sandbox-exec";

pub(crate) fn seatbelt_exec_path() -> &'static str {
    MACOS_PATH_TO_SEATBELT_EXECUTABLE
}

pub(crate) fn build_seatbelt_args(
    command: Vec<String>,
    sandbox_policy: &SandboxPolicy,
    sandbox_policy_cwd: &Path,
    enforce_managed_network: bool,
    env: &HashMap<String, String>,
) -> Vec<String> {
    create_seatbelt_command_args(
        command,
        sandbox_policy,
        sandbox_policy_cwd,
        enforce_managed_network,
        env,
    )
}

pub(crate) fn has_loopback_proxy_endpoints(env: &HashMap<String, String>) -> bool {
    !proxy_loopback_ports_from_env(env).is_empty()
}

pub async fn spawn_command_under_seatbelt(
    command: Vec<String>,
    command_cwd: PathBuf,
    sandbox_policy: &SandboxPolicy,
    sandbox_policy_cwd: &Path,
    stdio_policy: StdioPolicy,
    enforce_managed_network: bool,
    mut env: HashMap<String, String>,
) -> std::io::Result<Child> {
    let args = create_seatbelt_command_args(
        command,
        sandbox_policy,
        sandbox_policy_cwd,
        enforce_managed_network,
        &env,
    );
    let arg0 = None;
    env.insert(CODEX_SANDBOX_ENV_VAR.to_string(), "seatbelt".to_string());
    spawn_child_async(
        PathBuf::from(MACOS_PATH_TO_SEATBELT_EXECUTABLE),
        args,
        arg0,
        command_cwd,
        sandbox_policy,
        stdio_policy,
        env,
    )
    .await
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

fn proxy_scheme_default_port(scheme: &str) -> u16 {
    match scheme {
        "https" => 443,
        "socks5" | "socks5h" | "socks4" | "socks4a" => 1080,
        _ => 80,
    }
}

fn proxy_loopback_ports_from_env(env: &HashMap<String, String>) -> Vec<u16> {
    let mut ports = BTreeSet::new();
    for key in PROXY_URL_ENV_KEYS {
        let Some(proxy_url) = proxy_url_env_value(env, key) else {
            continue;
        };
        let trimmed = proxy_url.trim();
        if trimmed.is_empty() {
            continue;
        }

        let candidate = if trimmed.contains("://") {
            trimmed.to_string()
        } else {
            format!("http://{trimmed}")
        };
        let Ok(parsed) = Url::parse(&candidate) else {
            continue;
        };
        let Some(host) = parsed.host_str() else {
            continue;
        };
        if !is_loopback_host(host) {
            continue;
        }

        let scheme = parsed.scheme().to_ascii_lowercase();
        let port = parsed
            .port()
            .unwrap_or_else(|| proxy_scheme_default_port(scheme.as_str()));
        ports.insert(port);
    }
    ports.into_iter().collect()
}

fn local_binding_enabled(env: &HashMap<String, String>) -> bool {
    env.get(ALLOW_LOCAL_BINDING_ENV_KEY).is_some_and(|value| {
        let trimmed = value.trim();
        trimmed == "1" || trimmed.eq_ignore_ascii_case("true")
    })
}

fn dynamic_network_policy(
    sandbox_policy: &SandboxPolicy,
    enforce_managed_network: bool,
    env: &HashMap<String, String>,
) -> String {
    let ports = proxy_loopback_ports_from_env(env);
    if !ports.is_empty() {
        let mut policy =
            String::from("; allow outbound access only to configured loopback proxy endpoints\n");
        if local_binding_enabled(env) {
            policy.push_str("; allow localhost-only binding and loopback traffic\n");
            policy.push_str("(allow network-bind (local ip \"localhost:*\"))\n");
            policy.push_str("(allow network-inbound (local ip \"localhost:*\"))\n");
            policy.push_str("(allow network-outbound (remote ip \"localhost:*\"))\n");
        }
        for port in ports {
            policy.push_str(&format!(
                "(allow network-outbound (remote ip \"localhost:{port}\"))\n"
            ));
        }
        return format!("{policy}{MACOS_SEATBELT_NETWORK_POLICY}");
    }

    if has_proxy_url_env_vars(env) {
        // Proxy configuration is present but we could not infer any valid loopback endpoints.
        // Fail closed to avoid silently widening network access in proxy-enforced sessions.
        return String::new();
    }

    if enforce_managed_network {
        // Managed network requirements are active but no usable proxy endpoints
        // are available. Fail closed for network access.
        return String::new();
    }

    if sandbox_policy.has_full_network_access() {
        format!("(allow network-outbound)\n(allow network-inbound)\n{MACOS_SEATBELT_NETWORK_POLICY}")
    } else {
        String::new()
    }
}

#[cfg(target_os = "macos")]
fn confstr(name: libc::c_int) -> Option<String> {
    use std::ffi::CStr;
    let mut buf = vec![0_i8; (libc::PATH_MAX as usize) + 1];
    let len = unsafe { libc::confstr(name, buf.as_mut_ptr(), buf.len()) };
    if len == 0 {
        return None;
    }
    // confstr guarantees NUL-termination when len > 0.
    let cstr = unsafe { CStr::from_ptr(buf.as_ptr()) };
    cstr.to_str().ok().map(ToString::to_string)
}

#[cfg(target_os = "macos")]
fn confstr_path(name: libc::c_int) -> Option<PathBuf> {
    let s = confstr(name)?;
    let path = PathBuf::from(s);
    path.canonicalize().ok().or(Some(path))
}

fn macos_dir_params() -> Vec<(String, PathBuf)> {
    #[cfg(target_os = "macos")]
    {
        if let Some(p) = confstr_path(libc::_CS_DARWIN_USER_CACHE_DIR) {
            return vec![("DARWIN_USER_CACHE_DIR".to_string(), p)];
        }
    }

    vec![]
}

fn create_seatbelt_command_args(
    command: Vec<String>,
    sandbox_policy: &SandboxPolicy,
    sandbox_policy_cwd: &Path,
    enforce_managed_network: bool,
    env: &HashMap<String, String>,
) -> Vec<String> {
    let (file_write_policy, extra_cli_args) = {
        if sandbox_policy.has_full_disk_write_access() {
            // Allegedly, this is more permissive than `(allow file-write*)`.
            (
                r#"(allow file-write* (regex #"^/"))"#.to_string(),
                Vec::<String>::new(),
            )
        } else {
            let writable_roots = sandbox_policy.get_writable_roots_with_cwd(sandbox_policy_cwd);

            let mut writable_folder_policies: Vec<String> = Vec::new();
            let mut cli_args: Vec<String> = Vec::new();

            for (index, wr) in writable_roots.iter().enumerate() {
                // Canonicalize to avoid mismatches like /var vs /private/var on macOS.
                let canonical_root = wr.root.canonicalize().unwrap_or_else(|_| wr.root.clone());
                let root_param = format!("WRITABLE_ROOT_{index}");
                cli_args.push(format!(
                    "-D{root_param}={}",
                    canonical_root.to_string_lossy()
                ));

                // If the writable root is a file, allow writes to that exact file via literal.
                if canonical_root.is_file() {
                    writable_folder_policies.push(format!("(literal (param \"{root_param}\"))"));
                } else if wr.read_only_subpaths.is_empty() {
                    writable_folder_policies.push(format!("(subpath (param \"{root_param}\"))"));
                } else {
                    // Add parameters for each read-only subpath and generate
                    // the `(require-not ...)` clauses.
                    let mut require_parts: Vec<String> = Vec::new();
                    require_parts.push(format!("(subpath (param \"{root_param}\"))"));
                    for (subpath_index, ro) in wr.read_only_subpaths.iter().enumerate() {
                        let canonical_ro = ro.canonicalize().unwrap_or_else(|_| ro.clone());
                        let ro_param = format!("WRITABLE_ROOT_{index}_RO_{subpath_index}");
                        cli_args.push(format!("-D{ro_param}={}", canonical_ro.to_string_lossy()));
                        require_parts
                            .push(format!("(require-not (subpath (param \"{ro_param}\")))"));
                    }
                    let policy_component = format!("(require-all {} )", require_parts.join(" "));
                    writable_folder_policies.push(policy_component);
                }
            }

            if writable_folder_policies.is_empty() {
                ("".to_string(), Vec::<String>::new())
            } else {
                let file_write_policy = format!(
                    "(allow file-write*\n{}\n)",
                    writable_folder_policies.join(" ")
                );
                (file_write_policy, cli_args)
            }
        }
    };

    let file_read_policy = if sandbox_policy.has_full_disk_read_access() {
        "; allow read-only file operations\n(allow file-read*)"
    } else {
        ""
    };

    let network_policy = dynamic_network_policy(sandbox_policy, enforce_managed_network, env);

    let full_policy = format!(
        "{MACOS_SEATBELT_BASE_POLICY}\n{file_read_policy}\n{file_write_policy}\n{network_policy}"
    );

    if std::env::var("CODEX_DEBUG_PRINT_SEATBELT").is_ok() {
        tracing::debug!("--- Codex Seatbelt Policy ---\n{full_policy}\n------------------------------");
    }

    let mut seatbelt_args: Vec<String> = vec!["-p".to_string(), full_policy];
    seatbelt_args.extend(extra_cli_args);
    if !network_policy.is_empty() {
        seatbelt_args.extend(macos_dir_params().into_iter().map(|(key, value)| {
            format!("-D{key}={value}", value = value.to_string_lossy())
        }));
    }
    seatbelt_args.push("--".to_string());
    seatbelt_args.extend(command);
    seatbelt_args
}

#[cfg(test)]
mod tests {
    use super::MACOS_SEATBELT_BASE_POLICY;
    use super::create_seatbelt_command_args;
    use crate::protocol::SandboxPolicy;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn create_seatbelt_args_with_read_only_git_subpath() {
        if cfg!(target_os = "windows") {
            // /tmp does not exist on Windows, so skip this test.
            return;
        }

        // Create a temporary workspace with two writable roots: one containing
        // a top-level .git directory and one without it.
        let tmp = TempDir::new().expect("tempdir");
        let PopulatedTmp {
            root_with_git,
            root_without_git,
            root_with_git_canon,
            root_with_git_git_canon,
            root_without_git_canon,
        } = populate_tmpdir(tmp.path());
        let cwd = tmp.path().join("cwd");

        // Build a policy that only includes the two test roots as writable and
        // does not automatically include defaults TMPDIR or /tmp.
        let policy = SandboxPolicy::WorkspaceWrite {
            writable_roots: vec![root_with_git, root_without_git],
            network_access: false,
            exclude_tmpdir_env_var: true,
            exclude_slash_tmp: true,
            allow_git_writes: false,
        };
        let env = std::collections::HashMap::new();

        let args = create_seatbelt_command_args(
            vec!["/bin/echo".to_string(), "hello".to_string()],
            &policy,
            &cwd,
            false,
            &env,
        );

        // Build the expected policy text using a raw string for readability.
        // Note that the policy includes:
        // - the base policy,
        // - read-only access to the filesystem,
        // - write access to WRITABLE_ROOT_0 (but not its .git) and WRITABLE_ROOT_1.
        let expected_policy = format!(
            r#"{MACOS_SEATBELT_BASE_POLICY}
; allow read-only file operations
(allow file-read*)
(allow file-write*
(require-all (subpath (param "WRITABLE_ROOT_0")) (require-not (subpath (param "WRITABLE_ROOT_0_RO_0"))) ) (subpath (param "WRITABLE_ROOT_1")) (subpath (param "WRITABLE_ROOT_2"))
)
"#,
        );

        let mut expected_args = vec![
            "-p".to_string(),
            expected_policy,
            format!(
                "-DWRITABLE_ROOT_0={}",
                root_with_git_canon.to_string_lossy()
            ),
            format!(
                "-DWRITABLE_ROOT_0_RO_0={}",
                root_with_git_git_canon.to_string_lossy()
            ),
            format!(
                "-DWRITABLE_ROOT_1={}",
                root_without_git_canon.to_string_lossy()
            ),
            format!("-DWRITABLE_ROOT_2={}", cwd.to_string_lossy()),
        ];

        expected_args.extend(vec![
            "--".to_string(),
            "/bin/echo".to_string(),
            "hello".to_string(),
        ]);

        assert_eq!(expected_args, args);
    }

    #[test]
    fn create_seatbelt_args_for_cwd_as_git_repo() {
        if cfg!(target_os = "windows") {
            // /tmp does not exist on Windows, so skip this test.
            return;
        }

        // Create a temporary workspace with two writable roots: one containing
        // a top-level .git directory and one without it.
        let tmp = TempDir::new().expect("tempdir");
        let PopulatedTmp {
            root_with_git,
            root_with_git_canon,
            root_with_git_git_canon,
            ..
        } = populate_tmpdir(tmp.path());

        // Build a policy that does not specify any writable_roots, but does
        // use the default ones (cwd and TMPDIR) and verifies the `.git` check
        // is done properly for cwd.
        let policy = SandboxPolicy::WorkspaceWrite {
            writable_roots: vec![],
            network_access: false,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
            allow_git_writes: false,
        };
        let env = std::collections::HashMap::new();

        let args = create_seatbelt_command_args(
            vec!["/bin/echo".to_string(), "hello".to_string()],
            &policy,
            root_with_git.as_path(),
            false,
            &env,
        );

        let tmpdir_env_var = std::env::var("TMPDIR")
            .ok()
            .map(PathBuf::from)
            .and_then(|p| p.canonicalize().ok())
            .map(|p| p.to_string_lossy().to_string());

        let tempdir_policy_entry = if tmpdir_env_var.is_some() {
            r#" (subpath (param "WRITABLE_ROOT_2"))"#
        } else {
            ""
        };

        // Build the expected policy text using a raw string for readability.
        // Note that the policy includes:
        // - the base policy,
        // - read-only access to the filesystem,
        // - write access to WRITABLE_ROOT_0 (but not its .git) and WRITABLE_ROOT_1.
        let expected_policy = format!(
            r#"{MACOS_SEATBELT_BASE_POLICY}
; allow read-only file operations
(allow file-read*)
(allow file-write*
(require-all (subpath (param "WRITABLE_ROOT_0")) (require-not (subpath (param "WRITABLE_ROOT_0_RO_0"))) ) (subpath (param "WRITABLE_ROOT_1")){tempdir_policy_entry}
)
"#,
        );

        let mut expected_args = vec![
            "-p".to_string(),
            expected_policy,
            format!(
                "-DWRITABLE_ROOT_0={}",
                root_with_git_canon.to_string_lossy()
            ),
            format!(
                "-DWRITABLE_ROOT_0_RO_0={}",
                root_with_git_git_canon.to_string_lossy()
            ),
            format!(
                "-DWRITABLE_ROOT_1={}",
                PathBuf::from("/tmp")
                    .canonicalize()
                    .expect("canonicalize /tmp")
                    .to_string_lossy()
            ),
        ];

        if let Some(p) = tmpdir_env_var {
            expected_args.push(format!("-DWRITABLE_ROOT_2={p}"));
        }

        expected_args.extend(vec![
            "--".to_string(),
            "/bin/echo".to_string(),
            "hello".to_string(),
        ]);

        assert_eq!(expected_args, args);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn seatbelt_args_for_exec_command_wrap_shell_command() {
        let policy = SandboxPolicy::ReadOnly;
        let cwd = Path::new("/tmp");

        let mut env = std::collections::HashMap::new();
        env.insert(
            "HTTP_PROXY".to_string(),
            "http://127.0.0.1:8899".to_string(),
        );

        let command = vec![
            "/bin/bash".to_string(),
            "-lc".to_string(),
            "echo hi".to_string(),
        ];
        let args = super::build_seatbelt_args(command.clone(), &policy, cwd, true, &env);

        assert_eq!(args[0], "-p");
        assert!(
            args[1].contains("(allow network-outbound (remote ip \"localhost:8899\"))"),
            "expected loopback proxy port allow rule in seatbelt policy"
        );

        let dash_dash = args
            .iter()
            .position(|arg| arg == "--")
            .expect("seatbelt args should contain `--`");
        assert_eq!(
            &args[dash_dash..],
            &[
                "--".to_string(),
                "/bin/bash".to_string(),
                "-lc".to_string(),
                "echo hi".to_string(),
            ]
        );
    }

    struct PopulatedTmp {
        root_with_git: PathBuf,
        root_without_git: PathBuf,
        root_with_git_canon: PathBuf,
        root_with_git_git_canon: PathBuf,
        root_without_git_canon: PathBuf,
    }

    fn populate_tmpdir(tmp: &Path) -> PopulatedTmp {
        let root_with_git = tmp.join("with_git");
        let root_without_git = tmp.join("no_git");
        fs::create_dir_all(&root_with_git).expect("create with_git");
        fs::create_dir_all(&root_without_git).expect("create no_git");
        fs::create_dir_all(root_with_git.join(".git")).expect("create .git");

        // Ensure we have canonical paths for -D parameter matching.
        let root_with_git_canon = root_with_git.canonicalize().expect("canonicalize with_git");
        let root_with_git_git_canon = root_with_git_canon.join(".git");
        let root_without_git_canon = root_without_git
            .canonicalize()
            .expect("canonicalize no_git");
        PopulatedTmp {
            root_with_git,
            root_without_git,
            root_with_git_canon,
            root_with_git_git_canon,
            root_without_git_canon,
        }
    }
}
