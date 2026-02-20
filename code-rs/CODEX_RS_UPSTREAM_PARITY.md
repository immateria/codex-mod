# Codex-RS Upstream Parity Notes (code-rs)

This document tracks **high-leverage upstream (`codex-rs`) features** we’ve been
migrating (or still need to migrate) into this fork’s `code-rs` implementation.
It exists to avoid re-reading the same code paths every session and to keep the
migration decision-making centralized and explicit.

## Guardrails

- `codex-rs/` is **read-only** (mirror/reference). All changes land in `code-rs/`.
- Treat **warnings as errors**. Target: `./build-fast.sh` passes with **0 warnings**.
- **Never run rustfmt**.
- Do not “port” upstream in ways that delete local fork features (see below).

## Local Fork Features That Must Survive

- Shell selection + shell-style/profile routing (skills/profiles can vary by shell).
- Multi-account auth and account switching.
- Rich MCP settings UI: per-tool enable/disable, descriptions, server details.
- MCP access prompting: allow once/session/style include/exclude (and related UX).

## Tool Runtime Migration (Core)

### Current state (already migrated)

- Tool dispatch is centralized under `code-rs/core/src/tools/`:
  - `code-rs/core/src/tools/router.rs`
  - `code-rs/core/src/tools/registry.rs`
  - `code-rs/core/src/tools/context.rs`
  - `code-rs/core/src/tools/handlers/*`
- `code-rs/core/src/codex/streaming.rs` is primarily “stream + route” and batches tool
  calls for limited parallel execution (with strict ordering constraints).

### Known gaps (must fix next)

These are present in the tool schema / config, but not fully implemented in the
new tool runtime.

#### 1) `search_tool_bm25` (MCP tool discovery helper)

**Problem**
- Tool schema exists (`search_tool_bm25` is emitted when enabled), and developer
  instruction templates exist, but there is **no handler** and no session state
  to remember “selected” MCP tools.

**Repo facts**
- Tool schema: `code-rs/core/src/openai_tools.rs` (`SEARCH_TOOL_BM25_TOOL_NAME`)
- Templates:
  - `code-rs/core/templates/search_tool/tool_description.md`
  - `code-rs/core/templates/search_tool/developer_instructions.md`
- ToolsConfig flag is set in client config:
  - `code-rs/core/src/client.rs` sets `tools_config.search_tool = self.config.tools_search_tool;`
- Turn execution currently always includes MCP tools:
  - `code-rs/core/src/codex/streaming.rs` uses `mcp::policy::filter_tools_for_turn(...)`

**Locked decisions**
- When `tools.search_tool=true`, MCP tools are **hidden until searched**.
- Search scope: **all MCP servers/tools**, not only style-allowed servers.

**Implementation sketch (decision complete)**
- Add per-session, in-memory selection state (not persisted):
  - `code-rs/core/src/codex/session.rs` `State` gains `active_mcp_tool_selection: Option<Vec<String>>`
  - Session helpers: merge/dedupe selections; query selection.
- Add handler:
  - `code-rs/core/src/tools/handlers/search_tool_bm25.rs`
  - Add `bm25` dependency (workspace + `code-core`).
  - Index **enabled** MCP tools (exclude disabled tools from config + `excluded_tools`).
  - Return JSON payload with top matches + scores; update session selection to expose
    matched tools in subsequent turns.
- Gate MCP tool exposure in `run_turn`:
  - If `search_tool=true` and no selection exists, pass `None` / empty MCP tool map to `get_openai_tools`.
  - If selection exists, include only selected MCP tools.
- Defense-in-depth: if `search_tool=true` and model calls an MCP tool that isn’t selected,
  return a failure output instructing it to call `search_tool_bm25` first.

#### 2) `apply_patch` tool call support

**Problem**
- Tool schema exists (`apply_patch` as function or grammar/freeform), but the tool runtime
  has no `apply_patch` handler. Apply-patch execution only happens via shell heuristics.

**Repo facts**
- Tool schema: `code-rs/core/src/tool_apply_patch.rs` + wiring in `code-rs/core/src/openai_tools.rs`
- Apply-patch parsing/apply pipeline exists:
  - `code-rs/apply-patch/src/lib.rs` (`maybe_parse_apply_patch_verified`)
  - `code-rs/core/src/apply_patch.rs` (apply + hooks + events)
  - Shell-path integration today: `code-rs/core/src/codex/exec_tool.rs` (detect/apply + guard)

**Implementation sketch (decision complete)**
- Add handler:
  - `code-rs/core/src/tools/handlers/apply_patch.rs`
  - Support both payloads:
    - Function: parse JSON `{ "input": "<patch>" }`
    - Custom/freeform: raw `input` is the patch text
  - Reuse the existing apply pipeline (hooks + PatchApplyBegin/End events).
- Extract/reuse `/branch` safety guard:
  - `guard_apply_patch_outside_branch(...)` currently lives in `exec_tool.rs`.
  - Move it to a shared module (or `apply_patch` module) so both exec-tool path
    and apply_patch tool handler share identical behavior.

#### 3) Session `tools_config` parity

**Problem**
- During `ConfigureSession`, `tools_config` is created, but not all flags are copied.
  Example: `search_tool` and `web_search_external` are set on the per-turn tools config
  built in `ModelClient`, but not on the session’s stored config.

**Repo facts**
- ConfigureSession builds `tools_config` in `code-rs/core/src/codex/streaming.rs`
  and only sets `web_search_allowed_domains` after construction.

**Fix**
- In ConfigureSession, set:
  - `tools_config.search_tool = config.tools_search_tool;`
  - `tools_config.web_search_external = config.tools_web_search_external;`

## Tests To Add/Extend (Core)

- Extend the “registry completeness” test:
  - `code-rs/core/src/tools/router.rs` should include `apply_patch` + `search_tool_bm25`
    in the enabled tool set and assert handlers exist.
- Add unit tests for:
  - selection merge/dedupe semantics
  - MCP gating when search-tool is enabled (tool not selected => rejected with guidance)

## Acceptance Criteria

- `./build-fast.sh` passes from repo root with **0 warnings**.
- `search_tool_bm25` works end-to-end:
  - MCP tools hidden until searched
  - search selects tools for session and exposes them to the model
- `apply_patch` tool calls work end-to-end (with the same safety + hooks + events as today).

