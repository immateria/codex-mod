use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::openai_tools::OpenAiTool;
use crate::tools::router::ToolDispatchMeta;
use crate::tools::router::ToolRouter;
use crate::turn_diff_tracker::TurnDiffTracker;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStderr;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::sync::OnceCell;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::debug;

pub(crate) const JS_REPL_PRAGMA_PREFIX: &str = "// codex-js-repl:";

const KERNEL_SOURCE_NODE: &str = include_str!("kernel.js");
const KERNEL_SOURCE_DENO: &str = include_str!("kernel_deno.js");
const MERIYAH_UMD: &str = include_str!("meriyah.umd.min.js");

const DEFAULT_TIMEOUT_MS: u64 = 15_000;
const MAX_TIMEOUT_MS: u64 = 120_000;
const MIN_NODE_VERSION: (u64, u64, u64) = (18, 0, 0);

#[derive(Clone, Debug)]
pub(crate) struct JsReplRuntimeConfig {
    pub(crate) kind: crate::config::JsReplRuntimeKindToml,
    pub(crate) runtime_path: Option<PathBuf>,
    pub(crate) runtime_args: Vec<String>,
    pub(crate) node_module_dirs: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
struct ResolvedRuntime {
    kind: crate::config::JsReplRuntimeKindToml,
    executable: PathBuf,
    args: Vec<String>,
    version: String,
    node_module_dirs: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
struct ToolRequest {
    id: String,
    tool_name: String,
    arguments: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JsReplArgs {
    pub code: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsExecResult {
    pub output: String,
}

#[derive(Clone, Debug)]
pub(crate) struct JsExecError {
    pub output: String,
    pub error: String,
}

#[derive(Debug)]
struct ExecResultMessage {
    ok: bool,
    output: String,
    error: Option<String>,
}

pub(crate) struct JsReplHandle {
    runtime: JsReplRuntimeConfig,
    cell: OnceCell<Arc<JsReplManager>>,
}

impl JsReplHandle {
    pub(crate) fn new(runtime: JsReplRuntimeConfig) -> Self {
        Self {
            runtime,
            cell: OnceCell::new(),
        }
    }

    pub(crate) async fn manager(&self) -> Result<Arc<JsReplManager>, String> {
        self.cell
            .get_or_try_init(|| async {
                JsReplManager::new(self.runtime.clone()).await
            })
            .await
            .cloned()
    }

    pub(crate) fn manager_if_started(&self) -> Option<Arc<JsReplManager>> {
        self.cell.get().cloned()
    }
}

pub(crate) struct JsReplManager {
    runtime: ResolvedRuntime,
    tmp_dir: tempfile::TempDir,
    kernel_path: PathBuf,
    kernel: Mutex<Option<Kernel>>,
    exec_lock: Arc<Semaphore>,
    next_id: AtomicU64,
    next_tool_seq: AtomicU64,
}

struct Kernel {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    pending_execs: Arc<Mutex<HashMap<String, oneshot::Sender<ExecResultMessage>>>>,
    tool_requests: Arc<Mutex<mpsc::UnboundedReceiver<ToolRequest>>>,
    active_exec_id: Arc<Mutex<Option<String>>>,
    stdout_task: tokio::task::JoinHandle<()>,
    stderr_task: tokio::task::JoinHandle<()>,
}

impl JsReplManager {
    pub(crate) async fn new(runtime: JsReplRuntimeConfig) -> Result<Arc<Self>, String> {
        let runtime = resolve_runtime(runtime).await?;

        let tmp_dir = tempfile::tempdir()
            .map_err(|err| format!("failed to create js_repl temp dir: {err}"))?;

        let kernel_path = tmp_dir.path().join("kernel.js");
        let meriyah_path = tmp_dir.path().join("meriyah.umd.min.js");
        let kernel_source = match runtime.kind {
            crate::config::JsReplRuntimeKindToml::Node => KERNEL_SOURCE_NODE,
            crate::config::JsReplRuntimeKindToml::Deno => KERNEL_SOURCE_DENO,
        };
        tokio::fs::write(&kernel_path, kernel_source)
            .await
            .map_err(|err| format!("failed to write js_repl kernel: {err}"))?;
        tokio::fs::write(&meriyah_path, MERIYAH_UMD)
            .await
            .map_err(|err| format!("failed to write js_repl parser: {err}"))?;

        Ok(Arc::new(Self {
            runtime,
            tmp_dir,
            kernel_path,
            kernel: Mutex::new(None),
            exec_lock: Arc::new(Semaphore::new(1)),
            next_id: AtomicU64::new(0),
            next_tool_seq: AtomicU64::new(0),
        }))
    }

    pub(crate) async fn execute(
        &self,
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        parent_ctx: &ToolCallCtx,
        attempt_req: u64,
        cwd: &Path,
        args: JsReplArgs,
    ) -> Result<JsExecResult, JsExecError> {
        let _permit = match self.exec_lock.acquire().await {
            Ok(permit) => permit,
            Err(_) => {
                return Err(JsExecError {
                    output: String::new(),
                    error: "js_repl kernel is unavailable".to_string(),
                });
            }
        };

        let timeout_ms = args
            .timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);
        let timeout = Duration::from_millis(timeout_ms);

        let exec_id = self
            .next_id
            .fetch_add(1, Ordering::Relaxed);
        let id = format!("jsrepl-{exec_id}");

        let (tx, rx) = oneshot::channel();

        let (stdin, pending, tool_rx, active_exec_id) = {
            let mut guard = self.kernel.lock().await;
            if guard.is_none() {
                *guard = Some(
                    self.start_kernel(sess, parent_ctx, attempt_req, cwd)
                        .await
                        .map_err(|error| JsExecError {
                    output: String::new(),
                    error,
                })?,
                );
            }
            let Some(kernel) = guard.as_ref() else {
                return Err(JsExecError {
                    output: String::new(),
                    error: "js_repl kernel failed to start".to_string(),
                });
            };
            (
                Arc::clone(&kernel.stdin),
                Arc::clone(&kernel.pending_execs),
                Arc::clone(&kernel.tool_requests),
                Arc::clone(&kernel.active_exec_id),
            )
        };

        pending.lock().await.insert(id.clone(), tx);

        // Snapshot freeform tool names for this session so codex.tool can infer the correct payload.
        let freeform_tool_names = freeform_tool_name_snapshot(sess);

        // Mark this exec as active so background tool requests can be rejected once the run completes.
        {
            let mut active = active_exec_id.lock().await;
            *active = Some(id.clone());
        }

        let message = json!({
            "type": "exec",
            "id": id.clone(),
            "code": args.code,
        });

        if let Err(err) = send_json_line(&stdin, &message).await {
            pending.lock().await.remove(&id);
            {
                let mut active = active_exec_id.lock().await;
                *active = None;
            }
            let _ = self.reset().await;
            return Err(JsExecError {
                output: String::new(),
                error: format!("failed to send js_repl request: {err}"),
            });
        }

        let mut tool_rx = tool_rx.lock().await;
        let mut rx = rx;
        let timeout_sleep = tokio::time::sleep(timeout);
        tokio::pin!(timeout_sleep);

        let result: ExecResultMessage = loop {
            tokio::select! {
                _ = &mut timeout_sleep => {
                    pending.lock().await.remove(&id);
                    {
                        let mut active = active_exec_id.lock().await;
                        *active = None;
                    }
                    let _ = self.reset().await;
                    return Err(JsExecError {
                        output: String::new(),
                        error: format!("js_repl timed out after {timeout_ms}ms"),
                    });
                }
                tool_req = tool_rx.recv() => {
                    let Some(tool_req) = tool_req else {
                        pending.lock().await.remove(&id);
                        {
                            let mut active = active_exec_id.lock().await;
                            *active = None;
                        }
                        let _ = self.reset().await;
                        return Err(JsExecError {
                            output: String::new(),
                            error: "js_repl kernel terminated while waiting for tool requests".to_string(),
                        });
                    };
                    self.handle_tool_request(sess, turn_diff_tracker, parent_ctx, attempt_req, &freeform_tool_names, &stdin, &tool_req)
                        .await;
                }
                msg = &mut rx => {
                    match msg {
                        Ok(msg) => break msg,
                        Err(_) => {
                            {
                                let mut active = active_exec_id.lock().await;
                                *active = None;
                            }
                            let _ = self.reset().await;
                            return Err(JsExecError {
                                output: String::new(),
                                error: "js_repl kernel stopped before returning a result".to_string(),
                            });
                        }
                    }
                }
            }
        };

        {
            let mut active = active_exec_id.lock().await;
            *active = None;
        }

        if result.ok {
            Ok(JsExecResult {
                output: result.output,
            })
        } else {
            Err(JsExecError {
                output: result.output,
                error: result.error.unwrap_or_else(|| "js_repl failed".to_string()),
            })
        }
    }

    pub(crate) async fn reset(&self) -> Result<(), String> {
        self.kill_kernel().await;
        Ok(())
    }

    pub(crate) async fn kill(&self) {
        self.kill_kernel().await;
    }

    async fn kill_kernel(&self) {
        let mut guard = self.kernel.lock().await;
        let Some(mut kernel) = guard.take() else {
            return;
        };

        kernel.stdout_task.abort();
        kernel.stderr_task.abort();

        let pending = Arc::clone(&kernel.pending_execs);
        let mut pending = pending.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(ExecResultMessage {
                ok: false,
                output: String::new(),
                error: Some("js_repl kernel was reset".to_string()),
            });
        }
        drop(pending);

        if let Err(err) = kernel.child.kill().await {
            debug!("failed to kill js_repl kernel: {err}");
        }
        let _ = kernel.child.wait().await;
    }

    async fn start_kernel(
        &self,
        sess: &Session,
        parent_ctx: &ToolCallCtx,
        attempt_req: u64,
        cwd: &Path,
    ) -> Result<Kernel, String> {
        let mut command = self.build_runtime_command(sess, parent_ctx, attempt_req, cwd)?;
        let mut child = command
            .spawn()
            .map_err(|err| {
                format!(
                    "failed to spawn js_repl kernel (runtime={runtime:?} exe={exe}): {err}",
                    runtime = self.runtime.kind,
                    exe = self.runtime.executable.display(),
                )
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "js_repl kernel missing stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "js_repl kernel missing stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "js_repl kernel missing stderr".to_string())?;

        let stdin = Arc::new(Mutex::new(stdin));
        let pending_execs: Arc<Mutex<HashMap<String, oneshot::Sender<ExecResultMessage>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let (tool_tx, tool_rx) = mpsc::unbounded_channel::<ToolRequest>();
        let tool_requests = Arc::new(Mutex::new(tool_rx));
        let active_exec_id = Arc::new(Mutex::new(None::<String>));

        let stdout_task = tokio::spawn(kernel_stdout_loop(
            stdout,
            Arc::clone(&pending_execs),
            Arc::clone(&stdin),
            tool_tx,
            Arc::clone(&active_exec_id),
        ));
        let stderr_task = tokio::spawn(kernel_stderr_loop(stderr));

        Ok(Kernel {
            child,
            stdin,
            pending_execs,
            tool_requests,
            active_exec_id,
            stdout_task,
            stderr_task,
        })
    }

    fn build_runtime_command(
        &self,
        sess: &Session,
        _parent_ctx: &ToolCallCtx,
        _attempt_req: u64,
        cwd: &Path,
    ) -> Result<Command, String> {
        let sandbox_policy = sess.get_sandbox_policy();
        let sandbox_policy_cwd = sess.get_cwd();
        let enforce_managed_network = sess.managed_network_proxy().is_some();

        let mut env_overrides = HashMap::<String, String>::new();
        if let Some(proxy) = sess.managed_network_proxy() {
            proxy.apply_to_env(&mut env_overrides);
        }

        let seatbelt_enabled = cfg!(target_os = "macos")
            && matches!(self.runtime.kind, crate::config::JsReplRuntimeKindToml::Node)
            && !matches!(
                sandbox_policy,
                crate::protocol::SandboxPolicy::DangerFullAccess
            );

        let mut command = match self.runtime.kind {
            crate::config::JsReplRuntimeKindToml::Node => {
                if seatbelt_enabled {
                    if enforce_managed_network
                        && !crate::seatbelt::has_loopback_proxy_endpoints(&env_overrides)
                    {
                        return Err(
                            "managed network enforcement active but no usable proxy endpoints"
                                .to_string(),
                        );
                    }

                    let mut child_command: Vec<String> = Vec::new();
                    child_command.push(self.runtime.executable.to_string_lossy().to_string());
                    child_command.extend(self.runtime.args.iter().cloned());
                    child_command.push(self.kernel_path.to_string_lossy().to_string());

                    let seatbelt_args = crate::seatbelt::build_seatbelt_args(
                        child_command,
                        sandbox_policy,
                        sandbox_policy_cwd,
                        enforce_managed_network,
                        &env_overrides,
                    );
                    let mut command = Command::new(crate::seatbelt::seatbelt_exec_path());
                    command.args(seatbelt_args);
                    command.env(crate::spawn::CODEX_SANDBOX_ENV_VAR, "seatbelt");
                    command
                } else {
                    let mut command = Command::new(&self.runtime.executable);
                    command.args(&self.runtime.args);
                    command.arg(&self.kernel_path);
                    command
                }
            }
            crate::config::JsReplRuntimeKindToml::Deno => {
                let mut command = Command::new(&self.runtime.executable);

                // Deno provides its own permission sandboxing. Run the kernel with
                // minimal permissions and disable interactive prompts.
                let allow_env =
                    "CODEX_JS_TMP_DIR,CODEX_JS_REPL_RUNTIME,CODEX_JS_REPL_RUNTIME_VERSION";
                let tmp_dir = self.tmp_dir.path().display();
                command.arg("run");
                command.arg("--quiet");
                command.arg("--no-prompt");
                command.arg(format!("--allow-env={allow_env}"));
                command.arg(format!("--allow-read={tmp_dir}"));
                command.args(&self.runtime.args);
                command.arg(&self.kernel_path);
                command
            }
        };
        command.current_dir(cwd);
        command.kill_on_drop(true);

        command.env("CODEX_JS_TMP_DIR", self.tmp_dir.path());
        command.env(
            "CODEX_JS_REPL_RUNTIME",
            match self.runtime.kind {
                crate::config::JsReplRuntimeKindToml::Node => "node",
                crate::config::JsReplRuntimeKindToml::Deno => "deno",
            },
        );
        command.env("CODEX_JS_REPL_RUNTIME_VERSION", self.runtime.version.clone());

        if matches!(self.runtime.kind, crate::config::JsReplRuntimeKindToml::Node)
            && !self.runtime.node_module_dirs.is_empty()
        {
            let joined = std::env::join_paths(
                self.runtime
                    .node_module_dirs
                    .iter()
                    .map(|p| p.as_os_str()),
            )
            .map_err(|err| format!("failed to join js_repl_node_module_dirs: {err}"))?;
            command.env("CODEX_JS_REPL_NODE_MODULE_DIRS", joined);
        }

        for (key, value) in env_overrides {
            command.env(key, value);
        }

        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        Ok(command)
    }

    async fn handle_tool_request(
        &self,
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        parent_ctx: &ToolCallCtx,
        attempt_req: u64,
        freeform_tool_names: &HashSet<String>,
        stdin: &Arc<Mutex<ChildStdin>>,
        tool_req: &ToolRequest,
    ) {
        // Avoid infinite recursion: the kernel should not call itself.
        if tool_req
            .tool_name
            .eq_ignore_ascii_case(crate::openai_tools::JS_REPL_TOOL_NAME)
            || tool_req
                .tool_name
                .eq_ignore_ascii_case(crate::openai_tools::JS_REPL_RESET_TOOL_NAME)
        {
            let response = json!({
                "type": "run_tool_result",
                "id": tool_req.id,
                "ok": false,
                "response": JsonValue::Null,
                "error": "js_repl cannot call itself via codex.tool".to_string(),
            });
            let _ = send_json_line(stdin, &response).await;
            return;
        }

        // Keep nested tool calls ordered after the parent tool call within the
        // same (request_ordinal, output_index) bucket.
        let base_seq = parent_ctx.seq_hint.unwrap_or(0);
        let local_seq = self.next_tool_seq.fetch_add(1, Ordering::Relaxed);
        let seq_hint = Some(base_seq.saturating_add(1).saturating_add(local_seq));
        let meta = ToolDispatchMeta::new(
            &parent_ctx.sub_id,
            seq_hint,
            parent_ctx.output_index,
            attempt_req,
        );

        let tool_name_lower = tool_req.tool_name.to_ascii_lowercase();
        let is_mcp_tool = sess
            .mcp_connection_manager()
            .parse_tool_name(tool_req.tool_name.as_str())
            .is_some();
        let is_freeform_tool = !is_mcp_tool && freeform_tool_names.contains(&tool_name_lower);

        let item = if is_freeform_tool {
            code_protocol::models::ResponseItem::CustomToolCall {
                id: None,
                status: None,
                call_id: tool_req.id.clone(),
                name: tool_req.tool_name.clone(),
                input: tool_req.arguments.clone(),
            }
        } else {
            code_protocol::models::ResponseItem::FunctionCall {
                id: None,
                name: tool_req.tool_name.clone(),
                arguments: tool_req.arguments.clone(),
                call_id: tool_req.id.clone(),
            }
        };

        let output = ToolRouter::global()
            .dispatch_response_item(sess, turn_diff_tracker, meta, item)
            .await;

        let (ok, response, error) = match output {
            Some(output) => match serde_json::to_value(&output) {
                Ok(value) => (true, value, JsonValue::Null),
                Err(err) => (
                    false,
                    JsonValue::Null,
                    JsonValue::String(format!("failed to serialize tool output: {err}")),
                ),
            },
            None => {
                let tool_name = tool_req.tool_name.as_str();
                (
                    false,
                    JsonValue::Null,
                    JsonValue::String(format!(
                        "unknown tool or unsupported tool payload: `{tool_name}`"
                    )),
                )
            }
        };

        let response = json!({
            "type": "run_tool_result",
            "id": tool_req.id,
            "ok": ok,
            "response": response,
            "error": error,
        });
        let _ = send_json_line(stdin, &response).await;
    }
}

async fn send_json_line(stdin: &Arc<Mutex<ChildStdin>>, message: &JsonValue) -> Result<(), String> {
    let encoded = serde_json::to_vec(message).map_err(|err| format!("failed to encode json: {err}"))?;
    let mut guard = stdin.lock().await;
    guard
        .write_all(&encoded)
        .await
        .map_err(|err| format!("failed to write to kernel: {err}"))?;
    guard
        .write_all(b"\n")
        .await
        .map_err(|err| format!("failed to write to kernel: {err}"))?;
    guard
        .flush()
        .await
        .map_err(|err| format!("failed to flush kernel stdin: {err}"))?;
    Ok(())
}

async fn kernel_stdout_loop(
    stdout: ChildStdout,
    pending_execs: Arc<Mutex<HashMap<String, oneshot::Sender<ExecResultMessage>>>>,
    stdin: Arc<Mutex<ChildStdin>>,
    tool_tx: mpsc::UnboundedSender<ToolRequest>,
    active_exec_id: Arc<Mutex<Option<String>>>,
) {
    let mut reader = BufReader::new(stdout).lines();
    loop {
        let line = match reader.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(err) => {
                debug!("js_repl stdout read error: {err}");
                break;
            }
        };

        let Ok(message) = serde_json::from_str::<JsonValue>(&line) else {
            continue;
        };

        let Some(kind) = message.get("type").and_then(JsonValue::as_str) else {
            continue;
        };

        match kind {
            "exec_result" => {
                let Some(id) = message.get("id").and_then(JsonValue::as_str) else {
                    continue;
                };
                let ok = message.get("ok").and_then(JsonValue::as_bool).unwrap_or(false);
                let output = message
                    .get("output")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_string();
                let error = message
                    .get("error")
                    .and_then(JsonValue::as_str)
                    .map(std::string::ToString::to_string);
                let sender = pending_execs.lock().await.remove(id);
                if let Some(sender) = sender {
                    let _ = sender.send(ExecResultMessage { ok, output, error });
                }
            }
            "run_tool" => {
                let Some(id) = message.get("id").and_then(JsonValue::as_str) else {
                    continue;
                };
                let exec_id = message
                    .get("exec_id")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_string();
                let should_accept = {
                    let active = active_exec_id.lock().await;
                    active.as_deref() == Some(exec_id.as_str())
                };
                if !should_accept {
                    let response = json!({
                        "type": "run_tool_result",
                        "id": id,
                        "ok": false,
                        "response": JsonValue::Null,
                        "error": "js_repl exec context not found".to_string(),
                    });
                    let _ = send_json_line(&stdin, &response).await;
                    continue;
                }
                let tool_name = message
                    .get("tool_name")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_string();
                let arguments = message
                    .get("arguments")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_string();

                if let Err(err) = tool_tx.send(ToolRequest {
                    id: id.to_string(),
                    tool_name: tool_name.clone(),
                    arguments,
                }) {
                    let response = json!({
                        "type": "run_tool_result",
                        "id": id,
                        "ok": false,
                        "response": JsonValue::Null,
                        "error": format!("failed to enqueue tool request `{tool_name}`: {err}"),
                    });
                    let _ = send_json_line(&stdin, &response).await;
                }
            }
            _ => {}
        }
    }

    let mut pending = pending_execs.lock().await;
    for (_, tx) in pending.drain() {
        let _ = tx.send(ExecResultMessage {
            ok: false,
            output: String::new(),
            error: Some("js_repl kernel terminated".to_string()),
        });
    }
}

async fn kernel_stderr_loop(stderr: ChildStderr) {
    let mut reader = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        debug!("js_repl stderr: {line}");
    }
}

fn freeform_tool_name_snapshot(sess: &Session) -> HashSet<String> {
    crate::openai_tools::get_openai_tools(
        &sess.tools_config_snapshot(),
        None,
        true,
        false,
        &[],
    )
    .into_iter()
    .filter_map(|tool| match tool {
        OpenAiTool::Freeform(spec) => Some(spec.name.to_ascii_lowercase()),
        _ => None,
    })
    .collect()
}

async fn resolve_runtime(cfg: JsReplRuntimeConfig) -> Result<ResolvedRuntime, String> {
    let executable = cfg.runtime_path.unwrap_or_else(|| {
        PathBuf::from(match cfg.kind {
            crate::config::JsReplRuntimeKindToml::Node => "node",
            crate::config::JsReplRuntimeKindToml::Deno => "deno",
        })
    });

    let version = detect_runtime_version(cfg.kind, &executable).await?;
    if matches!(cfg.kind, crate::config::JsReplRuntimeKindToml::Node) {
        let parsed = parse_version_triplet(&version).ok_or_else(|| {
            format!("failed to parse Node version `{version}` (expected like `18.0.0`)")
        })?;
        if !version_at_least(parsed, MIN_NODE_VERSION) {
            return Err(format!(
                "Node version {version} is too old for js_repl (need >= {min_major}.{min_minor}.{min_patch}). Consider setting `[tools].js_repl_runtime = \"deno\"`.",
                min_major = MIN_NODE_VERSION.0,
                min_minor = MIN_NODE_VERSION.1,
                min_patch = MIN_NODE_VERSION.2,
            ));
        }
    }

    let mut args = Vec::new();
    if matches!(cfg.kind, crate::config::JsReplRuntimeKindToml::Node)
        && !cfg.runtime_args.iter().any(|arg| arg == "--experimental-vm-modules")
    {
        args.push("--experimental-vm-modules".to_string());
    }
    args.extend(cfg.runtime_args);

    Ok(ResolvedRuntime {
        kind: cfg.kind,
        executable,
        args,
        version,
        node_module_dirs: cfg.node_module_dirs,
    })
}

async fn detect_runtime_version(
    kind: crate::config::JsReplRuntimeKindToml,
    executable: &Path,
) -> Result<String, String> {
    let output = Command::new(executable)
        .arg("--version")
        .output()
        .await
        .map_err(|err| format!("failed to run `{executable}`: {err}", executable = executable.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let text = if !stdout.is_empty() { stdout } else { stderr };
    if text.is_empty() {
        return Err(format!("`{executable}` produced no version output", executable = executable.display()));
    }

    match kind {
        crate::config::JsReplRuntimeKindToml::Node => Ok(text.trim().trim_start_matches('v').to_string()),
        crate::config::JsReplRuntimeKindToml::Deno => {
            for line in text.lines() {
                let l = line.trim();
                if let Some(rest) = l.strip_prefix("deno ") {
                    let version = rest
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .trim();
                    if !version.is_empty() {
                        return Ok(version.to_string());
                    }
                }
            }
            // Fallback to first token of the first line.
            Ok(text
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_string())
        }
    }
}

fn parse_version_triplet(version: &str) -> Option<(u64, u64, u64)> {
    let cleaned = version.trim().trim_start_matches('v');
    let mut parts = cleaned.split('.');
    let major = take_leading_u64(parts.next()?)?;
    let minor = take_leading_u64(parts.next()?)?;
    let patch = take_leading_u64(parts.next()?)?;
    Some((major, minor, patch))
}

fn take_leading_u64(input: &str) -> Option<u64> {
    let mut end = 0;
    for (idx, ch) in input.char_indices() {
        if ch.is_ascii_digit() {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    input[..end].parse().ok()
}

fn version_at_least(found: (u64, u64, u64), min: (u64, u64, u64)) -> bool {
    if found.0 != min.0 {
        return found.0 > min.0;
    }
    if found.1 != min.1 {
        return found.1 > min.1;
    }
    found.2 >= min.2
}

#[cfg(test)]
mod tests {
    use super::parse_version_triplet;
    use super::version_at_least;

    #[test]
    fn parses_node_versions_with_prefix_and_suffix() {
        assert_eq!(parse_version_triplet("v18.17.1"), Some((18, 17, 1)));
        assert_eq!(parse_version_triplet("18.17.1"), Some((18, 17, 1)));
        assert_eq!(parse_version_triplet("18.17.1-nightly"), Some((18, 17, 1)));
        assert_eq!(parse_version_triplet("v18.17"), None);
    }

    #[test]
    fn version_at_least_compares_semver_triplets() {
        assert!(version_at_least((18, 0, 0), (18, 0, 0)));
        assert!(version_at_least((18, 0, 1), (18, 0, 0)));
        assert!(version_at_least((18, 1, 0), (18, 0, 9)));
        assert!(version_at_least((19, 0, 0), (18, 999, 999)));
        assert!(!version_at_least((17, 99, 99), (18, 0, 0)));
        assert!(!version_at_least((18, 0, 0), (18, 0, 1)));
    }
}
