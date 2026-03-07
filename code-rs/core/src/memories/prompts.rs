use std::path::Path;

use askama::Template;

use crate::truncate::truncate_middle;

use super::published_artifact_paths;

const MAX_MEMORY_PROMPT_BYTES: usize = 12_000;

#[derive(Template)]
#[template(path = "memories/read_path.md", escape = "none")]
struct MemoryToolDeveloperInstructionsTemplate<'a> {
    base_path: &'a str,
    memory_summary: &'a str,
}

pub(crate) async fn build_memory_tool_developer_instructions(code_home: &Path) -> Option<String> {
    let paths = published_artifact_paths(code_home).ok()?;
    let summary = tokio::fs::read_to_string(&paths.summary_path)
        .await
        .ok()?;
    let summary = summary.trim();
    if summary.is_empty() {
        return None;
    }

    let truncated = truncate_middle(summary, MAX_MEMORY_PROMPT_BYTES).0;
    let base_path = paths.base_dir.display().to_string();
    let template = MemoryToolDeveloperInstructionsTemplate {
        base_path: &base_path,
        memory_summary: &truncated,
    };
    template.render().ok()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::build_memory_tool_developer_instructions;

    #[tokio::test]
    async fn prompt_uses_memory_summary_file() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("memories");
        tokio::fs::create_dir_all(&root)
            .await
            .expect("create memories root");
        tokio::fs::write(root.join("memory_summary.md"), "Recent summary")
            .await
            .expect("write summary");

        let prompt = build_memory_tool_developer_instructions(temp.path())
            .await
            .expect("prompt should be generated");

        assert!(prompt.contains("Recent summary"));
        assert!(prompt.contains(&root.display().to_string()));
    }

    #[tokio::test]
    async fn prompt_prefers_active_snapshot_when_pointer_exists() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("memories");
        let snapshot_dir = root.join("snapshots").join("20260307T120000Z-test");
        tokio::fs::create_dir_all(&snapshot_dir)
            .await
            .expect("create snapshot dir");
        tokio::fs::write(root.join("current"), "20260307T120000Z-test\n")
            .await
            .expect("write current pointer");
        tokio::fs::write(root.join("memory_summary.md"), "Legacy summary")
            .await
            .expect("write legacy summary");
        tokio::fs::write(snapshot_dir.join("memory_summary.md"), "Snapshot summary")
            .await
            .expect("write snapshot summary");

        let prompt = build_memory_tool_developer_instructions(temp.path())
            .await
            .expect("prompt should be generated");

        assert!(prompt.contains("Snapshot summary"));
        assert!(!prompt.contains("Legacy summary"));
        assert!(prompt.contains(&snapshot_dir.display().to_string()));
    }

    #[tokio::test]
    async fn empty_or_missing_summary_yields_none() {
        let temp = tempdir().expect("tempdir");
        assert!(
            build_memory_tool_developer_instructions(temp.path())
                .await
                .is_none()
        );

        let root = temp.path().join("memories");
        tokio::fs::create_dir_all(&root)
            .await
            .expect("create memories root");
        tokio::fs::write(root.join("memory_summary.md"), "   \n")
            .await
            .expect("write empty summary");

        assert!(
            build_memory_tool_developer_instructions(temp.path())
                .await
                .is_none()
        );
    }
}
