use std::cmp::Reverse;
use std::path::Path;

use code_memories_state::{
    MemoryEpochId, MemoryPlatformFamily, MemoryShellStyle, Stage1EpochProvenance,
};
use serde::{Deserialize, Serialize};

use crate::config_types::ShellScriptStyle;
use crate::environment_context::EnvironmentContextSnapshot;
use crate::git_info::get_git_repo_root;
use crate::shell::Shell;

const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct SnapshotManifest {
    pub version: u32,
    pub epochs: Vec<SnapshotEpochManifestEntry>,
}

impl SnapshotManifest {
    pub(crate) fn new(epochs: Vec<SnapshotEpochManifestEntry>) -> Self {
        Self {
            version: MANIFEST_VERSION,
            epochs,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct SnapshotEpochManifestEntry {
    pub id: MemoryEpochId,
    pub provenance: Stage1EpochProvenance,
    pub platform_family: MemoryPlatformFamily,
    pub shell_style: MemoryShellStyle,
    pub shell_program: Option<String>,
    pub workspace_root: Option<String>,
    pub cwd_display: String,
    pub git_branch: Option<String>,
    pub source_updated_at: i64,
    pub usage_count: i64,
    pub last_usage: Option<i64>,
    pub rollout_summary_path: String,
    pub prompt_entry: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoriesCurrentContext {
    pub platform_family: MemoryPlatformFamily,
    pub shell_style: Option<MemoryShellStyle>,
    pub shell_program: Option<String>,
    pub workspace_root: Option<String>,
    pub git_branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryPromptSelection {
    pub summary_text: String,
    pub selected_epoch_ids: Vec<MemoryEpochId>,
}

pub(crate) fn current_context_from_runtime(
    snapshot: Option<&EnvironmentContextSnapshot>,
    fallback_shell: &Shell,
    fallback_cwd: &Path,
) -> MemoriesCurrentContext {
    let fallback_shell_style = fallback_shell
        .script_style()
        .map(memory_shell_style_from_script_style);
    let fallback_shell_program = fallback_shell.name();
    let fallback_workspace_root = get_git_repo_root(fallback_cwd)
        .map(|path| path.display().to_string());
    let fallback_platform_family = current_platform_family();

    let platform_family = snapshot
        .and_then(|snap| snap.operating_system.as_ref())
        .and_then(|os| os.family.as_deref())
        .map(memory_platform_family_from_os_family)
        .unwrap_or(fallback_platform_family);
    let shell_style = snapshot
        .and_then(|snap| snap.shell.as_ref())
        .and_then(|shell| shell.script_style())
        .map(memory_shell_style_from_script_style)
        .or(fallback_shell_style);
    let shell_program = snapshot
        .and_then(|snap| snap.shell.as_ref())
        .and_then(Shell::name)
        .or(fallback_shell_program);
    let workspace_root = snapshot
        .and_then(|snap| snap.git_project_root.clone())
        .or(fallback_workspace_root);
    let git_branch = snapshot.and_then(|snap| snap.git_branch.clone());

    MemoriesCurrentContext {
        platform_family,
        shell_style,
        shell_program,
        workspace_root,
        git_branch,
    }
}

pub(crate) fn select_prompt_entries(
    manifest: &SnapshotManifest,
    context: &MemoriesCurrentContext,
    max_bytes: usize,
) -> Option<MemoryPromptSelection> {
    let target_shell = context.shell_style?;
    let mut compatible: Vec<(bool, u8, u8, u8, u8, &SnapshotEpochManifestEntry)> = manifest
        .epochs
        .iter()
        .filter_map(|entry| {
            let platform_rank = platform_compatibility_rank(context.platform_family, entry.platform_family)?;
            let shell_rank = shell_compatibility_rank(target_shell, entry.shell_style)?;
            let same_workspace = workspace_match(&context.workspace_root, &entry.workspace_root);
            Some((
                same_workspace,
                branch_affinity_rank(&context.git_branch, &entry.git_branch, same_workspace),
                platform_rank,
                shell_rank,
                provenance_rank(entry.provenance),
                entry,
            ))
        })
        .collect();

    if compatible.is_empty() {
        return None;
    }

    compatible.sort_by_key(
        |(same_workspace, branch_rank, platform_rank, shell_rank, provenance_rank, entry)| {
        (
            !same_workspace,
            *branch_rank,
            *platform_rank,
            *shell_rank,
            *provenance_rank,
            Reverse(entry.usage_count),
            Reverse(entry.last_usage.unwrap_or(i64::MIN).max(entry.source_updated_at)),
            Reverse(entry.source_updated_at),
            entry.id.thread_id,
            entry.id.epoch_index,
        )
        },
    );

    let mut selected_epoch_ids = Vec::new();
    let mut summary_text = String::new();
    for (_, _, _, _, _, entry) in compatible {
        let chunk = if summary_text.is_empty() {
            entry.prompt_entry.clone()
        } else {
            format!("\n\n{}", entry.prompt_entry)
        };
        let would_fit = summary_text.len().saturating_add(chunk.len()) <= max_bytes;
        if would_fit || selected_epoch_ids.is_empty() {
            // Keep rank order strict once the prompt budget is exceeded. The
            // top-ranked entry always lands, even if it alone exceeds budget.
            summary_text.push_str(&chunk);
            selected_epoch_ids.push(entry.id);
        } else {
            break;
        }
    }

    (!selected_epoch_ids.is_empty()).then_some(MemoryPromptSelection {
        summary_text,
        selected_epoch_ids,
    })
}

fn workspace_match(current: &Option<String>, candidate: &Option<String>) -> bool {
    match (current, candidate) {
        (Some(current), Some(candidate)) => current == candidate,
        // Treat missing roots as neutral fallback rather than a positive match.
        _ => false,
    }
}

fn branch_affinity_rank(
    current: &Option<String>,
    candidate: &Option<String>,
    same_workspace: bool,
) -> u8 {
    if !same_workspace {
        return 1;
    }
    match (current, candidate) {
        (Some(current), Some(candidate)) if current == candidate => 0,
        (Some(_), Some(_)) => 2,
        _ => 1,
    }
}

fn provenance_rank(provenance: Stage1EpochProvenance) -> u8 {
    match provenance {
        Stage1EpochProvenance::Derived => 0,
        Stage1EpochProvenance::CatalogFallback => 1,
        Stage1EpochProvenance::EmptyDerivationFallback => 2,
    }
}

fn platform_compatibility_rank(
    current: MemoryPlatformFamily,
    candidate: MemoryPlatformFamily,
) -> Option<u8> {
    match (current, candidate) {
        (MemoryPlatformFamily::Unix, MemoryPlatformFamily::Unix)
        | (MemoryPlatformFamily::Windows, MemoryPlatformFamily::Windows)
        | (MemoryPlatformFamily::Unknown, MemoryPlatformFamily::Unknown) => Some(0),
        (_, MemoryPlatformFamily::Unknown) => Some(1),
        (MemoryPlatformFamily::Unknown, _) => Some(1),
        _ => None,
    }
}

fn shell_compatibility_rank(
    current: MemoryShellStyle,
    candidate: MemoryShellStyle,
) -> Option<u8> {
    use MemoryShellStyle::{BashZshCompatible, Cmd, Elvish, Nushell, PosixSh, PowerShell, Unknown, Zsh};

    let ordered = match current {
        Zsh => &[Zsh, BashZshCompatible, PosixSh, Unknown][..],
        BashZshCompatible => &[BashZshCompatible, PosixSh, Unknown][..],
        PosixSh => &[PosixSh, Unknown][..],
        PowerShell => &[PowerShell, Unknown][..],
        Cmd => &[Cmd, Unknown][..],
        Nushell => &[Nushell, Unknown][..],
        Elvish => &[Elvish, Unknown][..],
        Unknown => &[Unknown][..],
    };
    ordered.iter().position(|style| *style == candidate).map(|idx| idx as u8)
}

fn current_platform_family() -> MemoryPlatformFamily {
    if cfg!(windows) {
        MemoryPlatformFamily::Windows
    } else {
        MemoryPlatformFamily::Unix
    }
}

pub(crate) fn memory_platform_family_from_os_family(family: &str) -> MemoryPlatformFamily {
    if family.eq_ignore_ascii_case("windows") {
        MemoryPlatformFamily::Windows
    } else if family.trim().is_empty() {
        MemoryPlatformFamily::Unknown
    } else {
        MemoryPlatformFamily::Unix
    }
}

pub(crate) fn memory_shell_style_from_script_style(style: ShellScriptStyle) -> MemoryShellStyle {
    match style {
        ShellScriptStyle::PosixSh => MemoryShellStyle::PosixSh,
        ShellScriptStyle::BashZshCompatible => MemoryShellStyle::BashZshCompatible,
        ShellScriptStyle::Zsh => MemoryShellStyle::Zsh,
        ShellScriptStyle::PowerShell => MemoryShellStyle::PowerShell,
        ShellScriptStyle::Cmd => MemoryShellStyle::Cmd,
        ShellScriptStyle::Nushell => MemoryShellStyle::Nushell,
        ShellScriptStyle::Elvish => MemoryShellStyle::Elvish,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn entry(
        shell_style: MemoryShellStyle,
        workspace_root: Option<&str>,
        git_branch: Option<&str>,
        provenance: Stage1EpochProvenance,
        usage_count: i64,
        last_usage: Option<i64>,
        source_updated_at: i64,
    ) -> SnapshotEpochManifestEntry {
        SnapshotEpochManifestEntry {
            id: MemoryEpochId {
                thread_id: Uuid::nil(),
                epoch_index: usage_count,
            },
            provenance,
            platform_family: MemoryPlatformFamily::Unix,
            shell_style,
            shell_program: None,
            workspace_root: workspace_root.map(ToString::to_string),
            cwd_display: "~/workspace".to_string(),
            git_branch: git_branch.map(ToString::to_string),
            source_updated_at,
            usage_count,
            last_usage,
            rollout_summary_path: format!("rollout_summaries/{usage_count}.md"),
            prompt_entry: format!("entry-{usage_count}"),
        }
    }

    #[test]
    fn zsh_prefers_zsh_then_bash_then_posix() {
        let manifest = SnapshotManifest::new(vec![
            entry(
                MemoryShellStyle::PosixSh,
                None,
                Some("main"),
                Stage1EpochProvenance::Derived,
                1,
                None,
                10,
            ),
            entry(
                MemoryShellStyle::BashZshCompatible,
                None,
                Some("main"),
                Stage1EpochProvenance::Derived,
                2,
                None,
                10,
            ),
            entry(
                MemoryShellStyle::Zsh,
                None,
                Some("main"),
                Stage1EpochProvenance::Derived,
                3,
                None,
                10,
            ),
        ]);
        let context = MemoriesCurrentContext {
            platform_family: MemoryPlatformFamily::Unix,
            shell_style: Some(MemoryShellStyle::Zsh),
            shell_program: Some("zsh".to_string()),
            workspace_root: None,
            git_branch: Some("main".to_string()),
        };
        let selection = select_prompt_entries(&manifest, &context, 1024).expect("selection");
        assert_eq!(selection.selected_epoch_ids[0].epoch_index, 3);
        assert_eq!(selection.selected_epoch_ids[1].epoch_index, 2);
        assert_eq!(selection.selected_epoch_ids[2].epoch_index, 1);
    }

    #[test]
    fn workspace_match_outranks_usage() {
        let manifest = SnapshotManifest::new(vec![
            entry(
                MemoryShellStyle::Zsh,
                Some("/other"),
                Some("main"),
                Stage1EpochProvenance::Derived,
                10,
                Some(100),
                100,
            ),
            entry(
                MemoryShellStyle::Zsh,
                Some("/repo"),
                Some("main"),
                Stage1EpochProvenance::Derived,
                1,
                Some(1),
                1,
            ),
        ]);
        let context = MemoriesCurrentContext {
            platform_family: MemoryPlatformFamily::Unix,
            shell_style: Some(MemoryShellStyle::Zsh),
            shell_program: Some("zsh".to_string()),
            workspace_root: Some("/repo".to_string()),
            git_branch: Some("main".to_string()),
        };
        let selection = select_prompt_entries(&manifest, &context, 1024).expect("selection");
        assert_eq!(selection.selected_epoch_ids[0].epoch_index, 1);
    }

    #[test]
    fn same_workspace_same_branch_outranks_same_workspace_different_branch() {
        let manifest = SnapshotManifest::new(vec![
            entry(
                MemoryShellStyle::Zsh,
                Some("/repo"),
                Some("feature"),
                Stage1EpochProvenance::Derived,
                10,
                Some(100),
                100,
            ),
            entry(
                MemoryShellStyle::Zsh,
                Some("/repo"),
                Some("main"),
                Stage1EpochProvenance::Derived,
                1,
                Some(1),
                1,
            ),
        ]);
        let context = MemoriesCurrentContext {
            platform_family: MemoryPlatformFamily::Unix,
            shell_style: Some(MemoryShellStyle::Zsh),
            shell_program: Some("zsh".to_string()),
            workspace_root: Some("/repo".to_string()),
            git_branch: Some("main".to_string()),
        };

        let selection = select_prompt_entries(&manifest, &context, 1024).expect("selection");
        assert_eq!(selection.selected_epoch_ids[0].epoch_index, 1);
    }

    #[test]
    fn derived_outranks_fallback_after_workspace_and_branch() {
        let manifest = SnapshotManifest::new(vec![
            entry(
                MemoryShellStyle::Zsh,
                Some("/repo"),
                Some("main"),
                Stage1EpochProvenance::CatalogFallback,
                10,
                Some(100),
                100,
            ),
            entry(
                MemoryShellStyle::Zsh,
                Some("/repo"),
                Some("main"),
                Stage1EpochProvenance::Derived,
                1,
                Some(1),
                1,
            ),
        ]);
        let context = MemoriesCurrentContext {
            platform_family: MemoryPlatformFamily::Unix,
            shell_style: Some(MemoryShellStyle::Zsh),
            shell_program: Some("zsh".to_string()),
            workspace_root: Some("/repo".to_string()),
            git_branch: Some("main".to_string()),
        };

        let selection = select_prompt_entries(&manifest, &context, 1024).expect("selection");
        assert_eq!(selection.selected_epoch_ids[0].epoch_index, 1);
    }

    #[test]
    fn exact_platform_match_outranks_unknown_platform_fallback() {
        let mut exact = entry(
            MemoryShellStyle::BashZshCompatible,
            None,
            Some("main"),
            Stage1EpochProvenance::Derived,
            1,
            Some(1),
            1,
        );
        exact.platform_family = MemoryPlatformFamily::Unix;
        let mut unknown = entry(
            MemoryShellStyle::Zsh,
            None,
            Some("main"),
            Stage1EpochProvenance::Derived,
            2,
            Some(100),
            100,
        );
        unknown.platform_family = MemoryPlatformFamily::Unknown;
        let manifest = SnapshotManifest::new(vec![unknown, exact]);
        let context = MemoriesCurrentContext {
            platform_family: MemoryPlatformFamily::Unix,
            shell_style: Some(MemoryShellStyle::Zsh),
            shell_program: Some("zsh".to_string()),
            workspace_root: None,
            git_branch: Some("main".to_string()),
        };

        let selection = select_prompt_entries(&manifest, &context, 1024).expect("selection");
        assert_eq!(selection.selected_epoch_ids[0].epoch_index, 1);
        assert_eq!(selection.selected_epoch_ids[1].epoch_index, 2);
    }

    #[test]
    fn oversized_top_ranked_entry_is_still_selected() {
        let mut oversized = entry(
            MemoryShellStyle::Zsh,
            None,
            Some("main"),
            Stage1EpochProvenance::Derived,
            1,
            Some(1),
            1,
        );
        oversized.prompt_entry = "x".repeat(64);
        let manifest = SnapshotManifest::new(vec![oversized]);
        let context = MemoriesCurrentContext {
            platform_family: MemoryPlatformFamily::Unix,
            shell_style: Some(MemoryShellStyle::Zsh),
            shell_program: Some("zsh".to_string()),
            workspace_root: None,
            git_branch: Some("main".to_string()),
        };

        let selection = select_prompt_entries(&manifest, &context, 8).expect("selection");
        assert_eq!(selection.selected_epoch_ids.len(), 1);
        assert_eq!(selection.selected_epoch_ids[0].epoch_index, 1);
    }

    #[test]
    fn prompt_budget_breaks_after_first_overflow_in_rank_order() {
        let mut first = entry(
            MemoryShellStyle::Zsh,
            None,
            Some("main"),
            Stage1EpochProvenance::Derived,
            3,
            Some(30),
            30,
        );
        first.prompt_entry = "first".to_string();
        let mut second = entry(
            MemoryShellStyle::Zsh,
            None,
            Some("main"),
            Stage1EpochProvenance::Derived,
            2,
            Some(20),
            20,
        );
        second.prompt_entry = "x".repeat(64);
        let mut third = entry(
            MemoryShellStyle::Zsh,
            None,
            Some("main"),
            Stage1EpochProvenance::Derived,
            1,
            Some(10),
            10,
        );
        third.prompt_entry = "third".to_string();
        let manifest = SnapshotManifest::new(vec![third, second, first]);
        let context = MemoriesCurrentContext {
            platform_family: MemoryPlatformFamily::Unix,
            shell_style: Some(MemoryShellStyle::Zsh),
            shell_program: Some("zsh".to_string()),
            workspace_root: None,
            git_branch: Some("main".to_string()),
        };

        let selection = select_prompt_entries(&manifest, &context, 24).expect("selection");
        assert_eq!(
            selection.selected_epoch_ids,
            vec![MemoryEpochId {
                thread_id: Uuid::nil(),
                epoch_index: 3,
            }]
        );
        assert_eq!(selection.summary_text, "first");
    }
}
