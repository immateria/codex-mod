//! Heuristic tag inference for memory epochs.
//!
//! No LLM calls — tags are derived from keyword matching against the epoch's
//! text content, working directory, git branch, and workspace root.

/// Maximum number of tags per epoch.
const MAX_TAGS: usize = 8;

/// Infer freeform tags from epoch context using keyword + path heuristics.
///
/// Signals used: user snippet text, cwd, git branch, workspace root.
/// Tags are lowercase, deduplicated, and capped at [`MAX_TAGS`].
pub(crate) fn infer_tags(
    snippet: &str,
    cwd_display: &str,
    git_branch: Option<&str>,
    workspace_root: Option<&str>,
) -> Vec<String> {
    let mut tags = Vec::new();
    let snippet_lower = snippet.to_lowercase();

    // ── 1. Content-based tags from user snippet ─────────────────────
    content_tags(&snippet_lower, &mut tags);

    // ── 2. Context tags from git branch prefix ──────────────────────
    branch_tags(git_branch, &mut tags);

    // ── 3. Language / ecosystem from all available context ───────────
    let lang_ctx = build_lang_context(cwd_display, workspace_root, &snippet_lower);
    language_tags(&lang_ctx, &mut tags);

    // ── 4. Framework / library detection ────────────────────────────
    framework_tags(&lang_ctx, &snippet_lower, &mut tags);

    // Deduplicate, sort, and cap.
    tags.sort();
    tags.dedup();
    tags.truncate(MAX_TAGS);
    tags
}

// ─────────────────────────────────────────────────────────────────────
// Content keyword → tag mapping
// ─────────────────────────────────────────────────────────────────────

/// Each entry: (keywords that trigger the tag, tag name).
///
/// Keywords are tested as substrings of the lowercased snippet.  Order does
/// not matter — every matching category emits its tag independently.
static KEYWORD_TAGS: &[(&[&str], &str)] = &[
    // Development activities
    (&["test", "tests", "testing", "#[test]", "#[cfg(test)]", "assert", "mock", "fixture"], "testing"),
    (&["bug", "fix", "debug", "error", "panic", "crash", "backtrace", "stacktrace", "segfault"], "debugging"),
    (&["refactor", "cleanup", "clean up", "deduplicate", "dedup", "simplify", "restructure"], "refactoring"),
    (&["style", "format", "lint", "clippy", "rustfmt", "prettier", "eslint", "ruff"], "style"),
    (&["deploy", "release", "publish", "ci/cd", "pipeline", "github actions", "workflow"], "deployment"),
    (&["doc", "docs", "documentation", "readme", "/// ", "//! ", "docstring", "javadoc"], "documentation"),
    (&["perf", "performance", "optimize", "benchmark", "profil", "flamegraph", "latency", "throughput"], "performance"),
    (&["security", "auth", "login", "token", "secret", "credential", "csrf", "xss", "injection", "vulnerability"], "security"),
    (&["config", "configuration", "settings", "env", "environment", ".env", "dotenv", "toml", "yaml config"], "configuration"),
    (&["migration", "migrate", "schema", "database", "sql", "sqlite", "postgres", "mysql", "query"], "database"),
    (&["api", "endpoint", "route", "handler", "request", "response", "rest", "graphql", "grpc"], "api"),
    (&["ui", "tui", "render", "widget", "layout", "display", "frontend", "component", "view"], "ui"),
    (&["build", "compile", "cargo", "makefile", "bazel", "cmake", "webpack", "vite", "esbuild"], "build"),
    // Additional activity categories
    (&["design", "architect", "pattern", "abstraction", "interface", "trait", "protocol"], "design"),
    (&["async", "concurrency", "thread", "mutex", "channel", "spawn", "tokio", "futures"], "concurrency"),
    (&["network", "http", "tcp", "udp", "socket", "websocket", "dns", "tls", "ssl"], "networking"),
    (&["serialize", "deserialize", "serde", "json", "protobuf", "msgpack", "encoding"], "serialization"),
    (&["log", "logging", "tracing", "telemetry", "metrics", "observability", "instrument"], "observability"),
    (&["error handling", "result", "anyhow", "thiserror", "eyre", "unwrap", "expect("], "error-handling"),
    (&["dependency", "dependencies", "upgrade", "update crate", "update package", "bump version", "semver"], "dependencies"),
];

fn content_tags(snippet_lower: &str, tags: &mut Vec<String>) {
    for &(keywords, tag) in KEYWORD_TAGS {
        if keywords.iter().any(|kw| snippet_lower.contains(kw)) {
            tags.push(tag.to_string());
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Git branch prefix → tag mapping
// ─────────────────────────────────────────────────────────────────────

static BRANCH_PREFIX_TAGS: &[(&[&str], &str)] = &[
    (&["feat", "feature"], "feature"),
    (&["fix", "bugfix", "hotfix"], "debugging"),
    (&["docs", "doc"], "documentation"),
    (&["test", "tests"], "testing"),
    (&["refactor"], "refactoring"),
    (&["ci", "cd"], "deployment"),
    (&["perf"], "performance"),
    (&["chore"], "maintenance"),
    (&["release"], "deployment"),
    (&["security", "sec"], "security"),
    (&["design"], "design"),
];

fn branch_tags(git_branch: Option<&str>, tags: &mut Vec<String>) {
    let Some(branch) = git_branch else {
        return;
    };
    let branch_lower = branch.to_lowercase();
    if branch_lower == "main" || branch_lower == "master" || branch_lower == "develop" {
        return;
    }
    let Some(prefix) = branch_lower.split('/').next() else {
        return;
    };
    for &(prefixes, tag) in BRANCH_PREFIX_TAGS {
        if prefixes.contains(&prefix) {
            push_unique(tags, tag);
            return;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Language / ecosystem detection
// ─────────────────────────────────────────────────────────────────────

/// Build a combined string from snippet and immediate working directory for
/// language detection.  We intentionally exclude `workspace_root` because it
/// often points at a monorepo root containing configs for many languages
/// (e.g. `package.json` + `Cargo.toml`).  The *cwd* is more specific.
///
/// A trailing space is appended so that end-of-string extensions like ".rs"
/// will match patterns like ".rs " without special-casing.
fn build_lang_context(cwd_display: &str, _workspace_root: Option<&str>, snippet_lower: &str) -> String {
    format!("{cwd_display}/{snippet_lower} ")
}

/// Each entry: (indicators, tag).  Multiple languages can match.
///
/// Indicators are tested as substrings of the combined context string
/// (cwd + workspace_root + snippet).  Avoid overly generic markers like
/// `package.json` or `makefile` that appear in polyglot repos — prefer
/// extension patterns and tool-specific names.
static LANG_INDICATORS: &[(&[&str], &str)] = &[
    // Systems languages
    (&["cargo.toml", "/code-rs/", "-rs/", ".rs ", ".rs\"", ".rs\n", "rustc", "rustup", " rust "], "rust"),
    (&[".c\"", ".h\"", "cmake", "gcc ", "clang "], "c"),
    (&[".cpp", ".cxx", ".hpp", ".cc\"", "g++ "], "cpp"),
    (&[".zig", "build.zig"], "zig"),
    // JVM
    (&[".java", "pom.xml", "build.gradle", " maven ", " spring "], "java"),
    (&[".kt\"", ".kt ", ".kts", " kotlin "], "kotlin"),
    (&[".scala", " sbt "], "scala"),
    // Web / scripting — use file extensions, not config filenames
    (&[".js\"", ".js ", ".mjs", ".cjs", "node_modules", " javascript "], "javascript"),
    (&[".ts\"", ".ts ", ".tsx", "tsconfig", "deno.json", " typescript "], "typescript"),
    (&[".py\"", ".py ", "pyproject.toml", "requirements.txt", " python ", " pip ", "venv"], "python"),
    (&[".rb\"", ".rb ", "gemfile", " rails ", " ruby "], "ruby"),
    (&[".php", "composer.json", " laravel "], "php"),
    (&[".lua\"", ".lua ", "luarocks"], "lua"),
    (&[".pl\"", ".pl ", ".pm\"", " cpan "], "perl"),
    // Go
    (&[".go\"", ".go ", "go.mod", "go.sum", " golang "], "go"),
    // Apple / mobile
    (&[".swift", "package.swift", ".xcodeproj"], "swift"),
    (&[".dart", "pubspec.yaml", " flutter "], "dart"),
    // Shell
    (&[".sh\"", ".sh ", ".sh\n", ".bash", ".zsh", "#!/bin"], "shell"),
    // Other
    (&[".ex\"", ".exs", "mix.exs", " elixir "], "elixir"),
    (&[".clj", "deps.edn"], "clojure"),
    (&[".hs\"", ".hs ", " cabal ", "stack.yaml", " haskell "], "haskell"),
    (&[".ml\"", ".mli", " ocaml ", " dune "], "ocaml"),
    (&[".nim\"", ".nim ", "nimble"], "nim"),
    (&["wasm", "webassembly", ".wat"], "wasm"),
];

/// Allow up to 2 language tags (e.g. a Rust project with TypeScript bindings).
const MAX_LANG_TAGS: usize = 2;

fn language_tags(lang_ctx: &str, tags: &mut Vec<String>) {
    let mut count = 0;
    for &(indicators, tag) in LANG_INDICATORS {
        if indicators.iter().any(|ind| lang_ctx.contains(ind)) {
            push_unique(tags, tag);
            count += 1;
            if count >= MAX_LANG_TAGS {
                break;
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Framework / library detection
// ─────────────────────────────────────────────────────────────────────

static FRAMEWORK_INDICATORS: &[(&[&str], &str)] = &[
    // Rust ecosystem
    (&["tokio", "async-std", "smol"], "async-runtime"),
    (&["actix", "axum", "warp", "rocket", "hyper"], "web-framework"),
    (&["ratatui", "crossterm", "termion"], "tui-framework"),
    (&["serde", "serde_json", "serde_yaml"], "serde"),
    (&["sqlx", "diesel", "sea-orm", "rusqlite"], "orm"),
    // JS/TS ecosystem
    (&["react", "jsx", "tsx", "next.js", "nextjs", "gatsby"], "react"),
    (&["vue", "nuxt"], "vue"),
    (&["angular", "ng-"], "angular"),
    (&["svelte", "sveltekit"], "svelte"),
    (&["express", "fastify", "koa", "hono"], "node-web"),
    (&["prisma", "drizzle", "typeorm", "sequelize", "knex"], "orm"),
    // Python ecosystem
    (&["django", "flask", "fastapi", "starlette", "uvicorn"], "web-framework"),
    (&["pandas", "numpy", "scipy", "matplotlib"], "data-science"),
    (&["pytorch", "tensorflow", "keras", "transformer", "huggingface"], "ml"),
    // DevOps / infra
    (&["docker", "container", "dockerfile", "podman"], "docker"),
    (&["kubernetes", "k8s", "kubectl", "helm"], "kubernetes"),
    (&["terraform", "pulumi", "cloudformation", "ansible"], "infrastructure"),
    (&["nginx", "caddy", "apache", "reverse proxy"], "web-server"),
    // Data stores
    (&["redis", "memcached", "valkey"], "cache"),
    (&["mongodb", "dynamo", "cassandra", "couchdb", "firestore"], "nosql"),
    (&["kafka", "rabbitmq", "nats", "pulsar", "amqp"], "messaging"),
    // Testing frameworks
    (&["jest", "vitest", "mocha", "chai", "cypress", "playwright"], "test-framework"),
    (&["pytest", "unittest", "hypothesis"], "test-framework"),
    (&["nextest", "proptest", "criterion"], "test-framework"),
];

fn framework_tags(lang_ctx: &str, snippet_lower: &str, tags: &mut Vec<String>) {
    let combined = format!("{lang_ctx} {snippet_lower}");
    for &(indicators, tag) in FRAMEWORK_INDICATORS {
        if indicators.iter().any(|ind| combined.contains(ind)) {
            push_unique(tags, tag);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

fn push_unique(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|t| t == tag) {
        tags.push(tag.to_string());
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_keywords_from_snippet() {
        let tags = infer_tags(
            "fix the test error in the build",
            "~/project",
            Some("main"),
            Some("/tmp/project"),
        );
        assert!(tags.contains(&"debugging".to_string()), "{tags:?}");
        assert!(tags.contains(&"testing".to_string()), "{tags:?}");
        assert!(tags.contains(&"build".to_string()), "{tags:?}");
    }

    #[test]
    fn detects_branch_prefix() {
        let tags = infer_tags(
            "update something",
            "~/project",
            Some("feat/new-widget"),
            None,
        );
        assert!(tags.contains(&"feature".to_string()), "{tags:?}");
    }

    #[test]
    fn detects_rust_from_path() {
        let tags = infer_tags(
            "update Cargo.toml",
            "~/code-rs/core",
            Some("main"),
            Some("/Users/me/code-rs"),
        );
        assert!(tags.contains(&"rust".to_string()), "{tags:?}");
    }

    #[test]
    fn caps_at_max() {
        let tags = infer_tags(
            "test fix refactor style deploy doc perf security config migrate api ui build",
            "~/proj",
            Some("feat/x"),
            None,
        );
        assert!(tags.len() <= MAX_TAGS, "got {} tags", tags.len());
    }

    #[test]
    fn empty_for_no_signal() {
        let tags = infer_tags(
            "(no user snippet)",
            "~/unknown",
            None,
            None,
        );
        assert!(tags.is_empty(), "{tags:?}");
    }

    // ── New coverage ────────────────────────────────────────────────

    #[test]
    fn detects_typescript() {
        let tags = infer_tags(
            "update tsconfig.json for strict mode",
            "~/myapp",
            Some("main"),
            Some("/home/user/myapp"),
        );
        assert!(tags.contains(&"typescript".to_string()), "{tags:?}");
    }

    #[test]
    fn detects_python_from_requirements() {
        let tags = infer_tags(
            "add pandas to requirements.txt",
            "~/ml-project",
            None,
            None,
        );
        assert!(tags.contains(&"python".to_string()), "{tags:?}");
        assert!(tags.contains(&"data-science".to_string()), "{tags:?}");
    }

    #[test]
    fn detects_framework_tokio() {
        let tags = infer_tags(
            "spawn a tokio task for background processing",
            "~/server-rs",
            Some("main"),
            Some("/home/user/server-rs"),
        );
        assert!(tags.contains(&"async-runtime".to_string()), "{tags:?}");
        assert!(tags.contains(&"concurrency".to_string()), "{tags:?}");
    }

    #[test]
    fn detects_docker() {
        let tags = infer_tags(
            "update the Dockerfile to use multi-stage builds",
            "~/app",
            Some("main"),
            None,
        );
        assert!(tags.contains(&"docker".to_string()), "{tags:?}");
    }

    #[test]
    fn allows_two_languages() {
        let tags = infer_tags(
            "update both the rust backend and the typescript frontend",
            "~/fullstack",
            Some("main"),
            Some("/home/user/fullstack"),
        );
        // Should detect both rust and typescript
        let lang_count = tags.iter().filter(|t| {
            matches!(t.as_str(),
                "rust" | "typescript" | "javascript" | "python" | "go" |
                "java" | "kotlin" | "swift" | "ruby" | "php" | "c" | "cpp" |
                "zig" | "shell" | "dart" | "elixir" | "haskell" | "ocaml" |
                "lua" | "perl" | "scala" | "clojure" | "nim" | "wasm" | "verilog"
            )
        }).count();
        assert!(lang_count >= 2, "expected ≥2 language tags, got {lang_count}: {tags:?}");
    }

    #[test]
    fn detects_chore_branch() {
        let tags = infer_tags(
            "bump dependencies",
            "~/project",
            Some("chore/deps-update"),
            None,
        );
        assert!(tags.contains(&"maintenance".to_string()), "{tags:?}");
    }

    #[test]
    fn detects_security_branch() {
        let tags = infer_tags(
            "patch something",
            "~/project",
            Some("security/cve-fix"),
            None,
        );
        assert!(tags.contains(&"security".to_string()), "{tags:?}");
    }

    #[test]
    fn detects_error_handling() {
        let tags = infer_tags(
            "convert unwrap calls to proper error handling with anyhow",
            "~/project-rs",
            Some("main"),
            None,
        );
        assert!(tags.contains(&"error-handling".to_string()), "{tags:?}");
    }

    #[test]
    fn detects_react_framework() {
        let tags = infer_tags(
            "refactor the React component to use hooks with tsx",
            "~/webapp",
            Some("main"),
            None,
        );
        assert!(tags.contains(&"react".to_string()), "{tags:?}");
    }

    #[test]
    fn detects_go_language() {
        let tags = infer_tags(
            "add handler to go.mod",
            "~/service",
            Some("main"),
            Some("/home/user/service"),
        );
        assert!(tags.contains(&"go".to_string()), "{tags:?}");
    }

    #[test]
    fn detects_shell() {
        let tags = infer_tags(
            "fix the shebang line in deploy.sh",
            "~/ops",
            Some("main"),
            None,
        );
        assert!(tags.contains(&"shell".to_string()), "{tags:?}");
    }

    #[test]
    fn detects_ml_ecosystem() {
        let tags = infer_tags(
            "fine-tune the pytorch transformer model",
            "~/research",
            Some("main"),
            None,
        );
        assert!(tags.contains(&"ml".to_string()), "{tags:?}");
    }

    #[test]
    fn no_false_js_from_polyglot_repo() {
        // A Rust project that happens to have package.json at root should
        // detect Rust, not JavaScript.
        let tags = infer_tags(
            "update the Cargo.toml dependencies",
            "~/code-rs/core",
            Some("main"),
            Some("/Users/me/code-termux"),
        );
        assert!(tags.contains(&"rust".to_string()), "{tags:?}");
        assert!(!tags.contains(&"javascript".to_string()), "false JS tag: {tags:?}");
    }

    #[test]
    fn detects_rust_from_code_rs_cwd() {
        let tags = infer_tags(
            "refactor the memory system",
            "~/Codex-CLI-Mod/code-termux/code-rs/core",
            Some("main"),
            Some("/Users/me/code-termux"),
        );
        assert!(tags.contains(&"rust".to_string()), "{tags:?}");
    }
}
