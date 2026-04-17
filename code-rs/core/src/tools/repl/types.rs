use code_protocol::models::FunctionCallOutputContentItem;
use serde::Deserialize;
use std::path::PathBuf;

/// User-facing runtime configuration.
#[derive(Clone, Debug)]
pub struct ReplRuntimeConfig {
    pub kind: crate::config::ReplRuntimeKindToml,
    pub runtime_path: Option<PathBuf>,
    pub runtime_args: Vec<String>,
    /// Extra module/package search directories (Node: node_modules parents;
    /// Python: virtualenv paths; etc.).
    pub module_dirs: Vec<PathBuf>,
    /// Deno sandbox permission toggles.  Only meaningful when `kind` is `Deno`.
    pub deno_permissions: crate::config::DenoPermissions,
}

/// Resolved runtime after probing the binary for version/capabilities.
#[derive(Clone, Debug)]
pub(super) struct ResolvedRuntime {
    pub kind: crate::config::ReplRuntimeKindToml,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub version: String,
    pub module_dirs: Vec<PathBuf>,
    pub deno_permissions: crate::config::DenoPermissions,
}

#[derive(Clone, Debug)]
pub(crate) struct ReplExecResult {
    pub output: String,
    /// Content items emitted via `codex.emitImage()` during execution.
    pub content_items: Vec<FunctionCallOutputContentItem>,
}

#[derive(Clone, Debug)]
pub(crate) struct ReplExecError {
    pub output: String,
    pub error: String,
    /// Content items emitted before the error (e.g. images).
    pub content_items: Vec<FunctionCallOutputContentItem>,
}

#[derive(Debug)]
pub(super) enum ExecResultMessage {
    Ok {
        output: String,
        content_items: Vec<FunctionCallOutputContentItem>,
    },
    Err {
        output: String,
        message: String,
        /// Images emitted before the error occurred — preserved so the
        /// model can see completed work even when execution fails.
        content_items: Vec<FunctionCallOutputContentItem>,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReplArgs {
    pub code: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub runtime: Option<crate::config::ReplRuntimeKindToml>,
}

/// Per-exec nested tool-call request forwarded to the host.
#[derive(Clone, Debug)]
pub(super) struct ToolRequest {
    pub id: String,
    pub exec_id: String,
    pub tool_name: String,
    pub arguments: String,
    pub cancel: tokio_util::sync::CancellationToken,
}

/// Per-exec nested tool-call tracking with cancellation + settlement.
#[derive(Default)]
pub(super) struct ExecToolCalls {
    pub in_flight: usize,
    /// Content items accumulated during this exec (e.g. emitted images).
    pub content_items: Vec<FunctionCallOutputContentItem>,
    pub cancel: tokio_util::sync::CancellationToken,
    pub notify: std::sync::Arc<tokio::sync::Notify>,
}

/// Reason the kernel stdout loop ended.
pub(super) enum KernelStreamEnd {
    Shutdown,
    StdoutEof,
    StdoutReadError(String),
}

impl KernelStreamEnd {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Shutdown => "shutdown",
            Self::StdoutEof => "stdout_eof",
            Self::StdoutReadError(_) => "stdout_read_error",
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::StdoutReadError(err) => Some(err),
            _ => None,
        }
    }
}

/// Snapshot of kernel process state for diagnostics.
pub(super) struct KernelDebugSnapshot {
    pub pid: Option<u32>,
    pub status: String,
    pub stderr_tail: String,
}
