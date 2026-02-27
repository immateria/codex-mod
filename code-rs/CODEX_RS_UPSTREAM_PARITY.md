# Codex-RS Upstream Parity Notes (code-rs)

This document tracks **high-leverage upstream (`codex-rs`) features** we've been
migrating (or still need to migrate) into this fork's `code-rs` implementation.
It exists to avoid re-reading the same code paths every session and to keep the
migration decision-making centralized and explicit.

## Guardrails

- `codex-rs/` is **read-only** (mirror/reference). All changes land in `code-rs/`.
- Treat **warnings as errors**. Target: `./build-fast.sh` passes with **0 warnings**.
- **Never run rustfmt**.
- Do not "port" upstream in ways that delete local fork features (see below).

## Local Fork Features That Must Survive

- Shell selection + shell-style/profile routing (skills/profiles can vary by shell).
- Multi-account auth and account switching.
- Rich MCP settings UI: per-tool enable/disable, descriptions, server details.
- MCP access prompting: allow once/session/style include/exclude (and related UX).

## Code-RS Divergences To Integrate With (Don't Break These)

These are "fork-only" structures that upstream `codex-rs` does not have. When
porting upstream features, we should integrate with these surfaces directly
instead of landing an upstream copy first and patching afterwards.

### Shell Selection + Shell-Style Profiles

- Shell presets + picking active shell:
  `code-rs/core/src/config_types.rs` (`ShellPresetConfig`),
  `code-rs/core/src/config.rs` (merging `shell_presets_file`),
  `code-rs/tui/src/bottom_pane/shell_selection_view.rs`,
  `code-rs/tui/src/chatwidget/shell_config_flow.rs`
- Style profiles keyed by shell script style:
  `code-rs/core/src/config_types.rs` (`ShellStyleProfileConfig`),
  `code-rs/core/src/config/sources.rs` (`set_shell_style_profile_*`)
- Session wiring and per-style filtering:
  `code-rs/core/src/codex/streaming.rs` (style label, references, developer messages),
  `code-rs/core/src/codex/session.rs` (`Session.user_shell`, `shell_style_profile_messages`)

**Migration constraint**
Any upstream feature that assumes a single global "skill root", "references
path", or "MCP allow list" needs to be adapted to flow through
`shell_style_profiles.<style>.*` (and the session's selected shell style).
Style profiles can also override command safety (`command_safety`,
`dangerous_command_detection`), so exec-related ports must read the resolved
per-style settings instead of assuming global defaults.

### Skills + Style-Scoped Skill Routing

`ShellStyleProfileConfig` supports:
`references`, `prepend_developer_messages`, `skills` allow-list,
`disabled_skills`, `skill_roots`.

**Migration constraint**
When porting upstream "skill installer/creator" flows or any new skill-loading
logic, keep style scoping as the source of truth (do not collapse back into a
single global skills directory).

### MCP Richness (Tool Toggles + Access Prompting + Style Persistence)

- Tool discovery + tool-name sanitization is centralized:
  `code-rs/core/src/mcp_connection_manager.rs` (qualified tool keys, length caps, SHA1 suffixes)
- Per-tool enable/disable is real state:
  `code-rs/core/src/mcp_connection_manager.rs` (`excluded_tools`, `set_tool_enabled`)
- Access prompting is already implemented and style-aware:
  `code-rs/core/src/codex/mcp_access.rs`,
  `code-rs/core/src/codex/session.rs` (`McpAccessState`)
- Style persistence for include/exclude lists:
  `code-rs/core/src/config/sources.rs` (`set_shell_style_profile_mcp_servers`)
- UI expects this richness:
  `code-rs/tui/src/bottom_pane/mcp_settings_view.rs` (+ submodules)

**Migration constraint**
If we port upstream "MCP status listing" or "MCP resource" tooling, it should
surface the tool-level enable state and keep access prompting as the canonical
policy enforcement (selection/search must not silently grant access).

### MCP Schema Version Pinning (mcp-types Source of Truth)

This fork's MCP type source of truth is a pinned upstream revision:

- `code-rs/mcp-types` is a thin re-export crate: `code-rs/mcp-types/src/lib.rs`
- The upstream pin lives in `code-rs/Cargo.toml` as the workspace dependency
  `upstream-mcp-types` (git rev in `openai/codex`)
- `MCP_SCHEMA_VERSION` is used in the handshake:
  `code-rs/core/src/mcp_connection_manager.rs` (`InitializeRequestParams.protocol_version`)

**Migration constraint**
If a `codex-rs` port requires a newer MCP schema version, update the pinned
`upstream-mcp-types` rev and validate the client+server handshake. Do not
reintroduce local codegen as the MCP type source of truth.

### Multi-Account Auth + Rate-Limit Account Switching

This fork uses `CODE_HOME/auth.json` plus a separate account registry:

- `code-rs/core/src/auth.rs` (`AuthManager`, auth.json IO)
- `code-rs/core/src/auth_accounts.rs` (stored accounts + active account id)
- `code-rs/core/src/account_switching.rs` (rate-limit driven switching)
- UI/UX:
  `code-rs/tui/src/bottom_pane/login_accounts_view.rs`,
  `code-rs/tui/src/bottom_pane/account_switch_settings_view.rs`

**Migration constraint**
Upstream single-account assumptions (or "login mode" changes) must be adapted
to preserve multi-account behavior and active-account switching.

### Credential Store Modes (Auth Storage Backend Is Runtime-Mutable)

`cli_auth_credentials_store` (`file|keyring|auto|ephemeral`) is a fork feature
with a TUI setting and async apply pipeline.

**Migration constraint**
When porting upstream auth-related features, assume the auth backend can change
at runtime and config writes must be root-scoped (not profile-scoped).

### MCP OAuth Credential Store + Callback Server

Separately from CLI auth, this fork has explicit settings for MCP OAuth:

- Config fields:
  `code-rs/core/src/config.rs` (`mcp_oauth_credentials_store_mode`, `mcp_oauth_callback_port`)
- Runtime usage:
  `code-rs/core/src/mcp_connection_manager.rs` (OAuth login and credential persistence)

**Migration constraint**
When porting upstream MCP auth changes, keep MCP OAuth storage independent from
CLI auth storage and preserve the `auto` fallback behavior (keyring if
available, otherwise file). Termux-like environments often lack keyring support.

### Status Line Lanes (Top + Bottom + Primary/Secondary)

This fork supports two independently configured status line lanes with hover
hit regions and a slash command.

Reference pointers:
`code-rs/tui/src/chatwidget/status_line_flow.rs`,
`code-rs/tui/src/chatwidget/terminal_surface_render.rs`,
`code-rs/tui/src/bottom_pane/status_line_setup.rs`

Header rendering is also configurable and can override lane-based status lines:

- Config:
  `code-rs/core/src/config_types.rs` (`tui.header.show_top_line`, `top_line_text`, `show_bottom_line`, `bottom_line_text`)
- Template rendering + hit regions:
  `code-rs/tui/src/chatwidget/terminal_surface_header/template.rs`
- Layout and hover behavior:
  `code-rs/tui/src/chatwidget/terminal_surface_render.rs`

**Migration constraint**
Upstream `/statusline` should map into this "lane" model, not overwrite it.
Custom header templates must preserve the same "interactive affordances" as the
default header (hover styles + click targets for actionable segments), not just
render plain text.

### TUI Interaction Architecture (Mouse + Hover + Focus)

Mouse routing and hit testing are intentionally centralized:
`code-rs/tui/src/ui_interaction.rs`,
`code-rs/tui/src/app/input.rs`,
`code-rs/tui/src/chatwidget/settings_overlay.rs`

**Migration constraint**
When porting upstream UI features, prefer reusing `ui_interaction` rather than
copy/pasting per-view mouse logic (and keep "click to focus" semantics).

### Settings UI Architecture (Overlay + Bottom Pane)

This fork has a fairly deep settings system with shared interaction plumbing
and many settings pages rendered either in the settings overlay or the bottom
pane.

Reference pointers:

- Settings overlay shell + chrome (sidebar, overview list, content panel forwarding):
  `code-rs/tui/src/chatwidget/settings_overlay.rs`,
  `code-rs/tui/src/chatwidget/settings_overlay/chrome.rs`
- Bottom pane settings host:
  `code-rs/tui/src/bottom_pane/settings_panel.rs`,
  `code-rs/tui/src/bottom_pane/settings_overlay/*`
- Reusable components used across settings screens:
  `code-rs/tui/src/components/*`

**Migration constraint**
When porting upstream settings UI features, plug them into the existing overlay
and component system rather than landing a separate one-off modal/view.

### Auto Drive Modal/Esc Semantics

Auto Drive routing has fork-specific contracts that are easy to break during
UI ports:

Reference pointers:
`code-rs/tui/src/chatwidget.rs` (`ChatWidget::auto_should_handle_global_esc`,
`ChatWidget::handle_key_event`),
`code-rs/tui/src/bottom_pane/auto_coordinator_view.rs` (approval pane must not swallow Esc)

**Migration constraint**
Keep Auto Drive's Esc semantics owned by `ChatWidget`; don't add competing Esc
handlers in view layers.

### Strict Per-Turn Ordering (Stream IDs + OrderMeta)

The TUI enforces strict `(request_ordinal, output_index, sequence_number)` order
keys and drops streaming inserts without ids.

**Migration constraint**
Any upstream tool-orchestration or parallel tool-call work must preserve these
ordering invariants and ensure every streamed insert has a stable stream id.

### Config Layering + Persistence Semantics

Config is layered and may come from multiple sources:

- Layering pipeline (system/user/project/flags/managed):
  `code-rs/core/src/config_loader/mod.rs`
- Project-local dev config is a first-class layer:
  `.code/config.toml`
- Some config writes must be root-scoped (not profile-scoped):
  `code-rs/core/src/config_edit.rs` (`persist_root_overrides`)

**Migration constraint**
When porting upstream "config write" features, decide which layer is correct
up-front (root vs profile vs session override) so we don't ship something that
works for one workflow but breaks style/profile routing or dev overrides.

### Exec Sessions + Background Output

This fork has explicit foreground/background exec lifecycle tracking and UI
integration that relies on stable `call_id` and `OrderMeta`.

Reference pointers:
`code-rs/core/src/codex/session.rs` (`background_execs`, `running_execs`,
`background_exec_cmd_display`, background `OrderMeta` helpers),
`code-rs/core/src/codex/exec_tool.rs`,
`code-rs/tui/src/history_cell/exec.rs`

**Migration constraint**
When wiring `exec_command` / `write_stdin` tooling, reuse the existing exec
session manager and ensure the output events continue to map cleanly into the
TUI exec cell lifecycle.

### Command Guardrails (confirm_guard + dry-run guard + sensitive git)

Command execution in this fork is intentionally wrapped in multiple guardrails:

- Confirm guard patterns (require explicit `confirm:` prefix):
  `code-rs/core/src/config_types.rs` (`ConfirmGuardConfig`),
  `code-rs/core/src/codex.rs` (`ConfirmGuardRuntime`),
  `code-rs/core/src/codex/exec_tool.rs`
- Dry-run guard for common mutating formatters/linters (encourages `--check` / `--dry-run` first):
  `code-rs/core/src/dry_run_guard.rs`,
  `code-rs/core/src/codex/exec_tool.rs` (`DryRunGuardState` integration)
- Sensitive git command blocking guidance (branch switches, checkout -- paths, reset/revert):
  `code-rs/core/src/codex/exec_tool.rs`
- Command safety rules:
  `code-rs/core/src/command_safety/*`,
  `code-rs/core/src/config_types.rs` (`CommandSafetyProfileConfig`)

**Migration constraint**
Any new exec runtime (`exec_command`, `write_stdin`, unified exec, etc.) must
run through the same guardrails at command start. Do not introduce a second
shell execution path that bypasses `confirm_guard` or dry-run gating.

### Project Hooks + Project Commands

This fork supports project-defined hooks and commands with structured events:

- Config schema:
  `code-rs/core/src/config_types.rs` (`ProjectHookConfig`, `ProjectCommandConfig`, `ProjectHookEvent`)
- Runtime helpers:
  `code-rs/core/src/project_features.rs`
- Hook emission and execution:
  `code-rs/core/src/codex/exec.rs`,
  `code-rs/core/src/codex/exec_tool.rs` (ToolBefore/After + FileBeforeWrite for apply_patch)

**Migration constraint**
When porting upstream tools or adding new tool handlers, ensure we continue to
emit the relevant hook events. In particular, an `apply_patch` tool handler
must preserve the existing file-write hook behavior (currently driven from the
exec-tool apply_patch interception path).

### Agents + Subagents (Fork-First)

This fork has a substantial "agents/subagents" surface that does not map 1:1
to upstream:

- Config:
  `code-rs/core/src/config_types.rs` (`AgentConfig`, `SubagentsToml`, `SubagentCommandConfig`)
- Core agent tool/runtime:
  `code-rs/core/src/agent_tool/*`,
  `code-rs/core/src/agent_defaults.rs`
- TUI flows:
  `code-rs/tui/src/chatwidget/agent_runs/*`,
  `code-rs/tui/src/bottom_pane/agents_settings_view.rs`,
  `code-rs/tui/src/bottom_pane/agents_overview_view.rs`

**Migration constraint**
If we port upstream agent/runtime features, integrate with the existing
subagent command routing and agent-run UI flows rather than landing a parallel
agent subsystem.

### Model Providers + Remote Model Discovery

This fork supports multiple model providers and remote `/models` metadata:

- Provider registry and wire API selection:
  `code-rs/core/src/model_provider_info.rs`
- Remote model discovery (ETag + disk cache; server-provided defaults):
  `code-rs/core/src/remote_models/*`
- Session behavior for default model overrides:
  `code-rs/core/src/codex/session.rs` (`apply_remote_model_overrides`)

**Migration constraint**
Any upstream "models manager" style port must respect the provider registry
and remote model cache behavior, and must not assume a single OpenAI-only
backend.

### ClientTools / ACP (MCP-Backed File/Permission Helpers)

This fork has the concept of client-hosted MCP tools that can be used as
building blocks for file IO and permission prompts:

- Config:
  `code-rs/core/src/config_types.rs` (`ClientTools`, `McpToolId`)
- (Currently mostly optional) integration surface:
  `code-rs/core/src/acp.rs`
- Session state:
  `code-rs/core/src/codex/session.rs` (`client_tools: Option<ClientTools>`)

**Migration constraint**
If we port upstream file helper tools (read_file/list_dir/grep_files) or
permission flows, consider whether they should route through `client_tools`
when configured, rather than hardcoding local filesystem access.

### Context Timeline / Undo (Fork-Specific)

This fork tracks environment/context snapshots and supports undo/timeline UI:

- Core:
  `code-rs/core/src/context_timeline/*`,
  `code-rs/core/src/codex/session.rs` (`State.context_timeline`)
- TUI:
  `code-rs/tui/src/bottom_pane/undo_timeline_view.rs`

**Migration constraint**
Upstream "memories" / "context manager" work should be evaluated against (and
ideally integrated with) the existing timeline system to avoid duplicating two
competing forms of persistent context state.

### Git Snapshotting + Patch Application (Ghost Commits + git apply)

This fork relies on git-aware primitives that upstream ports should reuse:

- Workspace snapshot/restore via unreferenced "ghost commits":
  `code-rs/git-tooling/src/ghost_commits.rs`
  (used across Auto Drive, review flows, and TUI restore/undo paths)
- Patch application via `git apply --3way` (with preflight/revert support):
  `code-rs/git-apply/src/lib.rs`
  (used by `code-rs/chatgpt/src/apply_command.rs` and cloud task patch flows)
- The `GhostCommit` type is part of protocol surfaces:
  `code-rs/protocol/src/models.rs`

**Migration constraint**
When porting upstream `apply_patch` / unified exec / file-write flows, route
through these primitives and preserve existing hook emission and safety
contracts (confirm guard, dry-run gating, file-write hooks). Avoid adding a new
patch-apply or snapshot system that drifts from the ghost-commit semantics.

### Sandboxing + Approval Policy Integration

This fork has non-trivial platform sandbox integration and approval policy
rules that tool execution must respect:

- Platform sandbox implementations:
  `code-rs/core/src/landlock.rs`,
  `code-rs/core/src/seatbelt.rs`,
  `code-rs/core/src/safety.rs`
- Approval and sandbox policy plumbing:
  `code-rs/core/src/codex/exec_tool.rs`,
  `code-rs/core/src/tools/spec.rs` (`ToolsConfigParams`),
  `code-rs/core/src/codex/session.rs` (`ApprovedCommandPattern` state)

**Migration constraint**
When porting upstream runtimes (streamable shell, apply_patch, file helpers),
route execution through the same approval/sandbox decisions and avoid creating
new "fast paths" that accidentally run commands outside the sandbox policy.

### Rollouts + Session Catalog + Resume/Fork

This fork has a session catalog + rollout recorder system and UI for resuming
and forking sessions:

- Core persistence:
  `code-rs/core/src/rollout/*`,
  `code-rs/core/src/session_catalog.rs`
- TUI resume/selection flows:
  `code-rs/tui/src/resume/*`,
  `code-rs/tui/src/bottom_pane/resume_selection_view.rs`

**Migration constraint**
Upstream "thread/session" features should be adapted to integrate with the
existing rollout catalog and resume UI instead of introducing a second session
index format.

### App Server + Protocol Surfaces (Settings + Automation)

This fork relies on an app-server JSON-RPC surface (plus a newer V2 message
processor) that the TUI and CLI use for settings, automation, and richer MCP
status reporting.

Reference pointers:

- App-server message processors:
  `code-rs/app-server/src/message_processor.rs`,
  `code-rs/app-server/src/message_processor/v2.rs`,
  `code-rs/app-server/src/code_message_processor.rs`
- V2 MCP status conversion:
  `code-rs/app-server/src/message_processor/v2/status_conversion.rs`
- Protocol additions beyond upstream baseline:
  `code-rs/protocol/src/mcp_protocol.rs`,
  `code-rs/protocol/src/skills.rs`

**Migration constraint**
When porting upstream app-server or protocol features, integrate with the
existing V2 processor + protocol extensions instead of landing a parallel API.
The settings UI assumes these endpoints exist and stay compatible with the
fork's config layering and MCP richness.

### Protocol TypeScript Bindings (ts-rs Generator)

This fork includes a TS binding generator for protocol surfaces:

- Generator crate: `code-rs/protocol-ts/src/lib.rs`
- CLI wrapper: `code-rs/protocol-ts/src/main.rs`
- Bash helper: `code-rs/protocol-ts/generate-ts`

It exports types from:

- `code-app-server-protocol`
- `code-protocol`
- `mcp-types`

**Migration constraint**
When porting upstream protocol/schema changes, keep this generator compiling and
emitting consistent bindings (it is part of the API surface for any external
consumers that rely on TS types).

### Exec Runtime (Auto Drive + Review Flows)

`code-rs/exec` is fork-heavy and includes Auto Drive + review/session runtime
plumbing that upstream does not have in the same shape.

Reference pointers:

- Exec session runtime:
  `code-rs/exec/src/session_runtime/*`,
  `code-rs/exec/src/auto_runtime.rs`,
  `code-rs/exec/src/auto_drive_session.rs`
- Review command plumbing:
  `code-rs/exec/src/review_command.rs`,
  `code-rs/exec/src/review_scope.rs`,
  `code-rs/exec/src/review_output.rs`
- Auto Drive core libraries (shared state machine + diagnostics):
  `code-rs/code-auto-drive-core/src/*`,
  `code-rs/code-auto-drive-diagnostics/src/*`

**Migration constraint**
Upstream exec improvements (event processors, orchestration, CLI flows) should
be adapted to the existing exec runtime rather than reintroducing a second
execution subsystem that bypasses Auto Drive/review behavior.

### MCP Server (Fork Tool Runner + Session Store)

This fork's `mcp-server` crate diverges from upstream and implements its own
"code" tool runner and supporting session/state.

Reference pointers:

- Tool runners/config:
  `code-rs/mcp-server/src/code_tool_runner.rs`,
  `code-rs/mcp-server/src/code_tool_config.rs`,
  `code-rs/mcp-server/src/acp_tool_runner.rs`
- Conversation loop + state:
  `code-rs/mcp-server/src/conversation_loop.rs`,
  `code-rs/mcp-server/src/session_store.rs`

**Migration constraint**
If we port upstream MCP server improvements, they must be integrated into the
existing code tool runner (and its session store) instead of overwriting it
with upstream's codex_* tool runner naming/assumptions.

### Browser Runtime + UI Integration

This fork has a dedicated `code_browser` runtime and a global browser manager
used by both the tool runtime and the TUI.

Reference pointers:

- Browser tool handler:
  `code-rs/core/src/tools/handlers/browser.rs`
- Global browser manager:
  `code-rs/browser/*` (crate),
  `code-rs/tui/src/chatwidget.rs` (global manager wiring),
  `code-rs/tui/src/history_cell/browser.rs` (browser session cells)

**Migration constraint**
When porting upstream browser or web-fetch features, preserve the global
browser manager lifecycle and the TUI browser session UI. Do not introduce a
separate browser runtime that can't be controlled/inspected via the existing UI.

## Recently Migrated From `codex-rs` (For Context)

These are upstream ideas that were already brought into this fork (sometimes
with substantial adaptation for local features). Listing them here prevents us
from re-planning the same work repeatedly.

- Tool runtime modularization under `code-rs/core/src/tools/` (Phase 1).
- Limited parallel tool-call dispatch while preserving strict per-turn ordering (Phase 2).
- CLI config schema output + config validation against Codex vs fork schemas:
  - `code-rs/cli/src/config_cmd.rs`
  - `code-rs/core/src/config/schema.rs`
- Status line configuration (`/statusline`) with top/bottom lanes + "primary" lane:
  - `code-rs/tui/src/chatwidget/status_line_flow.rs`
  - `code-rs/tui/src/bottom_pane/status_line_setup.rs`
- Auth credential store modes with a TUI setting (top-level config persistence; async apply):
  - `cli_auth_credentials_store` (`file|keyring|auto|ephemeral`)
- Managed network mediation integration points:
  - `code-rs/tui/src/bottom_pane/network_settings_view.rs` (Settings -> Network)
  - `code-rs/core/src/config.rs` + `code_core::config::set_network_proxy_settings` (`[network]`)

## Tool Runtime Migration (Core)

### Current state (already migrated)

- Tool dispatch is centralized under `code-rs/core/src/tools/`:
  - `code-rs/core/src/tools/router.rs`
  - `code-rs/core/src/tools/registry.rs`
  - `code-rs/core/src/tools/context.rs`
  - `code-rs/core/src/tools/handlers/*`
- `code-rs/core/src/codex/streaming.rs` is primarily "stream + route" and batches tool
  calls for limited parallel execution (with strict ordering constraints).

### Known gaps (must fix next)

These are present in upstream `codex-rs`, but not fully integrated in this fork yet.

**Completed (Phase 3)**

- `search_tool_bm25` handler + per-session MCP tool selection state + turn-time MCP gating.
- `apply_patch` tool handler (function + freeform) sharing the same patch hooks + `/branch` guard.
- `exec_command` / `write_stdin` tool handlers (streamable shell) + best-effort cleanup on abort/kill.
- Session `tools_config` parity for `search_tool` + `web_search_external`.
- Registry completeness tests expanded for search/apply_patch/streamable shell variants.

**Completed (Phase 4)**

- Structured file helpers: `read_file`, `list_dir`, `grep_files` (schema + handlers + parallel-safe classification).
- Optional JS REPL tooling: `js_repl` + `js_repl_reset` (behind `[tools].js_repl=true`, default off).

**Completed (Phase 5)**

- MCP resource helper tools for headless automation parity:
  `list_mcp_resources`, `list_mcp_resource_templates`, `read_mcp_resource`.
- Optional managed network mediation during tool execution (proxy + per-host approvals).

## Tests (Core)

- Done: registry completeness test (`registry_has_handlers_for_default_openai_function_tools` in
  `code-rs/core/src/tools/router.rs`) covers:
  - `search_tool_bm25`
  - `apply_patch` (function/freeform)
  - `exec_command` / `write_stdin`
  - `js_repl` / `js_repl_reset` (when enabled)
- Remaining candidate tests:
  - selection merge/dedupe semantics
  - MCP gating when search-tool is enabled (tool not selected => rejected with guidance)

## Acceptance Criteria

- `./build-fast.sh` passes from repo root with **0 warnings**.
- `search_tool_bm25` works end-to-end:
  - MCP tools hidden until searched
  - search selects tools for session and exposes them to the model
- `apply_patch` tool calls work end-to-end (with the same safety + hooks + events as today).

## Streamable Shell (`exec_command` / `write_stdin`) Snapshot (Ported)

- Tool specs: emitted when `use_experimental_streamable_shell_tool=true` (see `code-rs/core/src/openai_tools.rs` + `code-rs/core/src/config.rs`).
- Handlers: `code-rs/core/src/tools/handlers/exec_command.rs` (registered in `code-rs/core/src/tools/router.rs`).
- Lifecycle: `Session::abort()` kills PTY sessions best-effort; `kill` also triggers cleanup.
- Guardrail: apply_patch-shaped commands invoked via `exec_command` are rejected with guidance to use `apply_patch` directly.

## Partial / Duplicate Migrations to Be Aware Of

No active duplicates are currently tracked. Streamable shell is implemented
exclusively via `exec_command` / `write_stdin` (`code-rs/core/src/exec_command/*`).

## Upstream Tool Inventory Delta (Handlers)

Upstream tool handlers live in `codex-rs/core/src/tools/handlers/`.
This fork's tool runtime handlers live in `code-rs/core/src/tools/handlers/`.

**Already present in code-rs (mapped)**
- `shell` / `container.exec` (code-rs uses `ShellHandler`)
- `update_plan`
- `request_user_input`
- `search_tool_bm25` (MCP tool selection + gating)
- `apply_patch` (function + freeform; shares existing patch hooks + `/branch` guard)
- `exec_command` / `write_stdin` (streamable shell; uses `exec_command::SessionManager`)
- MCP tool calls (qualified tool names via `McpConnectionManager::parse_tool_name`)
- `web_fetch`, `browser`, `image_view`
- `wait`, `kill`, `gh_run_wait`
- Bridge tools (`code_bridge`, `code_bridge_subscription`)
- Dynamic tool bridge (session-registered tools)
- File helpers:
  - `read_file`
  - `list_dir`
  - `grep_files`
- Optional runtime tools (behind config flags):
  - `js_repl`
  - `js_repl_reset`

**Upstream handlers not yet ported (or not exposed)**

- (none currently tracked)

## Upstream Tool Runtime Modules Not Yet Ported

Upstream `codex-rs/core/src/tools/` includes a more complete "tool runtime" stack:

- `orchestrator.rs`: central approval + sandbox attempt + retry semantics per tool runtime
- `sandboxing.rs`: shared traits/types for approvable/sandboxable tool runtimes, cached approvals, etc.
- `parallel.rs`: tool call runtime that supports cancellation + mutating vs parallel gating
- `runtimes/*`: per-tool runtime implementations (`shell`, `apply_patch`, `unified_exec`, ...)
- `network_approval.rs`: managed network policy approvals (proxy-mediated)

This fork's `code-rs/core/src/tools/` intentionally stayed lighter-weight:

- We kept most approval/sandbox behavior in the existing `exec_tool` path
  (`code-rs/core/src/codex/exec_tool.rs`) and per-tool handlers.
- Parallel tool call batching is implemented in `code-rs/core/src/codex/streaming.rs`,
  not in `code-rs/core/src/tools/parallel.rs` (which does not exist here).

**Implication**
- We should not attempt to "fully port" upstream's orchestrator/sandboxing stack as
  part of completing the missing tools. It's a large structural change and would
  risk regressions in local features (MCP prompting + shell/style routing).
- We *can* borrow the key ideas when it reduces duplication:
  - per-handler "mutating vs parallel-safe" metadata (instead of hardcoded name lists)
  - central tool-call begin/end emission helpers to avoid copy/paste

## Other Upstream Additions Worth Tracking

These are upstream `codex-rs` modules/crates that Every Code has not historically
ported into `code-rs`, but are worth considering. This list is intentionally
high-level; add detail only when we decide to port an item.

- `codex-rs/utils/stream-parser`:
  dependency-free incremental parser for stripping `<oai-mem-citation>` tags and
  extracting `<proposed_plan>` blocks across stream chunk boundaries.
- `codex-rs/utils/sleep-inhibitor`:
  cross-platform "keep awake while a turn is running" helper (IOKit /
  systemd-inhibit / PowerSetRequest).
- `codex-rs/core/src/state/*` + `codex-rs/core/src/memories/*`:
  persistent state DB + memory/citation plumbing (bigger architectural lift; must
  integrate with fork-only multi-account + style routing).
- `codex-rs/core/src/shell-escalation/*`:
  execution escalation UX/semantics (must not conflict with fork-only shell
  selection + command-safety routing).
- `codex-rs/core/src/tools/network_approval.rs` + `codex-rs/network-proxy/*`:
  upstream-managed proxy UX + approvals (this fork is already building a managed
  proxy; upstream pieces may still be useful reference for enforcement/UX).
- `codex-rs/tui/*` proposed plan streaming + "implement plan" prompt:
  plan-mode UX that relies on parsing `<proposed_plan>` segments during streaming.
- `codex-rs/tui/*` realtime/voice:
  opt-in audio + realtime conversation surfaces (large and currently low-leverage
  for this fork).

## MCP Resource Tools (Ported)

Upstream exposes MCP resource discovery + reading as tools:
- `list_mcp_resources`
- `list_mcp_resource_templates`
- `read_mcp_resource`

This fork ports those tools for headless/automation parity:
- Handler: `code-rs/core/src/tools/handlers/mcp_resource.rs`
- Policy: routes through existing MCP access prompting enforcement (per-turn).

## File Helper Tools (Ported)

Structured, non-mutating helpers are now exposed as first-class tools:

- Tool specs: `code-rs/core/src/openai_tools.rs`
- Handlers:
  - `code-rs/core/src/tools/handlers/read_file.rs`
  - `code-rs/core/src/tools/handlers/list_dir.rs`
  - `code-rs/core/src/tools/handlers/grep_files.rs`
- Parallelism classification (safe allowlist): `code-rs/core/src/codex/streaming.rs` (`classify_tool_call_parallelism`)

## CLI Parity / Automation Snapshot

This fork already carries a lot of CLI parity that upstream Every Code didn't
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
- A "primary" lane concept (primary vs secondary)
- Full settings UI + slash command wiring

Reference pointers:
- Slash command parsing: `code-rs/tui/src/slash_command.rs`
- Setup view: `code-rs/tui/src/bottom_pane/status_line_setup.rs`
- Render + hover hit regions: `code-rs/tui/src/chatwidget/terminal_surface_render.rs`
- State/events: `code-rs/tui/src/app/events.rs`, `code-rs/tui/src/app_event.rs`
- Config fields: `code-rs/core/src/config_types.rs` (`status_line_top`, `status_line_bottom`, `status_line_primary`)

<!-- Phase 3 checklist removed; items were implemented and tracked above. -->
