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
use std::path::Path;
use std::path::PathBuf;

pub(crate) struct ReadFileToolHandler;

const MAX_LINE_LENGTH: usize = 500;
const TAB_WIDTH: usize = 4;

// Best-effort comment/header detection.
const COMMENT_PREFIXES: &[&str] = &["#", "//", "--"];

#[derive(Deserialize)]
struct ReadFileArgs {
    file_path: String,
    #[serde(default = "defaults::offset")]
    offset: usize,
    #[serde(default = "defaults::limit")]
    limit: usize,
    #[serde(default)]
    mode: ReadMode,
    #[serde(default)]
    indentation: Option<IndentationArgs>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum ReadMode {
    #[default]
    Slice,
    Indentation,
}

#[derive(Deserialize, Clone)]
struct IndentationArgs {
    #[serde(default)]
    anchor_line: Option<usize>,
    #[serde(default = "defaults::max_levels")]
    max_levels: usize,
    #[serde(default = "defaults::include_siblings")]
    include_siblings: bool,
    #[serde(default = "defaults::include_header")]
    include_header: bool,
    #[serde(default)]
    max_lines: Option<usize>,
}

#[derive(Clone, Debug)]
struct LineRecord {
    number: usize,
    raw: String,
    display: String,
    indent: usize,
}

impl LineRecord {
    fn trimmed(&self) -> &str {
        self.raw.trim_start()
    }

    fn is_blank(&self) -> bool {
        self.trimmed().is_empty()
    }

    fn is_comment(&self) -> bool {
        COMMENT_PREFIXES
            .iter()
            .any(|prefix| self.raw.trim().starts_with(prefix))
    }
}

#[async_trait]
impl ToolHandler for ReadFileToolHandler {
    fn scheduling_hints(&self) -> crate::tools::registry::ToolSchedulingHints {
        crate::tools::registry::ToolSchedulingHints::pure_parallel()
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
            crate::openai_tools::READ_FILE_TOOL_NAME.to_string(),
            params_for_event,
            move || async move {
                let args: ReadFileArgs = match serde_json::from_str(&arguments) {
                    Ok(args) => args,
                    Err(err) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "invalid read_file arguments: {err}"
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
                                "offset must be a 1-indexed line number".to_string(),
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

                let path = resolve_path(&cwd, &args.file_path);
                let collected = match args.mode {
                    ReadMode::Slice => slice::read(&path, args.offset, args.limit).await,
                    ReadMode::Indentation => {
                        let indentation = args.indentation.unwrap_or_default();
                        indentation::read_block(&path, args.offset, args.limit, indentation).await
                    }
                };

                match collected {
                    Ok(lines) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(lines.join("\n")),
                            success: Some(true),
                        },
                    },
                    Err(err) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(err),
                            success: Some(false),
                        },
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

fn format_line(bytes: &[u8]) -> String {
    let decoded = String::from_utf8_lossy(bytes);
    truncate_utf8_prefix_by_bytes(&decoded, MAX_LINE_LENGTH)
}

fn trim_empty_lines(out: &mut VecDeque<&LineRecord>) {
    while matches!(out.front(), Some(line) if line.raw.trim().is_empty()) {
        out.pop_front();
    }
    while matches!(out.back(), Some(line) if line.raw.trim().is_empty()) {
        out.pop_back();
    }
}

mod slice {
    use super::format_line;
    use std::path::Path;
    use tokio::fs::File;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::BufReader;

    pub(super) async fn read(path: &Path, offset: usize, limit: usize) -> Result<Vec<String>, String> {
        let file = File::open(path)
            .await
            .map_err(|err| format!("failed to read file: {err}"))?;

        let mut reader = BufReader::new(file);
        let mut collected = Vec::new();
        let mut seen = 0usize;
        let mut buffer = Vec::new();

        loop {
            buffer.clear();
            let bytes_read = reader
                .read_until(b'\n', &mut buffer)
                .await
                .map_err(|err| format!("failed to read file: {err}"))?;

            if bytes_read == 0 {
                break;
            }

            if buffer.last() == Some(&b'\n') {
                buffer.pop();
                if buffer.last() == Some(&b'\r') {
                    buffer.pop();
                }
            }

            seen += 1;

            if seen < offset {
                continue;
            }

            let formatted = format_line(&buffer);
            collected.push(format!("L{seen}: {formatted}"));

            if collected.len() == limit {
                break;
            }
        }

        if seen < offset {
            return Err("offset exceeds file length".to_string());
        }

        Ok(collected)
    }
}

mod indentation {
    use super::format_line;
    use super::trim_empty_lines;
    use super::IndentationArgs;
    use super::LineRecord;
    use super::TAB_WIDTH;
    use std::collections::VecDeque;
    use std::path::Path;
    use tokio::fs::File;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::BufReader;

    pub(super) async fn read_block(
        path: &Path,
        offset: usize,
        limit: usize,
        options: IndentationArgs,
    ) -> Result<Vec<String>, String> {
        let anchor_line = options.anchor_line.unwrap_or(offset);
        if anchor_line == 0 {
            return Err("anchor_line must be a 1-indexed line number".to_string());
        }

        let guard_limit = options.max_lines.unwrap_or(limit);
        if guard_limit == 0 {
            return Err("max_lines must be greater than zero".to_string());
        }

        let collected = collect_file_lines(path).await?;
        if collected.is_empty() || anchor_line > collected.len() {
            return Err("anchor_line exceeds file length".to_string());
        }

        let anchor_index = anchor_line - 1;
        let effective_indents = compute_effective_indents(&collected);
        let anchor_indent = effective_indents[anchor_index];

        let min_indent = if options.max_levels == 0 {
            0
        } else {
            anchor_indent.saturating_sub(options.max_levels * TAB_WIDTH)
        };

        let final_limit = limit.min(guard_limit).min(collected.len());
        if final_limit == 1 {
            return Ok(vec![format!(
                "L{}: {}",
                collected[anchor_index].number, collected[anchor_index].display
            )]);
        }

        let mut i: isize = anchor_index as isize - 1;
        let mut j: usize = anchor_index + 1;
        let mut i_counter_min_indent = 0;
        let mut j_counter_min_indent = 0;

        let mut out = VecDeque::with_capacity(limit);
        out.push_back(&collected[anchor_index]);

        while out.len() < final_limit {
            let mut progressed = 0;

            if i >= 0 {
                let iu = i as usize;
                if effective_indents[iu] >= min_indent {
                    out.push_front(&collected[iu]);
                    progressed += 1;
                    i -= 1;

                    if effective_indents[iu] == min_indent && !options.include_siblings {
                        let allow_header_comment =
                            options.include_header && collected[iu].is_comment();
                        let can_take_line = allow_header_comment || i_counter_min_indent == 0;

                        if can_take_line {
                            i_counter_min_indent += 1;
                        } else {
                            out.pop_front();
                            progressed -= 1;
                            i = -1;
                        }
                    }

                    if out.len() >= final_limit {
                        break;
                    }
                } else {
                    i = -1;
                }
            }

            if j < collected.len() {
                let ju = j;
                if effective_indents[ju] >= min_indent {
                    out.push_back(&collected[ju]);
                    progressed += 1;
                    j += 1;

                    if effective_indents[ju] == min_indent && !options.include_siblings {
                        if j_counter_min_indent > 0 {
                            out.pop_back();
                            progressed -= 1;
                            j = collected.len();
                        }
                        j_counter_min_indent += 1;
                    }
                } else {
                    j = collected.len();
                }
            }

            if progressed == 0 {
                break;
            }
        }

        trim_empty_lines(&mut out);

        Ok(out
            .into_iter()
            .map(|record| format!("L{}: {}", record.number, record.display))
            .collect())
    }

    async fn collect_file_lines(path: &Path) -> Result<Vec<LineRecord>, String> {
        let file = File::open(path)
            .await
            .map_err(|err| format!("failed to read file: {err}"))?;

        let mut reader = BufReader::new(file);
        let mut buffer = Vec::new();
        let mut lines = Vec::new();
        let mut number = 0usize;

        loop {
            buffer.clear();
            let bytes_read = reader
                .read_until(b'\n', &mut buffer)
                .await
                .map_err(|err| format!("failed to read file: {err}"))?;

            if bytes_read == 0 {
                break;
            }

            if buffer.last() == Some(&b'\n') {
                buffer.pop();
                if buffer.last() == Some(&b'\r') {
                    buffer.pop();
                }
            }

            number += 1;
            let raw = String::from_utf8_lossy(&buffer).into_owned();
            let indent = measure_indent(&raw);
            let display = format_line(&buffer);
            lines.push(LineRecord {
                number,
                raw,
                display,
                indent,
            });
        }

        Ok(lines)
    }

    fn compute_effective_indents(records: &[LineRecord]) -> Vec<usize> {
        let mut effective = Vec::with_capacity(records.len());
        let mut previous_indent = 0usize;
        for record in records {
            if record.is_blank() {
                effective.push(previous_indent);
            } else {
                previous_indent = record.indent;
                effective.push(previous_indent);
            }
        }
        effective
    }

    fn measure_indent(line: &str) -> usize {
        line.chars()
            .take_while(|c| matches!(c, ' ' | '\t'))
            .map(|c| if c == '\t' { TAB_WIDTH } else { 1 })
            .sum()
    }
}

mod defaults {
    use super::*;

    impl Default for IndentationArgs {
        fn default() -> Self {
            Self {
                anchor_line: None,
                max_levels: max_levels(),
                include_siblings: include_siblings(),
                include_header: include_header(),
                max_lines: None,
            }
        }
    }

    pub(crate) fn offset() -> usize {
        1
    }

    pub(crate) fn limit() -> usize {
        2000
    }

    pub(crate) fn max_levels() -> usize {
        0
    }

    pub(crate) fn include_siblings() -> bool {
        false
    }

    pub(crate) fn include_header() -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::indentation::read_block;
    use super::slice::read;
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn reads_requested_range() -> anyhow::Result<()> {
        let mut temp = NamedTempFile::new()?;
        use std::io::Write as _;
        write!(
            temp,
            "alpha
beta
gamma
"
        )?;

        let lines = read(temp.path(), 2, 2).await.map_err(anyhow::Error::msg)?;
        assert_eq!(lines, vec!["L2: beta".to_string(), "L3: gamma".to_string()]);
        Ok(())
    }

    #[tokio::test]
    async fn errors_when_offset_exceeds_length() -> anyhow::Result<()> {
        let mut temp = NamedTempFile::new()?;
        use std::io::Write as _;
        writeln!(temp, "only")?;

        let err = read(temp.path(), 3, 1).await.expect_err("offset exceeds length");
        assert_eq!(err, "offset exceeds file length".to_string());
        Ok(())
    }

    #[tokio::test]
    async fn reads_indentation_block_around_anchor() -> anyhow::Result<()> {
        let mut temp = NamedTempFile::new()?;
        use std::io::Write as _;
        write!(
            temp,
            r#"
fn outer() {{
    let a = 1;
    if a > 0 {{
        let b = 2;
    }}
}}
"#
        )?;

        let lines = read_block(
            temp.path(),
            4,
            100,
            IndentationArgs {
                anchor_line: Some(5),
                max_levels: 0,
                include_siblings: false,
                include_header: true,
                max_lines: None,
            },
        )
        .await
        .map_err(anyhow::Error::msg)?;

        assert!(lines.iter().any(|line| line.contains("fn outer")));
        assert!(lines.iter().any(|line| line.contains("let b")));
        Ok(())
    }
}
