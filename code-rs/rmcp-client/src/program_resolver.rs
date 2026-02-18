//! Platform-specific program resolution for MCP server execution.
//!
//! Windows cannot execute script files (e.g. `.cmd`, `.bat`) directly through
//! `Command::new()` without their file extensions, while Unix systems handle
//! scripts natively through shebangs. We resolve these differences so configs
//! like `command = "npx"` work across platforms.

use std::collections::HashMap;
use std::ffi::OsString;

#[cfg(windows)]
use std::env;
#[cfg(windows)]
use tracing::debug;

/// Unix systems handle PATH resolution and script execution natively, so this
/// function simply returns the program name unchanged.
#[cfg(unix)]
pub fn resolve(program: OsString, _env: &HashMap<String, String>) -> std::io::Result<OsString> {
    Ok(program)
}

/// Windows requires explicit file extensions for script execution. This uses
/// `which` to resolve the full executable path including extensions defined in
/// `PATHEXT`.
#[cfg(windows)]
pub fn resolve(program: OsString, env: &HashMap<String, String>) -> std::io::Result<OsString> {
    let cwd = env::current_dir()
        .map_err(|e| std::io::Error::other(format!("Failed to get current directory: {e}")))?;

    let search_path = env.get("PATH");

    match which::which_in(&program, search_path, &cwd) {
        Ok(resolved) => {
            debug!("Resolved {program:?} to {resolved:?}");
            Ok(resolved.into_os_string())
        }
        Err(e) => {
            debug!("Failed to resolve {program:?}: {e}. Using original path");
            Ok(program)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::create_env_for_mcp_server;
    use anyhow::Result;
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;
    use tokio::process::Command;

    #[cfg(unix)]
    #[tokio::test]
    async fn test_unix_executes_script_without_extension() -> Result<()> {
        let env = TestExecutableEnv::new()?;
        let mut cmd = Command::new(&env.program_name);
        cmd.envs(&env.mcp_env);

        let output = cmd.output().await;
        assert!(output.is_ok(), "Unix should execute scripts directly");
        Ok(())
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_windows_fails_without_extension() -> Result<()> {
        let env = TestExecutableEnv::new()?;
        let mut cmd = Command::new(&env.program_name);
        cmd.envs(&env.mcp_env);

        let output = cmd.output().await;
        assert!(
            output.is_err(),
            "Windows requires .cmd/.bat extension for direct execution"
        );
        Ok(())
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_windows_succeeds_with_extension() -> Result<()> {
        let env = TestExecutableEnv::new()?;
        let program_with_ext = format!("{}.cmd", env.program_name);
        let mut cmd = Command::new(&program_with_ext);
        cmd.envs(&env.mcp_env);

        let output = cmd.output().await;
        assert!(
            output.is_ok(),
            "Windows should execute scripts when the extension is provided"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_resolved_program_executes_successfully() -> Result<()> {
        let env = TestExecutableEnv::new()?;
        let program = OsString::from(&env.program_name);

        let resolved = resolve(program, &env.mcp_env)?;

        let mut cmd = Command::new(resolved);
        cmd.envs(&env.mcp_env);
        let output = cmd.output().await;

        assert!(output.is_ok(), "Resolved program should execute successfully");
        Ok(())
    }

    struct TestExecutableEnv {
        _temp_dir: TempDir,
        program_name: String,
        mcp_env: HashMap<String, String>,
    }

    impl TestExecutableEnv {
        const TEST_PROGRAM: &'static str = "test_mcp_server";

        fn new() -> Result<Self> {
            let temp_dir = TempDir::new()?;
            let dir_path = temp_dir.path();

            Self::create_executable(dir_path)?;

            let mut extra_env = HashMap::new();
            extra_env.insert("PATH".to_string(), Self::build_path(dir_path));

            #[cfg(windows)]
            extra_env.insert("PATHEXT".to_string(), Self::ensure_cmd_extension());

            let mcp_env = create_env_for_mcp_server(Some(extra_env));

            Ok(Self {
                _temp_dir: temp_dir,
                program_name: Self::TEST_PROGRAM.to_string(),
                mcp_env,
            })
        }

        fn create_executable(dir: &Path) -> Result<()> {
            #[cfg(windows)]
            {
                let file = dir.join(format!("{}.cmd", Self::TEST_PROGRAM));
                fs::write(&file, "@echo off\nexit 0")?;
            }

            #[cfg(unix)]
            {
                let file = dir.join(Self::TEST_PROGRAM);
                fs::write(&file, "#!/bin/sh\nexit 0")?;
                Self::set_executable(&file)?;
            }

            Ok(())
        }

        #[cfg(unix)]
        fn set_executable(path: &Path) -> Result<()> {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms)?;
            Ok(())
        }

        fn build_path(dir: &Path) -> String {
            let current = std::env::var("PATH").unwrap_or_default();
            let sep = if cfg!(windows) { ";" } else { ":" };
            format!("{}{sep}{current}", dir.to_string_lossy())
        }

        #[cfg(windows)]
        fn ensure_cmd_extension() -> String {
            let current = std::env::var("PATHEXT").unwrap_or_default();
            if current.to_uppercase().contains(".CMD") {
                current
            } else {
                format!(".CMD;{current}")
            }
        }
    }
}

