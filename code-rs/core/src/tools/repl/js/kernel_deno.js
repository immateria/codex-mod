// Deno-based kernel for the REPL tool.
// Communicates over JSON lines on stdin/stdout.
//
// We intentionally keep this kernel self-contained: it doesn't import remote
// modules and relies on Deno's permission model for safety.

// Load shared kernel utilities (AST binding collection, timer system).
// kernel_common.js is written to the same tmp dir by the Rust host.
const _commonPath = new URL("./kernel_common.js", import.meta.url).href;
await import(_commonPath);
const {
  collectPatternNames,
  collectDeclarationBindings,
  collectBindings,
  keywordForBindingKind,
  buildModulePrelude,
  classifyBindingChanges,
  errorMentionsBinding,
  makeTimerSystem,
  normalizeToDataUrl,
} = globalThis.__kernelCommon;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

// Synchronous write to avoid making send() async.  The fatal error handler
// (_scheduleFatalExit) calls send() from error/rejection callbacks where
// async is unsafe, so sync is required there.  We use the same path for
// normal sends for simplicity; payloads are small and the host reads
// continuously, so pipe backpressure is not a practical concern.
function send(message) {
  const payload = encoder.encode(JSON.stringify(message) + "\n");
  Deno.stdout.writeSync(payload);
}

function formatLog(args) {
  return args
    .map((arg) => (typeof arg === "string" ? arg : Deno.inspect(arg, { depth: 4, colors: false })))
    .join(" ");
}

// ── Persistent console capture ──────────────────────────────────────
// Console is always captured — user-visible output is collected per-exec
// and returned inside exec_result. This prevents late callbacks from
// writing to protocol stdout.
let _capturedLogs = [];
let _captureGeneration = 0;

const _originalConsole = globalThis.console;
const _capturedConsole = { ..._originalConsole };
for (const method of ["log", "info", "warn", "error", "debug"]) {
  _capturedConsole[method] = (...args) => {
    if (execGeneration === _captureGeneration) _capturedLogs.push(formatLog(args));
  };
}
globalThis.console = _capturedConsole;

// ── Generation-scoped timer wrappers ────────────────────────────────
// Capture host timer functions before overriding them.
const _hostSetTimeout = globalThis.setTimeout;
const _hostClearTimeout = globalThis.clearTimeout;
const _hostSetInterval = globalThis.setInterval;
const _hostClearInterval = globalThis.clearInterval;
const _hostQueueMicrotask = globalThis.queueMicrotask;

const { wrapTimer: _wrapTimer, wrapClearTimer: _wrapClearTimer, cancelStaleTimers: _cancelStaleTimers } =
  makeTimerSystem(() => execGeneration);

globalThis.setTimeout = _wrapTimer(_hostSetTimeout, _hostClearTimeout, "timeout");
globalThis.clearTimeout = _wrapClearTimer("timeout");
globalThis.setInterval = _wrapTimer(_hostSetInterval, _hostClearInterval, "interval");
globalThis.clearInterval = _wrapClearTimer("interval");
globalThis.queueMicrotask = (callback) => {
  const gen = execGeneration;
  _hostQueueMicrotask(() => {
    if (execGeneration !== gen) return;
    callback();
  });
};

/**
 * @typedef {{ name: string, kind: "const"|"let"|"var"|"function"|"class" }} Binding
 */

// REPL state model:
// - Every exec is compiled as a fresh ESM "cell" evaluated via data-URL import.
// - `previousSnapshot` holds a plain object of binding values from the last cell.
// - `previousBindings` tracks which top-level names should be carried forward.
// The next cell reads carried values from `globalThis.__replBindings`,
// avoiding a growing data-URL import chain.
let previousSnapshot = null;
/** @type {Binding[]} */
let previousBindings = [];
let cellCounter = 0;
let activeExecId = null;
let execGeneration = 0;

/** @type {Map<string, (msg: any) => void>} */
const pendingTool = new Map();
/** @type {Map<string, (msg: any) => void>} */
const pendingEmitImage = new Map();
let toolCounter = 0;
let imageCounter = 0;

const runtimeName = Deno.env.get("CODEX_REPL_RUNTIME") || "deno";
const runtimeVersion = Deno.env.get("CODEX_REPL_RUNTIME_VERSION") || "";
const tmpDir = Deno.env.get("CODEX_REPL_TMP_DIR") || Deno.cwd();

// Explicit long-lived mutable store exposed as `codex.state`.
const state = {};

// Load meriyah (UMD) from the same directory as this kernel.
await import("./meriyah.umd.min.js");
const meriyah = globalThis.meriyah;
if (!meriyah || typeof meriyah.parseModule !== "function") {
  throw new Error("Failed to load meriyah parser in Deno kernel");
}

async function buildModuleSource(code) {
  const ast = meriyah.parseModule(code, {
    next: true,
    module: true,
    ranges: false,
    loc: false,
    disableWebCompat: true,
  });
  const { currentBindings, prelude, exportStmt, nextBindings } =
    buildModulePrelude(ast, previousSnapshot, previousBindings, "globalThis.__replBindings");
  return { ast, currentBindings, source: `${prelude}${code}${exportStmt}`, nextBindings };
}

function toDataUrl(source) {
  const suffix = `#cell-${cellCounter++}`;
  return `data:text/javascript;charset=utf-8,${encodeURIComponent(source)}${suffix}`;
}

async function handleExec(message) {
  activeExecId = message.id;
  const gen = ++execGeneration;

  // Reset capture state for this generation.
  _capturedLogs = [];
  _captureGeneration = gen;
  // Cancel stale timers from previous generations.
  _cancelStaleTimers();

  // Parsed AST — retained for error attribution when eval fails.
  let cellAst = null;
  // Redeclared bindings from classifyBindingChanges — provides context
  // for errors involving prior-cell bindings.
  let redeclared = [];

  // Background tasks (un-awaited tool calls, etc.) tracked per exec so
  // we can await them before finalising the result.  Unobserved failures
  // are surfaced as the cell error.
  const pendingBackgroundTasks = new Set();

  const tool = (toolName, args) => {
    if (typeof toolName !== "string" || !toolName) {
      return Promise.reject(new Error("codex.tool expects a tool name string"));
    }
    // Reject stale tool calls from dead generations.
    if (execGeneration !== gen) {
      return Promise.reject(
        new Error(`codex.tool rejected: stale generation (${gen} vs current ${execGeneration})`)
      );
    }
    const id = `${message.id}-tool-${toolCounter++}`;
    let argumentsJson = "{}";
    if (typeof args === "string") {
      argumentsJson = args;
    } else if (typeof args !== "undefined") {
      argumentsJson = JSON.stringify(args);
    }

    const operation = new Promise((resolve, reject) => {
      send({
        type: "run_tool",
        id,
        exec_id: message.id,
        tool_name: toolName,
        arguments: argumentsJson,
      });
      pendingTool.set(id, (res) => {
        if (!res.ok) {
          reject(new Error(`tool ${toolName} failed: ${res.error || "unknown error"}`));
          return;
        }
        resolve(res.response);
      });
    });

    // Track as a background task so un-awaited calls are caught.
    const observation = { observed: false };
    const tracked = operation.then(
      () => ({ ok: true, error: null, observation }),
      (error) => ({ ok: false, error, observation }),
    );
    pendingBackgroundTasks.add(tracked);

    // Return a thenable that marks itself as observed when the caller
    // interacts with it (via await, .then, .catch, or .finally).
    return {
      then(onFulfilled, onRejected) {
        observation.observed = true;
        const p = operation.then(onFulfilled, onRejected);
        // Prevent unhandled-rejection crashes when no error handler is
        // provided (e.g. `.then(f)` without `.catch()`).
        if (!onRejected) p.catch(() => {});
        return p;
      },
      catch(onRejected) { observation.observed = true; return operation.catch(onRejected); },
      finally(onFinally) { observation.observed = true; return operation.finally(onFinally); },
    };
  };

  try {
    const code = typeof message.code === "string" ? message.code : "";
    const { ast, currentBindings, source, nextBindings } = await buildModuleSource(code);
    cellAst = ast;

    // Classify binding changes — redeclared bindings provide richer
    // error context when a prior const/let is re-declared.
    redeclared = classifyBindingChanges(currentBindings, previousBindings).redeclared;

    globalThis.state = state;
    globalThis.codex = {
      state,
      tmpDir,
      runtime: { name: runtimeName, version: runtimeVersion },
      tool,
      /**
       * Emit an image that will be included in the tool-call output sent
       * back to the model.  Accepts a data-URL string or a Uint8Array /
       * ArrayBuffer (which will be base64-encoded automatically).
       *
       * @param {string|Uint8Array|ArrayBuffer} imageOrUrl
       * @param {"auto"|"low"|"high"} [detail="auto"]
       * @returns {Promise<{ok:boolean, error?:string}>}
       */
      emitImage(imageOrUrl, detail) {
        const dataUrl = normalizeToDataUrl(imageOrUrl);
        if (!dataUrl) {
          return Promise.reject(
            new Error("codex.emitImage: expected a data: URL string or binary buffer"),
          );
        }
        if (execGeneration !== gen) {
          return Promise.reject(
            new Error(`codex.emitImage rejected: stale generation (${gen} vs current ${execGeneration})`),
          );
        }
        const id = `${message.id}-img-${imageCounter++}`;
        const operation = new Promise((resolve, reject) => {
          send({
            type: "emit_image",
            id,
            exec_id: message.id,
            image_url: dataUrl,
            detail: detail || "auto",
          });
          pendingEmitImage.set(id, (res) => {
            if (!res.ok) {
              reject(new Error(res.error || "emitImage failed"));
            } else {
              resolve({ ok: true });
            }
          });
        });
        // Track as background task like tool calls.
        const observation = { observed: false };
        const tracked = operation.then(
          () => ({ ok: true, error: null, observation }),
          (error) => ({ ok: false, error, observation }),
        );
        pendingBackgroundTasks.add(tracked);
        return {
          then(onFulfilled, onRejected) {
            observation.observed = true;
            const p = operation.then(onFulfilled, onRejected);
            if (!onRejected) p.catch(() => {});
            return p;
          },
          catch(onRejected) { observation.observed = true; return operation.catch(onRejected); },
          finally(onFinally) { observation.observed = true; return operation.finally(onFinally); },
        };
      },
      generation: gen,
      // Introspection: list all tracked REPL bindings with their
      // declaration keyword and current snapshot value.
      bindings: () => previousBindings.map((b) => ({
        name: b.name,
        kind: keywordForBindingKind(b.kind),
        value: previousSnapshot ? previousSnapshot[b.name] : undefined,
      })),
      // Analyze code for its top-level bindings without executing it.
      analyze: (snippet) => {
        try {
          const a = meriyah.parseModule(snippet, {
            next: true, module: true, ranges: false, loc: false, disableWebCompat: true,
          });
          return collectBindings(a).map((b) => ({
            name: b.name,
            kind: keywordForBindingKind(b.kind),
          }));
        } catch (e) {
          return { error: e.message };
        }
      },
      // Look up the declaration keyword for a tracked binding name.
      kindOf: (name) => {
        const found = previousBindings.find((b) => b.name === name);
        return found ? keywordForBindingKind(found.kind) : null;
      },
    };
    // Freeze the codex API object so user code cannot replace or delete
    // methods (e.g. codex.tool = something_malicious).
    Object.freeze(globalThis.codex);
    globalThis.tmpDir = tmpDir;

    // Inject the snapshot of carried bindings so the prelude can read
    // values without importing from the previous data-URL module.
    if (previousSnapshot) {
      globalThis.__replBindings = previousSnapshot;
    }

    const moduleUrl = toDataUrl(source);
    const ns = await import(moduleUrl);

    // Await any un-awaited background tasks (tool calls, etc.) before
    // snapshotting.  Surface the first unobserved failure as a cell error.
    if (pendingBackgroundTasks.size > 0) {
      const bgResults = await Promise.all([...pendingBackgroundTasks]);
      const unhandled = bgResults.filter((r) => !r.ok && !r.observation.observed);
      if (unhandled.length === 1) {
        throw unhandled[0].error;
      }
      if (unhandled.length > 1) {
        const combined = unhandled.map((r) => r.error.message).join("; ");
        throw new Error(`${unhandled.length} un-awaited tool calls failed: ${combined}`);
      }
    }

    // Snapshot the namespace values so the next cell can access them
    // without retaining a reference to this module's data URL.
    const snapshot = Object.create(null);
    for (const b of nextBindings) {
      snapshot[b.name] = ns[b.name];
    }
    previousSnapshot = snapshot;
    previousBindings = nextBindings;
    const output = _capturedLogs.join("\n");

    send({
      type: "exec_result",
      id: message.id,
      ok: true,
      output,
      error: null,
    });
  } catch (error) {
    const output = _capturedLogs.join("\n");
    const errMsg = error && error.message ? error.message : String(error);
    let enhancedError = errMsg;

    // Try to attribute the error to a specific declaration for a more
    // actionable diagnostic.  Walk each statement with
    // collectDeclarationBindings; for destructuring declarations drill
    // into patterns with collectPatternNames.
    if (cellAst) {
      let hintAdded = false;
      outer:
      for (const stmt of cellAst.body ?? []) {
        // Check destructuring patterns first (more specific hint).
        if (stmt.type === "VariableDeclaration") {
          for (const decl of stmt.declarations) {
            if (!decl.id || decl.id.type === "Identifier") continue;
            const patternNames = new Map();
            collectPatternNames(decl.id, stmt.kind, patternNames);
            for (const [pName] of patternNames) {
              if (errorMentionsBinding(errMsg, pName)) {
                const shape = decl.id.type === "ObjectPattern" ? "object" : "array";
                enhancedError += `\n  hint: \`${pName}\` is destructured from an ${shape} pattern in a \`${keywordForBindingKind(stmt.kind)}\` declaration`;
                hintAdded = true;
                break outer;
              }
            }
          }
        }
        // General declaration check.
        if (!hintAdded) {
          const stmtMap = new Map();
          collectDeclarationBindings(stmt, stmtMap);
          for (const [dName, dKind] of stmtMap) {
            if (errorMentionsBinding(errMsg, dName)) {
              enhancedError += `\n  hint: check the \`${keywordForBindingKind(dKind)} ${dName}\` declaration`;
              hintAdded = true;
              break outer;
            }
          }
        }
      }
    }

    // If the error involves a binding that was redeclared from a previous
    // cell, note the prior declaration kind for context.
    for (const r of redeclared) {
      if (errorMentionsBinding(errMsg, r.name)) {
        enhancedError += `\n  note: \`${r.name}\` was previously declared as \`${keywordForBindingKind(r.priorKind)}\``;
        break;
      }
    }

    // NOTE: Partial binding recovery (reading initialized bindings from
    // a failed module's namespace) is only available in the Node kernel.
    // Deno's dynamic import() rejects without exposing the module
    // namespace, so failed cells preserve the prior snapshot as-is.

    send({
      type: "exec_result",
      id: message.id,
      ok: false,
      output,
      error: enhancedError,
    });
  } finally {
    // Clean up the injection point so user code in background callbacks
    // cannot access the raw snapshot.
    delete globalThis.__replBindings;
    // End the generation immediately so background timers/callbacks are dead.
    _cancelStaleTimers();
    // Prune any un-awaited tool call resolvers from this exec.  Fire
    // each with a synthetic error so that captured promises settle
    // instead of hanging indefinitely.
    for (const [callId, resolver] of pendingTool) {
      if (callId.startsWith(`${message.id}-tool-`)) {
        pendingTool.delete(callId);
        resolver({ ok: false, error: "cell terminated before tool call completed" });
      }
    }
    for (const [callId, resolver] of pendingEmitImage) {
      if (callId.startsWith(`${message.id}-img-`)) {
        pendingEmitImage.delete(callId);
        resolver({ ok: false, error: "cell terminated before emitImage completed" });
      }
    }
    if (activeExecId === message.id) {
      activeExecId = null;
    }
  }
}

function handleToolResult(message) {
  const resolver = pendingTool.get(message.id);
  if (resolver) {
    pendingTool.delete(message.id);
    resolver(message);
  } else {
    try {
      Deno.stderr.writeSync(encoder.encode(
        `[kernel_deno] unexpected run_tool_result for unknown call id: ${message.id}\n`
      ));
    } catch { /* best effort */ }
  }
}

let queue = Promise.resolve();

// ── Fatal error handlers ────────────────────────────────────────────
// Mirror Node kernel behavior: surface fatal errors and exit cleanly.
let _fatalExitScheduled = false;

function _scheduleFatalExit(reason, error) {
  const msg = error && error.message ? error.message : String(error ?? "unknown");
  if (_fatalExitScheduled) {
    // Already exiting — log the second error to stderr so it isn't lost.
    try {
      const detail = `REPL kernel (additional) ${reason}: ${msg}\n`;
      Deno.stderr.writeSync(encoder.encode(detail));
    } catch { /* best effort */ }
    return;
  }
  _fatalExitScheduled = true;
  try {
    send({
      type: "exec_result",
      id: activeExecId ?? "__fatal__",
      ok: false,
      output: "",
      error: `kernel fatal: ${reason}: ${msg}`,
    });
  } catch {
    // stdout may already be broken
  }
  _hostSetTimeout(() => Deno.exit(1), 0);
}

globalThis.addEventListener("error", (event) => {
  event.preventDefault();
  _scheduleFatalExit("uncaught error", event.error ?? event.message);
});

globalThis.addEventListener("unhandledrejection", (event) => {
  event.preventDefault();
  _scheduleFatalExit("unhandled rejection", event.reason);
});

async function handleLine(line) {
  if (!line.trim()) return;
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    try {
      Deno.stderr.writeSync(encoder.encode(`[kernel_deno] ignoring non-JSON line from host\n`));
    } catch { /* best effort */ }
    return;
  }

  if (message.type === "exec") {
    queue = queue.then(() => handleExec(message));
    return;
  }
  if (message.type === "run_tool_result") {
    handleToolResult(message);
    return;
  }
  if (message.type === "emit_image_result") {
    const resolver = pendingEmitImage.get(message.id);
    if (resolver) {
      pendingEmitImage.delete(message.id);
      resolver(message);
    } else {
      try {
        Deno.stderr.writeSync(encoder.encode(
          `[kernel_deno] unexpected emit_image_result for unknown id: ${message.id}\n`
        ));
      } catch { /* best effort */ }
    }
    return;
  }
  try {
    Deno.stderr.writeSync(encoder.encode(`[kernel_deno] ignoring unknown message type: ${message.type}\n`));
  } catch { /* best effort */ }
}

let buffered = "";
for await (const chunk of Deno.stdin.readable) {
  buffered += decoder.decode(chunk, { stream: true });
  const parts = buffered.split(/\r?\n/);
  buffered = parts.pop() ?? "";
  for (const line of parts) {
    await handleLine(line);
  }
}
