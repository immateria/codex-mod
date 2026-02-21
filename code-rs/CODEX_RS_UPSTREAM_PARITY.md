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

These are present in the tool schema / config, but not fully implemented in the
new tool runtime.

#### 1) `search_tool_bm25` (MCP tool discovery helper)

**Problem**
- Tool schema exists (`search_tool_bm25` is emitted when enabled), and developer
  instruction templates exist, but there is **no handler** and no session state
  to remember "selected" MCP tools.

**Repo facts**
- Tool schema: `code-rs/core/src/openai_tools.rs` (`SEARCH_TOOL_BM25_TOOL_NAME`)
- Templates:
  - `code-rs/core/templates/search_tool/tool_description.md`
  - `code-rs/core/templates/search_tool/developer_instructions.md`
- Upstream reference (Codex Apps only; useful for structure):
  - `codex-rs/core/src/tools/handlers/search_tool_bm25.rs`
  - `codex-rs/core/src/state/session.rs` (`active_mcp_tool_selection` merge/dedupe semantics)
- ToolsConfig flag is set in client config:
  - `code-rs/core/src/client.rs` sets `tools_config.search_tool = self.config.tools_search_tool;`
- Turn execution currently always includes MCP tools:
  - `code-rs/core/src/codex/streaming.rs` uses `mcp::policy::filter_tools_for_turn(...)`

**Tool-name stability (must use MCP-qualified keys)**

In `code-rs`, MCP tools are exposed to the model using a **Responses-API-safe**
fully-qualified tool name generated by `McpConnectionManager`:

- Raw qualified name: `"<server>__<tool>"`
- Sanitization: invalid chars replaced with `_` (`sanitize_responses_api_tool_name`)
- Disambiguation: possible SHA1 suffix when collisions or length limits occur

Implications for `search_tool_bm25`:

- The "tool name" the model sees (and must later call) is the **qualified key**
  in `McpConnectionManager`'s `tools` map.
- Do **not** derive tool names by splitting on `"__"` or re-sanitizing yourself.
  Store and return the *exact* qualified key.
- Use `Session::mcp_connection_manager().parse_tool_name(&qualified)` to map back
  to the server + raw tool name for actual MCP calls.

**Proposed behavior (aligned with local MCP access prompting)**

- When `tools.search_tool=true`, MCP tools are **hidden from the model until it
  searches**. This prevents "tool overload" and keeps turns cheaper.
- `search_tool_bm25` searches across **enabled + discovered** MCP tools.
  Selection does **not** grant access; the existing MCP access prompting remains
  the canonical enforcement point when a tool is actually invoked.
- If a tool is disabled via MCP settings (`excluded_tools`), it should not appear
  in search results and should not be selectable.

**Implementation sketch (decision complete)**
- Add per-session, in-memory selection state (not persisted):
  - `code-rs/core/src/codex/session.rs` `State` gains `active_mcp_tool_selection: Option<Vec<String>>`
  - Add Session methods mirroring upstream behavior (merge/dedupe + getters):
    - `Session::merge_mcp_tool_selection(Vec<String>) -> Vec<String>`
    - `Session::get_mcp_tool_selection() -> Option<Vec<String>>`
    - `Session::clear_mcp_tool_selection()`
- Add handler:
  - `code-rs/core/src/tools/handlers/search_tool_bm25.rs`
  - Add `bm25` dependency (workspace + `code-core`).
  - Index **enabled** MCP tools from `sess.mcp_connection_manager().list_all_tools_with_server_names()`.
  - Return JSON payload with matches + scores; update session selection so matched tools
    can be exposed on subsequent turns.
- Gate MCP tool exposure in `run_turn` / tool-building:
  - If `tools_config.search_tool=true` and **no selection exists**, pass *no* MCP tools to
    `get_openai_tools(...)` (so the model must call `search_tool_bm25` first).
  - If selection exists, include only selected MCP tools (plus always-available non-MCP tools).
- Defense-in-depth:
  - If `tools_config.search_tool=true` and the model calls an MCP tool name that isn't in the
    current selection, return a failure output instructing it to call `search_tool_bm25` first.

**Template note (needs update once implemented)**
- `code-rs/core/src/openai_tools.rs` currently renders `{{app_names}}` as
  `"Codex Apps MCP servers"`, but this fork's behavior is "search across enabled MCP
  tools". Once `search_tool_bm25` is live, we should update:
  - `code-rs/core/src/openai_tools.rs::render_search_tool_description()`
  - `code-rs/core/templates/search_tool/*`
  to avoid misleading instructions.

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
  built in `ModelClient`, but not on the session's stored config.

**Repo facts**
- ConfigureSession builds `tools_config` in `code-rs/core/src/codex/streaming.rs`
  and only sets `web_search_allowed_domains` after construction.

**Fix**
- In ConfigureSession, set:
  - `tools_config.search_tool = config.tools_search_tool;`
  - `tools_config.web_search_external = config.tools_web_search_external;`

## Tests To Add/Extend (Core)

- Extend the "registry completeness" test (`registry_has_handlers_for_default_openai_function_tools`
  in `code-rs/core/src/tools/router.rs`) to cover the "missing tools" flags:
  - `tools_config.search_tool=true` => `search_tool_bm25` has a handler
  - `include_apply_patch_tool=true` => `apply_patch` has a handler (function/freeform)
  - `use_streamable_shell_tool=true` => `exec_command` + `write_stdin` have handlers
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

Upstream (`codex-rs`) supports a "streamable shell" tool pair:

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
  that the core cannot handle (tool calls fail as "unsupported call").

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
    - Upstream warns: "apply_patch was requested via exec_command; use apply_patch tool"
    - In `code-rs`, this should share the same apply_patch verification + /branch guard
- Update tool parallelism classification:
  - `exec_command` / `write_stdin` should be `Exclusive` (ordering + mutation risk)

**Tests to add**
- Extend `code-rs/core/src/tools/router.rs` completeness test to cover
  `use_experimental_streamable_shell_tool=true` and assert handlers exist for both tools.

## Partial / Duplicate Migrations to Be Aware Of

There is some "half-migrated" upstream structure in this fork that can confuse
future work if we don't call it out explicitly.

### `unified_exec` duplication

- `code-rs/core/src/unified_exec/mod.rs` exists, is compiled, and is marked
  `#![allow(dead_code)]`.
- It is not wired into the tool router and does not map cleanly to the upstream
  `exec_command` / `write_stdin` model.

**Recommendation**
- Treat `exec_command` / `write_stdin` (via `code-rs/core/src/exec_command/*`)
  as the canonical streamable-shell implementation.
- Either delete `unified_exec` later or repurpose it as an internal helper, but
  do not build new tool routing around it.

### `state/` + `tasks/` scaffolding

`code-rs/core/src/state/*` and `code-rs/core/src/tasks/*` exist but are not
currently part of the compiled crate graph (not referenced from `lib.rs`).

**Recommendation**
- Ignore for Phase 3 tool parity work.
- Consider deleting or finishing a full migration later, but only if we decide
  to adopt upstream's task runtime patterns.

## Upstream Tool Inventory Delta (Handlers)

Upstream tool handlers live in `codex-rs/core/src/tools/handlers/`.
This fork's tool runtime handlers live in `code-rs/core/src/tools/handlers/`.

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
- File helpers not exposed in this fork's tool schema:
  - `read_file` (safe, structured file reads)
  - `list_dir` (structured directory listing)
  - `grep_files` (non-mutating repo search wrapper around `rg`)
- Optional runtime tools:
  - `js_repl` (requires a feature-flag + runtime integration that this fork doesn't have today)
- MCP "resource" helpers (`mcp_resource`) are not currently exposed as first-class tools here.

## Upstream Tool Runtime Modules Not Yet Ported

Upstream `codex-rs/core/src/tools/` includes a more complete "tool runtime" stack:

- `orchestrator.rs`: central approval + sandbox attempt + retry semantics per tool runtime
- `sandboxing.rs`: shared traits/types for approvable/sandboxable tool runtimes, cached approvals, etc.
- `parallel.rs`: tool call runtime that supports cancellation + mutating vs parallel gating
- `runtimes/*`: per-tool runtime implementations (`shell`, `apply_patch`, `unified_exec`, ...)
- `network_approval.rs`: managed network policy approvals (proxy-mediated)
- `js_repl/*`: JS REPL kernel + handler plumbing

This fork's `code-rs/core/src/tools/` intentionally stayed lighter-weight:

- We kept most approval/sandbox behavior in the existing `exec_tool` path
  (`code-rs/core/src/codex/exec_tool.rs`) and per-tool handlers.
- Parallel tool call batching is implemented in `code-rs/core/src/codex/streaming.rs`,
  not in `code-rs/core/src/tools/parallel.rs` (which does not exist here).

**Implication for Phase 3**
- We should not attempt to "fully port" upstream's orchestrator/sandboxing stack as
  part of completing the missing tools. It's a large structural change and would
  risk regressions in local features (MCP prompting + shell/style routing).
- We *can* borrow the key ideas when it reduces duplication:
  - per-handler "mutating vs parallel-safe" metadata (instead of hardcoded name lists)
  - central tool-call begin/end emission helpers to avoid copy/paste

## MCP Resource Tools (Optional)

Upstream exposes MCP resource discovery + reading as tools:
- `list_mcp_resources`
- `list_mcp_resource_templates`
- `read_mcp_resource`

This fork currently surfaces the same information primarily via UI/AppEvents:
- `code-rs/core/src/codex/streaming.rs` (`Op::ListMcpTools`) already gathers:
  - resources by server
  - resource templates by server
  - auth statuses

**Gap**
- `code-rs/rmcp-client` and `code-rs/core/src/mcp_connection_manager.rs` support
  listing resources/templates, but do **not** currently expose "read resource"
  RPCs as a first-class client call.

**Recommendation**
- Keep MCP resources as a UI feature for now.
- Revisit as a separate migration once `search_tool_bm25`/`apply_patch`/`exec_command`
  are stable, because adding `read_resource` likely requires expanding the RMCP client surface.

## File Helper Tools (Optional)

Upstream includes "safe, structured" file helpers that reduce reliance on raw shell:
- `read_file` (slice or indentation-aware)
- `list_dir` (paged, depth-bounded directory listing)
- `grep_files` (non-mutating wrapper around `rg`)

This fork currently prefers:
- `shell` / `container.exec` for inspection/search
- plus guardrails that prevent direct file writes outside `apply_patch`

**Recommendation**
- Consider porting these once `apply_patch` is a first-class tool handler.
- Benefits: less shell noise, easier to constrain outputs, more deterministic behavior.

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

## Proposed "Phase 3" Work Order (Next)

This is the next minimal set of migrations that unlock the remaining upstream
tooling without sacrificing local fork features:

1. Implement `search_tool_bm25` end-to-end (handler + per-session selection state + MCP gating).
2. Implement `apply_patch` tool call handling (share the same safety + hooks + /branch guard).
3. Wire `exec_command` / `write_stdin` tool handlers (reuse `exec_command::SessionManager`).
4. Expand tool registry completeness tests so "enabled tool == has handler" holds for:
   - `tools_search_tool`
   - `include_apply_patch_tool`
   - `use_experimental_streamable_shell_tool`

Optional after the above is stable:
- Add structured file helper tools (`read_file`, `list_dir`, `grep_files`) if we want to reduce
  reliance on arbitrary shell commands for inspection/search.

## Phase 3 Checklist (Concrete)

This is the "do it" checklist for the next implementation pass, written to
avoid re-reading upstream again:

1. `search_tool_bm25`
   - Add `bm25` dep.
   - Add `active_mcp_tool_selection` to `code-rs/core/src/codex/session.rs` state + Session helpers.
   - Implement handler + register it in:
     - `code-rs/core/src/tools/handlers/mod.rs`
     - `code-rs/core/src/tools/router.rs` registry init
   - Wire gating where tools are built for the turn (MCP tool list depends on selection).
   - Update tool description/template wording away from "Codex Apps".

2. `apply_patch`
   - Implement handler supporting `ToolPayload::Function` and `ToolPayload::Custom`.
   - Factor `/branch`-safety guard so shell interception and tool handler share one implementation.
   - Register handler + extend completeness test with `include_apply_patch_tool=true`.

3. `exec_command` / `write_stdin`
   - Implement handler(s) using `code-rs/core/src/exec_command/session_manager.rs`.
   - Register both tool names + extend completeness test with `use_streamable_shell_tool=true`.
   - Keep ordering strict (associate outputs with existing `(sub_id, call_id, seq_hint, output_index)`).
