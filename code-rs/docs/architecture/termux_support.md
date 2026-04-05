# Termux / Android Support Notes

This repo can be built and run in Termux (Android). This document captures the
Android-specific build/runtime constraints plus the current status of the
workarounds in `code-rs`.

## Terminology

- **Android build**: `target_os = "android"`.
- **Termux runtime**: an Android build running inside Termux. Usually detectable
  via `TERMUX_VERSION`.

## Build Guidance

- Recommended Termux build:
  - `cargo build -p code-cli --no-default-features`
- Desktop-only subsystems are intentionally blocked on Android:
  - `managed-network-proxy` (pulls `aws-lc-sys`)
  - `browser-automation` (pulls `chromiumoxide`)

See `code-rs/docs/architecture/build_features.md`.

## Known Android/Termux-Specific Items

### 1) URL Opening (`webbrowser`) on Android

Status: Implemented.

- Android uses `termux-open-url` (best-effort, non-blocking spawn):
  - TUI: `code-rs/tui/src/open_url.rs`
  - rmcp-client OAuth: `code-rs/rmcp-client/src/open_url.rs`
  - Login server: `code-rs/login/src/server.rs` (Android branch)
- `webbrowser` is only a non-Android dependency:
  - `code-rs/tui/Cargo.toml`
  - `code-rs/rmcp-client/Cargo.toml`
  - `code-rs/login/Cargo.toml`

Fast checks:
- `rg -n "\\bwebbrowser::" code-rs` should only match inside
  `#[cfg(not(target_os = "android"))]` blocks.

### 2) Managed Network Proxy (Desktop-Only)

Status: Implemented compile-time gating + Android fail-fast.

- Feature: `managed-network-proxy` (enabled by default in `code-cli`).
- Android builds fail fast (clear recipe to use `--no-default-features`):
  - `code-rs/cli/src/main.rs`
  - `code-rs/network-proxy/src/lib.rs`
- Feature-off behavior: `[network] enabled=true` is ignored and a warning is
  emitted during session configuration (see
  `code-rs/docs/architecture/network_mediation.md`).

Fast checks:
- `cargo tree -p code-cli --no-default-features | rg aws-lc-sys` should be empty.

### 3) Browser Automation / Chrome (Desktop-Only)

Status: Implemented compile-time gating + Android fail-fast.

- Feature: `browser-automation` (enabled by default in `code-cli`).
- Android builds fail fast:
  - `code-rs/cli/src/main.rs`
  - `code-rs/browser/src/lib.rs`
- Feature-off behavior:
  - No **Chrome** Settings section.
  - The `browser` tool is restricted to HTTP-only `fetch/status` (no interactive
    automation).

Fast checks:
- `cargo tree -p code-cli --no-default-features | rg chromiumoxide` should be empty.

### 4) Secrets Without a Functional OS Keyring

Status: Implemented Android fallback.

- Secrets are stored in `CODE_HOME/secrets/local.age` (Age-encrypted).
- Passphrase source:
  - Desktop: OS keyring.
  - Android/test: file fallback at `CODE_HOME/secrets/passphrase` when keyring is
    missing/broken (best-effort 0600 permissions).
- Implementation: `code-rs/secrets/src/local.rs`.

### 5) Hard-coded `/bin/bash` Assumptions

Status: Implemented (no runtime `/bin/bash` dependency).

- Termux typically installs shells under `$PREFIX/bin` (not `/bin`).
- Runtime code should use PATH-resolved `bash` (or the configured/detected
  session shell) rather than `/bin/bash`.
- Shell script style inference knows Termux bash paths:
  - `code-rs/core/src/config_types.rs`

Fast check:
- `rg -n "\\b/bin/bash\\b" code-rs` should not hit runtime code (docs/tests only).

### 6) Native Pickers / File Manager Integration

Status: Implemented Android hide/disable via platform caps.

- Central capability gating:
  - `code-rs/tui/src/platform_caps.rs`
- UI hides or disables picker/reveal affordances when unsupported (Android).

### 7) PTY / `portable-pty` Restrictions

Status: Implemented non-wedging fallbacks.

- Core exec sessions fall back to a pipe-backed implementation when PTY open
  fails (still streams output, supports stdin writes; not a full TTY):
  - `code-rs/core/src/exec_command/session_manager.rs`
  - `code-rs/core/src/unified_exec/mod.rs`
- TUI terminal runs surface a stable error and exit when PTY open fails:
  - `code-rs/tui/src/app/terminal/runs.rs`

### 8) TLS / Dependency Hygiene (rustls-only expectation)

Status: Implemented (avoid default-feature surprises).

- `reqwest` uses `default-features = false` and per-target rustls TLS features.
- Workspace `tokio-tungstenite` uses rustls (webpki) with `default-features = false`.

Fast checks:
- `cargo tree -p code-cli --no-default-features --target aarch64-linux-android | rg "openssl|native-tls"`
  should be empty.

### 9) Clipboard + “Prevent Sleep” (Android UX correctness)

Status: Implemented.

- Clipboard:
  - Android builds cannot enable the TUI clipboard feature (compile-time guard):
    - `code-rs/tui/src/lib.rs`
  - Android does not treat Ctrl+Alt+V as a clipboard-image paste hotkey:
    - `code-rs/tui/src/platform_caps.rs`
    - `code-rs/tui/src/app/events/priority.rs`
- “Prevent sleep while running”:
  - Not surfaced as an Experimental toggle on Android and uses a no-op backend if
    forced on.

### 10) Secure Mode Hardening (`CODEX_SECURE_MODE=1`)

Status: Implemented (fail-closed) with Termux-specific guidance.

- Secure mode runs pre-main hardening and exits if syscalls fail (fail-closed).
- On Termux, failures include an extra hint:
  - “Unset `CODEX_SECURE_MODE` to run without hardening.”
- Implementation:
  - `code-rs/process-hardening/src/lib.rs`

