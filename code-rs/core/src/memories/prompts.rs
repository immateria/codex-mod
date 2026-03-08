use std::io::ErrorKind;
use std::path::Path;

use askama::Template;
use code_memories_state::MemoryEpochId;

use crate::truncate::truncate_middle;

use super::manifest::{select_prompt_entries, MemoriesCurrentContext, SnapshotManifest};
use super::published_artifact_paths_async;

// This budget is applied asymmetrically:
// - manifest selection packs prompt entries until the budget is full
// - summary fallback truncates one canonical summary blob to the budget
const MAX_MEMORY_PROMPT_BYTES: usize = 12_000;

#[derive(Template)]
#[template(path = "memories/read_path.md", escape = "none")]
struct MemoryToolDeveloperInstructionsTemplate<'a> {
    base_path: &'a str,
    memory_summary: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryPromptInstructions {
    pub instructions: String,
    pub selected_epoch_ids: Vec<MemoryEpochId>,
    pub used_fallback_summary: bool,
}

pub(crate) async fn build_memory_tool_developer_instructions(
    code_home: &Path,
    current_context: &MemoriesCurrentContext,
) -> Option<MemoryPromptInstructions> {
    let paths = published_artifact_paths_async(code_home).await.ok()?;
    if paths.generation.is_some() {
        let manifest_text = match tokio::fs::read_to_string(&paths.manifest_path).await {
            Ok(manifest_text) => manifest_text,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                tracing::warn!(
                    "active memories snapshot is missing manifest at {}",
                    paths.manifest_path.display()
                );
                return None;
            }
            Err(err) => {
                tracing::warn!(
                    "failed to read active memories manifest at {}: {err}",
                    paths.manifest_path.display()
                );
                return None;
            }
        };
        let manifest = match serde_json::from_str::<SnapshotManifest>(&manifest_text) {
            Ok(manifest) => manifest,
            Err(err) => {
                tracing::warn!(
                    "failed to parse memories manifest at {}: {err}",
                    paths.manifest_path.display()
                );
                return None;
            }
        };
        let selection =
            select_prompt_entries(&manifest, current_context, MAX_MEMORY_PROMPT_BYTES)?;
        let base_path = paths.base_dir.display().to_string();
        let template = MemoryToolDeveloperInstructionsTemplate {
            base_path: &base_path,
            memory_summary: &selection.summary_text,
        };
        return Some(MemoryPromptInstructions {
            instructions: template.render().ok()?,
            selected_epoch_ids: selection.selected_epoch_ids,
            used_fallback_summary: false,
        });
    }

    let (summary, selected_epoch_ids, used_fallback_summary) = {
        let summary = match tokio::fs::read_to_string(&paths.summary_path).await {
            Ok(summary) => summary,
            Err(err) if err.kind() == ErrorKind::NotFound => return None,
            Err(err) => {
                tracing::debug!(
                    "failed to read memories summary at {}: {err}",
                    paths.summary_path.display()
                );
                return None;
            }
        };
        let summary = summary.trim();
        if summary.is_empty() {
            return None;
        }
        (truncate_middle(summary, MAX_MEMORY_PROMPT_BYTES).0, Vec::new(), true)
    };

    let base_path = paths.base_dir.display().to_string();
    let template = MemoryToolDeveloperInstructionsTemplate {
        base_path: &base_path,
        memory_summary: &summary,
    };
    Some(MemoryPromptInstructions {
        instructions: template.render().ok()?,
        selected_epoch_ids,
        used_fallback_summary,
    })
}

#[cfg(test)]
mod tests {
    use code_memories_state::{
        MemoryEpochId, MemoryPlatformFamily, MemoryShellStyle, Stage1EpochProvenance,
    };
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::*;
    use crate::memories::manifest::{MemoriesCurrentContext, SnapshotEpochManifestEntry};

    fn sample_context() -> MemoriesCurrentContext {
        MemoriesCurrentContext {
            platform_family: MemoryPlatformFamily::Unix,
            shell_style: Some(MemoryShellStyle::Zsh),
            shell_program: Some("zsh".to_string()),
            workspace_root: Some("/tmp/project".to_string()),
            git_branch: Some("main".to_string()),
        }
    }

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

        let prompt = build_memory_tool_developer_instructions(temp.path(), &sample_context())
            .await
            .expect("prompt should be generated");

        assert!(prompt.instructions.contains("Recent summary"));
        assert!(prompt.instructions.contains(&root.display().to_string()));
        assert!(prompt.selected_epoch_ids.is_empty());
        assert!(prompt.used_fallback_summary);
    }

    #[tokio::test]
    async fn active_snapshot_without_manifest_returns_none() {
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

        assert!(
            build_memory_tool_developer_instructions(temp.path(), &sample_context())
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn prompt_prefers_manifest_selection_when_compatible_entries_exist() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("memories");
        let snapshot_dir = root.join("snapshots").join("20260307T120000Z-test");
        tokio::fs::create_dir_all(&snapshot_dir)
            .await
            .expect("create snapshot dir");
        tokio::fs::write(root.join("current"), "20260307T120000Z-test\n")
            .await
            .expect("write current pointer");
        tokio::fs::write(snapshot_dir.join("memory_summary.md"), "Global summary")
            .await
            .expect("write global summary");
        let manifest = SnapshotManifest::new(vec![SnapshotEpochManifestEntry {
            id: MemoryEpochId {
                thread_id: Uuid::nil(),
                epoch_index: 0,
            },
            provenance: Stage1EpochProvenance::Derived,
            platform_family: MemoryPlatformFamily::Unix,
            shell_style: MemoryShellStyle::Zsh,
            shell_program: Some("zsh".to_string()),
            workspace_root: Some("/tmp/project".to_string()),
            cwd_display: "~/project".to_string(),
            git_branch: Some("main".to_string()),
            source_updated_at: 1,
            usage_count: 10,
            last_usage: Some(10),
            rollout_summary_path: "rollout_summaries/demo.md".to_string(),
            prompt_entry: "Manifest-selected memory".to_string(),
        }]);
        tokio::fs::write(
            snapshot_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("manifest json"),
        )
        .await
        .expect("write manifest");

        let prompt = build_memory_tool_developer_instructions(temp.path(), &sample_context())
            .await
            .expect("prompt should be generated");

        assert!(prompt.instructions.contains("Manifest-selected memory"));
        assert!(!prompt.instructions.contains("Global summary"));
        assert_eq!(prompt.selected_epoch_ids.len(), 1);
        assert!(!prompt.used_fallback_summary);
    }

    #[tokio::test]
    async fn prompt_returns_none_when_manifest_has_no_compatible_entries() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("memories");
        let snapshot_dir = root.join("snapshots").join("20260307T120000Z-test");
        tokio::fs::create_dir_all(&snapshot_dir)
            .await
            .expect("create snapshot dir");
        tokio::fs::write(root.join("current"), "20260307T120000Z-test\n")
            .await
            .expect("write current pointer");
        tokio::fs::write(snapshot_dir.join("memory_summary.md"), "Global summary")
            .await
            .expect("write global summary");
        let manifest = SnapshotManifest::new(vec![SnapshotEpochManifestEntry {
            id: MemoryEpochId {
                thread_id: Uuid::nil(),
                epoch_index: 0,
            },
            provenance: Stage1EpochProvenance::Derived,
            platform_family: MemoryPlatformFamily::Windows,
            shell_style: MemoryShellStyle::PowerShell,
            shell_program: Some("pwsh".to_string()),
            workspace_root: Some("C:/repo".to_string()),
            cwd_display: "C:/repo".to_string(),
            git_branch: Some("main".to_string()),
            source_updated_at: 1,
            usage_count: 10,
            last_usage: Some(10),
            rollout_summary_path: "rollout_summaries/demo.md".to_string(),
            prompt_entry: "Incompatible memory".to_string(),
        }]);
        tokio::fs::write(
            snapshot_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("manifest json"),
        )
        .await
        .expect("write manifest");

        assert!(
            build_memory_tool_developer_instructions(temp.path(), &sample_context())
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn active_snapshot_with_invalid_manifest_returns_none() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("memories");
        let snapshot_dir = root.join("snapshots").join("20260307T120000Z-test");
        tokio::fs::create_dir_all(&snapshot_dir)
            .await
            .expect("create snapshot dir");
        tokio::fs::write(root.join("current"), "20260307T120000Z-test\n")
            .await
            .expect("write current pointer");
        tokio::fs::write(snapshot_dir.join("memory_summary.md"), "Global summary")
            .await
            .expect("write global summary");
        tokio::fs::write(snapshot_dir.join("manifest.json"), "{not json")
            .await
            .expect("write invalid manifest");

        assert!(
            build_memory_tool_developer_instructions(temp.path(), &sample_context())
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn fallback_summary_is_truncated_and_has_no_epoch_ids() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("memories");
        tokio::fs::create_dir_all(&root)
            .await
            .expect("create memories root");
        let long_summary = format!(
            "START{}END",
            "x".repeat(MAX_MEMORY_PROMPT_BYTES.saturating_mul(2))
        );
        tokio::fs::write(root.join("memory_summary.md"), &long_summary)
            .await
            .expect("write summary");

        let prompt = build_memory_tool_developer_instructions(temp.path(), &sample_context())
            .await
            .expect("prompt should be generated");

        assert!(prompt.instructions.contains("START"));
        assert!(prompt.instructions.contains("END"));
        assert!(!prompt.instructions.contains(&"x".repeat(MAX_MEMORY_PROMPT_BYTES)));
        assert!(prompt.selected_epoch_ids.is_empty());
        assert!(prompt.used_fallback_summary);
    }

    #[tokio::test]
    async fn empty_or_missing_summary_yields_none() {
        let temp = tempdir().expect("tempdir");
        assert!(
            build_memory_tool_developer_instructions(temp.path(), &sample_context())
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
            build_memory_tool_developer_instructions(temp.path(), &sample_context())
                .await
                .is_none()
        );
    }
}
