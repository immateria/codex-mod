// Node module resolution for the REPL kernel.
//
// Handles specifier resolution, module linking/caching, and the
// vm-context-aware import pipeline.  Extracted from kernel_node.js to
// keep the main kernel focused on exec orchestration and protocol.
//
// Usage:
//   const resolver = require("./node_resolver.js");
//   resolver.init(context);              // set the vm context
//   const resolved = resolver.resolveSpecifier("lodash");
//   const ns = await resolver.importResolved(resolved);

"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { builtinModules, createRequire } = require("node:module");
const { pathToFileURL, fileURLToPath } = require("node:url");
const vm = require("node:vm");

const { SourceTextModule, SyntheticModule } = vm;

// ── Deny list ───────────────────────────────────────────────────────

const builtinModuleSet = new Set([
  ...builtinModules,
  ...builtinModules.map((name) => `node:${name}`),
]);

const deniedBuiltinModules = new Set([
  "process",
  "node:process",
  "child_process",
  "node:child_process",
  "worker_threads",
  "node:worker_threads",
  "module",
  "node:module",
  "net",
  "node:net",
  "tls",
  "node:tls",
  "http",
  "node:http",
  "https",
  "node:https",
  "http2",
  "node:http2",
  "dns",
  "node:dns",
  "dgram",
  "node:dgram",
  "undici",
  "node:undici",
]);

// ── Module search paths ─────────────────────────────────────────────

const nodeModuleDirEnv = process.env.CODEX_REPL_NODE_MODULE_DIRS ?? "";
const moduleSearchBases = (() => {
  const bases = [];
  const seen = new Set();
  for (const entry of nodeModuleDirEnv.split(path.delimiter)) {
    const trimmed = entry.trim();
    if (!trimmed) continue;
    const resolved = path.isAbsolute(trimmed)
      ? trimmed
      : path.resolve(process.cwd(), trimmed);
    const base =
      path.basename(resolved) === "node_modules"
        ? path.dirname(resolved)
        : resolved;
    if (seen.has(base)) continue;
    seen.add(base);
    bases.push(base);
  }
  const cwd = process.cwd();
  if (!seen.has(cwd)) bases.push(cwd);
  return bases;
})();

// ── Per-module caches ───────────────────────────────────────────────

const importResolveConditions = new Set(["node", "import"]);
const requireByBase = new Map();
const linkedFileModules = new Map();
const linkedNativeModules = new Map();
const linkedModuleEvaluations = new Map();

// The vm context is set once via init().
let _context = null;

// ── Pure helpers ────────────────────────────────────────────────────

function toNodeBuiltinSpecifier(specifier) {
  return specifier.startsWith("node:") ? specifier : `node:${specifier}`;
}

function isDeniedBuiltin(specifier) {
  const normalized = specifier.startsWith("node:")
    ? specifier.slice(5)
    : specifier;
  return (
    deniedBuiltinModules.has(specifier) ||
    deniedBuiltinModules.has(normalized)
  );
}

function canonicalizePath(value) {
  try {
    return fs.realpathSync.native(value);
  } catch {
    return value;
  }
}

function resolveResultToUrl(resolved) {
  if (resolved.kind === "builtin") return resolved.specifier;
  if (resolved.kind === "file") return pathToFileURL(resolved.path).href;
  if (resolved.kind === "package") return resolved.specifier;
  throw new Error(`Unsupported module resolution kind: ${resolved.kind}`);
}

function getRequireForBase(base) {
  let req = requireByBase.get(base);
  if (!req) {
    req = createRequire(path.join(base, "__codex_repl__.cjs"));
    requireByBase.set(base, req);
  }
  return req;
}

function isModuleNotFoundError(err) {
  return (
    err?.code === "MODULE_NOT_FOUND" || err?.code === "ERR_MODULE_NOT_FOUND"
  );
}

function isWithinBaseNodeModules(base, resolvedPath) {
  const canonicalBase = canonicalizePath(base);
  const canonicalResolved = canonicalizePath(resolvedPath);
  const nodeModulesRoot = path.resolve(canonicalBase, "node_modules");
  const relative = path.relative(nodeModulesRoot, canonicalResolved);
  return (
    relative !== "" &&
    !relative.startsWith("..") &&
    !path.isAbsolute(relative)
  );
}

function isBarePackageSpecifier(specifier) {
  if (
    typeof specifier !== "string" ||
    !specifier ||
    specifier.trim() !== specifier
  ) {
    return false;
  }
  if (specifier.startsWith("./") || specifier.startsWith("../")) return false;
  if (specifier.startsWith("/") || specifier.startsWith("\\")) return false;
  if (path.isAbsolute(specifier)) return false;
  if (/^[a-zA-Z][a-zA-Z\d+.-]*:/.test(specifier)) return false;
  if (specifier.includes("\\")) return false;
  return true;
}

function isExplicitRelativePathSpecifier(specifier) {
  return (
    specifier.startsWith("./") ||
    specifier.startsWith("../") ||
    specifier.startsWith(".\\") ||
    specifier.startsWith("..\\")
  );
}

function isFileUrlSpecifier(specifier) {
  if (typeof specifier !== "string" || !specifier.startsWith("file:")) {
    return false;
  }
  try {
    return new URL(specifier).protocol === "file:";
  } catch {
    return false;
  }
}

function isPathSpecifier(specifier) {
  if (
    typeof specifier !== "string" ||
    !specifier ||
    specifier.trim() !== specifier
  ) {
    return false;
  }
  return (
    isExplicitRelativePathSpecifier(specifier) ||
    path.isAbsolute(specifier) ||
    isFileUrlSpecifier(specifier)
  );
}

// ── Resolution ──────────────────────────────────────────────────────

function resolvePathSpecifier(specifier, referrerIdentifier = null) {
  let candidate;
  if (isFileUrlSpecifier(specifier)) {
    try {
      candidate = fileURLToPath(new URL(specifier));
    } catch (err) {
      throw new Error(
        `Failed to resolve module "${specifier}": ${err.message}`,
      );
    }
  } else {
    const baseDir =
      referrerIdentifier && path.isAbsolute(referrerIdentifier)
        ? path.dirname(referrerIdentifier)
        : process.cwd();
    candidate = path.isAbsolute(specifier)
      ? specifier
      : path.resolve(baseDir, specifier);
  }

  let resolvedPath;
  try {
    resolvedPath = fs.realpathSync.native(candidate);
  } catch (err) {
    if (err?.code === "ENOENT") {
      throw new Error(`Module not found: ${specifier}`);
    }
    throw new Error(
      `Failed to resolve module "${specifier}": ${err.message}`,
    );
  }

  let stats;
  try {
    stats = fs.statSync(resolvedPath);
  } catch (err) {
    if (err?.code === "ENOENT") {
      throw new Error(`Module not found: ${specifier}`);
    }
    throw new Error(
      `Failed to inspect module "${specifier}": ${err.message}`,
    );
  }

  if (!stats.isFile()) {
    throw new Error(
      `Unsupported import specifier "${specifier}" in the REPL. Directory imports are not supported.`,
    );
  }

  const extension = path.extname(resolvedPath).toLowerCase();
  if (extension !== ".js" && extension !== ".mjs") {
    throw new Error(
      `Unsupported import specifier "${specifier}" in the REPL. Only .js and .mjs files are supported.`,
    );
  }

  return { kind: "file", path: resolvedPath };
}

function resolveBareSpecifier(specifier) {
  let firstResolutionError = null;

  for (const base of moduleSearchBases) {
    try {
      const resolved = getRequireForBase(base).resolve(specifier, {
        conditions: importResolveConditions,
      });
      if (isWithinBaseNodeModules(base, resolved)) {
        return resolved;
      }
    } catch (err) {
      if (isModuleNotFoundError(err)) continue;
      if (!firstResolutionError) firstResolutionError = err;
    }
  }

  if (firstResolutionError) throw firstResolutionError;
  return null;
}

function resolveSpecifier(specifier, referrerIdentifier = null) {
  if (specifier.startsWith("node:") || builtinModuleSet.has(specifier)) {
    if (isDeniedBuiltin(specifier)) {
      throw new Error(
        `Importing module "${specifier}" is not allowed in the REPL`,
      );
    }
    return { kind: "builtin", specifier: toNodeBuiltinSpecifier(specifier) };
  }

  if (isPathSpecifier(specifier)) {
    return resolvePathSpecifier(specifier, referrerIdentifier);
  }

  if (!isBarePackageSpecifier(specifier)) {
    throw new Error(
      `Unsupported import specifier "${specifier}" in the REPL. Use a package name like "lodash" or "@scope/pkg", or a relative/absolute/file:// .js/.mjs path.`,
    );
  }

  const resolvedBare = resolveBareSpecifier(specifier);
  if (!resolvedBare) {
    throw new Error(`Module not found: ${specifier}`);
  }

  return { kind: "package", path: resolvedBare, specifier };
}

// ── Linking & importing ─────────────────────────────────────────────

function importNativeResolved(resolved) {
  if (resolved.kind === "builtin") return import(resolved.specifier);
  if (resolved.kind === "package") {
    return import(pathToFileURL(resolved.path).href);
  }
  throw new Error(`Unsupported module resolution kind: ${resolved.kind}`);
}

async function loadLinkedNativeModule(resolved) {
  const key =
    resolved.kind === "builtin"
      ? `builtin:${resolved.specifier}`
      : `package:${resolved.path}`;
  let modulePromise = linkedNativeModules.get(key);
  if (!modulePromise) {
    modulePromise = (async () => {
      const namespace = await importNativeResolved(resolved);
      const exportNames = Object.getOwnPropertyNames(namespace);
      return new SyntheticModule(
        exportNames,
        function initSyntheticModule() {
          for (const name of exportNames) {
            this.setExport(name, namespace[name]);
          }
        },
        { context: _context },
      );
    })();
    linkedNativeModules.set(key, modulePromise);
  }
  return modulePromise;
}

async function loadLinkedFileModule(modulePath) {
  let module = linkedFileModules.get(modulePath);
  if (!module) {
    const source = fs.readFileSync(modulePath, "utf8");
    module = new SourceTextModule(source, {
      context: _context,
      identifier: modulePath,
      initializeImportMeta(meta, mod) {
        setImportMeta(meta, mod, false);
      },
      importModuleDynamically(specifier, referrer) {
        return importResolved(
          resolveSpecifier(specifier, referrer?.identifier),
        );
      },
    });
    linkedFileModules.set(modulePath, module);
  }
  if (module.status === "unlinked") {
    await module.link(async (specifier, referencingModule) => {
      const resolved = resolveSpecifier(
        specifier,
        referencingModule?.identifier,
      );
      return loadLinkedModule(resolved);
    });
  }
  return module;
}

async function loadLinkedModule(resolved) {
  if (resolved.kind === "file") return loadLinkedFileModule(resolved.path);
  if (resolved.kind === "builtin" || resolved.kind === "package") {
    return loadLinkedNativeModule(resolved);
  }
  throw new Error(`Unsupported module resolution kind: ${resolved.kind}`);
}

async function importResolved(resolved) {
  if (resolved.kind === "file") {
    const module = await loadLinkedFileModule(resolved.path);
    let evaluation = linkedModuleEvaluations.get(resolved.path);
    if (!evaluation) {
      evaluation = module.evaluate();
      linkedModuleEvaluations.set(resolved.path, evaluation);
    }
    await evaluation;
    return module.namespace;
  }
  return importNativeResolved(resolved);
}

// ── import.meta helper ──────────────────────────────────────────────

function setImportMeta(meta, mod, isMain = false) {
  meta.url = pathToFileURL(mod.identifier).href;
  meta.filename = mod.identifier;
  meta.dirname = path.dirname(mod.identifier);
  meta.main = isMain;
  meta.resolve = (specifier) =>
    resolveResultToUrl(resolveSpecifier(specifier, mod.identifier));
}

// ── Cache management ────────────────────────────────────────────────

function clearLocalFileModuleCaches() {
  linkedFileModules.clear();
  linkedModuleEvaluations.clear();
}

// ── Public API ──────────────────────────────────────────────────────

module.exports = {
  /** Set the vm context used for module creation. Must be called once. */
  init(context) {
    _context = context;
  },
  resolveSpecifier,
  importResolved,
  loadLinkedModule,
  clearLocalFileModuleCaches,
  setImportMeta,
};
