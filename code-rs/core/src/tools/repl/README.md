# JS REPL Subsystem

The `repl` tool provides an in-process JavaScript REPL backed by either
Node.js (with `--experimental-vm-modules`) or Deno. Each runtime is managed by
a separate `ReplManager` accessed through a `HashMap<ReplRuntimeKindToml, ReplHandle>`
on the session.

## Architecture

```
Session
  └─ repl_handles: HashMap<RuntimeKind, ReplHandle>
       └─ ReplHandle (lazy init via tokio::OnceCell)
            └─ ReplManager
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
- **Kernel → Host:** `{"type":"emit_image","id":"...","exec_id":"...","image_url":"data:...","detail":"auto"}`
- **Host → Kernel:** `{"type":"emit_image_result","id":"...","ok":true/false,...}`

## Key Behaviors

### Timer and Async Lifecycle

**Timers do not survive cell boundaries.** All tracked `setTimeout` and
`setInterval` handles are cancelled when an exec completes (success or error)
and again when the next exec starts.  This is intentional — the REPL is not a
long-running event loop; it is a sequence of discrete evaluations.

The mechanism:
- Both kernels wrap `setTimeout`/`setInterval`/`queueMicrotask` so every
  callback is tagged with the `execGeneration` that created it.
- `_cancelStaleTimers()` clears **all** pending tracked timers. It runs at
  exec start (to catch anything that leaked) and in the `finally` block at
  exec completion.
- Late callbacks that fire after their generation ends are silently dropped.
- `codex.tool()` calls are similarly rejected after the owning generation ends.

Persistent state (bindings, npm module caches) survives between cells.
Local file module caches are cleared between execs so edits are picked up.
In-flight async work (timers, intervals, background tool calls) does not.

### Persistent Console Capture
`console.log/info/warn/error/debug` are permanently captured. Calls only accumulate
output when `execGeneration === _captureGeneration`. Background callbacks from dead
generations are silently dropped, preventing protocol stdout corruption.

### Snapshot-Based Persistence
REPL state is carried between cells via `__replBindings` on the global/context.
Each cell's bindings are snapshot-ed after evaluation; the next cell's prelude
reads values from the snapshot (not from a module import chain).

### Background Task Awaiting
Un-awaited `codex.tool()` and `codex.emitImage()` calls are tracked per-exec via
`pendingBackgroundTasks`. After module evaluation, `Promise.all` awaits all pending
tasks. Unobserved failures (where the caller never awaited) are surfaced as the
cell's error output.

### Image Emission (`codex.emitImage()`)
Kernels expose `codex.emitImage(imageOrUrl, detail?)` to emit images back to the
model. Accepts data URLs or binary buffers (Uint8Array, ArrayBuffer, Buffer).
The host validates format (data: URLs only), enforces a 20 MB size cap, and
accumulates content items per-exec. Images are preserved on both success and
error paths so the model can see completed work.

### Partial Binding Recovery (Node only)
When a cell errors after partially evaluating, the Node kernel uses V8's TDZ
semantics to read initialized bindings from the errored module's namespace.
Successfully initialized `let`/`const` bindings are merged into the snapshot;
uninitialized ones are skipped. Deno cannot do this because `import()` rejects
without exposing the module namespace.

### Kernel Hardening
- `Object.freeze(codex)` prevents user code from replacing kernel methods
- `__replBindings` is deleted in the `finally` block after module evaluation
- Self-invocation guard blocks the kernel from calling `repl` or `repl_reset`

---

## Runtime Contract

Both Node and Deno must satisfy the following shared contract. Any new runtime
added in the future must implement the same behaviors.

### Required capabilities (all runtimes)

| Capability | Description |
|------------|-------------|
| **JSON-lines protocol** | `exec` → `exec_result` on stdin/stdout; `run_tool` / `run_tool_result` for nested tool calls; `emit_image` / `emit_image_result` for image emission |
| **Generation-scoped async** | Timer/interval/microtask callbacks tagged with exec generation; stale callbacks dropped |
| **Timer cleanup** | All tracked timers cancelled on exec completion (success or error) |
| **Persistent console** | `console.*` output captured per-exec; late-generation output dropped |
| **Snapshot persistence** | Bindings carried between cells via `__replBindings` on the global scope |
| **Background task awaiting** | Un-awaited `codex.tool()` / `codex.emitImage()` calls tracked and awaited before result delivery |
| **Fatal error reporting** | `uncaughtException` / `unhandledRejection` → `exec_result` with error before exit |
| **Graceful reset** | Kernel process can be killed and restarted; state is lost on reset |

### Per-runtime differences

| Aspect | Node | Deno |
|--------|------|------|
| **Import model** | VM-linked local files + host-loaded packages (see below) | Data-URL evaluation via Deno runtime |
| **Containment** | Convenience/dev mode (not a sandbox) | Real permission-based sandbox |
| **Package imports** | Bare specifiers via `createRequire()` from `node_module_dirs` | Handled by Deno's native resolver |
| **Local file imports** | `SourceTextModule` in VM context, canonical path caching | Not supported |
| **Builtin imports** | `node:*` with deny list | Deno builtins via native runtime |
| **Platform sandbox** | macOS seatbelt only; no sandbox on other platforms | Deno `--allow-*` flags on all platforms |

### Node Import Boundary (Known Limitation)

Node's import system has a **split trust boundary** that is the largest
unresolved architectural issue:

- **Local files** (`./foo.js`, `../lib.mjs`) are loaded as `SourceTextModule`
  in the VM context. They share the REPL's console capture, globals, and
  generation tracking. This is correct.

- **Packages and builtins** (`lodash`, `node:fs`) are loaded via host
  `import()` and wrapped back as `SyntheticModule`. The actual package code
  executes in the **host realm**, not the VM context. This means:
  - Package code has access to the real `process`, `require`, etc.
  - Package `console.log()` writes to real stdout, not the captured console
  - Transitive imports inside packages bypass the REPL's deny list
  - The seatbelt (macOS only) constrains the process, but not the code path

This is documented honestly because the fix is non-trivial: making packages
load inside the VM module graph requires changes to Node's experimental
`--experimental-vm-modules` linker API. Until then, Node should be treated as
a convenience runtime, not a containment boundary.

### Runtime Health Preflight

At session build time, the host probes the default runtime binary with
`--version`. If the runtime is missing, too old, or broken, the `repl` tool
is automatically disabled so the model doesn't repeatedly invoke a dead REPL.
Warnings are logged to help diagnose configuration issues.

### Error Handling
- **Manager:** Bounded stderr tail buffer, `KernelDebugSnapshot` with PID/status/stderr,
  structured JSON diagnostics on all failure paths, exec tool-call settlement waits
- **Kernel fatal:** `uncaughtException`/`unhandledRejection` (Node) and
  `error`/`unhandledrejection` events (Deno) trigger a controlled shutdown with
  a diagnostic `exec_result` message before exit

## Files

| File | Purpose |
|------|---------|
| `mod.rs` | Host-side manager: kernel lifecycle, execute(), read_stdout/stderr, tool dispatch |
| `types.rs` | Shared data types: configs, results, protocol structs, tool-call tracking |
| `runtime.rs` | Runtime resolution, version probing, OS command building with sandbox support |
| `diagnostics.rs` | Stderr tail ring-buffer, truncation helpers, model failure formatting |
| `kernel_node.js` | Node kernel: VM sandbox, exec handler, timer wrappers, console capture |
| `node_resolver.js` | Node module resolution: specifier parsing, linking, caching, import pipeline |
| `kernel_deno.js` | Deno kernel: permission-based sandbox, data-URL evaluation |
| `kernel_common.js` | Shared JS utilities: AST binding collection, timer system, image helpers |
| `meriyah.umd.min.js` | Parser for binding collection (shared by both kernels) |
| `handlers/repl.rs` | Pragma parsing, runtime dispatch, tool handler registration |
