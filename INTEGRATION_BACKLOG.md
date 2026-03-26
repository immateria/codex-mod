# Integration Backlog

Last updated: 2026-03-26

This repo maintains two Rust workspaces:
- `codex-rs/`: read-only mirror of `openai/codex` (upstream landing zone).
- `code-rs/`: our fork (where all production changes land).

The goal is to selectively port upstream improvements into `code-rs/` while
preserving this fork’s modular, TUI-first architecture and richer MCP tooling.

## Guardrails

- Treat `codex-rs/` as read-only. Only edit Rust under `code-rs/`.
- No `rustfmt`.
- Warnings are failures. Validate with `./build-fast.sh` from repo root.
- Prefer mechanical moves + adapters over reintroducing upstream monoliths.

## Recently Completed (Local Wiring)

- Plugins UI + Settings integration (overlay + bottom pane) and `/plugins`.
  - Commits: `f5f4e554d5`, `36eafdfe12`, `ddb031a8ba`, `fbe36282d8`.
  - Includes `Settings -> Plugins -> Sources` editor for:
    - `plugins.curated_repo_url` / `plugins.curated_repo_ref`
    - `[[plugins.marketplace_repos]]` (`url` + optional `git_ref`)

- Persist session context tuning via the model selector (Ctrl+S).
  - Writes `model_context_window` + `model_auto_compact_token_limit` to config
    (active profile) via `code_core::config_edit`.
  - Commit: `0470b8c175`.

- MCP elicitation end-to-end:
  - Core emits `EventMsg::ElicitationRequest`; TUI prompts via the existing
    request-user-input picker and sends `Op::ResolveMcpElicitation`.
  - Accept returns `{}` (matches UI copy) with a focused unit test.
  - Commit: `31b4a30879`.

- Reduce `refresh_token_reused` friction:
  - Serialize refresh operations within-process and retry adopting rotated
    tokens from disk before failing.
  - Commit: `237adbea2a`.

- hooks.json lifecycle hooks:
  - Port upstream engine as `code-hooks` and wire into `code-core` + TUI.
  - Runs for: `session_start`, `user_prompt_submit`, `pre_tool_use`, `stop`.
  - Emits `HookStarted` / `HookCompleted` events (rendered in TUI history).
  - Stop hook blocks inject a `hook_prompt` continuation and retry the turn.
  - Commits: `82d8161d76`, `c32b59d612`, `54f0c045ed`, `299e43babd`.

## Next: Upstream Intake (Selective, Bisectable)

The high-level workflow:
1. `git fetch upstream OpenAI_Codex`
2. Review deltas separately:
   - `upstream/main` (just-every/code): fork-specific improvements we may want.
   - `OpenAI_Codex/main` (openai/codex): new upstream features; land in `codex-rs/`
     first, then selectively port to `code-rs/`.
3. Produce a shortlist with:
   - value/ROI (what it unlocks),
   - risk (surface area touched),
   - integration shape (port verbatim vs adapt to our architecture).
4. Port in small commits with `./build-fast.sh` green after each.

## Remaining Codex-RS-Only Subsystems Worth Considering

These exist in `codex-rs/` but are not fully ported into `code-rs/`.

### High ROI

- `codex-rs/hooks/`
  - Hook config + execution + lifecycle events.
  - Useful for “pre/post tool/exec/turn” automation with first-class UX.
  - Partially ported as `code-hooks` + lifecycle runtime; remaining work is the
    Stop hook continuation loop and any app-server parity gaps.

- `codex-rs/connectors/` + `codex-rs/features/`
  - Connector/app surface that upstream plugins build on.
  - Worth porting if we want richer plugin “apps” behavior without importing
    upstream’s monolithic managers.

- `codex-rs/secrets/`
  - General secret storage beyond `auth.json` (helps connectors/plugins auth).
  - Current state in `code-rs` already covers the core “store secrets in keychain” needs for:
    - CLI auth payload (`CODE_HOME/auth.json`) via `cli_auth_credentials_store` (`file|keyring|auto|ephemeral`):
      - Storage backends: `code-rs/core/src/auth/storage.rs`
      - Keyring abstraction: `code-rs/keyring-store/src/lib.rs` (macOS Keychain via `keyring` crate)
      - TUI settings UI: `code-rs/tui/src/bottom_pane/settings_pages/accounts/account_switch_settings_view/*`
    - Multi-account store (`auth_accounts.json`) uses the same backend modes:
      - `code-rs/core/src/auth_accounts.rs`
    - MCP OAuth tokens (streamable-http MCP servers) are stored separately:
      - Store modes: `code-rs/rmcp-client/src/oauth.rs` (`auto|file|keyring`)
      - Wiring: `code-rs/core/src/mcp_connection_manager.rs` + config fields in `code-rs/core/src/config.rs`
  - Remaining gap vs `codex-rs/secrets/`:
    - No general-purpose secret store for arbitrary provider keys / connector/plugin auth beyond the
      OpenAI/ChatGPT auth payload and MCP OAuth tokens; non-OpenAI provider keys are still
      sourced via the provider’s `env_key` in config.

### Medium ROI

- `codex-rs/exec-server/`
  - Out-of-process exec runner plumbing for stronger isolation and robustness.

- `codex-rs/package-manager/`
  - Package manager detection + common command helpers (useful for skills/hooks).

- `codex-rs/terminal-detection/` and parts of `codex-rs/shell-escalation/`
  - Better environment detection/routing (mux/alt-screen/escalation ergonomics).

- `codex-rs/feedback/`
  - Structured feedback capture/submission.

- `codex-rs/artifacts/`
  - Artifact packaging/retention plumbing beyond the transcript.

### Lower Priority / Mostly Tooling

- `codex-rs/app-server-client/`, `codex-rs/app-server-test-client/`, `codex-rs/tui_app_server/`
  - Useful mainly for integration testing and external client tooling.

- `codex-rs/codex-api/` + `codex-rs/codex-client/`
  - Only worth porting if we want to converge our backend client stack.

- `codex-rs/lmstudio/`
  - Local-model integration; optional.
