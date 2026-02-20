use std::path::Path;
use std::path::PathBuf;

pub(super) fn ensure_agent_dir(cwd: &Path, agent_id: &str) -> Result<PathBuf, String> {
    let safe_agent_id = crate::fs_sanitize::safe_path_component(agent_id, "agent");
    let dir = cwd.join(".code").join("agents").join(safe_agent_id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create agent dir {}: {}", dir.display(), e))?;
    Ok(dir)
}

pub(super) fn ensure_user_dir(cwd: &Path) -> Result<PathBuf, String> {
    let dir = cwd.join(".code").join("users");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create user dir {}: {}", dir.display(), e))?;
    Ok(dir)
}

pub(super) fn write_agent_file(
    dir: &Path,
    filename: &str,
    content: &str,
) -> Result<PathBuf, String> {
    if filename.chars().any(|ch| matches!(ch, '/' | '\\' | '\0')) {
        return Err(format!("Refusing to write invalid filename: {filename}"));
    }
    let candidate = Path::new(filename);
    if candidate.is_absolute() || candidate.components().count() != 1 {
        return Err(format!("Refusing to write non-file component: {filename}"));
    }
    let Some(file_name) = candidate.file_name() else {
        return Err(format!("Refusing to write invalid filename: {filename}"));
    };
    let file_name = file_name.to_string_lossy();
    if file_name.is_empty() || file_name == "." || file_name == ".." {
        return Err(format!("Refusing to write invalid filename: {filename}"));
    }

    let path = dir.join(file_name.as_ref());
    std::fs::write(&path, content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(path)
}

