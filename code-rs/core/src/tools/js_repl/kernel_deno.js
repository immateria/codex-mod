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

function withCapturedConsole(fn) {
  const logs = [];
  const original = globalThis.console;
  const captured = {
    ...original,
    log: (...args) => logs.push(formatLog(args)),
    info: (...args) => logs.push(formatLog(args)),
    warn: (...args) => logs.push(formatLog(args)),
    error: (...args) => logs.push(formatLog(args)),
    debug: (...args) => logs.push(formatLog(args)),
  };
  globalThis.console = captured;
  return Promise.resolve()
    .then(() => fn(logs))
    .finally(() => {
      globalThis.console = original;
    });
}

/**
 * @typedef {{ name: string, kind: "const"|"let"|"var"|"function"|"class" }} Binding
 */

let previousModuleUrl = null;
/** @type {Binding[]} */
let previousBindings = [];
let cellCounter = 0;

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

function collectBindings(ast) {
  const map = new Map();
  for (const stmt of ast.body ?? []) {
    if (stmt.type === "VariableDeclaration") {
      const kind = stmt.kind;
      for (const decl of stmt.declarations) {
        collectPatternNames(decl.id, kind, map);
      }
    } else if (stmt.type === "FunctionDeclaration" && stmt.id) {
      map.set(stmt.id.name, "function");
    } else if (stmt.type === "ClassDeclaration" && stmt.id) {
      map.set(stmt.id.name, "class");
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
  const priorBindings = previousModuleUrl ? previousBindings : [];

  let prelude = "";
  if (previousModuleUrl && priorBindings.length) {
    prelude += `import * as __prev from ${JSON.stringify(previousModuleUrl)};\n`;
    prelude += priorBindings
      .map((b) => `${keywordForBindingKind(b.kind)} ${b.name} = __prev.${b.name};`)
      .join("\n");
    prelude += "\n";
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
  const tool = (toolName, args) => {
    if (typeof toolName !== "string" || !toolName) {
      return Promise.reject(new Error("codex.tool expects a tool name string"));
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
    let output = "";

    globalThis.state = state;
    globalThis.codex = {
      state,
      tmpDir,
      runtime: { name: runtimeName, version: runtimeVersion },
      tool,
    };
    globalThis.tmpDir = tmpDir;

    await withCapturedConsole(async (logs) => {
      const moduleUrl = toDataUrl(source);
      await import(moduleUrl);
      previousModuleUrl = moduleUrl;
      previousBindings = nextBindings;
      output = logs.join("\n");
    });

    send({
      type: "exec_result",
      id: message.id,
      ok: true,
      output,
      error: null,
    });
  } catch (error) {
    send({
      type: "exec_result",
      id: message.id,
      ok: false,
      output: "",
      error: error && error.message ? error.message : String(error),
    });
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
