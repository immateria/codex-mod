# Integration Backlog

Last updated: 2026-04-01

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

- Apps/connectors + Experimental features:
  - Settings -> Apps (multi-account connector sources / OAuth pinning).
  - `/apps` picker + AppLinkView install/refresh flow.
  - Profile-scoped `[features]` map + Settings -> Experimental; `features.apps` gates Apps sources.
  - Commits: `15074f1bd3`, `c3dc11e25d`, `79759dc6f6`, `d28f33d240`, `871d588151`,
    `939d43d122`, `035a4c1a87`, `46d32fd12e`, `52c8018ee8`.

- App-server `app/list` parity:
  - `app/list` merges directory apps with accessible connector metadata from `codex_apps_*` MCP tools.
  - Sends full `app/listUpdated` payload after plugin install/uninstall.
  - Validates `threadId` (UUID) and supports gated listing based on thread config.
  - Commits: `d1b22be725`, `ddbb86eff6`, `2dbec778cc`.

- `codex-rs/package-manager`:
  - Port upstream managed package installer (platform detection + archive fetch/verify/extract + install locks).
  - Commit: `c0886bdb64`.

- General secrets store + credential env fallback:
  - Ported the local secrets store and CLI, plus fallback from selected credential env vars into the secrets store.
  - TUI now has a first-class Secrets settings page for listing/deleting entries; creation remains CLI-first.
  - Commits: `8ca315135c`, `5b2514810d`, `48d43cdc65`, `690ab7b44c`.

- Managed installer upgrade detection for `/update` + Upgrade settings UI:
  - Supports `tui.upgrade_command` override and installer inference (npm, bun, Homebrew formula).
  - Bun global installs are detected via both `~/.bun/bin` and custom `BUN_INSTALL` roots.
  - Commits: `9eeb5779a0`, `3064811f4e`, `1d7111b212`.

- `codex-rs/terminal-detection`:
  - Ported into `code-rs` and wired for better shell / terminal environment detection.
  - Commit: `ac153a300a`.

- `codex-rs/exec-server`:
  - Ported and wired for app-server `fs/*` routing.
  - Commit: `5f21114875`.

- Compile-time gating for managed network proxy:
  - Adds Cargo feature `managed-network-proxy` (default enabled in `code-cli`) to compile out the managed proxy stack for “small builds”.
  - When compiled without `managed-network-proxy`:
    - Settings UI has no **Network** section and no network status segment.
    - `[network] enabled=true` is ignored and Code emits a warning during session configuration.
  - Commit: `88750b31fa`.

- Compile-time gating for browser automation:
  - Adds Cargo feature `browser-automation` (default enabled in `code-cli`) to compile out the
    integrated Chrome/browser automation stack for “small builds”.
  - When compiled without `browser-automation`:
    - Settings UI has no **Chrome** section.
    - `[browser] enabled=true` is ignored and Code emits a warning during session configuration.
    - `browser` tool remains available but is restricted to HTTP-only `fetch/status`.
    - Login “via browser” fallback is compiled out.
  - Adds `code-rs/docs/architecture/build_features.md` and removes `phase0-baseline.md`.
  - Commit: `a9e8eeedac`.

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
  - Largely ported as `code-hooks` + lifecycle runtime + TUI rendering.
  - Remaining work is mostly parity/polish:
    - upstream schema/method-name audit
    - edge-case review around stop/continuation behavior
    - any remaining app-server/client hook-surface gaps

- `codex-rs/secrets/`
  - Core local secret storage is now ported:
    - general encrypted local store + CLI management
    - provider credential fallback from selected env vars into the store
    - first-class TUI Secrets page for listing/deleting stored keys
  - Remaining work is parity/polish rather than missing storage:
    - broader audit of remaining credential env-var call sites
    - richer in-TUI creation/edit UX if desired
    - any connector/plugin-specific secret flows that still assume env-only inputs

### Medium ROI

- parts of `codex-rs/shell-escalation/`
  - Additional escalation ergonomics beyond the now-ported terminal detection pieces.

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
