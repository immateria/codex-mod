// Shared logic for js_repl kernels (Node and Deno).
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

// ── Exports ─────────────────────────────────────────────────────────

if (typeof module !== "undefined" && module.exports) {
  // Node (CommonJS)
  module.exports = {
    collectPatternNames,
    collectDeclarationBindings,
    collectBindings,
    keywordForBindingKind,
    makeTimerSystem,
  };
}

// Also attach to globalThis for Deno import compatibility
if (typeof globalThis !== "undefined") {
  globalThis.__kernelCommon = {
    collectPatternNames,
    collectDeclarationBindings,
    collectBindings,
    keywordForBindingKind,
    makeTimerSystem,
  };
}
