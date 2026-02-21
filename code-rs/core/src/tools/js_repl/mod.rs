use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_json::json;
use std::collections::HashMap;
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
use tokio::sync::oneshot;
use tracing::debug;

pub(crate) const JS_REPL_PRAGMA_PREFIX: &str = "// codex-js-repl:";

const KERNEL_SOURCE: &str = include_str!("kernel.js");
const MERIYAH_UMD: &str = include_str!("meriyah.umd.min.js");

const DEFAULT_TIMEOUT_MS: u64 = 15_000;
const MAX_TIMEOUT_MS: u64 = 120_000;

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
    node_path: Option<PathBuf>,
    cell: OnceCell<Arc<JsReplManager>>,
}

impl JsReplHandle {
    pub(crate) fn new(node_path: Option<PathBuf>) -> Self {
        Self {
            node_path,
            cell: OnceCell::new(),
        }
    }

    pub(crate) async fn manager(&self) -> Result<Arc<JsReplManager>, String> {
        self.cell
            .get_or_try_init(|| async {
                JsReplManager::new(self.node_path.clone()).await
            })
            .await
            .cloned()
    }

    pub(crate) fn manager_if_started(&self) -> Option<Arc<JsReplManager>> {
        self.cell.get().cloned()
    }
}

pub(crate) struct JsReplManager {
    node_path: Option<PathBuf>,
    tmp_dir: tempfile::TempDir,
    kernel_path: PathBuf,
    kernel: Mutex<Option<Kernel>>,
    exec_lock: Arc<Semaphore>,
    next_id: AtomicU64,
}

struct Kernel {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    pending_execs: Arc<Mutex<HashMap<String, oneshot::Sender<ExecResultMessage>>>>,
    stdout_task: tokio::task::JoinHandle<()>,
    stderr_task: tokio::task::JoinHandle<()>,
}

impl JsReplManager {
    pub(crate) async fn new(node_path: Option<PathBuf>) -> Result<Arc<Self>, String> {
        let tmp_dir = tempfile::tempdir()
            .map_err(|err| format!("failed to create js_repl temp dir: {err}"))?;

        let kernel_path = tmp_dir.path().join("kernel.js");
        let meriyah_path = tmp_dir.path().join("meriyah.umd.min.js");
        tokio::fs::write(&kernel_path, KERNEL_SOURCE)
            .await
            .map_err(|err| format!("failed to write js_repl kernel: {err}"))?;
        tokio::fs::write(&meriyah_path, MERIYAH_UMD)
            .await
            .map_err(|err| format!("failed to write js_repl parser: {err}"))?;

        Ok(Arc::new(Self {
            node_path,
            tmp_dir,
            kernel_path,
            kernel: Mutex::new(None),
            exec_lock: Arc::new(Semaphore::new(1)),
            next_id: AtomicU64::new(0),
        }))
    }

    pub(crate) async fn execute(
        &self,
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

        let (stdin, pending) = {
            let mut guard = self.kernel.lock().await;
            if guard.is_none() {
                *guard = Some(self.start_kernel(cwd).await.map_err(|error| JsExecError {
                    output: String::new(),
                    error,
                })?);
            }
            let Some(kernel) = guard.as_ref() else {
                return Err(JsExecError {
                    output: String::new(),
                    error: "js_repl kernel failed to start".to_string(),
                });
            };
            (Arc::clone(&kernel.stdin), Arc::clone(&kernel.pending_execs))
        };

        pending.lock().await.insert(id.clone(), tx);

        let message = json!({
            "type": "exec",
            "id": id.clone(),
            "code": args.code,
        });

        if let Err(err) = send_json_line(&stdin, &message).await {
            pending.lock().await.remove(&id);
            let _ = self.reset().await;
            return Err(JsExecError {
                output: String::new(),
                error: format!("failed to send js_repl request: {err}"),
            });
        }

        let result = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(msg)) => msg,
            Ok(Err(_)) => {
                let _ = self.reset().await;
                return Err(JsExecError {
                    output: String::new(),
                    error: "js_repl kernel stopped before returning a result".to_string(),
                });
            }
            Err(_) => {
                pending.lock().await.remove(&id);
                let _ = self.reset().await;
                return Err(JsExecError {
                    output: String::new(),
                    error: format!("js_repl timed out after {timeout_ms}ms"),
                });
            }
        };

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

    async fn start_kernel(&self, cwd: &Path) -> Result<Kernel, String> {
        let node = self
            .node_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("node"));

        let mut command = Command::new(node);
        command
            .arg("--experimental-vm-modules")
            .arg(&self.kernel_path)
            .current_dir(cwd)
            .env("CODEX_JS_TMP_DIR", self.tmp_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|err| format!("failed to spawn js_repl node kernel: {err}"))?;

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

        let stdout_task = tokio::spawn(kernel_stdout_loop(
            stdout,
            Arc::clone(&pending_execs),
            Arc::clone(&stdin),
        ));
        let stderr_task = tokio::spawn(kernel_stderr_loop(stderr));

        Ok(Kernel {
            child,
            stdin,
            pending_execs,
            stdout_task,
            stderr_task,
        })
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
                let tool_name = message
                    .get("tool_name")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("<unknown>");
                let response = json!({
                    "type": "run_tool_result",
                    "id": id,
                    "ok": false,
                    "response": JsonValue::Null,
                    "error": format!("js_repl does not support codex.tool (requested `{tool_name}`)"),
                });
                let _ = send_json_line(&stdin, &response).await;
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
