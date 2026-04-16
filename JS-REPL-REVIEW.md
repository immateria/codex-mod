# JS REPL Subsystem Review

Review of `code-rs/core/src/tools/js_repl/` — the kernel-based JavaScript
execution subsystem — evaluated against the stated goal of supporting multiple
runtimes, not all JavaScript.

---

## What's Done Right

### 1. Clean Host/Kernel Separation via JSON-Lines Protocol

The protocol design (`exec`, `exec_result`, `run_tool`, `run_tool_result`) is
runtime-agnostic. Nothing in the wire format assumes JavaScript — it's
fundamentally "send code, get result, optionally call tools in-flight." This is
the single most important thing for multi-runtime extensibility and it's already
solid.

Unknown message types are silently dropped by both kernels and the host — good
for forward compatibility, though it makes debugging protocol mismatches harder
since there's no log of unrecognized types.

### 2. Data-Driven Capability System

`RuntimeCapabilities` and `RuntimeSandboxKind` in `config.rs` cleanly describe
what a runtime can and cannot do:

```rust
pub struct RuntimeCapabilities {
    pub sandbox: RuntimeSandboxKind,
    pub supports_seatbelt: bool,
    pub can_enforce_network_without_seatbelt: bool,
    pub sandbox_env_passthrough: &'static [&'static str],
    pub uses_node_module_dirs: bool,
}
```

The command builder in `build_runtime_command()` uses these capabilities for its
branching conditions (e.g., seatbelt eligibility, `BuiltinPermissions` vs
`ExternalOnly`). **However** — see issue #3 below — the branch bodies still
contain runtime-specific CLI flags.

### 3. Per-Runtime Config with Correct Isolation

Each runtime kind gets its own `JsReplHandle` in a `HashMap<RuntimeKind, Handle>`,
lazily initialized via `OnceCell`. Runtime-specific config fields
(`js_repl_node_path`, `js_repl_deno_path`, `js_repl_node_args`, etc.) are
correctly routed through `js_repl_runtime_config()`. The per-exec pragma
(`// codex-js-repl: runtime=deno`) allows runtime selection at call time while
keeping the default configurable. This is the right granularity.

### 4. Generation-Scoped Async Lifecycle

Both kernels implement generation tagging for timers, microtasks, and tool calls.
Stale callbacks from dead generations are silently dropped. This is a subtle but
critical correctness property — without it, a `setInterval` from cell N would
corrupt cell N+1's output capture. The implementation is nearly identical across
both kernels, which is good for behavioral consistency.

### 5. Robust Error Pipeline

The Rust side has thorough error handling:
- Bounded stderr ring buffer (`STDERR_TAIL_*` constants) prevents unbounded
  memory growth from chatty runtimes.
- `KernelDebugSnapshot` captures PID/status/stderr at failure time and feeds
  structured JSON diagnostics to the model.
- Every error path calls `settle_exec()` to wait for in-flight tool calls
  before cleanup — no leaked bookkeeping.
- The `__fatal__` sentinel from kernel → host for unhandled errors is handled
  correctly, including the multi-pending-exec edge case.

### 6. Tool-Call Synchronization in `read_stdout`

This is the most subtle and well-designed piece of the system. When `read_stdout`
receives an `exec_result` from the kernel, it does NOT immediately deliver it
to the oneshot channel. Instead it first waits:

```rust
// line 988-989 in read_stdout
JsReplManager::wait_for_exec_tool_calls_map(&exec_tool_calls, &resolved_id).await;
```

This ensures that fire-and-forget tool calls (where user code calls
`codex.tool()` without `await`) are fully settled before the result is
delivered. Without this barrier, the `execute()` select-loop could break on the
exec_result while tool requests are still in the `tool_tx` channel, creating a
hang in `settle_exec`. The current design avoids that entirely.

### 7. Health Preflight

The build-session code probes the default runtime with `--version` and
auto-disables `js_repl` if it's unavailable. This prevents the model from
repeatedly invoking a broken REPL and wasting tokens on error messages.

### 8. Self-Invocation Guard

`is_js_repl_internal_tool()` blocks the kernel from calling `js_repl` or
`js_repl_reset` through the tool interface, preventing infinite recursion.
Checked in `read_stdout` before dispatch — correct placement.

### 9. Deliberate Sandbox Asymmetry Between Runtimes

The Deno kernel runs with minimal permissions: `--allow-env=<codex vars>` and
`--allow-read=<tmp_dir>` only. No file read (outside tmp), no write, no
network, no subprocess execution. All side effects are mediated through
`codex.tool()`. This makes Deno a pure sandboxed-compute environment.

Node, by contrast, runs in a VM context with broader access (fs reads, etc.)
constrained only by the deny list and optional seatbelt. This is an intentional
design choice — Deno for security, Node for convenience — and the capability
system correctly reflects it.

---

## What's Wrong or Problematic

### 1. The Name "js_repl" Is a Trap

Every type, constant, file, and log message is namespaced under `js_repl`.
If the plan is to support Python, Lua, WASM, or other runtimes through this
same infrastructure, the naming creates a misleading abstraction. The protocol
is language-agnostic but the naming says "this is only for JavaScript."

**Scope of the problem:** `JsReplManager`, `JsReplHandle`, `JsReplArgs`,
`JsReplRuntimeConfig`, `JsExecResult`, `JsExecError`, `JsReplToolHandler`,
`JS_REPL_PRAGMA_PREFIX`, `js_repl_handles`, `js_repl_enabled()`,
`js_repl_default_runtime()`, event types `JsReplExecBeginEvent`, the tool names
`js_repl` and `js_repl_reset`, the pragma format `// codex-js-repl:`, config
keys `tools_js_repl`, `js_repl_runtime`, etc.

This is ~50+ identifiers. Renaming later becomes a cross-cutting change touching
config files, protocol types, the TUI, and the model's tool schema. If
multi-language is a real goal, renaming sooner is cheaper.

**Suggested direction:** Something like `repl` or `code_eval` at the Rust
layer. The tool name visible to the model can stay `js_repl` for backward
compatibility via an alias.

### 2. Massive Code Duplication Between Kernels

`kernel_node.js` (919 lines) and `kernel_deno.js` (435 lines) share ~300 lines
of effectively identical logic:

- `collectPatternNames`, `collectDeclarationBindings`, `collectBindings` — identical
- `buildModuleSource`, `keywordForBindingKind` — identical
- Timer wrapper system (`_wrapTimer`, `_wrapClearTimer`, `_cancelStaleTimers`) — identical
- Console capture (`_capturedLogs`, `_captureGeneration` pattern) — identical
- `handleExec` structure — 90% identical (the only difference is the module
  evaluation path: data-URL import for Deno vs `SourceTextModule` for Node)
- `handleToolResult` — identical
- `codex.tool()` closure inside `handleExec` — identical
- Generation tracking and `queue` serialization — identical
- Fatal error handler pattern — structurally identical (different APIs:
  `process.on` vs `globalThis.addEventListener`)
- Stdin reading dispatch (`exec` → queue, `run_tool_result` → immediate) — identical

The Node kernel is longer because it includes the VM module linker
(`resolveSpecifier`, `loadLinkedModule`, etc.), which is Node-specific. But the
shared core should be extracted into a common module that both kernels import.
Right now, any behavioral fix (e.g., to binding collection or timer semantics)
must be applied in two places, and divergence is a matter of time.

**Why this matters for multi-runtime:** If a Python kernel is added, it won't
share JS parsing logic, but it will need the same protocol handling, tool call
dispatch, and result formatting. The current pattern would require copying the
protocol layer into every new kernel, which doesn't scale.

### 3. The Rust Side Has JS-Specific Logic Baked In

`JsReplManager` handles:
- Writing `kernel_node.js` or `kernel_deno.js` to a temp dir
- Writing `meriyah.umd.min.js` (a JS parser) alongside
- Passing `--experimental-vm-modules` for Node
- Node version checking (`MIN_NODE_VERSION`)
- Kernel filename selection based on `JsReplRuntimeKindToml`

For a non-JS runtime, none of this applies. A Python runtime doesn't need
meriyah, doesn't need VM module flags, and its kernel file would be `.py`.
The current code would need substantial `match` arm additions for each new
runtime, interleaved with the existing JS logic.

Additionally, `build_runtime_command()` uses capabilities for its branching
*conditions* but the branch bodies are still runtime-specific. The
`BuiltinPermissions` branch (lines 642-654) hardcodes Deno CLI flags (`run`,
`--quiet`, `--no-prompt`, `--allow-env=...`, `--allow-read=...`). A
hypothetical new runtime with `BuiltinPermissions` sandbox would need
completely different flags. The capability system decides *which* branch to
take, but the branch itself is not generic.

**What a better structure looks like:** A trait (e.g., `RuntimeKernel`) that
each runtime implements, providing:
- Kernel source bytes and companion files to write
- Command construction (executable, args, env, sandbox flags)
- Version detection and validation
- Capability descriptor

The manager would then be generic over this trait rather than branching on an
enum everywhere.

### 4. `JsReplRuntimeKindToml` Mixes Concerns

This enum serves as:
1. A config/serialization type (TOML key)
2. A runtime identifier at the type level
3. A capability descriptor (via `.capabilities()`)
4. A display name (via `.label()`)
5. A cycling iterator (via `.next()`)

For two variants this is manageable, but adding a third (let alone a non-JS
runtime) means touching: the enum definition, `capabilities()`, `label()`,
`next()`, `ALL`, `default_executable()`, `Display`, every `match` in
`build_runtime_command()`, `resolve_runtime()`, config field routing in
`js_repl_runtime_config()`, the TOML schema, TUI settings pages, and tests.

The `Toml` suffix also leaks serialization concerns into domain logic.

### 5. Snapshot/Binding Persistence Is JS-Only

The REPL state model (parse AST with meriyah, collect bindings, inject via
`__replBindings`, export merged names) is deeply JavaScript-specific. For a
Python runtime, persistence would be different (e.g., pickling globals, or
maintaining a persistent interpreter process with `exec()`). There's no
abstraction point for "how does this runtime carry state between cells."

This isn't necessarily wrong — it might be fine for each kernel to own its own
persistence strategy. But the Rust side currently has no concept of this; it
treats all kernels as black boxes that accept code and return output. If
runtime-specific persistence controls are needed (e.g., "reset Python globals
but keep JS bindings"), the Rust layer has no hook for it.

### 6. Node's Import Boundary Is Honestly Documented but Unresolved

The README correctly flags that package code runs in the host realm, not the VM
context. This means:
- Packages can access real `process`, `require`, etc.
- Package `console.log()` goes to real stdout, corrupting the JSON-lines protocol
- Transitive imports bypass the REPL's deny list

For a "convenience REPL" this is acceptable. For anything security-sensitive
it's a real gap. The documentation is good — just noting that this is the single
largest correctness issue in the current implementation. And it directly
contrasts with the Deno kernel's strict sandboxing, creating a confusing trust
model: "pick Node for convenience but accept that imports can escape the sandbox;
pick Deno for containment but accept that you can't read project files."

### 7. Deno Data-URL Module Accumulation

Every cell in the Deno kernel is evaluated via `import(toDataUrl(source))`.
Each data URL gets a unique `#cell-N` fragment, so V8's module cache
accumulates a new compiled module per exec — source text, bytecode, and module
namespace — none of which can be evicted. Over many cells in a long session,
this is unbounded memory growth proportional to the number of executions.

The Node kernel has a similar per-cell `SourceTextModule`, but those are
standalone objects that can be GC'd once the snapshot drops the reference.
Node also calls `clearLocalFileModuleCaches()` between execs to invalidate
local file modules. The Deno kernel has no equivalent — data-URL modules
can't be removed from V8's module map.

In practice, for typical REPL sessions (tens to low hundreds of cells),
this is not a problem. For long-running automated sessions that execute
thousands of cells without kernel reset, it could become one.

### 8. Single-Exec Semaphore Prevents Pipelining

`exec_lock: Arc<Semaphore::new(1)>` serializes all execution. This is correct
for the current cell-based REPL model, but worth noting as a constraint. If
future runtimes support concurrent evaluation (e.g., isolated WASM instances),
the semaphore would need to become per-runtime or configurable.

### 9. `pendingTool` Leak on Fire-and-Forget Tool Calls

In both kernels, `codex.tool()` registers a resolver in the `pendingTool` map.
If user code calls `codex.tool()` without `await`, the exec may complete and
send `exec_result` before the host sends `run_tool_result`. The host handles
this correctly (the tool-call synchronization barrier in `read_stdout` ensures
the tool is dispatched before the result is delivered). However, in the kernel
itself, the resolver remains in `pendingTool` until the result arrives. If the
kernel is reset before the result arrives, those entries are never cleaned up.
Not a practical leak (the kernel process dies on reset), but `pendingTool` is
never pruned on exec boundaries, which is inconsistent with the otherwise
careful lifecycle management.

---

## Structural Assessment for Multi-Runtime Expansion

### What Transfers Cleanly

| Component                                                 | Reusable? | Notes                                       |
| --------------------------------------------------------- | --------- | ------------------------------------------- |
| JSON-lines protocol                                       | Yes       | Language-agnostic                           |
| Tool call dispatch (`run_tool`/`run_tool_result`)         | Yes       | Already generic                             |
| Exec lock + timeout machinery                             | Yes       | Runtime-independent                         |
| Error pipeline (stderr tail, debug snapshot, diagnostics) | Yes       | Runtime-independent                         |
| Health preflight (`--version` probe)                      | Mostly    | Version parsing is runtime-specific         |
| Handler layer (pragma parsing, payload routing)           | Partially | Pragma format and arg names are JS-specific |
| Capability system                                         | Yes       | Already designed for extensibility          |

### What Needs Rework for Non-JS Runtimes

| Component                                                            | Effort | Notes                                                  |
| -------------------------------------------------------------------- | ------ | ------------------------------------------------------ |
| Naming (`js_repl` everywhere)                                        | Medium | ~50 identifiers, config keys, tool names               |
| Kernel source management (write JS files to temp)                    | Medium | Needs per-runtime file list                            |
| Runtime resolution (Node version check, `--experimental-vm-modules`) | Medium | Needs trait-based dispatch                             |
| Binding persistence (meriyah-based AST analysis)                     | Low    | Already kernel-internal, just don't force it on non-JS |
| Config fields (`js_repl_node_path`, `js_repl_deno_path`)             | High   | Flat field-per-runtime doesn't scale past 3-4 runtimes |
| TUI settings pages                                                   | Medium | Currently has JS-specific layout                       |

### Recommended Extraction Order

If multi-runtime is imminent:

1. **Extract shared JS kernel logic** into a common module both kernels import.
   This is the highest-value, lowest-risk change and pays off immediately in
   maintenance cost.

2. **Define a `RuntimeKernel` trait** that encapsulates kernel source, companion
   files, command construction, and version validation. Implement it for
   `NodeKernel` and `DenoKernel`.

3. **Rename the Rust-side types** from `JsRepl*` to `Repl*` or `CodeEval*`.
   Keep `js_repl` as the model-facing tool name via alias.

4. **Restructure config** from flat per-runtime fields to a map or array of
   runtime configs. The current pattern of adding 3 fields per new runtime
   (path, args, module_dirs) won't scale.

If multi-runtime is distant, the only immediately worthwhile change is #1
(shared kernel code), because the duplication is already a maintenance burden
for JS-only development.

---

## Minor Issues

- `kernel_deno.js` uses synchronous `Deno.stdout.writeSync()` for `send()`.
  This blocks the Deno event loop on large payloads or pipe backpressure. If the
  host-side `read_stdout` is slow for any reason and the OS pipe buffer fills,
  `writeSync` blocks the kernel, preventing it from processing incoming
  `run_tool_result` messages. The Node kernel uses `process.stdout.write()`
  which buffers internally. In practice this doesn't matter (payloads are small,
  host reads continuously), but the inconsistency is worth noting.

- The `next()` cycling method on `JsReplRuntimeKindToml` is a UI convenience
  but encodes an implicit ordering. If a third runtime is added, the cycle
  becomes less intuitive.

- `parse_freeform_args` rejects markdown fences and JSON-wrapped code with
  good error messages, but the pragma format (`// codex-js-repl:`) is a
  JS comment. A Python runtime would need a different pragma sigil
  (`# codex-repl:`). The handler would need pragma detection that's language-aware
  or uses a language-neutral format.

- The `Kernel` struct stores `tool_rx` as `Arc<Mutex<UnboundedReceiver>>`. The
  `Mutex` is held for the entire duration of `execute()`, which is correct
  (single consumer) but means the lock is held across awaits. This is fine with
  tokio's Mutex (designed for this), but would deadlock with `std::sync::Mutex`.
  Worth a comment on the field for future maintainers.

- `freeform_tool_name_snapshot()` captures available tool names at exec start.
  If dynamic tools are added/removed during a long-running exec, the snapshot
  is stale. Probably intentional for consistency, but undocumented.

- The Deno kernel uses `Deno.inspect()` for formatting non-string console args;
  the Node kernel uses `util.inspect()`. Their output formats differ slightly
  (depth handling, symbol display, etc.). This means the same `console.log(obj)`
  call can produce different output depending on runtime — a minor behavioral
  inconsistency visible to the model.

---

## Summary

The architecture is well-built for what it is: a robust JavaScript REPL with
thorough error handling, careful async lifecycle management, and a genuinely
language-agnostic protocol. The tool-call synchronization design in
`read_stdout` (waiting for in-flight tool calls before delivering exec results)
is the most impressive piece — it solves a subtle ordering problem that would
otherwise cause hangs on fire-and-forget tool calls.

The main barriers to multi-runtime support, in order of practical impact:

1. **Kernel code duplication** (~300 lines shared between Node and Deno) — already
   a maintenance burden, and adding any runtime (JS or otherwise) copies the
   protocol layer again.
2. **Rust-side JS coupling** — `JsReplManager` mixes generic REPL management
   (kernel lifecycle, exec dispatch, error pipeline) with JS-specific concerns
   (meriyah, VM module flags, version parsing). No trait boundary to split on.
3. **`build_runtime_command` branch bodies** — capability-driven branching
   conditions but runtime-specific branch contents. The `BuiltinPermissions`
   branch is Deno's CLI flags verbatim.
4. **Naming** (`js_repl` everywhere) — cosmetic but pervasive (~50+ identifiers,
   config keys, tool names, events).
5. **Config structure** — flat per-runtime fields don't scale past 3-4 runtimes.

The Deno data-URL module accumulation and Node's host-realm import boundary are
the two correctness issues worth tracking. Neither is urgent for typical usage
but both have sharp edges at scale or under adversarial input.

The protocol, error pipeline, capability system, and exec synchronization all
transfer cleanly to non-JS runtimes. The kernel dedup is the highest-value
immediate change regardless of timeline.
