use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;

fn exec_server_binary_file_name() -> &'static str {
    if cfg!(windows) {
        "codex-exec-server.exe"
    } else {
        "codex-exec-server"
    }
}

pub(crate) fn resolve_exec_server_binary_path(
    current_exe: &Path,
    path_env: Option<&OsStr>,
) -> Option<PathBuf> {
    let sibling = current_exe.with_file_name(exec_server_binary_file_name());
    if sibling.is_file() {
        return Some(sibling);
    }

    let path_env = path_env?;
    for dir in std::env::split_paths(path_env) {
        let candidate = dir.join(exec_server_binary_file_name());
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

pub(crate) fn resolve_exec_server_binary_path_from_env() -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    resolve_exec_server_binary_path(&current_exe, std::env::var_os("PATH").as_deref())
}

pub(crate) struct SpawnedExecServer {
    listen_url: String,
    child: tokio::process::Child,
    _stdout_drain_task: tokio::task::JoinHandle<()>,
    _stderr_drain_task: tokio::task::JoinHandle<()>,
}

impl SpawnedExecServer {
    pub(crate) fn listen_url(&self) -> &str {
        &self.listen_url
    }
}

impl Drop for SpawnedExecServer {
    fn drop(&mut self) {
        self._stdout_drain_task.abort();
        self._stderr_drain_task.abort();
        let _ = self.child.start_kill();
    }
}

pub(crate) async fn spawn_exec_server(binary: &Path) -> std::io::Result<SpawnedExecServer> {
    let mut cmd = tokio::process::Command::new(binary);
    cmd.arg("--listen").arg("ws://127.0.0.1:0");
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| {
        std::io::Error::other("failed to capture exec-server stdout")
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        std::io::Error::other("failed to capture exec-server stderr")
    })?;

    let mut stdout_reader = tokio::io::BufReader::new(stdout);
    let mut first_line = String::new();
    let read_first = tokio::time::timeout(
        Duration::from_secs(2),
        stdout_reader.read_line(&mut first_line),
    )
    .await
    .map_err(|_| std::io::Error::other("timed out waiting for exec-server listen URL"))??;

    if read_first == 0 {
        return Err(std::io::Error::other(
            "exec-server exited before printing listen URL",
        ));
    }

    let listen_url = first_line.trim().to_string();
    if !listen_url.starts_with("ws://") {
        return Err(std::io::Error::other(format!(
            "exec-server printed unexpected listen URL: {listen_url:?}"
        )));
    }

    // Drain stdout/stderr so the child never blocks on filled pipes.
    let _stdout_drain_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stdout_reader.read_to_end(&mut buf).await;
    });
    let _stderr_drain_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr);
        let mut buf = Vec::new();
        let _ = reader.read_to_end(&mut buf).await;
    });

    Ok(SpawnedExecServer {
        listen_url,
        child,
        _stdout_drain_task,
        _stderr_drain_task,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_binary_prefers_sibling() {
        let temp = tempfile::tempdir().expect("tempdir");
        let current_exe = temp.path().join("code-app-server");
        let sibling = temp.path().join(exec_server_binary_file_name());
        std::fs::write(&sibling, b"").expect("write dummy sibling");

        let resolved = resolve_exec_server_binary_path(&current_exe, Some(OsStr::new("")));
        assert_eq!(resolved.as_deref(), Some(sibling.as_path()));
    }

    #[test]
    fn resolve_binary_falls_back_to_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path_dir = temp.path().join("bin");
        std::fs::create_dir_all(&path_dir).expect("create bin dir");
        let candidate = path_dir.join(exec_server_binary_file_name());
        std::fs::write(&candidate, b"").expect("write dummy path candidate");

        let current_exe = temp.path().join("not-in-bin");
        let path_str = path_dir.to_string_lossy();
        let path_env = OsStr::new(path_str.as_ref());
        let resolved = resolve_exec_server_binary_path(&current_exe, Some(path_env));
        assert_eq!(resolved.as_deref(), Some(candidate.as_path()));
    }

    #[test]
    fn resolve_binary_returns_none_when_missing() {
        let current_exe = Path::new("/tmp/code-app-server");
        let resolved = resolve_exec_server_binary_path(current_exe, Some(OsStr::new("")));
        assert!(resolved.is_none());
    }
}
