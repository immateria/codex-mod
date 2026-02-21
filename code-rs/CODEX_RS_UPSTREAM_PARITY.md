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

## Streamable Shell (`exec_command` / `write_stdin`) Parity

Upstream (`codex-rs`) supports a “streamable shell” tool pair:

- `exec_command`: start a command, stream output for a time slice, then return a process/session id
- `write_stdin`: write more input to that running process and poll for output

**Why it matters**
- Enables long-running interactive tools (REPLs, ssh, prompts) without blocking the model
- Makes headless automation more reliable than `shell` alone

**Current state in this fork**
- Tool specs exist and can be emitted when enabled:
  - `code-rs/core/src/openai_tools.rs` emits `exec_command` + `write_stdin` when
    `ToolsConfig.shell_type == StreamableShell`
  - Config flag: `use_experimental_streamable_shell_tool` (see `code-rs/core/src/config.rs`)
- Runtime exists but is **not wired into the tool router**:
  - `code-rs/core/src/exec_command/*` contains a working session manager + tool schemas
  - `code-rs/core/src/unified_exec/mod.rs` exists but is currently `#![allow(dead_code)]`
  - `code-rs/core/src/tools/router.rs` does **not** register handlers for
    `exec_command` / `write_stdin`

**Result**
- Enabling `use_experimental_streamable_shell_tool=true` will surface tool names
  that the core cannot handle (tool calls fail as “unsupported call”).

**Plan (decision complete)**
- Add a handler and register it:
  - `code-rs/core/src/tools/handlers/unified_exec.rs` (or `exec_command.rs`)
  - Register tool names:
    - `exec_command`
    - `write_stdin`
- Implementation should reuse existing `exec_command::SessionManager` and match
  current safety/approval behavior:
  - Respect `AskForApproval` and `SandboxPolicy`
  - Support `sandbox_permissions=require_escalated` only when approval policy allows it
  - Intercept apply_patch pasted into exec streams:
    - Upstream warns: “apply_patch was requested via exec_command; use apply_patch tool”
    - In `code-rs`, this should share the same apply_patch verification + /branch guard
- Update tool parallelism classification:
  - `exec_command` / `write_stdin` should be `Exclusive` (ordering + mutation risk)

**Tests to add**
- Extend `code-rs/core/src/tools/router.rs` completeness test to cover
  `use_experimental_streamable_shell_tool=true` and assert handlers exist for both tools.

## Upstream Tool Inventory Delta (Handlers)

Upstream tool handlers live in `codex-rs/core/src/tools/handlers/`.
This fork’s tool runtime handlers live in `code-rs/core/src/tools/handlers/`.

**Already present in code-rs (mapped)**
- `shell` / `container.exec` (code-rs uses `ShellHandler`)
- `update_plan`
- `request_user_input`
- MCP tool calls (qualified tool names via `McpConnectionManager::parse_tool_name`)
- `web_fetch`, `browser`, `image_view`
- `wait`, `kill`, `gh_run_wait`
- Bridge tools (`code_bridge`, `code_bridge_subscription`)
- Dynamic tool bridge (session-registered tools)

**Upstream handlers not yet ported (or not exposed)**
- `search_tool_bm25` (schema exists; handler missing)
- `apply_patch` (schema exists; handler missing)
- `unified_exec` / `exec_command` / `write_stdin` (schema exists; handler missing)
- File helpers not exposed in this fork’s tool schema:
  - `read_file` (safe, structured file reads)
  - `list_dir` (structured directory listing)
  - `grep_files` (non-mutating repo search wrapper around `rg`)
- Optional runtime tools:
  - `js_repl` (requires a feature-flag + runtime integration that this fork doesn’t have today)
- MCP “resource” helpers (`mcp_resource`) are not currently exposed as first-class tools here.

## CLI Parity / Automation Snapshot

This fork already carries a lot of CLI parity that upstream Every Code didn’t
bring over initially. Useful automation commands include:

- `code fork` (fork prior interactive sessions)
- `code sandbox` (macOS Seatbelt, Linux Landlock/seccomp, with clear unsupported errors elsewhere)
- `code debug app-server send-message-v2` (scriptable diagnostics without TUI)
- `code config schema` / `code config validate` (validate against codex vs fork schemas)

Reference entry point: `code-rs/cli/src/main.rs`.

## Status Line (`/statusline`) Snapshot

Upstream (`codex-rs`) has `/statusline` to configure which items appear on the status line.

This fork supports:
- A **top** lane and a **bottom** lane
- A “primary” lane concept (primary vs secondary)
- Full settings UI + slash command wiring

Reference pointers:
- Slash command parsing: `code-rs/tui/src/slash_command.rs`
- Setup view: `code-rs/tui/src/bottom_pane/status_line_setup.rs`
- Render + hover hit regions: `code-rs/tui/src/chatwidget/terminal_surface_render.rs`
- State/events: `code-rs/tui/src/app/events.rs`, `code-rs/tui/src/app_event.rs`
- Config fields: `code-rs/core/src/config_types.rs` (`status_line_top`, `status_line_bottom`, `status_line_primary`)

## Proposed “Phase 3” Work Order (Next)

This is the next minimal set of migrations that unlock the remaining upstream
tooling without sacrificing local fork features:

1. Implement `search_tool_bm25` end-to-end (handler + per-session selection state + MCP gating).
2. Implement `apply_patch` tool call handling (share the same safety + hooks + /branch guard).
3. Wire `exec_command` / `write_stdin` tool handlers (reuse `exec_command::SessionManager`).
4. Expand tool registry completeness tests so “enabled tool == has handler” holds for:
   - `tools_search_tool`
   - `include_apply_patch_tool`
   - `use_experimental_streamable_shell_tool`

Optional after the above is stable:
- Add structured file helper tools (`read_file`, `list_dir`, `grep_files`) if we want to reduce
  reliance on arbitrary shell commands for inspection/search.
