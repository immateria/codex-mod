// Deno-based kernel for js_repl.
// Communicates over JSON lines on stdin/stdout.
//
// We intentionally keep this kernel self-contained: it doesn't import remote
// modules and relies on Deno's permission model for safety.

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function send(message) {
  const payload = encoder.encode(JSON.stringify(message) + "\n");
  Deno.stdout.writeSync(payload);
}

function formatLog(args) {
  return args
    .map((arg) => (typeof arg === "string" ? arg : Deno.inspect(arg, { depth: 4, colors: false })))
    .join(" ");
}

function withCapturedConsole(_fn) {
  // DEPRECATED: Console capture is now persistent. This function is kept
  // for structural compatibility but the capture is always active.
  throw new Error("withCapturedConsole is deprecated — console is permanently captured");
}

// ── Persistent console capture ──────────────────────────────────────
// Console is always captured — user-visible output is collected per-exec
// and returned inside exec_result. This prevents late callbacks from
// writing to protocol stdout.
let _capturedLogs = [];
let _captureGeneration = 0;

const _originalConsole = globalThis.console;
globalThis.console = {
  ..._originalConsole,
  log: (...args) => {
    if (execGeneration === _captureGeneration) _capturedLogs.push(formatLog(args));
  },
  info: (...args) => {
    if (execGeneration === _captureGeneration) _capturedLogs.push(formatLog(args));
  },
  warn: (...args) => {
    if (execGeneration === _captureGeneration) _capturedLogs.push(formatLog(args));
  },
  error: (...args) => {
    if (execGeneration === _captureGeneration) _capturedLogs.push(formatLog(args));
  },
  debug: (...args) => {
    if (execGeneration === _captureGeneration) _capturedLogs.push(formatLog(args));
  },
};

// ── Generation-scoped timer wrappers ────────────────────────────────
const _activeTimers = new Map();
let _nextTimerId = 1;

function _wrapTimer(hostFn, clearHostFn, kind) {
  return (callback, ...args) => {
    const gen = execGeneration;
    const wrapperId = _nextTimerId++;
    const hostId = hostFn((...cbArgs) => {
      _activeTimers.delete(wrapperId);
      if (execGeneration !== gen) return;
      callback(...cbArgs);
    }, ...args);
    _activeTimers.set(wrapperId, { gen, kind, hostId, clearFn: clearHostFn });
    return wrapperId;
  };
}

function _wrapClearTimer(kind) {
  return (wrapperId) => {
    const entry = _activeTimers.get(wrapperId);
    if (entry && entry.kind === kind) {
      entry.clearFn(entry.hostId);
      _activeTimers.delete(wrapperId);
    }
  };
}

function _cancelStaleTimers() {
  for (const [id, entry] of _activeTimers) {
    entry.clearFn(entry.hostId);
    _activeTimers.delete(id);
  }
}

// Deno has setTimeout/setInterval on globalThis.
const _hostSetTimeout = globalThis.setTimeout;
const _hostClearTimeout = globalThis.clearTimeout;
const _hostSetInterval = globalThis.setInterval;
const _hostClearInterval = globalThis.clearInterval;
const _hostQueueMicrotask = globalThis.queueMicrotask;

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

function collectPatternNames(pattern, kind, map) {
  if (!pattern) return;
  switch (pattern.type) {
    case "Identifier":
      if (!map.has(pattern.name)) map.set(pattern.name, kind);
      return;
    case "ObjectPattern":
      for (const prop of pattern.properties ?? []) {
        if (prop.type === "Property") {
          collectPatternNames(prop.value, kind, map);
        } else if (prop.type === "RestElement") {
          collectPatternNames(prop.argument, kind, map);
        }
      }
      return;
    case "ArrayPattern":
      for (const elem of pattern.elements ?? []) {
        if (!elem) continue;
        if (elem.type === "RestElement") {
          collectPatternNames(elem.argument, kind, map);
        } else {
          collectPatternNames(elem, kind, map);
        }
      }
      return;
    case "AssignmentPattern":
      collectPatternNames(pattern.left, kind, map);
      return;
    case "RestElement":
      collectPatternNames(pattern.argument, kind, map);
      return;
    default:
      return;
  }
}

function collectDeclarationBindings(stmt, map) {
  if (stmt.type === "VariableDeclaration") {
    const kind = stmt.kind;
    for (const decl of stmt.declarations) {
      collectPatternNames(decl.id, kind, map);
    }
  } else if (stmt.type === "FunctionDeclaration" && stmt.id) {
    map.set(stmt.id.name, "function");
  } else if (stmt.type === "ClassDeclaration" && stmt.id) {
    map.set(stmt.id.name, "class");
  }
}

function collectBindings(ast) {
  const map = new Map();
  for (const stmt of ast.body ?? []) {
    collectDeclarationBindings(stmt, map);
    if (stmt.type === "ImportDeclaration") {
      for (const spec of stmt.specifiers ?? []) {
        if (spec.local?.name && !map.has(spec.local.name)) {
          map.set(spec.local.name, "const");
        }
      }
    } else if (stmt.type === "ExportNamedDeclaration" && stmt.declaration) {
      collectDeclarationBindings(stmt.declaration, map);
    } else if (stmt.type === "ForStatement") {
      if (stmt.init && stmt.init.type === "VariableDeclaration" && stmt.init.kind === "var") {
        for (const decl of stmt.init.declarations) {
          collectPatternNames(decl.id, "var", map);
        }
      }
    } else if (stmt.type === "ForInStatement" || stmt.type === "ForOfStatement") {
      if (stmt.left && stmt.left.type === "VariableDeclaration" && stmt.left.kind === "var") {
        for (const decl of stmt.left.declarations) {
          collectPatternNames(decl.id, "var", map);
        }
      }
    }
  }
  return Array.from(map.entries()).map(([name, kind]) => ({ name, kind }));
}

function keywordForBindingKind(kind) {
  return kind === "var" ? "var" : kind === "const" ? "const" : "let";
}

async function buildModuleSource(code) {
  const ast = meriyah.parseModule(code, {
    next: true,
    module: true,
    ranges: false,
    loc: false,
    disableWebCompat: true,
  });
  const currentBindings = collectBindings(ast);
  const priorBindings = previousSnapshot ? previousBindings : [];

  // Names declared in the current cell should NOT be injected from the
  // snapshot — the user's new declaration takes precedence.
  const currentNames = new Set(currentBindings.map((b) => b.name));

  let prelude = "";
  if (previousSnapshot && priorBindings.length) {
    const injected = priorBindings.filter((b) => !currentNames.has(b.name));
    if (injected.length) {
      prelude = injected
        .map((b) => `${keywordForBindingKind(b.kind)} ${b.name} = globalThis.__replBindings.${b.name};`)
        .join("\n");
      prelude += "\n";
    }
  }

  const mergedBindings = new Map();
  for (const binding of priorBindings) mergedBindings.set(binding.name, binding.kind);
  for (const binding of currentBindings) mergedBindings.set(binding.name, binding.kind);

  const exportNames = Array.from(mergedBindings.keys());
  const exportStmt = exportNames.length ? `\nexport { ${exportNames.join(", ")} };` : "";
  const nextBindings = Array.from(mergedBindings, ([name, kind]) => ({ name, kind }));

  return { source: `${prelude}${code}${exportStmt}`, nextBindings };
}

function toDataUrl(source) {
  const suffix = `#cell-${cellCounter++}`;
  return `data:text/javascript;charset=utf-8,${encodeURIComponent(source)}${suffix}`;
}

async function handleExec(message) {
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
  }
}

function handleToolResult(message) {
  const resolver = pendingTool.get(message.id);
  if (resolver) {
    pendingTool.delete(message.id);
    resolver(message);
  }
}

let queue = Promise.resolve();

// ── Fatal error handlers ────────────────────────────────────────────
// Mirror Node kernel behavior: surface fatal errors and exit cleanly.
let _fatalExitScheduled = false;

function _scheduleFatalExit(reason, error) {
  if (_fatalExitScheduled) return;
  _fatalExitScheduled = true;
  const msg = error && error.message ? error.message : String(error ?? "unknown");
  try {
    send({
      type: "exec_result",
      id: "__fatal__",
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
    return;
  }

  if (message.type === "exec") {
    queue = queue.then(() => handleExec(message));
    return;
  }
  if (message.type === "run_tool_result") {
    handleToolResult(message);
  }
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
