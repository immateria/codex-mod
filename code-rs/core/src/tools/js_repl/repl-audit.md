I unpacked the updated archive and re-read the current `code-rs` REPL files directly with `rg`/`sed`, then compared them to the `codex-rs` Node REPL code that is bundled alongside it.

Here is the handoff I would give.

## What is already better in this updated version

These earlier issues are no longer the top problems:

* Both runtimes now use a **snapshot model** (`previousSnapshot` / `previousBindings`) instead of importing the previous module chain.

  * `code-rs/core/src/tools/js_repl/kernel.js`
  * `code-rs/core/src/tools/js_repl/kernel_deno.js`
* Both kernels now collect:

  * `ImportDeclaration`
  * `ExportNamedDeclaration` wrapped declarations
* The reset tool description is stronger and now says it clears:

  * top-level bindings
  * imported modules
  * in-flight timers
  * `code-rs/core/src/openai_tools/builtin_tools.rs`

So the new REPL problems are different from the older audit.

---

# REPL handoff: things still wrong in the updated `code-rs` version

## Priority 0: do not treat Node as sandboxed as currently implemented

### Problem

Node runtime still resolves accepted package/builtin imports through host `import(...)`, not through `SourceTextModule` inside the vm context:

* `code-rs/core/src/tools/js_repl/kernel.js`

  * `resolveSpecifier(...)`
  * `importResolved(...)`

Specifically:

* builtins: `import(resolved.specifier)`
* packages: `import(pathToFileURL(resolved.path).href)`

That means:

* imported package code is **not running in the vm realm**
* package code is **not using the captured vm console**
* the denylist only constrains **top-level user specifiers**
* transitive imports inside packages are **not governed by the same linker policy**

### Why it matters

This is still the biggest Node-specific correctness/security boundary problem.

The current code looks like a vm-isolated REPL, but imported packages are not actually isolated that way.

### Instruction

Do one of these, explicitly:

1. **Short-term honest fix**

   * keep host-loaded package imports
   * document Node mode as **not a containment boundary**
   * treat Node restrictions as convenience policy, not sandbox policy

2. **Real fix**

   * stop using host `import(...)` for accepted package/file modules
   * implement a recursive linker that loads all allowed modules as `SourceTextModule`s in the same vm context
   * apply import policy on every edge, not just the root specifier

If Node stays and the long-term plan includes more runtimes, the second option is the right architecture.

---

## Priority 1: stdout/protocol is still not safe against background async or host-import logging

### Problem

The protocol is still “JSON lines on stdout”, but the kernels do not fully control who can write to stdout.

In Node:

* `withCapturedConsole(...)` temporarily swaps `ctx.console`
* but host-loaded package modules do not use `ctx.console`
* timers/intervals can run after exec ends, when console has been restored

In Deno:

* console is also only swapped around one exec
* there is no generation-aware timer/task cleanup
* late callbacks can log after the exec completes

Relevant files:

* `code-rs/core/src/tools/js_repl/kernel.js`
* `code-rs/core/src/tools/js_repl/kernel_deno.js`
* `code-rs/core/src/tools/js_repl/mod.rs`

### Why it matters

You still have these failure modes:

* raw log lines reaching protocol stdout
* logs silently disappearing because the host only accepts JSON
* stale async work from a previous exec leaking into later turns
* late `codex.tool(...)` attempts after the active exec has ended

### Instruction

Make stdout **protocol-only**.

Concretely:

* all user-visible output must be buffered and returned inside `exec_result`
* no runtime code path should ever write raw stdout/stderr used by the host protocol
* background callbacks from dead exec generations must be cancelled or dropped
* host-loaded Node packages must not bypass capture if you keep package imports

This is a core runtime contract, not just a JS detail.

---

## Priority 2: `execGeneration` exists, but it is not actually enforcing generation isolation

### Problem

Both kernels now expose a generation counter:

* `execGeneration`
* `codex.generation`

But that value is currently just informational.

The kernels still expose raw scheduling APIs:

* `setTimeout`
* `setInterval`
* `setImmediate` / `clearImmediate`
* `queueMicrotask`

and they do not wrap them to enforce generation ownership.

### Why it matters

This means stale callbacks can still:

* log after the exec ends
* mutate shared state
* attempt nested tool calls
* surface errors out-of-band

### Instruction

Make async callbacks generation-scoped.

For both runtimes:

* wrap all scheduler APIs
* tag every callback with the generation that created it
* cancel repeating callbacks on exec completion/reset
* drop one-shot callbacks that fire after their generation is dead
* reject stale `codex.tool(...)` **inside the kernel**, before sending any protocol message

This is one of the most important remaining correctness gaps.

---

## Priority 3: Deno runtime behavior is still materially weaker / less finished than Node

### Problem

The Deno kernel is much thinner than the Node kernel.

Current Deno issues:

* no explicit fatal handlers analogous to Node’s:

  * `uncaughtException`
  * `unhandledRejection`
* no import resolution policy parallel to Node
* no module loading policy parallel to Node
* no local workspace module story
* still no generation-aware async cleanup

Relevant file:

* `code-rs/core/src/tools/js_repl/kernel_deno.js`

### Why it matters

Right now “runtime=node” and “runtime=deno” are not two backends for the same product contract. They are two materially different tools.

That is manageable for two runtimes, but it will become a mess if you add Python/Ruby in the same style.

### Instruction

Define the runtime contract first, then make both Node and Deno conform to it.

At minimum, both runtimes should share the same host/kernel guarantees for:

* protocol framing
* output capture
* generation-scoped async
* nested tool-call lifecycle
* reset semantics
* fatal error semantics

---

## Priority 4: config/session structure is still hard-coded for exactly two runtimes

### Problem

The current session/config shape is still bespoke:

* `Session` stores:

  * `js_repl_node`
  * `js_repl_deno`
  * `js_repl_default_runtime`
* `build_session.rs` instantiates exactly two handles
* `config.rs` still has both:

  * Node-specific path/args
  * Deno-specific path/args
  * plus legacy/default-runtime bridging

Relevant files:

* `code-rs/core/src/codex/session.rs`
* `code-rs/core/src/codex/streaming/submission/configure_session/build_session.rs`
* `code-rs/core/src/config.rs`
* `code-rs/core/src/protocol.rs`
* `code-rs/mcp-server/src/message_processor.rs`

### Why it matters

This is the wrong structure if you want Python/Ruby later.

You will end up duplicating:

* handle fields
* config keys
* protocol fields
* build-session branches
* handler branches

for every runtime.

### Instruction

Replace the current two-runtime shape with a runtime registry now.

Use:

* `default_runtime: RuntimeKind`
* `runtime_handles: HashMap<RuntimeKind, ReplHandle>`
* `runtime_configs: HashMap<RuntimeKind, RuntimeSpec>`

Do not add Python or Ruby by repeating the Node/Deno pattern.

---

## Priority 5: docs and user-facing contract are stale and contradictory

### Problem

The code has moved, but some docs still describe the older upstream Node-only model.

Examples from the unpacked repo:

* `docs/js_repl.md` still describes:

  * “persistent Node-backed kernel”
  * `js_repl_node_path`
  * `js_repl_node_module_dirs`
  * local file module reload behavior
* while current `code-rs` supports:

  * runtime-selectable Node/Deno
  * `js_repl_runtime`
  * `js_repl_runtime_path`
  * `js_repl_runtime_args`

But current Node code does **not** support local path imports the way the doc claims.

### Why it matters

The current docs are promising behavior that the current `code-rs` REPL does not actually implement.

### Instruction

Do a REPL docs pass after the code changes.

The docs must reflect the actual runtime contract:

* what Node supports
* what Deno supports
* what “reset” really clears
* whether local files are supported
* whether package imports are allowed
* whether runtimes are sandboxed or merely restricted

---

## Priority 6: manager hardening is still much lighter than upstream

### Problem

The updated `code-rs` manager is still relatively minimal:

* no bounded stderr tail
* no rich kernel snapshot diagnostics
* no per-exec nested tool-call tracking/cancellation map
* no wait-for-tool-settlement on reset/kernel death
* no top-level exec state machine
* generic “kernel terminated” errors in the simpler paths

Relevant file:

* `code-rs/core/src/tools/js_repl/mod.rs`

### Why it matters

When the kernel misbehaves, you will not get enough structured data to debug it, and reset/timeout behavior can race with nested tool work.

### Instruction

Adopt the upstream hardening items listed below.

---

# Good upstream `codex-rs` Node REPL items that should be adopted

These are the upstream items I like and would explicitly pull into `code-rs`.

## Adopt 1: canonicalized local-file module identities + module caches

From upstream Node kernel:

* `canonicalizePath(...)`
* `linkedFileModules`
* `linkedNativeModules`
* `linkedModuleEvaluations`

File:

* `codex-rs/core/src/tools/js_repl/kernel.js`

### Why adopt

This is the right foundation for a persistent module-aware REPL:

* avoids duplicate module identities
* gives you stable cache keys
* supports reload/clear behavior cleanly

### Instruction

Adopt this model in the Node backend, and use the same conceptual structure for future runtimes:

* canonical module id
* linked module cache
* evaluation cache

---

## Adopt 2: structured module resolution kinds

Upstream resolver returns distinct resolution kinds:

* builtin
* file
* package

File:

* `codex-rs/core/src/tools/js_repl/kernel.js`

### Why adopt

This is cleaner than passing raw strings around and makes loader policy explicit.

### Instruction

Use typed resolution results as the central loader API for the Node backend, and generalize that idea host-side for future runtimes.

---

## Adopt 3: local `.js` / `.mjs` file module loading

Upstream Node now supports local file modules with:

* `.js`
* `.mjs`

and links them as `SourceTextModule`s.

File:

* `codex-rs/core/src/tools/js_repl/kernel.js`

### Why adopt

This is the single most useful import/linking feature upstream has that your current `code-rs` Node kernel does not.

It also matches the stale docs you currently ship.

### Instruction

Adopt local file module support in Node, but do it as part of the linker redesign, not as a one-off bolt-on.

Also decide whether Deno gets a comparable local file policy, and document the difference if not.

---

## Adopt 4: `OnceCell` lazy manager initialization and “if started” lookup

You already have this in updated `code-rs`:

* `JsReplHandle`
* `OnceCell`
* `manager_if_started()`

Files:

* `code-rs/core/src/tools/js_repl/mod.rs`

This one is already adopted. Keep it.

### Instruction

Preserve this pattern when you move to a runtime registry:

* one lazy handle per runtime
* one non-forcing inspection path per runtime

---

## Adopt 5: bounded stderr tail + kernel debug snapshots

From upstream manager:

* `recent_stderr`
* `kernel_stderr_tail_snapshot(...)`
* `kernel_debug_snapshot(...)`
* model-facing kernel failure message construction

File:

* `codex-rs/core/src/tools/js_repl/mod.rs`

### Why adopt

This is one of the highest-value hardening items in upstream.

When the kernel dies unexpectedly, you want:

* pid
* status
* bounded recent stderr
* stream error context
* pending exec ids/count

### Instruction

Port this almost directly into `code-rs`, but make it runtime-neutral.

---

## Adopt 6: per-exec nested tool-call tracking and settlement waits

From upstream manager:

* `begin_exec_tool_call(...)`
* `wait_for_exec_tool_calls(...)`
* `wait_for_exec_tool_calls_map(...)`
* `clear_exec_tool_calls_map(...)`

File:

* `codex-rs/core/src/tools/js_repl/mod.rs`

### Why adopt

This is the right way to make reset/timeout/kernel-exit deterministic when nested tool calls are in flight.

Your current `code-rs` manager only uses `active_exec_id` gating, which is not enough.

### Instruction

Adopt per-exec nested tool-call bookkeeping in `code-rs` and make reset/timeout wait for or cancel those tasks cleanly.

---

## Adopt 7: richer unexpected-kernel-exit handling

Upstream does much better than a bare “kernel terminated”:

* captures snapshot
* logs structured warning
* returns a better model-facing failure

File:

* `codex-rs/core/src/tools/js_repl/mod.rs`

### Why adopt

This will save time every time the kernel crashes.

### Instruction

Port the structured unexpected-exit handling to `code-rs`.

---

## Adopt 8: explicit self-invocation guard

Upstream blocks JS REPL from recursively invoking itself:

* `is_js_repl_internal_tool(...)`
* `js_repl cannot invoke itself`

File:

* `codex-rs/core/src/tools/js_repl/mod.rs`

### Why adopt

This is a good sanity guard and should exist in `code-rs`.

### Instruction

Add a direct self-invocation guard for:

* `js_repl`
* `js_repl_reset`
* any future runtime-kernel self tools

---

## Adopt 9: top-level exec state tracking, if turn interruption matters

Upstream has a `TopLevelExecState` state machine:

* `Idle`
* `FreshKernel`
* etc.

File:

* `codex-rs/core/src/tools/js_repl/mod.rs`

### Why adopt

This is useful if you care about:

* turn interruption semantics
* fresh-kernel registration
* clearing top-level exec state correctly on reset/death

### Instruction

This is lower priority than stderr snapshots and tool-call tracking, but worth adopting if your TUI/history flow depends on top-level exec lifecycle.

---

# Things from upstream I would *not* adopt as-is

## Do not re-adopt the old `previousModule` persistence model

Upstream still uses its older previous-module carry-forward logic in Node.

Your updated `code-rs` snapshot model is better than that.

### Instruction

Keep the snapshot model in `code-rs`.
Do not regress back to upstream’s `previousModule` / `previousBindings` design.

---

# Suggested implementation order

## Phase 1: architecture / host layer

1. Replace the two-runtime session/config shape with a runtime registry.
2. Keep `OnceCell` handles, but keyed by runtime kind.
3. Define a single runtime-kernel protocol contract.

## Phase 2: runtime correctness

4. Fix Node loader/linker properly:

   * typed resolution kinds
   * canonical module ids
   * local file support
   * recursive vm-context linking if Node is meant to be more than convenience mode
5. Bring Deno up to the same host/kernel lifecycle guarantees:

   * fatal handling
   * generation-scoped async
   * protocol-only stdout

## Phase 3: manager hardening

6. Port upstream:

   * stderr tail buffering
   * kernel debug snapshots
   * per-exec nested tool-call tracking
   * structured unexpected-exit handling
   * self-invocation guard

## Phase 4: docs

7. Rewrite REPL docs so they match the actual runtime contract.

---

# Short version to hand off

If you want the shortest handoff block possible, I would hand over this:

1. **Node loader is still wrong**: package/builtin imports go through host `import(...)`, so imported code escapes the vm realm. Fix linker semantics before claiming Node is restricted.
2. **Protocol is still unsafe**: background callbacks and host-imported package logs can bypass captured console. Make stdout protocol-only and add generation-scoped async cleanup.
3. **Deno is not at parity**: no equivalent fatal/error lifecycle hardening and no shared runtime contract with Node.
4. **Session/config is still hard-coded to Node+Deno**: replace with a runtime registry now if Python/Ruby are coming.
5. **Docs are stale**: current docs still describe older Node-only/local-file behavior that current `code-rs` does not actually implement.
6. **Pull in upstream hardening**:

   * canonical module ids + caches
   * local `.js/.mjs` file linking
   * bounded stderr tail
   * kernel debug snapshots
   * per-exec nested tool-call tracking
   * unexpected-exit diagnostics
   * self-invocation guard
7. **Do not pull back upstream’s old previous-module persistence model**; keep the new snapshot-based persistence in `code-rs`.

If you want, I can turn this into a PR-by-PR implementation plan next.
-----------
yes
-----------

Here’s a handoff-ready, **PR-by-PR plan** for the updated `code-rs` REPL, with the good upstream `codex-rs` Node REPL items folded in.

## Non-goals / guardrails

Do **not** regress these parts of the updated `code-rs` REPL:

* Keep the new **snapshot-based persistence** in both kernels.

  * Do **not** go back to upstream’s `previousModule` / `previousBindings` carry-forward model.
* Keep the stronger reset semantics wording.
* Keep the runtime-selectable Node/Deno behavior for now.

---

# PR 1: Protocol and lifecycle hardening in the manager

## Goal

Make kernel failure, timeout, reset, and nested tool-call behavior deterministic and debuggable.

## Why first

This is the highest-leverage host-side hardening, and most of it can be ported from upstream without waiting for the Node linker rewrite.

## Files

* `code-rs/core/src/tools/js_repl/mod.rs`
* `code-rs/core/src/openai_tools/builtin_tools.rs` if reset wording needs minor sync

## Port from upstream

Bring over the design patterns from `codex-rs/core/src/tools/js_repl/mod.rs`:

* bounded stderr tail buffering
* kernel debug snapshot helpers
* structured unexpected-kernel-exit diagnostics
* per-exec nested tool-call tracking
* wait-for-tool-settlement before cleanup/reset
* self-invocation guard
* mark-top-level-exec-submitted-before async write completes

## Concrete tasks

1. Add a bounded stderr tail buffer to `KernelState`.

   * keep a `VecDeque<String>`
   * bound by:

     * line count
     * per-line byte count
     * total byte count

2. Add a runtime-neutral kernel debug snapshot.

   * include:

     * pid
     * process status
     * stderr tail
     * pending exec count / ids if cheap

3. Upgrade error paths to use structured diagnostics.

   * write failure
   * response channel unexpectedly closed
   * kernel stdout EOF
   * stdout read error
   * kernel terminated before exec result

4. Add per-exec nested tool-call bookkeeping.

   * `exec_id -> { in_flight, notify, cancel_token, collected content }`
   * on timeout/reset/fatal:

     * cancel
     * wait briefly for settlement
     * clear bookkeeping deterministically

5. Add a hard self-invocation guard.

   * `js_repl` cannot call:

     * `js_repl`
     * `js_repl_reset`

6. Make the “submitted” state transition happen before awaiting pipe writes.

## Acceptance criteria

* Unexpected kernel death returns a debug-rich error, not just “kernel terminated”.
* Reset and timeout do not leave orphaned nested tool calls behind.
* A `js_repl` invocation cannot recursively call `js_repl` or `js_repl_reset`.
* Stderr context is preserved in failures without unbounded memory growth.

## Tests

Add manager tests for:

* stderr tail truncation
* write failure includes kernel snapshot when appropriate
* unexpected close includes kernel snapshot
* self-invocation rejection
* reset clears outstanding exec tool calls

---

# PR 2: Generation-scoped async and protocol-only stdout in both kernels

## Goal

Prevent stale async work and raw runtime output from corrupting or bypassing the host protocol.

## Why second

This is the biggest remaining correctness issue in the updated kernels.

## Files

* `code-rs/core/src/tools/js_repl/kernel.js`
* `code-rs/core/src/tools/js_repl/kernel_deno.js`

## Concrete tasks

1. Treat stdout as **protocol-only**.

   * kernel protocol messages only
   * all user-visible console output must be buffered and returned in `exec_result`

2. Make async work generation-scoped.

   * wrap:

     * `setTimeout`
     * `setInterval`
     * `setImmediate` / `clearImmediate` (Node)
     * `queueMicrotask`
   * tag every scheduled callback with `execGeneration`
   * when exec ends:

     * cancel repeating callbacks from that generation
     * drop one-shot callbacks that fire late

3. Make `codex.tool(...)` generation-aware inside the kernel.

   * stale generation calls should be rejected **before** emitting a tool request to the host

4. Keep console capture persistent and generation-aware.

   * do not rely on a temporary swap that lets late callbacks write to protocol stdout

5. Make `send(...)` a single-write operation in both kernels.

## Acceptance criteria

* A timer started in one exec cannot log or call tools after that exec is dead.
* Raw `console.log()` output never appears on protocol stdout.
* Stale `codex.tool(...)` calls are blocked inside the kernel.
* Protocol framing cannot be corrupted by late async logs.

## Tests

Add kernel-level tests for:

* late timer output is dropped
* interval is cancelled on exec completion/reset
* stale generation `codex.tool(...)` is rejected
* protocol stdout contains only framed messages

---

# PR 3: Node import/linker overhaul using upstream’s good pieces

## Goal

Fix the Node backend so imports/linking are coherent and reusable, instead of mixing vm execution with host `import(...)`.

## Why third

This is the biggest Node-specific correctness issue and the main thing upstream improved that is worth adopting.

## Files

* `code-rs/core/src/tools/js_repl/kernel.js`

## Keep from updated `code-rs`

* snapshot persistence
* updated binding collector

## Port/adapt from upstream

From `codex-rs/core/src/tools/js_repl/kernel.js`, adopt:

* typed resolution results
* canonicalized local-file identities
* module caches:

  * linked file modules
  * linked native modules
  * evaluation cache
* local `.js` / `.mjs` file module support

## Concrete tasks

1. Refactor resolution into explicit kinds:

   * builtin
   * file
   * package

2. Add canonical module ids.

   * use realpath/native canonicalization for local files
   * use stable identifiers for builtins/packages

3. Add module caches.

   * module instance cache
   * evaluation cache
   * native wrapper cache

4. Add local file module support for Node.

   * allow workspace/local `.js` and `.mjs`
   * link them as `SourceTextModule`s in the vm context

5. Stop using host `import(...)` for file/package modules that are meant to participate in the vm module graph.

   * If you keep any host-native loading path, make it explicit and narrow.

6. Decide and document the Node import policy.

   * which builtins are allowed
   * which packages are allowed
   * whether package loading is convenience-only or part of the vm-linked graph

## Important note

If package modules continue to be host-loaded, then Node must be documented as **not** being a real containment boundary. If you want stronger guarantees, the loader has to stay inside the vm linker path.

## Acceptance criteria

* Local `.js` / `.mjs` imports work in Node.
* Module identity is stable across repeated imports.
* Duplicate module instances from path aliasing are avoided.
* The loader architecture is explicit enough to extend later.

## Tests

Add Node kernel tests for:

* repeated local import reuses the same canonical module instance
* `.js` / `.mjs` local imports work
* denied builtins are rejected
* allowed local imports are linked through the Node vm loader path
* snapshot persistence still works with imports

---

# PR 4: Bring Deno to parity with the shared runtime contract

## Goal

Make Deno conform to the same lifecycle/protocol contract as Node, even if the import model remains different.

## Why fourth

Deno is currently much thinner and less hardened than Node.

## Files

* `code-rs/core/src/tools/js_repl/kernel_deno.js`
* possibly minor touchpoints in `mod.rs` if protocol/event handling changes

## Concrete tasks

1. Add explicit fatal/unhandled error handling in Deno.

   * mirror Node’s intent for:

     * uncaught errors
     * unhandled rejections
   * convert them into controlled kernel failure messages

2. Align Deno with the same generation-scoped async/output rules from PR 2.

3. Make Deno’s runtime contract explicit.

   * If Deno will not support the same import behavior as Node, document that intentionally.
   * If Deno should support local file imports later, define the policy now.

4. Keep snapshot persistence and updated binding collection consistent with Node semantics.

## Acceptance criteria

* Fatal async/runtime failures in Deno are surfaced cleanly.
* Deno reset/timeout behavior matches Node at the host contract level.
* Deno does not lag behind on protocol/output discipline.

## Tests

Add Deno kernel tests for:

* uncaught async error produces controlled failure
* late callbacks are dropped/cancelled
* reset clears timers/state
* snapshot persistence remains correct

---

# PR 5: Replace the hard-coded Node/Deno session/config shape with a runtime registry

## Goal

Make the host architecture extensible to more runtimes without repeating the current Node/Deno pattern.

## Why fifth

This is the structural change that prepares for Python/Ruby, but it is safer after the runtime semantics are cleaned up.

## Files

* `code-rs/core/src/codex/session.rs`
* `code-rs/core/src/codex/streaming/submission/configure_session/build_session.rs`
* `code-rs/core/src/codex/streaming/submission/configure_session/mod.rs`
* `code-rs/core/src/codex/streaming/submission/configure_session/prepare.rs`
* `code-rs/core/src/config.rs`
* `code-rs/core/src/config/sources.rs`
* `code-rs/core/src/protocol.rs`
* `code-rs/mcp-server/src/message_processor.rs`
* TUI settings plumbing:

  * `code-rs/tui/src/app/events/run/tools_and_ui_settings.rs`
  * `code-rs/tui/src/chatwidget/settings_routing/*`
  * related REPL settings views

## Concrete tasks

1. Introduce a runtime registry shape.

   * `default_runtime`
   * `runtime_handles: HashMap<RuntimeKind, ReplHandle>`
   * `runtime_configs: HashMap<RuntimeKind, RuntimeSpec>`

2. Stop storing bespoke session fields:

   * `js_repl_node`
   * `js_repl_deno`

3. Replace flat Node/Deno config plumbing with a runtime map.

   * do this cleanly; no need to preserve backward compatibility if that is not a requirement

4. Keep the external tool surface the same for now:

   * `js_repl`
   * `js_repl_reset`

5. Make the handler runtime lookup generic.

## Suggested target config shape

Something like:

```toml
[tools.repl]
enabled = true
default_runtime = "node"

[tools.repl.runtimes.node]
path = "node"
args = ["--experimental-vm-modules"]
module_roots = []

[tools.repl.runtimes.deno]
path = "deno"
args = []
```

## Acceptance criteria

* No code path hard-codes separate Node/Deno handle fields.
* Runtime override uses the selected runtime’s own config, not the default-runtime split hack.
* Adding a third runtime does not require copying the Node/Deno session pattern.

## Tests

Add config/session tests for:

* runtime registry round-trip
* default runtime selection
* explicit runtime override uses that runtime’s config
* session shutdown iterates all configured runtime handles cleanly

---

# PR 6: Documentation, schema, and test expansion

## Goal

Make the shipped docs and schema match the actual REPL behavior, and lock the new architecture in with end-to-end tests.

## Why last

Docs and schema should follow the code once the runtime contract settles.

## Files

* `code-rs/core/config.schema.codex.json`
* `code-rs/core/src/config/schema.rs`
* REPL docs wherever they now live in `code-rs`
* any feature/help text:

  * `code-rs/core/src/openai_tools/builtin_tools.rs`
  * TUI explanatory strings if needed

## Concrete tasks

1. Update the checked-in schema fixture after the config refactor.

   * the schema fixture lock test already exists in `core/src/config/schema.rs`

2. Rewrite REPL docs to match reality.

   * Node behavior
   * Deno behavior
   * reset semantics
   * local file import support
   * runtime configuration shape
   * whether any runtime is sandboxed vs merely restricted

3. Expand tests beyond parser/pragma tests.

   * end-to-end manager/kernel tests
   * Node import/link tests
   * Deno lifecycle tests
   * runtime registry tests

## Acceptance criteria

* No stale REPL config keys remain in the schema.
* Docs no longer describe an outdated upstream Node-only/local-file story unless that story is now truly implemented.
* End-to-end tests cover the host/kernel contract, not just input parsing.

---

# Strong recommendations from upstream to adopt

These are the upstream items worth bringing in, explicitly:

1. **Bounded stderr tail buffering**
2. **Kernel debug snapshots on failure**
3. **Structured unexpected-exit diagnostics**
4. **Per-exec nested tool-call bookkeeping**
5. **Wait-for-tool-settlement before cleanup**
6. **Self-invocation guard**
7. **Mark exec submitted before async write completes**
8. **Canonical module identities**
9. **Module caches for linked modules/evaluations**
10. **Local `.js` / `.mjs` file module support in Node**
11. **Typed module resolution results**

Do **not** import upstream’s old `previousModule` persistence model.

---

# Suggested ownership / sequencing

## PR 1

Manager hardening in Rust only.
Low product-risk, high debugging value.

## PR 2

Shared kernel contract: generation-scoped async + protocol-only stdout.
This is the biggest correctness win.

## PR 3

Node linker/import rewrite using upstream’s good module-resolution/caching ideas.
This is the biggest Node-specific technical work.

## PR 4

Deno parity work.
Bring Deno up to the same host/kernel guarantees.

## PR 5

Runtime registry refactor across session/config/TUI/protocol.
This is the extensibility step.

## PR 6

Docs, schema fixture, and full test pass.
Locks the work in.

---

# One-line handoff summary

**Keep the new snapshot persistence, port upstream’s manager hardening and Node module-resolution/caching work, fix stdout/generation isolation in both kernels, then replace the hard-coded Node/Deno session/config shape with a runtime registry before adding more runtimes.**
