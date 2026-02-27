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
- **Tool/runtime parity work (codex-rs -> code-rs)**
  - Tool router/registry + per-tool handlers (instead of monolithic dispatch).
  - Tool scheduling support for safe parallel tool calls.
  - New tool handlers used by automation: `search_tool_bm25`, `apply_patch`, `exec_command`, `write_stdin` (and optional `js_repl`).
- **Managed network mediation (proxy + approvals + UI)**
  - Core “managed proxy” mediation with allow/deny lists and temporary approvals.
  - TUI: Settings → Network, plus status line indicator and deep link.
  - macOS: best-effort enforcement for `exec_command` via seatbelt-wrapped PTY children.
- **Layered config and diagnostics**
  - Additional config layers (system + project + user) with better error/diagnostic surfaces.
  - CLI schema/validation helpers via `code config schema` and `code config validate`.
  - Can validate against both the Codex and code-rs schemas.
- **CLI automation/debug parity**
  - `code fork` (fork a recorded session).
  - `code sandbox` (debug/inspect sandbox support).
  - `code debug app-server send-message-v2` (headless scripting/debugging for the app-server v2 protocol).
- **App-server backports**
  - More app-server message processor/runtime wiring (including v2 surfaces) to expose richer config + MCP status information to the TUI (server status, tools, resources, failures, auth).
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
  - TUI MCP settings support per-tool enable/disable, tool detail inspection, and improved mouse/keyboard interactions.
- **MCP access policy backports**
  - Style/profile-aware allow/deny prompting for MCP server access, including per-session allow-lists.
- **Shell selection + style/profile routing**
  - Shell selection is configurable and drives style profile behavior.
  - Style profiles can influence skills/profiles and MCP allow/deny behavior.
  - TUI improvements: native file pickers + file-manager shortcuts, user-defined profile summaries (with optional AI-generated summary).
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
- **Maintainability refactors**
  - Split large modules (core streaming/configure-session, client transport/SSE, browser manager, TUI input pipeline) to reduce merge pain and make backports easier.
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
