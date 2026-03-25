use crate::default_client::create_client;
use crate::default_client::DEFAULT_ORIGINATOR;
use crate::config_types::PluginMarketplaceRepoToml;
use crate::config_types::PluginsToml;
use reqwest::Client;
use serde::Deserialize;
use sha1::Digest;
use sha1::Sha1;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::time::Duration;
use zip::ZipArchive;

const GITHUB_API_BASE_URL: &str = "https://api.github.com";
const GITHUB_API_ACCEPT_HEADER: &str = "application/vnd.github+json";
const GITHUB_API_VERSION_HEADER: &str = "2022-11-28";
const OPENAI_PLUGINS_OWNER: &str = "openai";
const OPENAI_PLUGINS_REPO: &str = "plugins";
const CURATED_PLUGINS_RELATIVE_DIR: &str = ".tmp/plugins";
const CURATED_PLUGINS_SHA_FILE: &str = ".tmp/plugins.sha";
const MARKETPLACE_REPOS_RELATIVE_DIR: &str = ".tmp/plugin-marketplaces";
const CURATED_PLUGINS_GIT_TIMEOUT: Duration = Duration::from_secs(30);
const CURATED_PLUGINS_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct GitHubRepositorySummary {
    default_branch: String,
}

#[derive(Debug, Deserialize)]
struct GitHubGitRefSummary {
    object: GitHubGitRefObject,
}

#[derive(Debug, Deserialize)]
struct GitHubGitRefObject {
    sha: String,
}

pub(crate) fn curated_plugins_repo_path(code_home: &Path) -> PathBuf {
    code_home.join(CURATED_PLUGINS_RELATIVE_DIR)
}

pub(crate) fn read_curated_plugins_sha(code_home: &Path) -> Option<String> {
    read_sha_file(code_home.join(CURATED_PLUGINS_SHA_FILE).as_path())
}

pub(crate) fn synced_marketplace_repo_path(
    code_home: &Path,
    repo: &PluginMarketplaceRepoToml,
) -> PathBuf {
    code_home
        .join(MARKETPLACE_REPOS_RELATIVE_DIR)
        .join(marketplace_repo_cache_key(repo))
}

pub(crate) fn sync_openai_plugins_repo(code_home: &Path) -> Result<String, String> {
    sync_openai_plugins_repo_with_transport_overrides(code_home, "git", GITHUB_API_BASE_URL)
}

pub(crate) fn sync_curated_plugins_repo(
    code_home: &Path,
    plugins: &PluginsToml,
) -> Result<String, String> {
    match plugins.curated_repo_url.as_deref() {
        Some(repo_url) => sync_repo_via_git(
            repo_url,
            plugins.curated_repo_ref.as_deref(),
            curated_plugins_repo_path(code_home).as_path(),
            code_home.join(CURATED_PLUGINS_SHA_FILE).as_path(),
            "git",
            "curated plugin marketplace repo",
        ),
        None => sync_openai_plugins_repo(code_home),
    }
}

pub(crate) fn sync_git_marketplace_repo(
    code_home: &Path,
    repo: &PluginMarketplaceRepoToml,
) -> Result<String, String> {
    let repo_path = synced_marketplace_repo_path(code_home, repo);
    let sha_path = code_home
        .join(MARKETPLACE_REPOS_RELATIVE_DIR)
        .join(format!("{}.sha", marketplace_repo_cache_key(repo)));
    sync_repo_via_git(
        repo.url.as_str(),
        repo.git_ref.as_deref(),
        repo_path.as_path(),
        sha_path.as_path(),
        "git",
        "plugin marketplace repo",
    )
}

fn sync_openai_plugins_repo_with_transport_overrides(
    code_home: &Path,
    git_binary: &str,
    api_base_url: &str,
) -> Result<String, String> {
    match sync_openai_plugins_repo_via_git(code_home, git_binary) {
        Ok(remote_sha) => Ok(remote_sha),
        Err(err) => {
            tracing::warn!(
                error = %err,
                git_binary,
                "git sync failed for curated plugin sync; falling back to GitHub HTTP"
            );
            sync_openai_plugins_repo_via_http(code_home, api_base_url)
        }
    }
}

fn sync_openai_plugins_repo_via_git(code_home: &Path, git_binary: &str) -> Result<String, String> {
    sync_repo_via_git(
        "https://github.com/openai/plugins.git",
        None,
        curated_plugins_repo_path(code_home).as_path(),
        code_home.join(CURATED_PLUGINS_SHA_FILE).as_path(),
        git_binary,
        "curated plugins repo",
    )
}

fn sync_openai_plugins_repo_via_http(code_home: &Path, api_base_url: &str) -> Result<String, String> {
    let repo_path = curated_plugins_repo_path(code_home);
    let sha_path = code_home.join(CURATED_PLUGINS_SHA_FILE);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("failed to create curated plugins sync runtime: {err}"))?;
    let remote_sha = runtime.block_on(fetch_curated_repo_remote_sha(api_base_url))?;
    let local_sha = read_sha_file(&sha_path);

    if local_sha.as_deref() == Some(remote_sha.as_str()) && repo_path.is_dir() {
        return Ok(remote_sha);
    }

    let cloned_repo_path = prepare_repo_parent_and_temp_dir(&repo_path, "curated plugins repo")?;
    let zipball_bytes = runtime.block_on(fetch_curated_repo_zipball(api_base_url, &remote_sha))?;
    extract_zipball_to_dir(&zipball_bytes, &cloned_repo_path)?;
    ensure_marketplace_manifest_exists(&cloned_repo_path)?;
    activate_repo(&repo_path, &cloned_repo_path, "curated plugins repo")?;
    write_repo_sha(&sha_path, &remote_sha, "curated plugins repo")?;
    Ok(remote_sha)
}

fn prepare_repo_parent_and_temp_dir(repo_path: &Path, label: &str) -> Result<PathBuf, String> {
    let Some(parent) = repo_path.parent() else {
        return Err(format!(
            "failed to determine {label} parent directory for {}",
            repo_path.display()
        ));
    };
    std::fs::create_dir_all(parent).map_err(|err| {
        format!(
            "failed to create {label} parent directory {}: {err}",
            parent.display()
        )
    })?;

    let clone_dir = tempfile::Builder::new()
        .prefix("plugins-clone-")
        .tempdir_in(parent)
        .map_err(|err| {
            format!(
                "failed to create temporary {label} directory in {}: {err}",
                parent.display()
            )
        })?;
    Ok(clone_dir.keep())
}

fn ensure_marketplace_manifest_exists(repo_path: &Path) -> Result<(), String> {
    if repo_path.join(".agents/plugins/marketplace.json").is_file() {
        return Ok(());
    }
    Err(format!(
        "curated plugins archive missing marketplace manifest at {}",
        repo_path.join(".agents/plugins/marketplace.json").display()
    ))
}

fn activate_repo(repo_path: &Path, staged_repo_path: &Path, label: &str) -> Result<(), String> {
    if repo_path.exists() {
        let parent = repo_path.parent().ok_or_else(|| {
            format!(
                "failed to determine {label} parent directory for {}",
                repo_path.display()
            )
        })?;
        let backup_dir = tempfile::Builder::new()
            .prefix("plugins-backup-")
            .tempdir_in(parent)
            .map_err(|err| {
                format!(
                    "failed to create {label} backup directory in {}: {err}",
                    parent.display()
                )
            })?;
        let backup_repo_path = backup_dir.path().join("repo");

        std::fs::rename(repo_path, &backup_repo_path).map_err(|err| {
            format!(
                "failed to move previous {label} out of the way at {}: {err}",
                repo_path.display()
            )
        })?;

        if let Err(err) = std::fs::rename(staged_repo_path, repo_path) {
            let rollback_result = std::fs::rename(&backup_repo_path, repo_path);
            return match rollback_result {
                Ok(()) => Err(format!(
                    "failed to activate new {label} at {}: {err}",
                    repo_path.display()
                )),
                Err(rollback_err) => {
                    let backup_path = backup_dir.keep().join("repo");
                    Err(format!(
                        "failed to activate new {label} at {}: {err}; failed to restore previous repo (left at {}): {rollback_err}",
                        repo_path.display(),
                        backup_path.display()
                    ))
                }
            };
        }
    } else {
        std::fs::rename(staged_repo_path, repo_path).map_err(|err| {
            format!(
                "failed to activate {label} at {}: {err}",
                repo_path.display()
            )
        })?;
    }

    Ok(())
}

fn write_repo_sha(sha_path: &Path, remote_sha: &str, label: &str) -> Result<(), String> {
    if let Some(parent) = sha_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create {label} sha directory {}: {err}",
                parent.display()
            )
        })?;
    }
    std::fs::write(sha_path, format!("{remote_sha}\n")).map_err(|err| {
        format!(
            "failed to write {label} sha file {}: {err}",
            sha_path.display()
        )
    })
}

fn read_local_git_or_sha_file(repo_path: &Path, sha_path: &Path, git_binary: &str) -> Option<String> {
    if repo_path.join(".git").is_dir()
        && let Ok(sha) = git_head_sha(repo_path, git_binary)
    {
        return Some(sha);
    }

    read_sha_file(sha_path)
}

fn sync_repo_via_git(
    repo_url: &str,
    git_ref: Option<&str>,
    repo_path: &Path,
    sha_path: &Path,
    git_binary: &str,
    label: &str,
) -> Result<String, String> {
    let remote_sha = git_ls_remote_sha(git_binary, repo_url, git_ref)?;
    let local_sha = read_local_git_or_sha_file(repo_path, sha_path, git_binary);

    if local_sha.as_deref() == Some(remote_sha.as_str()) && repo_path.join(".git").is_dir() {
        return Ok(remote_sha);
    }

    let cloned_repo_path = prepare_repo_parent_and_temp_dir(repo_path, label)?;
    let mut clone_command = Command::new(git_binary);
    clone_command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("clone")
        .arg("--depth")
        .arg("1");
    if let Some(git_ref) = git_ref.filter(|git_ref| !git_ref.trim().is_empty()) {
        clone_command.arg("--branch").arg(git_ref);
    }
    clone_command.arg(repo_url).arg(&cloned_repo_path);

    let clone_context = format!("git clone {label}");
    let clone_output = run_git_command_with_timeout(
        &mut clone_command,
        clone_context.as_str(),
        CURATED_PLUGINS_GIT_TIMEOUT,
    )?;
    ensure_git_success(&clone_output, clone_context.as_str())?;

    let cloned_sha = git_head_sha(&cloned_repo_path, git_binary)?;
    if cloned_sha != remote_sha {
        return Err(format!(
            "{label} clone HEAD mismatch: expected {remote_sha}, got {cloned_sha}"
        ));
    }

    ensure_marketplace_manifest_exists(&cloned_repo_path)?;
    activate_repo(repo_path, &cloned_repo_path, label)?;
    write_repo_sha(sha_path, &remote_sha, label)?;
    Ok(remote_sha)
}

fn git_ls_remote_sha(
    git_binary: &str,
    repo_url: &str,
    git_ref: Option<&str>,
) -> Result<String, String> {
    let refspec = git_ref.unwrap_or("HEAD");
    let output = run_git_command_with_timeout(
        Command::new(git_binary)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .arg("ls-remote")
            .arg(repo_url)
            .arg(refspec),
        "git ls-remote plugin marketplace repo",
        CURATED_PLUGINS_GIT_TIMEOUT,
    )?;
    ensure_git_success(&output, "git ls-remote plugin marketplace repo")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(first_line) = stdout.lines().next() else {
        return Err(format!("git ls-remote returned empty output for {repo_url}"));
    };
    let Some((sha, _)) = first_line.split_once('\t') else {
        return Err(format!(
            "unexpected git ls-remote output for {repo_url}: {first_line}"
        ));
    };
    if sha.is_empty() {
        return Err(format!("git ls-remote returned empty sha for {repo_url}"));
    }
    Ok(sha.to_string())
}

fn marketplace_repo_cache_key(repo: &PluginMarketplaceRepoToml) -> String {
    let mut hasher = Sha1::new();
    hasher.update(repo.url.as_bytes());
    hasher.update(b"\n");
    if let Some(git_ref) = repo.git_ref.as_deref() {
        hasher.update(git_ref.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn git_head_sha(repo_path: &Path, git_binary: &str) -> Result<String, String> {
    let output = Command::new(git_binary)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(repo_path)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .map_err(|err| {
            format!(
                "failed to run git rev-parse HEAD in {}: {err}",
                repo_path.display()
            )
        })?;
    ensure_git_success(&output, "git rev-parse HEAD")?;

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(format!(
            "git rev-parse HEAD returned empty output in {}",
            repo_path.display()
        ));
    }
    Ok(sha)
}

fn run_git_command_with_timeout(
    command: &mut Command,
    context: &str,
    timeout: Duration,
) -> Result<Output, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to run {context}: {err}"))?;

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|err| format!("failed to wait for {context}: {err}"));
            }
            Ok(None) => {}
            Err(err) => return Err(format!("failed to poll {context}: {err}")),
        }

        if start.elapsed() >= timeout {
            match child.try_wait() {
                Ok(Some(_)) => {
                    return child
                        .wait_with_output()
                        .map_err(|err| format!("failed to wait for {context}: {err}"));
                }
                Ok(None) => {}
                Err(err) => return Err(format!("failed to poll {context}: {err}")),
            }

            let _ = child.kill();
            let output = child
                .wait_with_output()
                .map_err(|err| format!("failed to wait for {context} after timeout: {err}"))?;
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return if stderr.is_empty() {
                Err(format!("{context} timed out after {}s", timeout.as_secs()))
            } else {
                Err(format!(
                    "{context} timed out after {}s: {stderr}",
                    timeout.as_secs()
                ))
            };
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn ensure_git_success(output: &Output, context: &str) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(format!("{context} failed with status {}", output.status))
    } else {
        Err(format!(
            "{context} failed with status {}: {stderr}",
            output.status
        ))
    }
}

async fn fetch_curated_repo_remote_sha(api_base_url: &str) -> Result<String, String> {
    let api_base_url = api_base_url.trim_end_matches('/');
    let repo_url = format!("{api_base_url}/repos/{OPENAI_PLUGINS_OWNER}/{OPENAI_PLUGINS_REPO}");
    let client = create_client(DEFAULT_ORIGINATOR);
    let repo_body = fetch_github_text(&client, &repo_url, "get curated plugins repository").await?;
    let repo_summary: GitHubRepositorySummary =
        serde_json::from_str(&repo_body).map_err(|err| {
            format!("failed to parse curated plugins repository response from {repo_url}: {err}")
        })?;
    if repo_summary.default_branch.is_empty() {
        return Err(format!(
            "curated plugins repository response from {repo_url} did not include a default branch"
        ));
    }

    let git_ref_url = format!("{repo_url}/git/ref/heads/{}", repo_summary.default_branch);
    let git_ref_body =
        fetch_github_text(&client, &git_ref_url, "get curated plugins HEAD ref").await?;
    let git_ref: GitHubGitRefSummary = serde_json::from_str(&git_ref_body).map_err(|err| {
        format!("failed to parse curated plugins ref response from {git_ref_url}: {err}")
    })?;
    if git_ref.object.sha.is_empty() {
        return Err(format!(
            "curated plugins ref response from {git_ref_url} did not include a HEAD sha"
        ));
    }

    Ok(git_ref.object.sha)
}

async fn fetch_curated_repo_zipball(api_base_url: &str, remote_sha: &str) -> Result<Vec<u8>, String> {
    let api_base_url = api_base_url.trim_end_matches('/');
    let repo_url = format!("{api_base_url}/repos/{OPENAI_PLUGINS_OWNER}/{OPENAI_PLUGINS_REPO}");
    let zipball_url = format!("{repo_url}/zipball/{remote_sha}");
    let client = create_client(DEFAULT_ORIGINATOR);
    fetch_github_bytes(&client, &zipball_url, "download curated plugins archive").await
}

async fn fetch_github_text(client: &Client, url: &str, context: &str) -> Result<String, String> {
    let response = github_request(client, url)
        .send()
        .await
        .map_err(|err| format!("failed to {context} from {url}: {err}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "{context} from {url} failed with status {status}: {body}"
        ));
    }
    Ok(body)
}

async fn fetch_github_bytes(client: &Client, url: &str, context: &str) -> Result<Vec<u8>, String> {
    let response = github_request(client, url)
        .send()
        .await
        .map_err(|err| format!("failed to {context} from {url}: {err}"))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|err| format!("failed to read {context} response from {url}: {err}"))?;
    if !status.is_success() {
        let body_text = String::from_utf8_lossy(&body);
        return Err(format!(
            "{context} from {url} failed with status {status}: {body_text}"
        ));
    }
    Ok(body.to_vec())
}

fn github_request(client: &Client, url: &str) -> reqwest::RequestBuilder {
    client
        .get(url)
        .timeout(CURATED_PLUGINS_HTTP_TIMEOUT)
        .header("accept", GITHUB_API_ACCEPT_HEADER)
        .header("x-github-api-version", GITHUB_API_VERSION_HEADER)
}

fn read_sha_file(sha_path: &Path) -> Option<String> {
    std::fs::read_to_string(sha_path)
        .ok()
        .map(|sha| sha.trim().to_string())
        .filter(|sha| !sha.is_empty())
}

fn extract_zipball_to_dir(bytes: &[u8], destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|err| {
        format!(
            "failed to create curated plugins extraction directory {}: {err}",
            destination.display()
        )
    })?;

    let cursor = std::io::Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|err| format!("failed to open curated plugins zip archive: {err}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("failed to read curated plugins zip entry: {err}"))?;
        let Some(relative_path) = entry.enclosed_name() else {
            return Err(format!(
                "curated plugins zip entry `{}` escapes extraction root",
                entry.name()
            ));
        };

        let mut components = relative_path.components();
        let Some(std::path::Component::Normal(_)) = components.next() else {
            continue;
        };

        let output_relative = components.fold(PathBuf::new(), |mut path, component| {
            if let std::path::Component::Normal(segment) = component {
                path.push(segment);
            }
            path
        });
        if output_relative.as_os_str().is_empty() {
            continue;
        }

        let output_path = destination.join(&output_relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&output_path).map_err(|err| {
                format!(
                    "failed to create curated plugins directory {}: {err}",
                    output_path.display()
                )
            })?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "failed to create curated plugins directory {}: {err}",
                    parent.display()
                )
            })?;
        }
        let mut output = std::fs::File::create(&output_path).map_err(|err| {
            format!(
                "failed to create curated plugins file {}: {err}",
                output_path.display()
            )
        })?;
        std::io::copy(&mut entry, &mut output).map_err(|err| {
            format!(
                "failed to write curated plugins file {}: {err}",
                output_path.display()
            )
        })?;
        apply_zip_permissions(&entry, &output_path)?;
    }

    Ok(())
}

#[cfg(unix)]
fn apply_zip_permissions(entry: &zip::read::ZipFile<'_>, output_path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let Some(mode) = entry.unix_mode() else {
        return Ok(());
    };
    std::fs::set_permissions(output_path, std::fs::Permissions::from_mode(mode)).map_err(|err| {
        format!(
            "failed to set permissions on curated plugins file {}: {err}",
            output_path.display()
        )
    })
}

#[cfg(not(unix))]
fn apply_zip_permissions(_entry: &zip::read::ZipFile<'_>, _output_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn init_marketplace_repo() -> (TempDir, String) {
        let repo = TempDir::new().expect("temp repo");
        fs::create_dir_all(repo.path().join(".agents/plugins"))
            .expect("create marketplace dir");
        fs::create_dir_all(repo.path().join("plugins/sample"))
            .expect("create plugin dir");
        fs::write(
            repo.path().join(".agents/plugins/marketplace.json"),
            r#"{
  "name": "custom-marketplace",
  "plugins": [
    {
      "name": "sample",
      "source": { "path": "plugins/sample" },
      "policy": { "installation": "AVAILABLE", "authentication": "ON_INSTALL" }
    }
  ]
}"#,
        )
        .expect("write marketplace manifest");
        fs::write(
            repo.path().join("plugins/sample/plugin.json"),
            r#"{"name":"sample","version":"1.0.0"}"#,
        )
        .expect("write plugin manifest");

        run_git(repo.path(), ["init", "--initial-branch=main"]);
        run_git(repo.path(), ["config", "user.email", "test@example.com"]);
        run_git(repo.path(), ["config", "user.name", "Test User"]);
        run_git(repo.path(), ["add", "."]);
        run_git(repo.path(), ["commit", "-m", "initial"]);

        (repo, "main".to_string())
    }

    fn run_git<const N: usize>(cwd: &Path, args: [&str; N]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git should run");
        assert!(status.success(), "git command failed: {:?}", args);
    }

    #[test]
    fn sync_git_marketplace_repo_clones_local_repo() {
        let (repo, git_ref) = init_marketplace_repo();
        let code_home = TempDir::new().expect("code home");
        let marketplace_repo = PluginMarketplaceRepoToml {
            url: repo.path().to_string_lossy().to_string(),
            git_ref: Some(git_ref),
        };

        let sha =
            sync_git_marketplace_repo(code_home.path(), &marketplace_repo).expect("sync should work");
        let synced_repo_path = synced_marketplace_repo_path(code_home.path(), &marketplace_repo);

        assert!(!sha.is_empty(), "expected synced sha to be recorded");
        assert!(synced_repo_path.join(".agents/plugins/marketplace.json").is_file());
    }

    #[test]
    fn sync_curated_plugins_repo_uses_override_repo_url() {
        let (repo, git_ref) = init_marketplace_repo();
        let code_home = TempDir::new().expect("code home");
        let plugins = PluginsToml {
            curated_repo_url: Some(repo.path().to_string_lossy().to_string()),
            curated_repo_ref: Some(git_ref),
            marketplace_repos: Vec::new(),
        };

        sync_curated_plugins_repo(code_home.path(), &plugins).expect("sync should work");

        assert!(
            curated_plugins_repo_path(code_home.path())
                .join(".agents/plugins/marketplace.json")
                .is_file()
        );
        assert!(read_curated_plugins_sha(code_home.path()).is_some());
    }
}
