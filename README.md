<p align="center">
  <img src="docs/images/every-logo.png" alt="Every Code" width="260" />
</p>

<h1 align="center">codex-mod</h1>

<p align="center">
  Based on <code>every-code/main</code> with a focused set of local changes (Android/Termux builds, tool/runtime parity work, managed network mediation, and TUI UX improvements).
</p>

<p align="center">
  <a href="#differences-vs-upstream-every-code">Differences</a> ·
  <a href="#dev-notes">Dev notes</a> ·
  <a href="#compare-with-upstream">Compare</a> ·
  <a href="#validate-locally">Validate</a>
</p>

---

## Differences vs upstream Every Code

This fork focuses on pulling newer upstream (`codex-rs`) capabilities into
`code-rs` while integrating local platform work and TUI/automation features.

> [!NOTE]
> This README is intentionally short: it only lists what’s different from
> upstream Every Code. For install/usage docs and the general feature overview,
> use upstream.
>
> - Upstream remote: `upstream` (just-every/code)
> - Upstream baseline branch: `every-code/main`

- **Android/Termux build support**
  - Extra scripts and docs in the repo root: `build.zsh`, `ANDROID_BUILD*.md`, `android-build-demo.sh`.
- **Build + toolchain hygiene**
  - A fast zsh build wrapper with caching and guardrails: `./build-fast.sh` + `build-fast.zsh`.
  - Toolchain pinning/selection fixes so local builds don’t silently drift.
- **Tool/runtime parity work (codex-rs -> code-rs)**
  - Tool router/registry + per-tool handlers (instead of monolithic dispatch).
  - Tool scheduling support for safe parallel tool calls.
  - New tool handlers used by automation: `search_tool_bm25` (tool discovery), `apply_patch`, `exec_command`/`write_stdin` (PTY sessions), plus file helpers and optional `js_repl`.
  - Optional MCP tool gating: hide MCP tools until the model selects them via `search_tool_bm25`.
- **Deterministic streaming + recovery**
  - Stricter per-turn ordering for streamed UI inserts, plus a CLI `order_replay` helper for debugging ordering issues.
  - Safer retry/auto-compact reconciliation for tool calls and tool outputs across more tool-call types.
- **Managed network mediation (proxy + approvals + UI)**
  - Core “managed proxy” mediation with allow/deny lists and temporary approvals.
  - TUI: Settings → Network, plus status line indicator and deep link.
  - macOS: enforce mediated egress for `exec_command` via seatbelt-wrapped PTY children (other platforms are best-effort).
  - Network approvals are network-scoped and temporary; persistent policy is edited via allow/deny lists.
- **Layered config and diagnostics**
  - Additional config layers (system + project + user) with better error/diagnostic surfaces.
  - CLI schema/validation helpers via `code config schema` and `code config validate`.
  - Can validate against both the Codex and code-rs schemas.
- **CLI automation/debug parity**
  - `code fork` (fork a recorded session).
  - `code sandbox` (debug/inspect sandbox support).
  - `code debug app-server send-message-v2` (headless scripting/debugging for the app-server v2 protocol).
- **App-server backports**
  - More app-server message processor/runtime wiring (including v2 surfaces) to expose richer config + MCP status information to the TUI (server status, tools/resources, failures, auth).
  - Settings flows use config read/write helpers so changes can apply immediately.
- **Browser/CDP backports (where supported)**
  - Split/refactored browser manager and expanded CDP operations.
  - Android builds compile out Chrome/CDP integration.
- **Auth + accounts**
  - Runtime-selectable credential store mode (`file`, `keyring`, `auto`, `ephemeral`) with TUI wiring and per-change apply/migrate prompts.
  - Multi-account flows kept working across store-mode changes.
- **Agent execution and automation backports**
  - Refactored agent exec runner modules (command detection, arg planning, runtime paths) to make execution behavior more consistent across shells.
  - Exec runtime/session loop refactors to improve non-interactive automation flows.
- **MCP auth/status + richer TUI controls**
  - OAuth `login`/`logout` flow for streamable HTTP MCP servers.
  - Per-server auth status surfaced end-to-end (core -> app-server -> CLI -> TUI).
  - CLI: `mcp status` dumps live server status (supports `--json`).
  - TUI MCP settings support per-tool enable/disable, tool detail inspection, server resource listing, and improved mouse/keyboard interactions.
- **MCP access policy backports**
  - Style/profile-aware allow/deny prompting for MCP server access, including per-session allow-lists.
- **Shell selection + style/profile routing**
  - Shell selection is configurable and drives style profile behavior.
  - Style profiles can influence skills/profiles and MCP allow/deny behavior.
  - TUI improvements: native file pickers + file-manager shortcuts, user-defined profile summaries (with optional AI-generated summary), and better settings navigation.
  - Profile filters (skills/MCP) are selectable lists instead of “free text you have to remember”.
- **Skills backports**
  - Explicit skill/file mentions can inject skill contents into prompts.
  - Warn when mentioned skills depend on MCP servers that are missing/disabled.
  - Preserve extra `SKILL.md` frontmatter on edits.
- **Collaboration modes and tool specs**
  - Collaboration-mode instruction templates in core.
  - Tool spec/output-format plumbing used for automation and structured tool configuration.
- **Approvals/command safety backports**
  - Canonicalize command approval matching across shells.
  - Expanded command safety context and Windows/PowerShell-specific safety handling.
- **Cross-platform MCP improvements**
  - Better MCP server program resolution on Windows.
- **Performance + regression harnesses**
  - TUI perf harnesses and targeted rendering optimizations (markdown wrapping/history rendering).
  - VT100 snapshot harnesses to keep rendering changes reviewable.
- **Maintainability refactors**
  - Split large modules (core streaming/configure-session, client transport/SSE, browser manager, TUI input pipeline) to reduce merge pain and make backports easier.
  - Parity + migration docs to keep future backports honest: `code-rs/CODEX_RS_UPSTREAM_PARITY.md`, `code-rs/MIGRATION_*`.
- **Status line customization**
  - Configurable top/bottom status lines (separate settings, not necessarily symmetric).
  - Custom line rendering supports hover/click affordances similar to the default bar.

## Dev notes

- Rust sources to edit live under `code-rs/`.
- `codex-rs/` is treated as a read-only mirror of OpenAI Codex and used for
  parity work and reference.

## Compare with upstream

```bash
git diff every-code/main..main

git log --oneline every-code/main..main
```

## Validate locally

```bash
./build-fast.sh
```
