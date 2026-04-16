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
use std::collections::VecDeque;
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
use tokio::sync::Notify;
use tokio::sync::OnceCell;
use tokio::sync::Semaphore;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::warn;

pub(crate) const JS_REPL_PRAGMA_PREFIX: &str = "// codex-js-repl:";

const KERNEL_SOURCE_NODE: &str = include_str!("kernel.js");
const KERNEL_SOURCE_DENO: &str = include_str!("kernel_deno.js");
const MERIYAH_UMD: &str = include_str!("meriyah.umd.min.js");

/// Default per-exec timeout (15 s).  Keeps interactive feedback snappy
/// while giving non-trivial computations time to finish.
pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 15_000;
/// Hard ceiling on per-exec timeout (2 min).  Prevents the model from
/// requesting unbounded execution time.
pub(crate) const MAX_TIMEOUT_MS: u64 = 120_000;
/// Minimum Node.js version required for `--experimental-vm-modules`.
const MIN_NODE_VERSION: (u64, u64, u64) = (18, 0, 0);

/// Maximum recent-stderr lines kept for diagnostics on kernel failure.
const STDERR_TAIL_LINE_LIMIT: usize = 20;
/// Per-line byte cap in the stderr tail ring buffer.
const STDERR_TAIL_LINE_MAX_BYTES: usize = 512;
/// Total byte cap across all lines in the stderr tail.
const STDERR_TAIL_MAX_BYTES: usize = 4_096;
const STDERR_TAIL_SEPARATOR: &str = " | ";
/// Max exec IDs to include in unexpected-close log messages.
const EXEC_ID_LOG_LIMIT: usize = 8;
/// Byte budget for stderr context sent to the model in error diagnostics.
const MODEL_DIAG_STDERR_MAX_BYTES: usize = 1_024;
/// Byte budget for the error string sent to the model in error diagnostics.
const MODEL_DIAG_ERROR_MAX_BYTES: usize = 256;

#[derive(Clone, Debug)]
pub struct JsReplRuntimeConfig {
    pub kind: crate::config::JsReplRuntimeKindToml,
    pub runtime_path: Option<PathBuf>,
    pub runtime_args: Vec<String>,
    pub node_module_dirs: Vec<PathBuf>,
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
    exec_id: String,
    tool_name: String,
    arguments: String,
    cancel: CancellationToken,
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
enum ExecResultMessage {
    Ok { output: String },
    Err { output: String, message: String },
}

/// Per-exec nested tool-call tracking with cancellation + settlement.
#[derive(Default)]
struct ExecToolCalls {
    in_flight: usize,
    cancel: CancellationToken,
    notify: Arc<Notify>,
}

/// Reason the kernel stdout loop ended.
enum KernelStreamEnd {
    Shutdown,
    StdoutEof,
    StdoutReadError(String),
}

impl KernelStreamEnd {
    fn reason(&self) -> &'static str {
        match self {
            Self::Shutdown => "shutdown",
            Self::StdoutEof => "stdout_eof",
            Self::StdoutReadError(_) => "stdout_read_error",
        }
    }

    fn error(&self) -> Option<&str> {
        match self {
            Self::StdoutReadError(err) => Some(err),
            _ => None,
        }
    }
}

struct KernelDebugSnapshot {
    pid: Option<u32>,
    status: String,
    stderr_tail: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JsReplArgs {
    pub code: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub runtime: Option<crate::config::JsReplRuntimeKindToml>,
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

    /// Quick check whether the configured runtime binary exists and responds
    /// to `--version`.  Returns `Ok(version_string)` on success or an error
    /// describing why the runtime is unavailable.
    pub(crate) async fn probe_health(&self) -> Result<String, String> {
        let executable = self.runtime.runtime_path.clone().unwrap_or_else(|| {
            PathBuf::from(self.runtime.kind.default_executable())
        });
        detect_runtime_version(self.runtime.kind, &executable).await
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
    kernel: Arc<Mutex<Option<Kernel>>>,
    exec_lock: Arc<Semaphore>,
    exec_tool_calls: Arc<Mutex<HashMap<String, ExecToolCalls>>>,
    next_id: AtomicU64,
    next_tool_seq: AtomicU64,
}

struct Kernel {
    child: Arc<Mutex<Child>>,
    recent_stderr: Arc<Mutex<VecDeque<String>>>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending_execs: Arc<Mutex<HashMap<String, oneshot::Sender<ExecResultMessage>>>>,
    tool_rx: Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<ToolRequest>>>,
    shutdown: CancellationToken,
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
        tokio::try_join!(
            async {
                tokio::fs::write(&kernel_path, kernel_source)
                    .await
                    .map_err(|err| format!("failed to write js_repl kernel: {err}"))
            },
            async {
                tokio::fs::write(&meriyah_path, MERIYAH_UMD)
                    .await
                    .map_err(|err| format!("failed to write js_repl parser: {err}"))
            },
        )?;

        Ok(Arc::new(Self {
            runtime,
            tmp_dir,
            kernel_path,
            kernel: Arc::new(Mutex::new(None)),
            exec_lock: Arc::new(Semaphore::new(1)),
            exec_tool_calls: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(0),
            next_tool_seq: AtomicU64::new(0),
        }))
    }

    pub(crate) fn runtime_kind_str(&self) -> &str {
        self.runtime.kind.label()
    }

    pub(crate) fn runtime_version(&self) -> &str {
        &self.runtime.version
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
        let Ok(_permit) = self.exec_lock.acquire().await else {
            return Err(JsExecError {
                output: String::new(),
                error: "js_repl kernel is unavailable".to_owned(),
            });
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

        // Register per-exec tool-call tracking before starting.
        self.register_exec_tool_calls(&id).await;

        let (stdin, pending, child, recent_stderr, tool_rx) = {
            let mut guard = self.kernel.lock().await;
            if guard.is_none() {
                *guard = Some(
                    self.start_kernel(sess, cwd)
                        .await
                        .map_err(|error| JsExecError {
                    output: String::new(),
                    error,
                })?,
                );
            }
            let Some(kernel) = guard.as_ref() else {
                self.clear_exec_tool_calls(&id).await;
                return Err(JsExecError {
                    output: String::new(),
                    error: "js_repl kernel failed to start".to_owned(),
                });
            };
            (
                Arc::clone(&kernel.stdin),
                Arc::clone(&kernel.pending_execs),
                Arc::clone(&kernel.child),
                Arc::clone(&kernel.recent_stderr),
                Arc::clone(&kernel.tool_rx),
            )
        };

        pending.lock().await.insert(id.clone(), tx);

        let freeform_tool_names = freeform_tool_name_snapshot(sess);

        let message = json!({
            "type": "exec",
            "id": id.clone(),
            "code": args.code,
        });

        if let Err(err) = send_json_line(&stdin, &message).await {
            pending.lock().await.remove(&id);
            self.settle_exec(&id).await;
            let snapshot = Self::kernel_debug_snapshot(&child, &recent_stderr).await;
            let error = if should_include_diagnostics_for_write_error(&err, &snapshot) {
                with_model_failure_message(
                    "failed to send js_repl request",
                    "write_error",
                    Some(&err),
                    &snapshot,
                )
            } else {
                format!("failed to send js_repl request: {err}")
            };
            if let Err(e) = self.reset().await { warn!("js_repl reset failed: {e}"); }
            return Err(JsExecError { output: String::new(), error });
        }

        let mut tool_rx_guard = tool_rx.lock().await;
        let mut rx = rx;
        let timeout_sleep = tokio::time::sleep(timeout);
        tokio::pin!(timeout_sleep);

        let result: ExecResultMessage = loop {
            tokio::select! {
                _ = &mut timeout_sleep => {
                    pending.lock().await.remove(&id);
                    drop(tool_rx_guard);
                    self.settle_and_reset(&id).await;
                    return Err(JsExecError {
                        output: String::new(),
                        error: format!("js_repl timed out after {timeout_ms}ms"),
                    });
                }
                tool_req = tool_rx_guard.recv() => {
                    let Some(tool_req) = tool_req else {
                        pending.lock().await.remove(&id);
                        drop(tool_rx_guard);
                        self.settle_exec(&id).await;
                        let snapshot = Self::kernel_debug_snapshot(&child, &recent_stderr).await;
                        let msg = with_model_failure_message(
                            "js_repl kernel terminated while waiting for tool requests",
                            "tool_channel_closed",
                            None,
                            &snapshot,
                        );
                        if let Err(e) = self.reset().await { warn!("js_repl reset failed: {e}"); }
                        return Err(JsExecError { output: String::new(), error: msg });
                    };
                    Self::handle_tool_request(
                        sess,
                        turn_diff_tracker,
                        parent_ctx,
                        attempt_req,
                        &freeform_tool_names,
                        &stdin,
                        &self.next_tool_seq,
                        &tool_req,
                    )
                    .await;
                    // Mark this nested tool call as finished.
                    Self::finish_exec_tool_call(&self.exec_tool_calls, &tool_req.exec_id).await;
                }
                msg = &mut rx => {
                    match msg {
                        Ok(msg) => break msg,
                        Err(_) => {
                            drop(tool_rx_guard);
                            self.settle_exec(&id).await;
                            let snapshot = Self::kernel_debug_snapshot(&child, &recent_stderr).await;
                            let msg = with_model_failure_message(
                                "js_repl kernel stopped before returning a result",
                                "response_channel_closed",
                                None,
                                &snapshot,
                            );
                            if let Err(e) = self.reset().await { warn!("js_repl reset failed: {e}"); }
                            return Err(JsExecError { output: String::new(), error: msg });
                        }
                    }
                }
            }
        };
        drop(tool_rx_guard);

        // Exec finished — wait for any nested tool calls to settle, then clean up.
        self.settle_exec(&id).await;

        match result {
            ExecResultMessage::Ok { output } => Ok(JsExecResult { output }),
            ExecResultMessage::Err { output, message } => Err(JsExecError {
                output,
                error: message,
            }),
        }
    }

    /// Wait for nested tool calls to settle and clear tracking state.
    /// Every error/completion path in [`execute`] calls this to avoid
    /// leaking exec-tool-call bookkeeping.
    async fn settle_exec(&self, exec_id: &str) {
        self.wait_for_exec_tool_calls(exec_id).await;
        self.clear_exec_tool_calls(exec_id).await;
    }

    /// [`settle_exec`] followed by a kernel reset.  Logs a warning if the
    /// reset itself fails.
    async fn settle_and_reset(&self, exec_id: &str) {
        self.settle_exec(exec_id).await;
        if let Err(e) = self.reset().await {
            warn!("js_repl reset failed: {e}");
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
        // Clear all outstanding exec tool calls first.
        Self::clear_all_exec_tool_calls(&self.exec_tool_calls).await;

        let mut guard = self.kernel.lock().await;
        let Some(kernel) = guard.take() else {
            return;
        };

        // Signal shutdown to the read loops.
        kernel.shutdown.cancel();

        let pending = Arc::clone(&kernel.pending_execs);
        let mut pending = pending.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(ExecResultMessage::Err {
                output: String::new(),
                message: "js_repl kernel was reset".to_owned(),
            });
        }
        drop(pending);

        Self::kill_kernel_child(&kernel.child, "reset").await;
    }

    async fn kill_kernel_child(child: &Arc<Mutex<Child>>, reason: &str) {
        let mut guard = child.lock().await;
        let pid = guard.id();
        match guard.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(err) => {
                warn!(
                    kernel_pid = ?pid,
                    kill_reason = reason,
                    error = %err,
                    "failed to inspect js_repl kernel before kill"
                );
            }
        }
        if let Err(err) = guard.kill().await {
            debug!("failed to kill js_repl kernel (reason={reason}): {err}");
        }
        let _ = guard.wait().await;
    }

    async fn start_kernel(
        &self,
        sess: &Session,
        cwd: &Path,
    ) -> Result<Kernel, String> {
        let mut command = self.build_runtime_command(sess, cwd)?;
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
            .ok_or_else(|| "js_repl kernel missing stdin".to_owned())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "js_repl kernel missing stdout".to_owned())?;
        let stderr = child
            .stderr
            .take();

        let shutdown = CancellationToken::new();
        let stdin = Arc::new(Mutex::new(stdin));
        let pending_execs: Arc<Mutex<HashMap<String, oneshot::Sender<ExecResultMessage>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let child = Arc::new(Mutex::new(child));
        let recent_stderr = Arc::new(Mutex::new(VecDeque::with_capacity(
            STDERR_TAIL_LINE_LIMIT,
        )));

        let (tool_tx, tool_rx) = tokio::sync::mpsc::unbounded_channel::<ToolRequest>();
        let tool_rx = Arc::new(Mutex::new(tool_rx));

        tokio::spawn(Self::read_stdout(
            stdout,
            Arc::clone(&child),
            Arc::clone(&self.kernel),
            Arc::clone(&recent_stderr),
            Arc::clone(&pending_execs),
            Arc::clone(&self.exec_tool_calls),
            Arc::clone(&stdin),
            tool_tx,
            shutdown.clone(),
        ));
        if let Some(stderr) = stderr {
            tokio::spawn(Self::read_stderr(
                stderr,
                Arc::clone(&recent_stderr),
                shutdown.clone(),
            ));
        } else {
            warn!("js_repl kernel missing stderr");
        }

        Ok(Kernel {
            child,
            recent_stderr,
            stdin,
            pending_execs,
            tool_rx,
            shutdown,
        })
    }

    fn build_runtime_command(
        &self,
        sess: &Session,
        cwd: &Path,
    ) -> Result<Command, String> {
        let sandbox_policy = sess.get_sandbox_policy();
        let sandbox_policy_cwd = sess.get_cwd();
        let enforce_managed_network = sess.managed_network_proxy().is_some();
        let caps = self.runtime.kind.capabilities();

        let mut env_overrides = HashMap::<String, String>::new();
        if let Some(proxy) = sess.managed_network_proxy() {
            proxy.apply_to_env(&mut env_overrides);
        }

        // Seatbelt is available only when: the runtime supports it, we're on
        // macOS, and the sandbox policy isn't DangerFullAccess.
        let seatbelt_enabled = cfg!(target_os = "macos")
            && caps.supports_seatbelt
            && !matches!(
                sandbox_policy,
                crate::protocol::SandboxPolicy::DangerFullAccess
            );

        // If managed network is required but this runtime can't enforce it
        // (neither via its own sandbox nor via seatbelt), reject early.
        if enforce_managed_network
            && !caps.can_enforce_network_without_seatbelt
            && !seatbelt_enabled
            && !matches!(
                sandbox_policy,
                crate::protocol::SandboxPolicy::DangerFullAccess
            )
        {
            return Err(format!(
                "js_repl {} runtime cannot enforce network mediation on this platform. \
                 Set `[tools].js_repl_runtime = \"deno\"` (recommended) or disable \
                 network mediation.",
                self.runtime.kind
            ));
        }

        let mut command = if seatbelt_enabled {
            // Wrap the runtime invocation inside macOS seatbelt.
            if enforce_managed_network
                && !crate::seatbelt::has_loopback_proxy_endpoints(&env_overrides)
            {
                return Err(
                    "managed network enforcement active but no usable proxy endpoints".to_owned(),
                );
            }

            let mut child_command: Vec<String> = Vec::with_capacity(2 + self.runtime.args.len());
            child_command.push(self.runtime.executable.to_string_lossy().into_owned());
            child_command.extend(self.runtime.args.iter().cloned());
            child_command.push(self.kernel_path.to_string_lossy().into_owned());

            let seatbelt_args = crate::seatbelt::build_seatbelt_args(
                child_command,
                sandbox_policy,
                sandbox_policy_cwd,
                enforce_managed_network,
                &env_overrides,
            );
            let mut cmd = Command::new(crate::seatbelt::seatbelt_exec_path());
            cmd.args(seatbelt_args);
            cmd.env(crate::spawn::CODEX_SANDBOX_ENV_VAR, "seatbelt");
            cmd
        } else if matches!(caps.sandbox, crate::config::RuntimeSandboxKind::BuiltinPermissions) {
            // Runtime has its own permission sandbox (Deno).
            let mut cmd = Command::new(&self.runtime.executable);
            let allow_env = caps.sandbox_env_passthrough.join(",");
            let tmp_dir = self.tmp_dir.path().display();
            cmd.arg("run");
            cmd.arg("--quiet");
            cmd.arg("--no-prompt");
            cmd.arg(format!("--allow-env={allow_env}"));
            cmd.arg(format!("--allow-read={tmp_dir}"));
            cmd.args(&self.runtime.args);
            cmd.arg(&self.kernel_path);
            cmd
        } else {
            // No sandbox available — run directly.
            let mut cmd = Command::new(&self.runtime.executable);
            cmd.args(&self.runtime.args);
            cmd.arg(&self.kernel_path);
            cmd
        };

        command.current_dir(cwd);
        command.kill_on_drop(true);

        command.env("CODEX_JS_TMP_DIR", self.tmp_dir.path());
        command.env("CODEX_JS_REPL_RUNTIME", self.runtime.kind.label());
        command.env("CODEX_JS_REPL_RUNTIME_VERSION", self.runtime.version.clone());

        if caps.uses_node_module_dirs && !self.runtime.node_module_dirs.is_empty() {
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

    #[allow(clippy::too_many_arguments)]
    async fn handle_tool_request(
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        parent_ctx: &ToolCallCtx,
        attempt_req: u64,
        freeform_tool_names: &HashSet<String>,
        stdin: &Arc<Mutex<ChildStdin>>,
        next_tool_seq: &AtomicU64,
        tool_req: &ToolRequest,
    ) {
        let base_seq = parent_ctx.seq_hint.unwrap_or(0);
        let local_seq = next_tool_seq.fetch_add(1, Ordering::Relaxed);
        let seq_hint = Some(base_seq.saturating_add(1).saturating_add(local_seq));
        let mut meta = ToolDispatchMeta::new(
            &parent_ctx.sub_id,
            seq_hint,
            parent_ctx.output_index,
            attempt_req,
        );
        meta.parent_call_id = Some(&parent_ctx.call_id);

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
                namespace: None,
                arguments: tool_req.arguments.clone(),
                call_id: tool_req.id.clone(),
            }
        };

        let output = tokio::select! {
            result = ToolRouter::global()
                .dispatch_response_item(sess, turn_diff_tracker, meta, item) => result,
            _ = tool_req.cancel.cancelled() => {
                let response = json!({
                    "type": "run_tool_result",
                    "id": tool_req.id,
                    "ok": false,
                    "response": JsonValue::Null,
                    "error": "js_repl tool call cancelled (exec reset or timeout)",
                });
                let _ = send_json_line(stdin, &response).await;
                return;
            }
        };

        let (ok, response, error) = if let Some(output) = output {
            match serde_json::to_value(&output) {
                Ok(value) => (true, value, JsonValue::Null),
                Err(err) => (
                    false,
                    JsonValue::Null,
                    JsonValue::String(format!("failed to serialize tool output: {err}")),
                ),
            }
        } else {
            let tool_name = tool_req.tool_name.as_str();
            (
                false,
                JsonValue::Null,
                JsonValue::String(format!(
                    "unknown tool or unsupported tool payload: `{tool_name}`"
                )),
            )
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

    // ── Per-exec tool-call tracking ─────────────────────────────────────

    async fn register_exec_tool_calls(&self, exec_id: &str) {
        self.exec_tool_calls
            .lock()
            .await
            .insert(exec_id.to_string(), ExecToolCalls::default());
    }

    async fn clear_exec_tool_calls(&self, exec_id: &str) {
        Self::clear_exec_tool_calls_map(&self.exec_tool_calls, exec_id).await;
    }

    async fn wait_for_exec_tool_calls(&self, exec_id: &str) {
        Self::wait_for_exec_tool_calls_map(&self.exec_tool_calls, exec_id).await;
    }

    async fn begin_exec_tool_call(
        exec_tool_calls: &Arc<Mutex<HashMap<String, ExecToolCalls>>>,
        exec_id: &str,
    ) -> Option<CancellationToken> {
        let mut calls = exec_tool_calls.lock().await;
        let state = calls.get_mut(exec_id)?;
        state.in_flight += 1;
        Some(state.cancel.clone())
    }

    async fn finish_exec_tool_call(
        exec_tool_calls: &Arc<Mutex<HashMap<String, ExecToolCalls>>>,
        exec_id: &str,
    ) {
        let notify = {
            let mut calls = exec_tool_calls.lock().await;
            let Some(state) = calls.get_mut(exec_id) else {
                return;
            };
            if state.in_flight == 0 {
                return;
            }
            state.in_flight -= 1;
            if state.in_flight == 0 {
                Some(Arc::clone(&state.notify))
            } else {
                None
            }
        };
        if let Some(notify) = notify {
            notify.notify_waiters();
        }
    }

    async fn wait_for_exec_tool_calls_map(
        exec_tool_calls: &Arc<Mutex<HashMap<String, ExecToolCalls>>>,
        exec_id: &str,
    ) {
        loop {
            let notified = {
                let calls = exec_tool_calls.lock().await;
                calls
                    .get(exec_id)
                    .filter(|state| state.in_flight > 0)
                    .map(|state| Arc::clone(&state.notify).notified_owned())
            };
            match notified {
                Some(notified) => notified.await,
                None => return,
            }
        }
    }

    async fn clear_exec_tool_calls_map(
        exec_tool_calls: &Arc<Mutex<HashMap<String, ExecToolCalls>>>,
        exec_id: &str,
    ) {
        if let Some(state) = exec_tool_calls.lock().await.remove(exec_id) {
            state.cancel.cancel();
            state.notify.notify_waiters();
        }
    }

    async fn clear_all_exec_tool_calls(
        exec_tool_calls: &Arc<Mutex<HashMap<String, ExecToolCalls>>>,
    ) {
        let states = {
            let mut calls = exec_tool_calls.lock().await;
            calls.drain().map(|(_, state)| state).collect::<Vec<_>>()
        };
        for state in states {
            state.cancel.cancel();
            state.notify.notify_waiters();
        }
    }

    // ── Debug snapshot helpers ───────────────────────────────────────────

    async fn kernel_debug_snapshot(
        child: &Arc<Mutex<Child>>,
        recent_stderr: &Arc<Mutex<VecDeque<String>>>,
    ) -> KernelDebugSnapshot {
        let (pid, status) = {
            let mut guard = child.lock().await;
            let pid = guard.id();
            let status = match guard.try_wait() {
                Ok(Some(status)) => format!("exited({})", format_exit_status(status)),
                Ok(None) => "running".to_string(),
                Err(err) => format!("unknown ({err})"),
            };
            (pid, status)
        };
        let stderr_tail = {
            let tail = recent_stderr.lock().await;
            format_stderr_tail(&tail)
        };
        KernelDebugSnapshot {
            pid,
            status,
            stderr_tail,
        }
    }

    fn truncate_id_list(ids: &[String]) -> Vec<&str> {
        ids.iter()
            .take(EXEC_ID_LOG_LIMIT)
            .map(String::as_str)
            .collect()
    }

    // ── Background I/O loops ────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    async fn read_stdout(
        stdout: ChildStdout,
        child: Arc<Mutex<Child>>,
        manager_kernel: Arc<Mutex<Option<Kernel>>>,
        recent_stderr: Arc<Mutex<VecDeque<String>>>,
        pending_execs: Arc<Mutex<HashMap<String, oneshot::Sender<ExecResultMessage>>>>,
        exec_tool_calls: Arc<Mutex<HashMap<String, ExecToolCalls>>>,
        stdin: Arc<Mutex<ChildStdin>>,
        tool_tx: tokio::sync::mpsc::UnboundedSender<ToolRequest>,
        shutdown: CancellationToken,
    ) {
        let mut reader = BufReader::new(stdout).lines();
        let end_reason = loop {
            let line = tokio::select! {
                _ = shutdown.cancelled() => break KernelStreamEnd::Shutdown,
                res = reader.next_line() => match res {
                    Ok(Some(line)) => line,
                    Ok(None) => break KernelStreamEnd::StdoutEof,
                    Err(err) => break KernelStreamEnd::StdoutReadError(err.to_string()),
                },
            };

            let Ok(message) = serde_json::from_str::<JsonValue>(&line) else {
                warn!("js_repl kernel sent invalid json: {line}");
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

                    // Deno sends __fatal__ when an unhandled error crashes the
                    // kernel.  Route it to whatever exec is currently pending so
                    // the user sees the error instead of silently dropping it.
                    let resolved_id = if id == "__fatal__" {
                        let lock = pending_execs.lock().await;
                        match lock.len() {
                            0 => None,
                            1 => lock.keys().next().cloned(),
                            n => {
                                warn!(
                                    count = n,
                                    "js_repl kernel sent __fatal__ but multiple execs pending; \
                                     this should not happen — routing to none"
                                );
                                None
                            }
                        }
                    } else {
                        Some(id.to_owned())
                    };
                    let Some(resolved_id) = resolved_id else {
                        warn!("js_repl kernel sent __fatal__ with no pending exec");
                        continue;
                    };

                    // Wait for any nested tool calls to settle before delivering result.
                    JsReplManager::wait_for_exec_tool_calls_map(&exec_tool_calls, &resolved_id).await;

                    let ok = message.get("ok").and_then(JsonValue::as_bool).unwrap_or(false);
                    let output = message
                        .get("output")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default().to_owned();
                    let error = message
                        .get("error")
                        .and_then(JsonValue::as_str)
                        .map(ToString::to_string);
                    let sender = pending_execs.lock().await.remove(&resolved_id);
                    if let Some(sender) = sender {
                        let payload = if ok {
                            ExecResultMessage::Ok { output }
                        } else {
                            ExecResultMessage::Err {
                                output,
                                message: error
                                    .unwrap_or_else(|| "js_repl execution failed".to_string()),
                            }
                        };
                        let _ = sender.send(payload);
                    }
                    JsReplManager::clear_exec_tool_calls_map(&exec_tool_calls, &resolved_id).await;
                }
                "run_tool" => {
                    let Some(id) = message.get("id").and_then(JsonValue::as_str) else {
                        continue;
                    };
                    let exec_id = message
                        .get("exec_id")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default().to_owned();
                    let tool_name = message
                        .get("tool_name")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default().to_owned();
                    let arguments = message
                        .get("arguments")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default().to_owned();

                    // Check if the exec is still active via exec_tool_calls registration.
                    let Some(exec_cancel) =
                        JsReplManager::begin_exec_tool_call(&exec_tool_calls, &exec_id).await
                    else {
                        let snapshot =
                            JsReplManager::kernel_debug_snapshot(&child, &recent_stderr).await;
                        warn!(
                            exec_id = %exec_id,
                            tool_call_id = %id,
                            tool_name = %tool_name,
                            kernel_pid = ?snapshot.pid,
                            kernel_status = %snapshot.status,
                            "js_repl tool request for unknown/finished exec"
                        );
                        let response = json!({
                            "type": "run_tool_result",
                            "id": id,
                            "ok": false,
                            "response": JsonValue::Null,
                            "error": "js_repl exec context not found",
                        });
                        let _ = send_json_line(&stdin, &response).await;
                        continue;
                    };

                    // Self-invocation guard (checked in read_stdout to avoid
                    // dispatching the tool call at all).
                    if is_js_repl_internal_tool(&tool_name) {
                        let response = json!({
                            "type": "run_tool_result",
                            "id": id,
                            "ok": false,
                            "response": JsonValue::Null,
                            "error": "js_repl cannot invoke itself",
                        });
                        if let Err(err) = send_json_line(&stdin, &response).await {
                            let snapshot =
                                JsReplManager::kernel_debug_snapshot(&child, &recent_stderr).await;
                            warn!(
                                exec_id = %exec_id,
                                tool_call_id = %id,
                                error = %err,
                                kernel_pid = ?snapshot.pid,
                                kernel_status = %snapshot.status,
                                kernel_stderr_tail = %snapshot.stderr_tail,
                                "failed to reply to kernel run_tool request"
                            );
                        }
                        JsReplManager::finish_exec_tool_call(&exec_tool_calls, &exec_id).await;
                        continue;
                    }

                    // Pipe the tool request to the executor via the channel.
                    // The executor dispatches tools synchronously (it has &mut
                    // TurnDiffTracker) and calls finish_exec_tool_call.
                    let tool_req = ToolRequest {
                        id: id.to_owned(),
                        exec_id: exec_id.clone(),
                        tool_name,
                        arguments,
                        cancel: exec_cancel,
                    };
                    if let Err(err) = tool_tx.send(tool_req) {
                        let response = json!({
                            "type": "run_tool_result",
                            "id": id,
                            "ok": false,
                            "response": JsonValue::Null,
                            "error": format!("failed to enqueue tool request: {err}"),
                        });
                        let _ = send_json_line(&stdin, &response).await;
                        JsReplManager::finish_exec_tool_call(&exec_tool_calls, &exec_id).await;
                    }
                }
                _ => {}
            }
        };

        // ── Kernel stream ended ────────────────────────────────────────
        // Wait for outstanding tool calls to settle before notifying pending execs.
        let exec_ids = {
            let calls = exec_tool_calls.lock().await;
            calls.keys().cloned().collect::<Vec<_>>()
        };
        for exec_id in &exec_ids {
            JsReplManager::wait_for_exec_tool_calls_map(&exec_tool_calls, exec_id).await;
            JsReplManager::clear_exec_tool_calls_map(&exec_tool_calls, exec_id).await;
        }

        let unexpected_snapshot = if matches!(end_reason, KernelStreamEnd::Shutdown) {
            None
        } else {
            Some(Self::kernel_debug_snapshot(&child, &recent_stderr).await)
        };
        let kernel_failure_message = unexpected_snapshot.as_ref().map(|snapshot| {
            with_model_failure_message(
                "js_repl kernel exited unexpectedly",
                end_reason.reason(),
                end_reason.error(),
                snapshot,
            )
        });
        let kernel_exit_message = kernel_failure_message
            .clone()
            .unwrap_or_else(|| "js_repl kernel exited unexpectedly".to_string());

        // Clear the kernel from the manager so a new one will be started.
        {
            let mut kernel = manager_kernel.lock().await;
            let should_clear = kernel
                .as_ref()
                .is_some_and(|state| Arc::ptr_eq(&state.child, &child));
            if should_clear {
                kernel.take();
            }
        }

        let mut pending = pending_execs.lock().await;
        let pending_exec_ids: Vec<String> = pending.keys().cloned().collect();
        for (_, tx) in pending.drain() {
            let _ = tx.send(ExecResultMessage::Err {
                output: String::new(),
                message: kernel_exit_message.clone(),
            });
        }
        drop(pending);

        if !matches!(end_reason, KernelStreamEnd::Shutdown) {
            let mut sorted_ids = pending_exec_ids;
            sorted_ids.sort_unstable();
            let snapshot = Self::kernel_debug_snapshot(&child, &recent_stderr).await;
            warn!(
                reason = %end_reason.reason(),
                stream_error = %end_reason.error().unwrap_or(""),
                kernel_pid = ?snapshot.pid,
                kernel_status = %snapshot.status,
                pending_exec_count = sorted_ids.len(),
                pending_exec_ids = ?Self::truncate_id_list(&sorted_ids),
                kernel_stderr_tail = %snapshot.stderr_tail,
                "js_repl kernel terminated unexpectedly"
            );
        }
    }

    async fn read_stderr(
        stderr: ChildStderr,
        recent_stderr: Arc<Mutex<VecDeque<String>>>,
        shutdown: CancellationToken,
    ) {
        let mut reader = BufReader::new(stderr).lines();

        loop {
            let line = tokio::select! {
                _ = shutdown.cancelled() => break,
                res = reader.next_line() => match res {
                    Ok(Some(line)) => line,
                    Ok(None) => break,
                    Err(err) => {
                        warn!("js_repl kernel stderr ended: {err}");
                        break;
                    }
                },
            };
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let bounded_line = {
                    let mut tail = recent_stderr.lock().await;
                    push_stderr_tail_line(&mut tail, trimmed)
                };
                if bounded_line.is_empty() {
                    continue;
                }
                warn!("js_repl stderr: {bounded_line}");
            }
        }
    }
}

// ── Free helper functions ───────────────────────────────────────────────

async fn send_json_line(stdin: &Arc<Mutex<ChildStdin>>, message: &JsonValue) -> Result<(), String> {
    let mut encoded = serde_json::to_vec(message)
        .map_err(|err| format!("failed to encode json: {err}"))?;
    encoded.push(b'\n');
    let mut guard = stdin.lock().await;
    guard
        .write_all(&encoded)
        .await
        .map_err(|err| format!("failed to write to kernel: {err}"))?;
    guard
        .flush()
        .await
        .map_err(|err| format!("failed to flush kernel stdin: {err}"))?;
    Ok(())
}

fn is_js_repl_internal_tool(name: &str) -> bool {
    name.eq_ignore_ascii_case(crate::openai_tools::JS_REPL_TOOL_NAME)
        || name.eq_ignore_ascii_case(crate::openai_tools::JS_REPL_RESET_TOOL_NAME)
}

fn format_exit_status(status: std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("code={code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("signal={signal}");
        }
    }
    "unknown".to_string()
}

fn format_stderr_tail(lines: &VecDeque<String>) -> String {
    if lines.is_empty() {
        return "<empty>".to_string();
    }
    let mut iter = lines.iter();
    let mut out = iter.next().unwrap().clone();
    for line in iter {
        out.push_str(STDERR_TAIL_SEPARATOR);
        out.push_str(line);
    }
    out
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

fn stderr_tail_formatted_bytes(lines: &VecDeque<String>) -> usize {
    if lines.is_empty() {
        return 0;
    }
    let payload_bytes: usize = lines.iter().map(String::len).sum();
    let separator_bytes = STDERR_TAIL_SEPARATOR.len() * (lines.len() - 1);
    payload_bytes + separator_bytes
}

fn stderr_tail_bytes_with_candidate(lines: &VecDeque<String>, line: &str) -> usize {
    if lines.is_empty() {
        return line.len();
    }
    stderr_tail_formatted_bytes(lines) + STDERR_TAIL_SEPARATOR.len() + line.len()
}

fn push_stderr_tail_line(lines: &mut VecDeque<String>, line: &str) -> String {
    let max_line_bytes = STDERR_TAIL_LINE_MAX_BYTES.min(STDERR_TAIL_MAX_BYTES);
    let bounded_line = truncate_utf8_prefix_by_bytes(line, max_line_bytes);
    if bounded_line.is_empty() {
        return bounded_line;
    }

    while !lines.is_empty()
        && (lines.len() >= STDERR_TAIL_LINE_LIMIT
            || stderr_tail_bytes_with_candidate(lines, &bounded_line) > STDERR_TAIL_MAX_BYTES)
    {
        lines.pop_front();
    }

    lines.push_back(bounded_line.clone());
    bounded_line
}

fn is_kernel_status_exited(status: &str) -> bool {
    status.starts_with("exited(")
}

fn should_include_diagnostics_for_write_error(
    err_message: &str,
    snapshot: &KernelDebugSnapshot,
) -> bool {
    is_kernel_status_exited(&snapshot.status)
        || err_message.to_ascii_lowercase().contains("broken pipe")
}

fn format_model_kernel_failure_details(
    reason: &str,
    stream_error: Option<&str>,
    snapshot: &KernelDebugSnapshot,
) -> String {
    let payload = serde_json::json!({
        "reason": reason,
        "stream_error": stream_error
            .map(|err| truncate_utf8_prefix_by_bytes(err, MODEL_DIAG_ERROR_MAX_BYTES)),
        "kernel_pid": snapshot.pid,
        "kernel_status": snapshot.status,
        "kernel_stderr_tail": truncate_utf8_prefix_by_bytes(
            &snapshot.stderr_tail,
            MODEL_DIAG_STDERR_MAX_BYTES,
        ),
    });
    let encoded = serde_json::to_string(&payload)
        .unwrap_or_else(|err| format!(r#"{{"reason":"serialization_error","error":"{err}"}}"#));
    format!("js_repl diagnostics: {encoded}")
}

fn with_model_failure_message(
    base_message: &str,
    reason: &str,
    stream_error: Option<&str>,
    snapshot: &KernelDebugSnapshot,
) -> String {
    format!(
        "{base_message}\n\n{}",
        format_model_kernel_failure_details(reason, stream_error, snapshot)
    )
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
        PathBuf::from(cfg.kind.default_executable())
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

    let mut args = Vec::with_capacity(cfg.runtime_args.len() + 1);
    if matches!(cfg.kind, crate::config::JsReplRuntimeKindToml::Node)
        && !cfg.runtime_args.iter().any(|arg| arg == "--experimental-vm-modules")
    {
        args.push("--experimental-vm-modules".to_owned());
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
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let text = if stdout.is_empty() { stderr } else { stdout };
    if text.is_empty() {
        return Err(format!("`{executable}` produced no version output", executable = executable.display()));
    }

    match kind {
        crate::config::JsReplRuntimeKindToml::Node => Ok(text.trim().trim_start_matches('v').to_owned()),
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
                        return Ok(version.to_owned());
                    }
                }
            }
            // Fallback to first token of the first line.
            Ok(text
                .lines()
                .next()
                .unwrap_or_default()
                .trim().to_owned())
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
    use super::ExecResultMessage;
    use super::JsReplRuntimeConfig;
    use super::ToolRequest;

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

    #[test]
    fn exec_result_err_preserves_output() {
        let msg = ExecResultMessage::Err {
            output: "console.log output before error".to_owned(),
            message: "ReferenceError: x is not defined".to_owned(),
        };
        match msg {
            ExecResultMessage::Err { output, message } => {
                assert_eq!(output, "console.log output before error");
                assert_eq!(message, "ReferenceError: x is not defined");
            }
            ExecResultMessage::Ok { .. } => panic!("expected Err variant"),
        }
    }

    #[test]
    fn exec_result_ok_carries_output() {
        let msg = ExecResultMessage::Ok {
            output: "42".to_owned(),
        };
        match msg {
            ExecResultMessage::Ok { output } => assert_eq!(output, "42"),
            ExecResultMessage::Err { .. } => panic!("expected Ok variant"),
        }
    }

    #[test]
    fn runtime_config_clone_preserves_fields() {
        let cfg = JsReplRuntimeConfig {
            kind: crate::config::JsReplRuntimeKindToml::Node,
            runtime_path: Some(std::path::PathBuf::from("/usr/bin/node")),
            runtime_args: vec!["--max-old-space-size=512".to_owned()],
            node_module_dirs: vec![std::path::PathBuf::from("/app/node_modules")],
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.kind, cfg.kind);
        assert_eq!(cloned.runtime_path, cfg.runtime_path);
        assert_eq!(cloned.runtime_args, cfg.runtime_args);
        assert_eq!(cloned.node_module_dirs, cfg.node_module_dirs);
    }

    #[test]
    fn tool_request_carries_cancel_token() {
        let token = tokio_util::sync::CancellationToken::new();
        let req = ToolRequest {
            id: "t-1".to_owned(),
            exec_id: "e-1".to_owned(),
            tool_name: "shell".to_owned(),
            arguments: "{}".to_owned(),
            cancel: token.clone(),
        };
        assert!(!req.cancel.is_cancelled());
        token.cancel();
        assert!(req.cancel.is_cancelled());
    }
}
