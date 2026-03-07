use std::path::Path;

use askama::Template;

use crate::truncate::truncate_middle;

use super::memory_root;
use super::memory_summary_path;

const MAX_MEMORY_PROMPT_BYTES: usize = 12_000;

#[derive(Template)]
#[template(path = "memories/read_path.md", escape = "none")]
struct MemoryToolDeveloperInstructionsTemplate<'a> {
    base_path: &'a str,
    memory_summary: &'a str,
}

pub(crate) async fn build_memory_tool_developer_instructions(code_home: &Path) -> Option<String> {
    let summary = tokio::fs::read_to_string(memory_summary_path(code_home))
        .await
        .ok()?;
    let summary = summary.trim();
    if summary.is_empty() {
        return None;
    }

    let truncated = truncate_middle(summary, MAX_MEMORY_PROMPT_BYTES).0;
    let base_path = memory_root(code_home).display().to_string();
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
