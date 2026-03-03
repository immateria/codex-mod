<p align="center">
  <img src="docs/images/every-logo.png" alt="Every Code" width="260" />
</p>

<h1 align="center">codex-mod</h1>

<p align="center">
  Fork of <code>every-code/main</code> with a focused set of changes (Android/Termux builds, Codex ports into <code>code-rs</code>, managed network mediation, and TUI UX cleanup).
</p>

<p align="center">
  <a href="#differences-vs-upstream-every-code">Differences</a> ·
  <a href="#feature-matrix-quick-map">Matrix</a> ·
  <a href="#dev-notes">Dev notes</a> ·
  <a href="#compare-with-upstream">Compare</a> ·
  <a href="#validate-locally">Validate</a>
</p>

---

## Differences vs upstream Every Code

Two kinds of changes vs `every-code/main`:
- `Codex port`: pulled from upstream `codex-rs` into `code-rs`.
- `Fork choice`: intentional behavior/architecture change (not just a straight port).

## Feature matrix (quick map)

Legend: `Hybrid` means it started as a Codex port, then got adapted to fit this fork.

| Area                                             | Source      | How it works here                                                                                              | Pointers                                                                                                                           |
| ------------------------------------------------ | ----------- | -------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Tool runtime architecture                        | Codex port  | Tool routing moved out of streaming into a router/registry/handler layout.                                     | `code-rs/core/src/tools/*`                                                                                                         |
| Parallel tool dispatch                           | Codex port  | Parallel batches exist, but get downgraded when ordering metadata is missing.                                  | `code-rs/core/src/tools/scheduler.rs`                                                                                              |
| Tool handler coverage checks                     | Codex port  | Tests ensure enabled function/freeform tools have handlers.                                                    | `code-rs/core/src/tools/router.rs`                                                                                                 |
| MCP dispatch policy                              | Fork choice | MCP `dispatch=parallel` is user-configurable and still access-gated.                                           | `code-rs/core/src/mcp_connection_manager.rs`<br>`code-rs/core/src/config_types.rs`                                                 |
| `search_tool_bm25`                               | Hybrid      | MCP tools stay hidden until search selection exists, then only selected tools are exposed.                     | `code-rs/core/src/tools/handlers/search_tool_bm25.rs`<br>`code-rs/core/src/tools/router.rs`                                        |
| `apply_patch`                                    | Hybrid      | Dedicated handler, but still uses local branch/safety/hook/diff guardrails.                                    | `code-rs/core/src/tools/handlers/apply_patch.rs`<br>`code-rs/core/src/codex/exec_tool.rs`                                          |
| Streamable shell (`exec_command`, `write_stdin`) | Hybrid      | PTY sessions with cleanup on abort/kill and apply-patch interception guidance.                                 | `code-rs/core/src/tools/handlers/exec_command.rs`<br>`code-rs/core/src/exec_command/session_manager.rs`                            |
| JS REPL                                          | Hybrid      | Runtime-selectable (`node`/`deno`), configurable runtime path/args, per-call override; Node >=18 required.     | `code-rs/core/src/tools/js_repl/*`<br>`code-rs/core/src/tools/handlers/js_repl.rs`                                                 |
| JS REPL history linkage                          | Fork choice | Child execs track `parent_call_id`, show lineage markers, and support jump-to-parent navigation.               | `code-rs/tui/src/history_cell/js_repl.rs`<br>`code-rs/tui/src/chatwidget/exec_tools/lifecycle/begin_flow.rs`                       |
| MCP resource tools                               | Codex port  | `list_mcp_resources`, `list_mcp_resource_templates`, `read_mcp_resource` are implemented.                      | `code-rs/core/src/tools/handlers/mcp_resource.rs`                                                                                  |
| MCP settings editor                              | Fork choice | TUI supports server/tool scheduling edits (persist + apply without a full restart).                            | `code-rs/tui/src/bottom_pane/mcp_settings_view/*`<br>`code-rs/tui/src/app/events.rs`                                               |
| Network mediation                                | Hybrid      | Upstream mediation ideas, but fork UX/policy: temporary approvals + macOS fail-closed path for sandboxed runs. | `code-rs/core/src/network_approval.rs`<br>`code-rs/core/src/seatbelt.rs`<br>`code-rs/tui/src/bottom_pane/network_settings_view.rs` |
| Network approval UX                              | Fork choice | Network prompts have network-specific options (`allow once`, `allow for session`, deny run/open settings).     | `code-rs/tui/src/user_approval_widget.rs`<br>`code-rs/tui/src/chatwidget/history_pipeline/runtime_flow/approvals.rs`               |
| Status line lanes                                | Fork choice | Independent top/bottom status lanes with `/statusline` routing and clickable actions.                          | `code-rs/tui/src/chatwidget/status_line_flow.rs`<br>`code-rs/tui/src/chatwidget/terminal_surface_render.rs`                        |
| Settings UX routing                              | Fork choice | Overlay + bottom pane are both first-class; auto mode switches by width threshold.                             | `code-rs/tui/src/chatwidget/settings_routing.rs`                                                                                   |
| Hotkeys                                          | Fork choice | Global + per-platform overrides (`macos/windows/linux/android/termux/BSD`), Fn keys + modifier chords.         | `code-rs/core/src/config_types.rs`<br>`code-rs/tui/src/bottom_pane/interface_settings_view.rs`                                     |
| Output folding in history                        | Fork choice | Tool/exec/JS-heavy outputs can collapse to keep history readable.                                              | `code-rs/tui/src/history_cell/*`                                                                                                   |
| Shell profile system                             | Fork choice | Shell-style profiles include summaries, scoped skills/references, MCP include/exclude, and safety overrides.   | `code-rs/core/src/config_types.rs`<br>`code-rs/core/src/config/sources.rs`                                                         |
| Shell/profile UX                                 | Fork choice | Shell selection + profile editing are exposed in settings views.                                               | `code-rs/tui/src/bottom_pane/shell_selection_view.rs`<br>`code-rs/tui/src/chatwidget/shell_config_flow.rs`                         |
| Auth model                                       | Fork choice | Multi-account workflows and separate CLI auth store mode vs MCP OAuth store mode.                              | `code-rs/core/src/auth_accounts.rs`<br>`code-rs/core/src/config.rs`                                                                |
| Credentials storage controls                     | Fork choice | `file`, `keyring`, `auto`, and `ephemeral` modes are configurable and used by TUI account flows.               | `code-rs/core/src/auth/storage.rs`<br>`code-rs/tui/src/bottom_pane/login_accounts_view.rs`                                         |
| Picker/file-manager actions                      | Fork choice | Profile/config path workflows support a picker and "open in file manager", with fallbacks.                     | `code-rs/tui/src/bottom_pane/*`                                                                                                    |
| CLI automation commands                          | Codex port  | `code fork`, `code sandbox`, debug send-message-v2 helpers are present.                                        | `code-rs/cli/src/*`                                                                                                                |
| Streaming recovery                               | Codex port  | Retry/auto-compact reconciliation hardening with stronger tool-call correlation.                               | `code-rs/core/src/codex/streaming/*`                                                                                               |
| Stream ordering diagnostics                      | Fork choice | Ordering guarantees are backed by replay tooling for debugging regressions.                                    | `code-rs/cli/src/bin/order_replay.rs`                                                                                              |
| Utility crates                                   | Codex port  | `stream-parser` and `sleep-inhibitor` are in `code-rs/utils`.                                                  | `code-rs/utils/stream-parser`<br>`code-rs/utils/sleep-inhibitor`                                                                   |
| Android/Termux gating                            | Fork choice | Unsupported browser/CDP paths are compile-time gated; build flow is fork-specific.                             | `code-rs/tui/src/chatwidget.rs`<br>`build.zsh`                                                                                     |

<details>
<summary>More detail (ports, fork choices, guardrails)</summary>

### Codex ports integrated into `code-rs`

- Tool runtime modularization: router/registry/handler replaces monolithic tool routing in streaming.
- Parallel tool scheduler parity: batch/exclusive planning with ordering safeguards.
- Tool surface parity: `search_tool_bm25`, `apply_patch`, `exec_command`, `write_stdin`, optional `js_repl`.
- MCP parity plumbing: richer status/auth/runtime handling.
- MCP resource tools: list/read resource surfaces.
- CLI parity commands: `code fork`, `code sandbox`, and debug send-message-v2 helpers.
- Streaming/recovery parity: retry/compaction reconciliation and tool-call correlation hardening.
- Utility backports: `stream-parser`, `sleep-inhibitor`.

### Fork-specific choices

- Shell style/profile model is first-class config + UX.
- MCP scheduling is enforced in core and editable in TUI.
- Network mediation UX is explicit (settings + approvals + statusline deep links).
- Settings exist in two surfaces (overlay + bottom pane) with width-based routing.
- Hotkeys are configurable with per-platform overrides.
- Multi-account/auth-store behavior is split between CLI auth vs MCP OAuth.
- Android/Termux compile-time gating for unsupported browser/CDP surfaces.

### Hybrid ports (ported, but adapted here)

- `search_tool_bm25` gates MCP visibility until a session selection exists.
- `apply_patch` handler is wired through local safety/hook/diff guardrails.
- `exec_command`/`write_stdin` uses the local exec lifecycle and guardrails.
- Network mediation enforcement policy differs by platform (macOS stricter; others best-effort).

### Guardrails

- `codex-rs` is treated as read-only reference; runtime changes land in `code-rs`.
- Stream ordering stays strict and testable (order replay tooling exists).
- Config persistence stays layered (root/profile/session semantics are deliberate).
</details>

> [!NOTE]
> This README is only about what's different from upstream Every Code. For
> install/usage docs and the general overview, start with upstream.
>
> - Upstream remote: `upstream` (just-every/code)
> - Upstream baseline branch: `every-code/main`

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
