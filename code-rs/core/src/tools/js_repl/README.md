# JS REPL Subsystem

The `js_repl` tool provides an in-process JavaScript REPL backed by either
Node.js (with `--experimental-vm-modules`) or Deno. Each runtime is managed by
a separate `JsReplManager` accessed through a `HashMap<JsReplRuntimeKindToml, JsReplHandle>`
on the session.

## Architecture

```
Session
  └─ js_repl_handles: HashMap<RuntimeKind, JsReplHandle>
       └─ JsReplHandle (lazy init via tokio::OnceCell)
            └─ JsReplManager
                 ├─ kernel: Arc<Mutex<Option<Kernel>>>
                 ├─ exec_tool_calls: Arc<Mutex<HashMap<String, ExecToolCalls>>>
                 └─ Kernel
                      ├─ child: Arc<Mutex<Child>>  (Node/Deno process)
                      ├─ recent_stderr: Arc<Mutex<VecDeque<String>>>
                      ├─ tool_rx: mpsc channel for tool requests
                      └─ shutdown: CancellationToken
```

## Protocol

Communication is JSON-lines over stdin/stdout:

- **Host → Kernel:** `{"type":"exec","id":"...","code":"..."}`
- **Kernel → Host:** `{"type":"exec_result","id":"...","ok":true/false,"output":"...","error":null/string}`
- **Kernel → Host:** `{"type":"run_tool","id":"...","exec_id":"...","tool_name":"...","arguments":"..."}`
- **Host → Kernel:** `{"type":"run_tool_result","id":"...","ok":true/false,...}`

## Key Behaviors

### Generation-Scoped Async
Every exec increments `execGeneration`. Timer callbacks (`setTimeout`, `setInterval`,
`queueMicrotask`) and `codex.tool()` calls check the generation — stale callbacks
from previous execs are silently dropped. `_cancelStaleTimers()` clears all pending
timers at the start of each exec.

### Persistent Console Capture
`console.log/info/warn/error/debug` are permanently captured. Calls only accumulate
output when `execGeneration === _captureGeneration`. Background callbacks from dead
generations are silently dropped, preventing protocol stdout corruption.

### Snapshot-Based Persistence
REPL state is carried between cells via `__replBindings` on the global/context.
Each cell's bindings are snapshot-ed after evaluation; the next cell's prelude
reads values from the snapshot (not from a module import chain).

### Node Module Linker
The Node kernel supports three resolution kinds:
- **builtin** — `node:*` modules (with a deny list for process, child_process, etc.)
- **file** — local `.js`/`.mjs` files via relative/absolute/`file://` paths, loaded as
  `SourceTextModule` in the VM context with module caching
- **package** — bare specifiers resolved via `createRequire()` across configured
  `node_module_dirs`, sandboxed to `node_modules/` boundaries

### Deno Kernel
Uses Deno's native permission model. Imports are handled by Deno itself via
data-URL `import()`. Has the same generation-scoped async, persistent console,
and fatal error handlers as the Node kernel.

### Error Handling
- **Manager:** Bounded stderr tail buffer, `KernelDebugSnapshot` with PID/status/stderr,
  structured JSON diagnostics on all failure paths, exec tool-call settlement waits
- **Kernel fatal:** `uncaughtException`/`unhandledRejection` (Node) and
  `error`/`unhandledrejection` events (Deno) trigger a controlled shutdown with
  a diagnostic `exec_result` message before exit

## Files

| File | Purpose |
|------|---------|
| `mod.rs` | Host-side manager: kernel lifecycle, execute(), read_stdout/stderr, error handling |
| `kernel.js` | Node kernel: VM sandbox, module linker, timer wrappers, console capture |
| `kernel_deno.js` | Deno kernel: permission-based sandbox, data-URL evaluation |
| `meriyah.umd.min.js` | Parser for binding collection (shared by both kernels) |
| `handlers/js_repl.rs` | Pragma parsing, runtime dispatch, tool handler registration |
