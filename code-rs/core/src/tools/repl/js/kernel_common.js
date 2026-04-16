// Shared logic for REPL kernels (Node and Deno).
// Pure functions with no runtime-specific dependencies.
//
// Node loads this via require(), Deno via import().  Both runtimes write
// this file to the same temp directory as the kernel scripts.

// ── AST binding collection ──────────────────────────────────────────

/**
 * @typedef {{ name: string, kind: string }} Binding
 */

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
  if (kind === "var") return "var";
  if (kind === "const") return "const";
  return "let";
}

// ── Module source builder ───────────────────────────────────────────
// Shared logic for building a module source string from user code.
// `bindingsPrefix` is the accessor for __replBindings (e.g. "__replBindings"
// for Node's vm context, "globalThis.__replBindings" for Deno's global).

function buildModulePrelude(ast, previousSnapshot, previousBindings, bindingsPrefix) {
  const currentBindings = collectBindings(ast);
  const priorBindings = previousSnapshot ? previousBindings : [];

  const currentNames = new Set(currentBindings.map((b) => b.name));

  let prelude = "";
  if (previousSnapshot && priorBindings.length) {
    const injected = priorBindings.filter((b) => !currentNames.has(b.name));
    if (injected.length) {
      prelude = injected
        .map((b) => `${keywordForBindingKind(b.kind)} ${b.name} = ${bindingsPrefix}.${b.name};`)
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

  return { currentBindings, prelude, exportStmt, nextBindings };
}

// ── Binding change classification ───────────────────────────────────
// Compare current cell bindings against prior bindings to identify
// newly introduced names vs redeclarations of existing names.

function classifyBindingChanges(currentBindings, previousBindings) {
  const priorNames = new Map(previousBindings.map((b) => [b.name, b.kind]));
  const introduced = [];
  const redeclared = [];
  for (const b of currentBindings) {
    if (priorNames.has(b.name)) {
      redeclared.push({ name: b.name, priorKind: priorNames.get(b.name), newKind: b.kind });
    } else {
      introduced.push(b);
    }
  }
  return { introduced, redeclared };
}

// Test whether an error message mentions a specific binding name as a
// whole word (avoids false positives from substring matches).
function errorMentionsBinding(errorMessage, bindingName) {
  if (!bindingName || bindingName.length < 1) return false;
  const escaped = bindingName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`\\b${escaped}\\b`).test(errorMessage);
}

// ── Generation-scoped timer wrappers ────────────────────────────────

function makeTimerSystem(execGenerationFn) {
  const activeTimers = new Map();
  let nextTimerId = 1;

  function wrapTimer(hostFn, clearHostFn, kind) {
    return (callback, ...args) => {
      const gen = execGenerationFn();
      const wrapperId = nextTimerId++;
      const hostId = hostFn((...cbArgs) => {
        activeTimers.delete(wrapperId);
        if (execGenerationFn() !== gen) return;
        callback(...cbArgs);
      }, ...args);
      activeTimers.set(wrapperId, { hostId, clearFn: clearHostFn, kind });
      return wrapperId;
    };
  }

  function wrapClearTimer(kind) {
    return (wrapperId) => {
      const entry = activeTimers.get(wrapperId);
      if (entry && entry.kind === kind) {
        entry.clearFn(entry.hostId);
        activeTimers.delete(wrapperId);
      }
    };
  }

  function cancelStaleTimers() {
    for (const [id, { hostId, clearFn }] of activeTimers) {
      clearFn(hostId);
      activeTimers.delete(id);
    }
  }

  return { wrapTimer, wrapClearTimer, cancelStaleTimers };
}

// ── Image helpers ────────────────────────────────────────────────────

/**
 * Accept an image input and ensure it is a data: URL string.
 *
 * Supported inputs:
 *  - A string starting with "data:" → returned as-is
 *  - A Uint8Array, ArrayBuffer, or Node Buffer → base64-encoded as
 *    `data:application/octet-stream;base64,...`
 *
 * Returns `null` for unsupported inputs so callers can reject gracefully.
 */
function normalizeToDataUrl(input) {
  if (typeof input === "string") {
    if (input.slice(0, 5).toLowerCase() === "data:") return input;
    return null;
  }
  let bytes;
  if (input instanceof Uint8Array) {
    bytes = input;
  } else if (input instanceof ArrayBuffer) {
    bytes = new Uint8Array(input);
  } else if (typeof Buffer !== "undefined" && Buffer.isBuffer(input)) {
    bytes = new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
  } else {
    return null;
  }
  if (bytes.length === 0) return null;
  let b64;
  if (typeof Buffer !== "undefined") {
    b64 = Buffer.from(bytes).toString("base64");
  } else {
    // Chunked conversion avoids call-stack overflow from spread on
    // large arrays (Deno lacks Buffer).
    let binary = "";
    for (let i = 0; i < bytes.length; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    b64 = btoa(binary);
  }
  return `data:application/octet-stream;base64,${b64}`;
}

// ── Exports ─────────────────────────────────────────────────────────

if (typeof module !== "undefined" && module.exports) {
  // Node (CommonJS)
  module.exports = {
    collectPatternNames,
    collectDeclarationBindings,
    collectBindings,
    keywordForBindingKind,
    buildModulePrelude,
    classifyBindingChanges,
    errorMentionsBinding,
    makeTimerSystem,
    normalizeToDataUrl,
  };
}

// Also attach to globalThis for Deno import compatibility
if (typeof globalThis !== "undefined") {
  globalThis.__kernelCommon = {
    collectPatternNames,
    collectDeclarationBindings,
    collectBindings,
    keywordForBindingKind,
    buildModulePrelude,
    classifyBindingChanges,
    errorMentionsBinding,
    makeTimerSystem,
    normalizeToDataUrl,
  };
}
