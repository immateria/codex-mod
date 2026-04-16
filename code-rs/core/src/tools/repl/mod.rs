//! REPL tool — manages JavaScript kernel processes (Node / Deno) and
//! dispatches exec requests over a line-delimited JSON protocol.
//!
//! ## Module layout
//!
//! | Module        | Responsibility                                         |
//! |---------------|--------------------------------------------------------|
//! | `types`       | Shared data types (configs, results, protocol structs) |
//! | `runtime`     | Runtime resolution, version probing, command building  |
//! | `diagnostics` | Stderr tail ring-buffer, model failure formatting      |
//! | `js/`         | JavaScript kernel sources (Node, Deno, shared common)  |

mod diagnostics;
mod runtime;
pub(crate) mod types;

// Re-export public interface so callers don't need to reach into submodules.
pub(crate) use types::{ReplArgs, ReplExecError, ReplExecResult, ReplRuntimeConfig};

use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::openai_tools::OpenAiTool;
use crate::tools::router::ToolDispatchMeta;
use crate::tools::router::ToolRouter;
use crate::turn_diff_tracker::TurnDiffTracker;
use diagnostics::*;
use runtime::{build_runtime_command, detect_runtime_version, resolve_runtime};
use serde_json::Value as JsonValue;
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;
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
use tokio::sync::Mutex;
use tokio::sync::OnceCell;
use tokio::sync::Semaphore;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::warn;
use types::{ExecResultMessage, ExecToolCalls, KernelDebugSnapshot, KernelStreamEnd, ResolvedRuntime, ToolRequest};

pub(crate) const REPL_PRAGMA_PREFIX: &str = "// codex-repl:";

const KERNEL_SOURCE_NODE: &str = include_str!("js/kernel_node.js");
const KERNEL_SOURCE_DENO: &str = include_str!("js/kernel_deno.js");
const KERNEL_COMMON: &str = include_str!("js/kernel_common.js");
const MERIYAH_UMD: &str = include_str!("js/meriyah.umd.min.js");

/// Default per-exec timeout (15 s).
pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 15_000;
/// Hard ceiling on per-exec timeout (2 min).
pub(crate) const MAX_TIMEOUT_MS: u64 = 120_000;

// ── ReplHandle ──────────────────────────────────────────────────────────

pub(crate) struct ReplHandle {
    runtime: ReplRuntimeConfig,
    cell: OnceCell<Arc<ReplManager>>,
}

impl ReplHandle {
    pub(crate) fn new(runtime: ReplRuntimeConfig) -> Self {
        Self {
            runtime,
            cell: OnceCell::new(),
        }
    }

    /// Quick check whether the configured runtime binary exists and responds
    /// to `--version`.  Returns `Ok(version_string)` on success.
    pub(crate) async fn probe_health(&self) -> Result<String, String> {
        let executable = self.runtime.runtime_path.as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(self.runtime.kind.default_executable()));
        detect_runtime_version(self.runtime.kind, &executable).await
    }

    pub(crate) async fn manager(&self) -> Result<Arc<ReplManager>, String> {
        self.cell
            .get_or_try_init(|| async {
                ReplManager::new(self.runtime.clone()).await
            })
            .await
            .cloned()
    }

    pub(crate) fn manager_if_started(&self) -> Option<Arc<ReplManager>> {
        self.cell.get().cloned()
    }
}

// ── ReplManager ─────────────────────────────────────────────────────────

pub(crate) struct ReplManager {
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
    /// Uses tokio::sync::Mutex (not std) because the lock is intentionally
    /// held across awaits for the entire duration of `execute()`.
    tool_rx: Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<ToolRequest>>>,
    shutdown: CancellationToken,
}

impl ReplManager {
    pub(crate) async fn new(runtime: ReplRuntimeConfig) -> Result<Arc<Self>, String> {
        let runtime = resolve_runtime(runtime).await?;

        let tmp_dir = tempfile::tempdir()
            .map_err(|err| format!("failed to create repl temp dir: {err}"))?;

        let kernel_filename = match runtime.kind {
            crate::config::ReplRuntimeKindToml::Node => "kernel_node.js",
            crate::config::ReplRuntimeKindToml::Deno => "kernel_deno.js",
        };
        let kernel_path = tmp_dir.path().join(kernel_filename);
        let common_path = tmp_dir.path().join("kernel_common.js");
        let meriyah_path = tmp_dir.path().join("meriyah.umd.min.js");
        let kernel_source = match runtime.kind {
            crate::config::ReplRuntimeKindToml::Node => KERNEL_SOURCE_NODE,
            crate::config::ReplRuntimeKindToml::Deno => KERNEL_SOURCE_DENO,
        };
        tokio::try_join!(
            async {
                tokio::fs::write(&kernel_path, kernel_source)
                    .await
                    .map_err(|err| format!("failed to write repl kernel: {err}"))
            },
            async {
                tokio::fs::write(&common_path, KERNEL_COMMON)
                    .await
                    .map_err(|err| format!("failed to write repl common: {err}"))
            },
            async {
                tokio::fs::write(&meriyah_path, MERIYAH_UMD)
                    .await
                    .map_err(|err| format!("failed to write repl parser: {err}"))
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

    // ── Execute ─────────────────────────────────────────────────────────

    pub(crate) async fn execute(
        &self,
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        parent_ctx: &ToolCallCtx,
        attempt_req: u64,
        cwd: &Path,
        args: ReplArgs,
    ) -> Result<ReplExecResult, ReplExecError> {
        let Ok(_permit) = self.exec_lock.acquire().await else {
            return Err(ReplExecError {
                output: String::new(),
                error: "repl kernel is unavailable".to_owned(),
                content_items: Vec::new(),
            });
        };

        let timeout_ms = args
            .timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);
        let timeout = Duration::from_millis(timeout_ms);

        let exec_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id = format!("repl-{exec_id}");

        let (tx, rx) = oneshot::channel();

        self.register_exec_tool_calls(&id).await;

        let (stdin, pending, child, recent_stderr, tool_rx) = {
            let mut guard = self.kernel.lock().await;
            if guard.is_none() {
                *guard = Some(
                    self.start_kernel(sess, cwd)
                        .await
                        .map_err(|error| ReplExecError {
                    output: String::new(),
                    error,
                    content_items: Vec::new(),
                })?,
                );
            }
            let Some(kernel) = guard.as_ref() else {
                self.clear_exec_tool_calls(&id).await;
                return Err(ReplExecError {
                    output: String::new(),
                    error: "repl kernel failed to start".to_owned(),
                    content_items: Vec::new(),
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
                    "failed to send repl request",
                    "write_error",
                    Some(&err),
                    &snapshot,
                )
            } else {
                format!("failed to send repl request: {err}")
            };
            if let Err(e) = self.reset().await { warn!("repl reset failed: {e}"); }
            return Err(ReplExecError { output: String::new(), error, content_items: Vec::new() });
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
                    return Err(ReplExecError {
                        output: String::new(),
                        error: format!("repl timed out after {timeout_ms}ms"),
                        content_items: Vec::new(),
                    });
                }
                tool_req = tool_rx_guard.recv() => {
                    let Some(tool_req) = tool_req else {
                        pending.lock().await.remove(&id);
                        drop(tool_rx_guard);
                        self.settle_exec(&id).await;
                        let snapshot = Self::kernel_debug_snapshot(&child, &recent_stderr).await;
                        let msg = with_model_failure_message(
                            "repl kernel terminated while waiting for tool requests",
                            "tool_channel_closed",
                            None,
                            &snapshot,
                        );
                        if let Err(e) = self.reset().await { warn!("repl reset failed: {e}"); }
                        return Err(ReplExecError { output: String::new(), error: msg, content_items: Vec::new() });
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
                                "repl kernel stopped before returning a result",
                                "response_channel_closed",
                                None,
                                &snapshot,
                            );
                            if let Err(e) = self.reset().await { warn!("repl reset failed: {e}"); }
                            return Err(ReplExecError { output: String::new(), error: msg, content_items: Vec::new() });
                        }
                    }
                }
            }
        };
        drop(tool_rx_guard);

        self.settle_exec(&id).await;

        match result {
            ExecResultMessage::Ok { output, content_items } => Ok(ReplExecResult { output, content_items }),
            ExecResultMessage::Err { output, message, content_items } => Err(ReplExecError {
                output,
                error: message,
                content_items,
            }),
        }
    }

    // ── Lifecycle helpers ───────────────────────────────────────────────

    async fn settle_exec(&self, exec_id: &str) {
        self.wait_for_exec_tool_calls(exec_id).await;
        self.clear_exec_tool_calls(exec_id).await;
    }

    async fn settle_and_reset(&self, exec_id: &str) {
        self.settle_exec(exec_id).await;
        if let Err(e) = self.reset().await {
            warn!("repl reset failed: {e}");
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
        Self::clear_all_exec_tool_calls(&self.exec_tool_calls).await;

        let mut guard = self.kernel.lock().await;
        let Some(kernel) = guard.take() else {
            return;
        };

        kernel.shutdown.cancel();

        let pending = Arc::clone(&kernel.pending_execs);
        let mut pending = pending.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(ExecResultMessage::Err {
                output: String::new(),
                message: "repl kernel was reset".to_owned(),
                content_items: Vec::new(),
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
                    "failed to inspect repl kernel before kill"
                );
            }
        }
        if let Err(err) = guard.kill().await {
            debug!("failed to kill repl kernel (reason={reason}): {err}");
        }
        let _ = guard.wait().await;
    }

    // ── Kernel spawning ─────────────────────────────────────────────────

    async fn start_kernel(
        &self,
        sess: &Session,
        cwd: &Path,
    ) -> Result<Kernel, String> {
        let mut command = build_runtime_command(
            &self.runtime,
            &self.kernel_path,
            self.tmp_dir.path(),
            sess,
            cwd,
        )?;
        let mut child = command
            .spawn()
            .map_err(|err| {
                format!(
                    "failed to spawn repl kernel (runtime={runtime:?} exe={exe}): {err}",
                    runtime = self.runtime.kind,
                    exe = self.runtime.executable.display(),
                )
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "repl kernel missing stdin".to_owned())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "repl kernel missing stdout".to_owned())?;
        let stderr = child.stderr.take();

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
            warn!("repl kernel missing stderr");
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

    // ── Tool request dispatch ───────────────────────────────────────────

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
                    "error": "repl tool call cancelled (exec reset or timeout)",
                });
                if let Err(err) = send_json_line(stdin, &response).await {
                    warn!(
                        tool_call_id = %tool_req.id,
                        error = %err,
                        "failed to send cancel reply to kernel"
                    );
                }
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
        if let Err(err) = send_json_line(stdin, &response).await {
            warn!(
                tool_call_id = %tool_req.id,
                error = %err,
                "failed to send tool result to kernel"
            );
        }
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
                warn!("repl kernel sent invalid json: {line}");
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

                    let resolved_id = if id == "__fatal__" {
                        let lock = pending_execs.lock().await;
                        match lock.len() {
                            0 => None,
                            1 => lock.keys().next().cloned(),
                            n => {
                                warn!(
                                    count = n,
                                    "repl kernel sent __fatal__ but multiple execs pending; \
                                     this should not happen — routing to none"
                                );
                                None
                            }
                        }
                    } else {
                        Some(id.to_owned())
                    };
                    let Some(resolved_id) = resolved_id else {
                        warn!("repl kernel sent __fatal__ with no pending exec");
                        continue;
                    };

                    ReplManager::wait_for_exec_tool_calls_map(&exec_tool_calls, &resolved_id).await;

                    let ok = message.get("ok").and_then(JsonValue::as_bool).unwrap_or(false);
                    let output = message
                        .get("output")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default().to_owned();
                    let error = message
                        .get("error")
                        .and_then(JsonValue::as_str)
                        .map(ToString::to_string);

                    // Drain content items (emitted images, etc.) accumulated
                    // during this exec before the tool-calls state is cleared.
                    let content_items = {
                        let mut calls = exec_tool_calls.lock().await;
                        calls
                            .get_mut(&resolved_id)
                            .map(|state| std::mem::take(&mut state.content_items))
                            .unwrap_or_default()
                    };

                    let sender = pending_execs.lock().await.remove(&resolved_id);
                    if let Some(sender) = sender {
                        let payload = if ok {
                            ExecResultMessage::Ok { output, content_items }
                        } else {
                            ExecResultMessage::Err {
                                output,
                                message: error
                                    .unwrap_or_else(|| "repl execution failed".to_string()),
                                content_items,
                            }
                        };
                        let _ = sender.send(payload);
                    }
                    ReplManager::clear_exec_tool_calls_map(&exec_tool_calls, &resolved_id).await;
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

                    let Some(exec_cancel) =
                        ReplManager::begin_exec_tool_call(&exec_tool_calls, &exec_id).await
                    else {
                        let snapshot =
                            ReplManager::kernel_debug_snapshot(&child, &recent_stderr).await;
                        warn!(
                            exec_id = %exec_id,
                            tool_call_id = %id,
                            tool_name = %tool_name,
                            kernel_pid = ?snapshot.pid,
                            kernel_status = %snapshot.status,
                            "repl tool request for unknown/finished exec"
                        );
                        let response = json!({
                            "type": "run_tool_result",
                            "id": id,
                            "ok": false,
                            "response": JsonValue::Null,
                            "error": "repl exec context not found",
                        });
                        if let Err(err) = send_json_line(&stdin, &response).await {
                            warn!(
                                tool_call_id = %id,
                                error = %err,
                                "failed to send context-not-found reply to kernel"
                            );
                        }
                        continue;
                    };

                    if is_repl_internal_tool(&tool_name) {
                        let response = json!({
                            "type": "run_tool_result",
                            "id": id,
                            "ok": false,
                            "response": JsonValue::Null,
                            "error": "repl cannot invoke itself",
                        });
                        if let Err(err) = send_json_line(&stdin, &response).await {
                            let snapshot =
                                ReplManager::kernel_debug_snapshot(&child, &recent_stderr).await;
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
                        ReplManager::finish_exec_tool_call(&exec_tool_calls, &exec_id).await;
                        continue;
                    }

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
                        if let Err(err) = send_json_line(&stdin, &response).await {
                            warn!(
                                tool_call_id = %id,
                                error = %err,
                                "failed to send enqueue-error reply to kernel"
                            );
                        }
                        ReplManager::finish_exec_tool_call(&exec_tool_calls, &exec_id).await;
                    }
                }
                "emit_image" => {
                    let Some(id) = message.get("id").and_then(JsonValue::as_str) else {
                        continue;
                    };
                    let exec_id = message
                        .get("exec_id")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default();
                    let image_url = message
                        .get("image_url")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default();
                    let detail_str = message
                        .get("detail")
                        .and_then(JsonValue::as_str);

                    // 20 MB encoded limit to prevent OOM and API rejection.
                    const MAX_DATA_URL_BYTES: usize = 20 * 1024 * 1024;

                    let response = if !image_url
                        .get(..5)
                        .is_some_and(|s| s.eq_ignore_ascii_case("data:"))
                    {
                        json!({
                            "type": "emit_image_result",
                            "id": id,
                            "ok": false,
                            "error": "codex.emitImage only accepts data URLs",
                        })
                    } else if image_url.len() > MAX_DATA_URL_BYTES {
                        json!({
                            "type": "emit_image_result",
                            "id": id,
                            "ok": false,
                            "error": format!(
                                "image data URL exceeds {MAX_DATA_URL_BYTES} byte limit ({} bytes)",
                                image_url.len(),
                            ),
                        })
                    } else {
                        let detail = detail_str.and_then(|s| match s {
                            "low" => Some(code_protocol::models::ImageDetail::Low),
                            "high" => Some(code_protocol::models::ImageDetail::High),
                            "auto" => Some(code_protocol::models::ImageDetail::Auto),
                            _ => None,
                        });
                        let content_item =
                            code_protocol::models::FunctionCallOutputContentItem::InputImage {
                                image_url: image_url.to_owned(),
                                detail,
                            };
                        let stored = {
                            let mut calls = exec_tool_calls.lock().await;
                            if let Some(state) = calls.get_mut(exec_id) {
                                state.content_items.push(content_item);
                                true
                            } else {
                                false
                            }
                        };
                        if stored {
                            json!({
                                "type": "emit_image_result",
                                "id": id,
                                "ok": true,
                            })
                        } else {
                            json!({
                                "type": "emit_image_result",
                                "id": id,
                                "ok": false,
                                "error": "exec context not found (possibly timed out)",
                            })
                        }
                    };

                    if let Err(err) = send_json_line(&stdin, &response).await {
                        let snapshot =
                            ReplManager::kernel_debug_snapshot(&child, &recent_stderr).await;
                        warn!(
                            exec_id = %exec_id,
                            emit_id = %id,
                            error = %err,
                            kernel_pid = ?snapshot.pid,
                            kernel_status = %snapshot.status,
                            kernel_stderr_tail = %snapshot.stderr_tail,
                            "failed to reply to kernel emit_image request"
                        );
                    }
                }
                other => {
                    warn!(message_type = ?other, "repl kernel sent unrecognized message type");
                }
            }
        };

        // Kernel stream ended — settle tool calls, notify pending execs.
        let exec_ids = {
            let calls = exec_tool_calls.lock().await;
            calls.keys().cloned().collect::<Vec<_>>()
        };
        for exec_id in &exec_ids {
            ReplManager::wait_for_exec_tool_calls_map(&exec_tool_calls, exec_id).await;
            ReplManager::clear_exec_tool_calls_map(&exec_tool_calls, exec_id).await;
        }

        let unexpected_snapshot = if matches!(end_reason, KernelStreamEnd::Shutdown) {
            None
        } else {
            Some(Self::kernel_debug_snapshot(&child, &recent_stderr).await)
        };
        let kernel_failure_message = unexpected_snapshot.as_ref().map(|snapshot| {
            with_model_failure_message(
                "repl kernel exited unexpectedly",
                end_reason.reason(),
                end_reason.error(),
                snapshot,
            )
        });
        let kernel_exit_message = kernel_failure_message
            .clone()
            .unwrap_or_else(|| "repl kernel exited unexpectedly".to_string());

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
                content_items: Vec::new(),
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
                "repl kernel terminated unexpectedly"
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
                        warn!("repl kernel stderr ended: {err}");
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
                warn!("repl stderr: {bounded_line}");
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

fn is_repl_internal_tool(name: &str) -> bool {
    name.eq_ignore_ascii_case(crate::openai_tools::REPL_TOOL_NAME)
        || name.eq_ignore_ascii_case(crate::openai_tools::REPL_RESET_TOOL_NAME)
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

#[cfg(test)]
mod tests {
    use super::runtime::{parse_version_triplet, version_at_least};
    use super::types::{ExecResultMessage, ReplRuntimeConfig, ToolRequest};

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
            content_items: Vec::new(),
        };
        match msg {
            ExecResultMessage::Err { output, message, .. } => {
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
            content_items: Vec::new(),
        };
        match msg {
            ExecResultMessage::Ok { output, .. } => assert_eq!(output, "42"),
            ExecResultMessage::Err { .. } => panic!("expected Ok variant"),
        }
    }

    #[test]
    fn runtime_config_clone_preserves_fields() {
        let cfg = ReplRuntimeConfig {
            kind: crate::config::ReplRuntimeKindToml::Node,
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
