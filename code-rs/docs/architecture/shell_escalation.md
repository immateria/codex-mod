# Shell Escalation (Patched zsh Fork + `codex-execve-wrapper`)

Code can optionally run sandboxed `shell` tool calls through a Unix-only
zsh-fork path that supports interactive approvals for specific intercepted
subcommands (network-heavy / git-write-heavy).

This is **disabled by default** and only activates when all of these are true:

- The build is running on Unix.
- `features.shell_zsh_fork = true` is enabled in `config.toml`.
- The session shell is zsh (detected or configured).
- `zsh_path` is set to an **absolute path** to a **patched** `zsh` that supports
  `EXEC_WRAPPER`.
- `codex-execve-wrapper` is available (see discovery rules below).
- The command being executed is a zsh wrapper invocation: `zsh -lc <script>` or
  `zsh -c <script>`.
- The execution is sandboxed (`sandbox_type != None`).

When active, intercepted subcommands may request approvals to rerun with:

- Expanded sandbox permissions (preferred), e.g. enabling network for a single
  subcommand while remaining sandboxed.
- Unsandboxed execution as a fallback when the requested permissions cannot be
  represented with the current sandbox policy surface (e.g. "network enabled"
  under a read-only policy).

## Enabling

In `CODE_HOME/config.toml`:

```toml
[features]
shell_zsh_fork = true

# Absolute path to a patched zsh binary.
zsh_path = "/abs/path/to/patched/zsh"

# Optional override (otherwise auto-discovered).
# main_execve_wrapper_exe = "/abs/path/to/codex-execve-wrapper"
```

## `codex-execve-wrapper` discovery

If `main_execve_wrapper_exe` is unset, Code looks for `codex-execve-wrapper` in:

1. The directory containing the running `code` executable (sibling binary).
2. The first hit on `PATH`.

If a configured or discovered wrapper path does not exist, the zsh-fork backend
is disabled for the session (and Code falls back to normal shell execution).

## Building the wrapper

From `code-rs/`:

```bash
cargo build -p codex-shell-escalation --bin codex-execve-wrapper
```
