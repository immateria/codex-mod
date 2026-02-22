use crate::codex::Session;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::execute_custom_tool;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::unsupported_tool_call_output;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde::Deserialize;
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::FileType;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;

pub(crate) struct ListDirToolHandler;

const MAX_ENTRY_LENGTH: usize = 500;
const INDENTATION_SPACES: usize = 2;

fn default_offset() -> usize {
    1
}

fn default_limit() -> usize {
    25
}

fn default_depth() -> usize {
    2
}

#[derive(Deserialize)]
struct ListDirArgs {
    dir_path: String,
    #[serde(default = "default_offset")]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_depth")]
    depth: usize,
}

#[async_trait]
impl ToolHandler for ListDirToolHandler {
    fn is_parallel_safe(&self) -> bool {
        true
    }

    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Function { arguments } = &inv.payload else {
            return unsupported_tool_call_output(
                &inv.ctx.call_id,
                inv.payload.outputs_custom(),
                format!("{} expects function-call arguments", inv.tool_name),
            );
        };

        let params_for_event = serde_json::from_str::<serde_json::Value>(arguments).ok();
        let arguments = arguments.clone();
        let ctx = inv.ctx.clone();
        let call_id = ctx.call_id.clone();
        let cwd = sess.get_cwd().to_path_buf();

        execute_custom_tool(
            sess,
            &ctx,
            crate::openai_tools::LIST_DIR_TOOL_NAME.to_string(),
            params_for_event,
            move || async move {
                let args: ListDirArgs = match serde_json::from_str(&arguments) {
                    Ok(args) => args,
                    Err(err) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "invalid list_dir arguments: {err}"
                                )),
                                success: Some(false),
                            },
                        };
                    }
                };

                if args.offset == 0 {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "offset must be a 1-indexed entry number".to_string(),
                            ),
                            success: Some(false),
                        },
                    };
                }

                if args.limit == 0 {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "limit must be greater than zero".to_string(),
                            ),
                            success: Some(false),
                        },
                    };
                }

                if args.depth == 0 {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "depth must be greater than zero".to_string(),
                            ),
                            success: Some(false),
                        },
                    };
                }

                let path = resolve_path(&cwd, &args.dir_path);
                if let Err(err) = verify_dir_exists(&path).await {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(err),
                            success: Some(false),
                        },
                    };
                }

                let entries = match list_dir_slice(&path, args.offset, args.limit, args.depth).await
                {
                    Ok(entries) => entries,
                    Err(err) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(err),
                                success: Some(false),
                            },
                        };
                    }
                };

                let mut output = Vec::with_capacity(entries.len() + 1);
                output.push(format!("Absolute path: {}", path.display()));
                output.extend(entries);
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(output.join("\n")),
                        success: Some(true),
                    },
                }
            },
        )
        .await
    }
}

fn resolve_path(cwd: &Path, path: &str) -> PathBuf {
    let raw = path.trim();
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        p
    } else {
        cwd.join(p)
    }
}

async fn verify_dir_exists(path: &Path) -> Result<(), String> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|err| format!("unable to access `{}`: {err}", path.display()))?;
    if !meta.is_dir() {
        return Err(format!("`{}` is not a directory", path.display()));
    }
    Ok(())
}

async fn list_dir_slice(
    path: &Path,
    offset: usize,
    limit: usize,
    depth: usize,
) -> Result<Vec<String>, String> {
    let mut entries = Vec::new();
    collect_entries(path, Path::new(""), depth, &mut entries).await?;

    if entries.is_empty() {
        return Ok(Vec::new());
    }

    entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    let start_index = offset - 1;
    if start_index >= entries.len() {
        return Err("offset exceeds directory entry count".to_string());
    }

    let remaining_entries = entries.len() - start_index;
    let capped_limit = limit.min(remaining_entries);
    let end_index = start_index + capped_limit;
    let selected_entries = &entries[start_index..end_index];
    let width = end_index.to_string().len().max(1);
    let mut formatted = Vec::with_capacity(selected_entries.len() + 1);

    for (idx, entry) in selected_entries.iter().enumerate() {
        let display_idx = start_index + idx + 1;
        formatted.push(format!(
            "{display_idx:>width$}. {}",
            format_entry_line(entry)
        ));
    }

    if end_index < entries.len() {
        formatted.push(format!("More than {capped_limit} entries found"));
    }

    Ok(formatted)
}

async fn collect_entries(
    dir_path: &Path,
    relative_prefix: &Path,
    depth: usize,
    entries: &mut Vec<DirEntry>,
) -> Result<(), String> {
    let mut queue = VecDeque::new();
    queue.push_back((dir_path.to_path_buf(), relative_prefix.to_path_buf(), depth));

    while let Some((current_dir, prefix, remaining_depth)) = queue.pop_front() {
        let mut read_dir = fs::read_dir(&current_dir)
            .await
            .map_err(|err| format!("failed to read directory: {err}"))?;

        let mut dir_entries = Vec::new();

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|err| format!("failed to read directory: {err}"))?
        {
            let file_type = entry
                .file_type()
                .await
                .map_err(|err| format!("failed to inspect entry: {err}"))?;

            let file_name = entry.file_name();
            let relative_path = if prefix.as_os_str().is_empty() {
                PathBuf::from(&file_name)
            } else {
                prefix.join(&file_name)
            };

            let display_name = format_entry_component(&file_name);
            let display_depth = prefix.components().count();
            let sort_key = format_entry_name(&relative_path);
            let kind = DirEntryKind::from(&file_type);
            dir_entries.push((
                entry.path(),
                relative_path,
                kind,
                DirEntry {
                    name: sort_key,
                    display_name,
                    depth: display_depth,
                    kind,
                },
            ));
        }

        dir_entries.sort_unstable_by(|a, b| a.3.name.cmp(&b.3.name));

        for (entry_path, relative_path, kind, dir_entry) in dir_entries {
            if kind == DirEntryKind::Directory && remaining_depth > 1 {
                queue.push_back((entry_path, relative_path, remaining_depth - 1));
            }
            entries.push(dir_entry);
        }
    }

    Ok(())
}

fn truncate_utf8_prefix_by_bytes(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }
    if max_bytes == 0 {
        return String::new();
    }
    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    input[..end].to_string()
}

fn format_entry_name(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace("\\", "/");
    truncate_utf8_prefix_by_bytes(&normalized, MAX_ENTRY_LENGTH)
}

fn format_entry_component(name: &OsStr) -> String {
    let normalized = name.to_string_lossy();
    truncate_utf8_prefix_by_bytes(&normalized, MAX_ENTRY_LENGTH)
}

fn format_entry_line(entry: &DirEntry) -> String {
    let indent = " ".repeat(entry.depth * INDENTATION_SPACES);
    let mut name = entry.display_name.clone();
    match entry.kind {
        DirEntryKind::Directory => name.push('/'),
        DirEntryKind::Symlink => name.push('@'),
        DirEntryKind::Other => name.push('?'),
        DirEntryKind::File => {}
    }
    format!("{indent}{name}")
}

#[derive(Clone)]
struct DirEntry {
    name: String,
    display_name: String,
    depth: usize,
    kind: DirEntryKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DirEntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

impl From<&FileType> for DirEntryKind {
    fn from(file_type: &FileType) -> Self {
        if file_type.is_symlink() {
            DirEntryKind::Symlink
        } else if file_type.is_dir() {
            DirEntryKind::Directory
        } else if file_type.is_file() {
            DirEntryKind::File
        } else {
            DirEntryKind::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[tokio::test]
    async fn lists_directory_entries() {
        let temp = tempdir().expect("create tempdir");
        let dir_path = temp.path();

        let sub_dir = dir_path.join("nested");
        tokio::fs::create_dir(&sub_dir)
            .await
            .expect("create sub dir");

        let deeper_dir = sub_dir.join("deeper");
        tokio::fs::create_dir(&deeper_dir)
            .await
            .expect("create deeper dir");

        tokio::fs::write(dir_path.join("entry.txt"), b"content")
            .await
            .expect("write file");
        tokio::fs::write(sub_dir.join("child.txt"), b"child")
            .await
            .expect("write child");
        tokio::fs::write(deeper_dir.join("grandchild.txt"), b"grandchild")
            .await
            .expect("write grandchild");

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link_path = dir_path.join("link");
            symlink(dir_path.join("entry.txt"), &link_path).expect("create symlink");
        }

        let entries = list_dir_slice(dir_path, 1, 20, 3)
            .await
            .expect("list directory");

        #[cfg(unix)]
        let expected = vec![
            "1. entry.txt".to_string(),
            "2. link@".to_string(),
            "3. nested/".to_string(),
            "4.   child.txt".to_string(),
            "5.   deeper/".to_string(),
            "6.     grandchild.txt".to_string(),
        ];

        #[cfg(not(unix))]
        let expected = vec![
            "1. entry.txt".to_string(),
            "2. nested/".to_string(),
            "3.   child.txt".to_string(),
            "4.   deeper/".to_string(),
            "5.     grandchild.txt".to_string(),
        ];

        assert_eq!(entries, expected);
    }

    #[tokio::test]
    async fn errors_when_offset_exceeds_entries() {
        let temp = tempdir().expect("create tempdir");
        let dir_path = temp.path();
        tokio::fs::create_dir(dir_path.join("nested"))
            .await
            .expect("create sub dir");

        let err = list_dir_slice(dir_path, 10, 1, 2)
            .await
            .expect_err("offset exceeds entries");
        assert_eq!(err, "offset exceeds directory entry count".to_string());
    }

    #[tokio::test]
    async fn respects_depth_parameter() {
        let temp = tempdir().expect("create tempdir");
        let dir_path = temp.path();
        let nested = dir_path.join("nested");
        let deeper = nested.join("deeper");
        tokio::fs::create_dir(&nested).await.expect("create nested");
        tokio::fs::create_dir(&deeper).await.expect("create deeper");
        tokio::fs::write(dir_path.join("root.txt"), b"root")
            .await
            .expect("write root");
        tokio::fs::write(nested.join("child.txt"), b"child")
            .await
            .expect("write nested");
        tokio::fs::write(deeper.join("grandchild.txt"), b"deep")
            .await
            .expect("write deeper");

        let entries_depth_one = list_dir_slice(dir_path, 1, 10, 1)
            .await
            .expect("list depth 1");
        assert_eq!(
            entries_depth_one,
            vec!["1. nested/".to_string(), "2. root.txt".to_string(),]
        );

        let entries_depth_two = list_dir_slice(dir_path, 1, 20, 2)
            .await
            .expect("list depth 2");
        assert_eq!(
            entries_depth_two,
            vec![
                "1. nested/".to_string(),
                "2.   child.txt".to_string(),
                "3.   deeper/".to_string(),
                "4. root.txt".to_string(),
            ]
        );

        let entries_depth_three = list_dir_slice(dir_path, 1, 30, 3)
            .await
            .expect("list depth 3");
        assert_eq!(
            entries_depth_three,
            vec![
                "1. nested/".to_string(),
                "2.   child.txt".to_string(),
                "3.   deeper/".to_string(),
                "4.     grandchild.txt".to_string(),
                "5. root.txt".to_string(),
            ]
        );
    }
}
