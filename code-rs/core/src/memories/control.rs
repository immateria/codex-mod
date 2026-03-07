use std::path::Path;

pub(crate) async fn clear_memory_root_contents(memory_root: &Path) -> std::io::Result<()> {
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::clear_memory_root_contents;

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
}
