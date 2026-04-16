// Node-based kernel for the REPL tool.
// Communicates over JSON lines on stdin/stdout.
// Requires Node started with --experimental-vm-modules.

const { Buffer } = require("node:buffer");
const crypto = require("node:crypto");
const fs = require("node:fs");
const { createInterface } = require("node:readline");
const { performance } = require("node:perf_hooks");
const { URL, URLSearchParams } = require("node:url");
const { inspect, TextDecoder, TextEncoder } = require("node:util");
const vm = require("node:vm");

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
} = require("./kernel_common.js");

const resolver = require("./node_resolver.js");

const { SourceTextModule } = vm;
const meriyahPromise = import("./meriyah.umd.min.js").then((m) => m.default ?? m);
let _meriyah = null;

// vm contexts start with very few globals. Populate common Node/web globals
// so snippets and dependencies behave like a normal modern JS runtime.
const context = vm.createContext({});
context.globalThis = context;
context.global = context;
context.Buffer = Buffer;
context.console = console;
context.URL = URL;
context.URLSearchParams = URLSearchParams;
if (typeof TextEncoder !== "undefined") {
  context.TextEncoder = TextEncoder;
}
if (typeof TextDecoder !== "undefined") {
  context.TextDecoder = TextDecoder;
}
if (typeof AbortController !== "undefined") {
  context.AbortController = AbortController;
}
if (typeof AbortSignal !== "undefined") {
  context.AbortSignal = AbortSignal;
}
if (typeof structuredClone !== "undefined") {
  context.structuredClone = structuredClone;
}
// Intentionally do not expose fetch/Headers/Request/Response. Network access should
// be mediated via tools or explicit sandbox policy.
if (typeof performance !== "undefined") {
  context.performance = performance;
}
context.crypto = crypto.webcrypto ?? crypto;

// ── Generation-scoped timer wrappers ────────────────────────────────
// Every scheduled callback is tagged with the generation that created it.
// When a generation ends (exec completes or reset), repeating callbacks
// are cancelled and one-shot callbacks that fire after their generation
// is dead are silently dropped.
const { wrapTimer: _wrapTimer, wrapClearTimer: _wrapClearTimer, cancelStaleTimers: _cancelStaleTimers } =
  makeTimerSystem(() => execGeneration);

context.setTimeout = _wrapTimer(setTimeout, clearTimeout, "timeout");
context.clearTimeout = _wrapClearTimer("timeout");
context.setInterval = _wrapTimer(setInterval, clearInterval, "interval");
context.clearInterval = _wrapClearTimer("interval");
context.queueMicrotask = (callback) => {
  const gen = execGeneration;
  queueMicrotask(() => {
    if (execGeneration !== gen) return;
    callback();
  });
};
if (typeof setImmediate !== "undefined") {
  context.setImmediate = _wrapTimer(setImmediate, clearImmediate, "immediate");
  context.clearImmediate = _wrapClearTimer("immediate");
}
context.atob = (data) => Buffer.from(data, "base64").toString("binary");
context.btoa = (data) => Buffer.from(data, "binary").toString("base64");

/**
 * @typedef {{ name: string, kind: "const"|"let"|"var"|"function"|"class" }} Binding
 */

// REPL state model:
// - Every exec is compiled as a fresh ESM "cell".
// - `previousSnapshot` holds a plain object of binding values from the last cell.
// - `previousBindings` tracks which top-level names should be carried forward.
// The next cell reads carried values from `__replBindings` on the vm context,
// avoiding a growing module chain.
let previousSnapshot = null;
/** @type {Binding[]} */
let previousBindings = [];
let cellCounter = 0;
let activeExecId = null;
let _fatalExitScheduled = false;
// Generation counter: incremented on every exec and exposed to the vm context
// so user code can guard long-lived callbacks against stale generations.
let execGeneration = 0;

// Initialize the module resolver with the vm context.
resolver.init(context);

/** @type {Map<string, (msg: any) => void>} */
const pendingTool = new Map();
/** @type {Map<string, (msg: any) => void>} */
const pendingEmitImage = new Map();
let toolCounter = 0;
let imageCounter = 0;
const tmpDir = process.env.CODEX_REPL_TMP_DIR || process.cwd();
const runtimeName = process.env.CODEX_REPL_RUNTIME || "node";
const runtimeVersion =
  process.env.CODEX_REPL_RUNTIME_VERSION ||
  (process.version ? process.version.replace(/^v/, "") : "");
// Explicit long-lived mutable store exposed as `codex.state`. This is useful
// when callers want shared state without relying on lexical binding carry-over.
const state = {};

async function buildModuleSource(code) {
  if (!_meriyah) _meriyah = await meriyahPromise;
  const ast = _meriyah.parseModule(code, {
    next: true,
    module: true,
    ranges: false,
    loc: false,
    disableWebCompat: true,
  });
  const { currentBindings, prelude, exportStmt, nextBindings } =
    buildModulePrelude(ast, previousSnapshot, previousBindings, "__replBindings");
  return { ast, currentBindings, source: `${prelude}${code}${exportStmt}`, nextBindings };
}

function send(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function formatErrorMessage(error) {
  if (error && typeof error === "object" && "message" in error) {
    return error.message ? String(error.message) : String(error);
  }
  return String(error);
}

function sendFatalExecResultSync(kind, error) {
  if (!activeExecId) {
    return;
  }
  const payload = {
    type: "exec_result",
    id: activeExecId,
    ok: false,
    output: "",
    error: `REPL kernel ${kind}: ${formatErrorMessage(error)}; kernel reset. Catch or handle async errors (including Promise rejections and EventEmitter 'error' events) to avoid kernel termination.`,
  };
  try {
    fs.writeSync(process.stdout.fd, `${JSON.stringify(payload)}\n`);
  } catch {
    // Best effort only; the host will still surface stdout EOF diagnostics.
  }
}

function scheduleFatalExit(kind, error) {
  if (_fatalExitScheduled) {
    // Already exiting — log the second error to stderr so it isn't lost.
    try {
      fs.writeSync(
        process.stderr.fd,
        `REPL kernel (additional) ${kind}: ${formatErrorMessage(error)}\n`,
      );
    } catch { /* best effort */ }
    process.exitCode = 1;
    return;
  }
  _fatalExitScheduled = true;
  sendFatalExecResultSync(kind, error);

  try {
    fs.writeSync(
      process.stderr.fd,
      `REPL kernel ${kind}: ${formatErrorMessage(error)}\n`,
    );
  } catch {
    // ignore
  }

  // The host will observe stdout EOF, reset kernel state, and restart on demand.
  setImmediate(() => {
    process.exit(1);
  });
}

function formatLog(args) {
  return args
    .map((arg) => (typeof arg === "string" ? arg : inspect(arg, { depth: 4, colors: false })))
    .join(" ");
}

// ── Persistent console capture ──────────────────────────────────────
// Console is always captured — user-visible output is collected per-exec
// and returned inside exec_result. This prevents background callbacks or
// host-loaded packages from writing to protocol stdout.
let _capturedLogs = [];
let _captureGeneration = 0;

const _capturedConsole = {};
for (const method of ["log", "info", "warn", "error", "debug"]) {
  _capturedConsole[method] = (...args) => {
    if (execGeneration === _captureGeneration) {
      _capturedLogs.push(formatLog(args));
    }
  };
}
// Install captured console permanently on the vm context.
context.console = _capturedConsole;

async function handleExec(message) {
  activeExecId = message.id;
  const gen = ++execGeneration;

  // Reset capture state for this generation.
  _capturedLogs = [];
  _captureGeneration = gen;
  // Cancel stale timers from previous generations.
  _cancelStaleTimers();
  // Clear local file module caches so edits between execs are picked up.
  // Native (npm) module caches are intentionally preserved.
  resolver.clearLocalFileModuleCaches();

  // Parsed AST — retained for error attribution when eval fails.
  let cellAst = null;
  // Redeclared bindings from classifyBindingChanges — provides context
  // for errors involving prior-cell bindings.
  let redeclared = [];
  // Hoisted for catch-block access (partial binding recovery).
  let cellModule = null;
  let cellNextBindings = [];

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
      const payload = {
        type: "run_tool",
        id,
        exec_id: message.id,
        tool_name: toolName,
        arguments: argumentsJson,
      };
      send(payload);
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
    cellNextBindings = nextBindings;

    // Classify binding changes — redeclared bindings provide richer
    // error context when a prior const/let is re-declared.
    redeclared = classifyBindingChanges(currentBindings, previousBindings).redeclared;

    context.state = state;
    context.codex = {
      state,
      tmpDir,
      runtime: { name: runtimeName, version: runtimeVersion },
      tool,
      /**
       * Emit an image that will be included in the tool-call output sent
       * back to the model.  Accepts a data-URL string or a Uint8Array /
       * Buffer / ArrayBuffer (which will be base64-encoded automatically).
       *
       * @param {string|Uint8Array|ArrayBuffer|Buffer} imageOrUrl
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
          const a = _meriyah.parseModule(snippet, {
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
    Object.freeze(context.codex);
    context.tmpDir = tmpDir;

    // Inject the snapshot of carried bindings into the vm context so
    // the prelude can read values without an import chain.
    if (previousSnapshot) {
      context.__replBindings = previousSnapshot;
    }

    const module = new SourceTextModule(source, {
      context,
      identifier: `cell-${cellCounter++}.mjs`,
      initializeImportMeta(meta, mod) {
        resolver.setImportMeta(meta, mod, true);
      },
      importModuleDynamically(specifier, referrer) {
        return resolver.importResolved(resolver.resolveSpecifier(specifier, referrer?.identifier));
      },
    });
    cellModule = module;

    await module.link(async (specifier, referencingModule) => {
      const resolved = resolver.resolveSpecifier(specifier, referencingModule?.identifier);
      return resolver.loadLinkedModule(resolved);
    });

    await module.evaluate();

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
    // without retaining a reference to this module.
    const snapshot = Object.create(null);
    for (const b of nextBindings) {
      snapshot[b.name] = module.namespace[b.name];
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

    // Partial binding recovery.  If the module fully evaluated (bg-task
    // error), take a complete snapshot.  Otherwise probe the errored
    // module's namespace — TDZ errors identify uninitialized bindings.
    if (cellModule && cellNextBindings.length > 0) {
      if (cellModule.status === "evaluated") {
        // Module ran to completion — snapshot all bindings (same as
        // the happy path) so binding kinds stay in sync.
        const snapshot = Object.create(null);
        for (const b of cellNextBindings) {
          snapshot[b.name] = cellModule.namespace[b.name];
        }
        previousSnapshot = snapshot;
        previousBindings = cellNextBindings;
      } else {
        const partialSnapshot = previousSnapshot
          ? Object.assign(Object.create(null), previousSnapshot)
          : Object.create(null);
        const knownNames = new Set(
          previousBindings ? previousBindings.map((b) => b.name) : [],
        );
        const partialBindings = previousBindings ? [...previousBindings] : [];
        let recovered = false;

        for (const b of cellNextBindings) {
          try {
            const value = cellModule.namespace[b.name];
            // Skip var-hoisted undefined that would clobber a prior
            // binding's real value (the assignment didn't complete).
            if (value === undefined && b.kind === "var" && knownNames.has(b.name)) {
              continue;
            }
            partialSnapshot[b.name] = value;
            if (!knownNames.has(b.name)) {
              partialBindings.push(b);
              knownNames.add(b.name);
            } else {
              // Update the binding kind for redeclared names so the
              // next prelude uses the correct keyword.
              const idx = partialBindings.findIndex((pb) => pb.name === b.name);
              if (idx !== -1) partialBindings[idx] = b;
            }
            recovered = true;
          } catch {
            // TDZ or namespace access error — binding not initialized.
          }
        }

        if (recovered) {
          previousSnapshot = partialSnapshot;
          previousBindings = partialBindings;
        }
      }
    }

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
    delete context.__replBindings;
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
    process.stderr.write(
      `[kernel] unexpected run_tool_result for unknown call id: ${message.id}\n`
    );
  }
}

let queue = Promise.resolve();

process.on("uncaughtException", (error) => {
  scheduleFatalExit("uncaught exception", error);
});

process.on("unhandledRejection", (reason) => {
  scheduleFatalExit("unhandled rejection", reason);
});

const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
input.on("line", (line) => {
  if (!line.trim()) {
    return;
  }

  let message;
  try {
    message = JSON.parse(line);
  } catch {
    process.stderr.write(`[kernel_node] ignoring non-JSON line from host\n`);
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
      process.stderr.write(
        `[kernel_node] unexpected emit_image_result for unknown id: ${message.id}\n`
      );
    }
    return;
  }
  process.stderr.write(`[kernel_node] ignoring unknown message type: ${message.type}\n`);
});
