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
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

pub(crate) struct GrepFilesToolHandler;

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 2000;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

#[derive(Deserialize)]
struct GrepFilesArgs {
    pattern: String,
    #[serde(default)]
    include: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[async_trait]
impl ToolHandler for GrepFilesToolHandler {
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
            crate::openai_tools::GREP_FILES_TOOL_NAME.to_string(),
            params_for_event,
            move || async move {
                let args: GrepFilesArgs = match serde_json::from_str(&arguments) {
                    Ok(args) => args,
                    Err(err) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "invalid grep_files arguments: {err}"
                                )),
                                success: Some(false),
                            },
                        };
                    }
                };

                let pattern = args.pattern.trim();
                if pattern.is_empty() {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "pattern must not be empty".to_string(),
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

                let limit = args.limit.min(MAX_LIMIT);

                let search_path = resolve_path(&cwd, args.path.as_deref());
                if let Err(err) = verify_path_exists(&search_path).await {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(err),
                            success: Some(false),
                        },
                    };
                }

                let include = args
                    .include
                    .as_deref()
                    .map(str::trim)
                    .and_then(|val| (!val.is_empty()).then(|| val.to_string()));

                let search_results =
                    match run_rg_search(pattern, include.as_deref(), &search_path, limit, &cwd)
                        .await
                    {
                        Ok(results) => results,
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

                if search_results.is_empty() {
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text("No matches found.".to_string()),
                            success: Some(false),
                        },
                    }
                } else {
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(search_results.join("\n")),
                            success: Some(true),
                        },
                    }
                }
            },
        )
        .await
    }
}

fn resolve_path(cwd: &Path, path: Option<&str>) -> PathBuf {
    match path.map(str::trim).filter(|p| !p.is_empty()) {
        Some(path) => {
            let p = PathBuf::from(path);
            if p.is_absolute() {
                p
            } else {
                cwd.join(p)
            }
        }
        None => cwd.to_path_buf(),
    }
}

async fn verify_path_exists(path: &Path) -> Result<(), String> {
    tokio::fs::metadata(path)
        .await
        .map_err(|err| format!("unable to access `{}`: {err}", path.display()))?;
    Ok(())
}

async fn run_rg_search(
    pattern: &str,
    include: Option<&str>,
    search_path: &Path,
    limit: usize,
    cwd: &Path,
) -> Result<Vec<String>, String> {
    let mut command = Command::new("rg");
    command
        .current_dir(cwd)
        .arg("--files-with-matches")
        .arg("--sortr=modified")
        .arg("--regexp")
        .arg(pattern)
        .arg("--no-messages");

    if let Some(glob) = include {
        command.arg("--glob").arg(glob);
    }

    command.arg("--").arg(search_path);

    let output = timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| "rg timed out after 30 seconds".to_string())?
        .map_err(|err| {
            format!(
                "failed to launch rg: {err}. Ensure ripgrep is installed and on PATH."
            )
        })?;

    match output.status.code() {
        Some(0) => Ok(parse_results(&output.stdout, limit)),
        Some(1) => Ok(Vec::new()),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("rg failed: {stderr}"))
        }
    }
}

fn parse_results(stdout: &[u8], limit: usize) -> Vec<String> {
    let mut results = Vec::new();
    for line in stdout.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        if let Ok(text) = std::str::from_utf8(line) {
            if text.is_empty() {
                continue;
            }
            results.push(text.to_string());
            if results.len() == limit {
                break;
            }
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_basic_results() {
        let stdout = b"/tmp/file_a.rs\n/tmp/file_b.rs\n";
        let parsed = parse_results(stdout, 10);
        assert_eq!(
            parsed,
            vec!["/tmp/file_a.rs".to_string(), "/tmp/file_b.rs".to_string()]
        );
    }

    #[test]
    fn parse_truncates_after_limit() {
        let stdout = b"/tmp/file_a.rs\n/tmp/file_b.rs\n/tmp/file_c.rs\n";
        let parsed = parse_results(stdout, 2);
        assert_eq!(
            parsed,
            vec!["/tmp/file_a.rs".to_string(), "/tmp/file_b.rs".to_string()]
        );
    }
}

