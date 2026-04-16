// Deno-based kernel for js_repl.
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
  makeTimerSystem,
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
let toolCounter = 0;

const runtimeName = Deno.env.get("CODEX_JS_REPL_RUNTIME") || "deno";
const runtimeVersion = Deno.env.get("CODEX_JS_REPL_RUNTIME_VERSION") || "";
const tmpDir = Deno.env.get("CODEX_JS_TMP_DIR") || Deno.cwd();

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
  const { prelude, exportStmt, nextBindings } =
    buildModulePrelude(ast, previousSnapshot, previousBindings, "globalThis.__replBindings");
  return { source: `${prelude}${code}${exportStmt}`, nextBindings };
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

    return new Promise((resolve, reject) => {
      send({
        type: "run_tool",
        id,
        exec_id: message.id,
        tool_name: toolName,
        arguments: argumentsJson,
      });
      pendingTool.set(id, (res) => {
        if (!res.ok) {
          reject(new Error(res.error || "tool failed"));
          return;
        }
        resolve(res.response);
      });
    });
  };

  try {
    const code = typeof message.code === "string" ? message.code : "";
    const { source, nextBindings } = await buildModuleSource(code);

    globalThis.state = state;
    globalThis.codex = {
      state,
      tmpDir,
      runtime: { name: runtimeName, version: runtimeVersion },
      tool,
      generation: gen,
    };
    globalThis.tmpDir = tmpDir;

    // Inject the snapshot of carried bindings so the prelude can read
    // values without importing from the previous data-URL module.
    if (previousSnapshot) {
      globalThis.__replBindings = previousSnapshot;
    }

    const moduleUrl = toDataUrl(source);
    const ns = await import(moduleUrl);

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
    send({
      type: "exec_result",
      id: message.id,
      ok: false,
      output,
      error: error && error.message ? error.message : String(error),
    });
  } finally {
    // End the generation immediately so background timers/callbacks are dead.
    _cancelStaleTimers();
    // Prune any un-awaited tool call resolvers from this exec to prevent
    // unbounded growth of pendingTool if codex.tool() is called without await.
    for (const [callId] of pendingTool) {
      if (callId.startsWith(`${message.id}-tool-`)) {
        pendingTool.delete(callId);
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
      const detail = `js_repl kernel (additional) ${reason}: ${msg}\n`;
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
