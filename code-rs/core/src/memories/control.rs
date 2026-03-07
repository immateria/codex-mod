use std::path::Path;

const MEMORY_SUMMARY_FILENAME: &str = "memory_summary.md";
const RAW_MEMORIES_FILENAME: &str = "raw_memories.md";
const ROLLOUT_SUMMARIES_SUBDIR: &str = "rollout_summaries";
const SNAPSHOTS_SUBDIR: &str = "snapshots";

pub(crate) async fn ensure_safe_memory_root(memory_root: &Path) -> std::io::Result<()> {
    match tokio::fs::symlink_metadata(memory_root).await {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "refusing to clear symlinked memory root {}",
                    memory_root.display()
                ),
            ));
        }
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }

    tokio::fs::create_dir_all(memory_root).await?;
    Ok(())
}

pub(crate) async fn clear_memory_root_contents(memory_root: &Path) -> std::io::Result<()> {
    ensure_safe_memory_root(memory_root).await?;
    let mut entries = tokio::fs::read_dir(memory_root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let file_type = entry.file_type().await?;
        if file_type.is_dir() {
            tokio::fs::remove_dir_all(path).await?;
        } else {
            tokio::fs::remove_file(path).await?;
        }
    }

    Ok(())
}

pub(crate) async fn remove_legacy_artifacts(memory_root: &Path) -> std::io::Result<()> {
    ensure_safe_memory_root(memory_root).await?;
    for legacy_path in [
        memory_root.join(MEMORY_SUMMARY_FILENAME),
        memory_root.join(RAW_MEMORIES_FILENAME),
    ] {
        match tokio::fs::remove_file(&legacy_path).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }

    let legacy_rollout_dir = memory_root.join(ROLLOUT_SUMMARIES_SUBDIR);
    match tokio::fs::remove_dir_all(&legacy_rollout_dir).await {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    Ok(())
}

pub(crate) async fn prune_noncurrent_snapshots(
    memory_root: &Path,
    active_generation: &str,
) -> std::io::Result<()> {
    ensure_safe_memory_root(memory_root).await?;
    let snapshots_root = memory_root.join(SNAPSHOTS_SUBDIR);
    let mut entries = match tokio::fs::read_dir(&snapshots_root).await {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_name() == active_generation {
            continue;
        }
        let path = entry.path();
        let file_type = entry.file_type().await?;
        if file_type.is_dir() {
            tokio::fs::remove_dir_all(path).await?;
        } else {
            tokio::fs::remove_file(path).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::clear_memory_root_contents;
    use super::prune_noncurrent_snapshots;
    use super::remove_legacy_artifacts;

    #[tokio::test]
    async fn clear_preserves_root_directory() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("memories");
        tokio::fs::create_dir_all(root.join("rollout_summaries"))
            .await
            .expect("create memory layout");
        tokio::fs::write(root.join("memory_summary.md"), "summary")
            .await
            .expect("write summary");

        clear_memory_root_contents(&root)
            .await
            .expect("clear memory root");

        assert!(tokio::fs::try_exists(&root).await.expect("stat root"));
        let mut entries = tokio::fs::read_dir(&root).await.expect("read cleared root");
        assert!(entries.next_entry().await.expect("read dir").is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn clear_rejects_symlinked_root() {
        use std::os::unix::fs as unix_fs;

        let temp = tempdir().expect("tempdir");
        let target = temp.path().join("target");
        tokio::fs::create_dir_all(&target)
            .await
            .expect("create target");
        let link = temp.path().join("memories");
        unix_fs::symlink(&target, &link).expect("create symlink");

        let err = clear_memory_root_contents(&link)
            .await
            .expect_err("symlinked root should fail");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn remove_legacy_artifacts_only_clears_legacy_paths() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("memories");
        tokio::fs::create_dir_all(root.join("rollout_summaries"))
            .await
            .expect("create rollout summaries");
        tokio::fs::create_dir_all(root.join("snapshots").join("active"))
            .await
            .expect("create snapshots");
        tokio::fs::write(root.join("memory_summary.md"), "summary")
            .await
            .expect("write summary");
        tokio::fs::write(root.join("raw_memories.md"), "raw")
            .await
            .expect("write raw memories");

        remove_legacy_artifacts(&root)
            .await
            .expect("remove legacy artifacts");

        assert!(!tokio::fs::try_exists(root.join("memory_summary.md")).await.expect("stat summary"));
        assert!(!tokio::fs::try_exists(root.join("raw_memories.md")).await.expect("stat raw"));
        assert!(!tokio::fs::try_exists(root.join("rollout_summaries")).await.expect("stat rollout dir"));
        assert!(tokio::fs::try_exists(root.join("snapshots").join("active")).await.expect("stat snapshots"));
    }

    #[tokio::test]
    async fn prune_noncurrent_snapshots_keeps_active_generation_only() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("memories");
        tokio::fs::create_dir_all(root.join("snapshots").join("active"))
            .await
            .expect("create active snapshot");
        tokio::fs::create_dir_all(root.join("snapshots").join("stale"))
            .await
            .expect("create stale snapshot");

        prune_noncurrent_snapshots(&root, "active")
            .await
            .expect("prune snapshots");

        assert!(tokio::fs::try_exists(root.join("snapshots").join("active")).await.expect("stat active"));
        assert!(!tokio::fs::try_exists(root.join("snapshots").join("stale")).await.expect("stat stale"));
    }
}
