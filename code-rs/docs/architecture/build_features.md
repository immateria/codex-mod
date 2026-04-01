# Build Features (Compile-Time Gating)

`code-cli` uses Cargo features to cleanly compile out large subsystems for
smaller builds.

## Features

### `managed-network-proxy`

Includes the managed network mediation proxy (and its TUI Settings UI).

- Default build: enabled.
- When disabled: `[network] enabled=true` is ignored and Code emits a warning
  during session configuration.

### `browser-automation`

Includes the integrated browser automation stack (Chrome integration, related
TUI UI, and interactive `browser` tool actions).

- Default build: enabled.
- When disabled:
  - The Settings UI has no **Chrome** section.
  - `[browser] enabled=true` is ignored and Code emits a warning during session
    configuration.
  - The `browser` tool remains available, but is restricted to HTTP-only
    `fetch`/`status` behavior (no interactive automation).

## Build Recipes

```bash
# Full build (status quo)
cargo build -p code-cli

# Small build (no managed proxy, no browser automation)
cargo build -p code-cli --no-default-features

# Small build + managed proxy only
cargo build -p code-cli --no-default-features --features managed-network-proxy

# Small build + browser automation only
cargo build -p code-cli --no-default-features --features browser-automation
```

