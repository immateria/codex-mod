<p align="center">
  <img src="docs/images/every-logo.png" alt="Every Code" width="260" />
</p>

<h1 align="center">codex-mod</h1>

<p align="center">
  A fork of <code>every-code/main</code> that went a bit further than planned.
  Personality traits, autonomous runs, browser tools, local LLM support,
  session resume, and a TUI with too many settings pages.
</p>

<p align="center">
  <a href="#whats-different">Differences</a> ·
  <a href="#feature-matrix">Feature matrix</a> ·
  <a href="#personality-system">Personality</a> ·
  <a href="#the-tui">TUI</a> ·
  <a href="#dev-notes">Dev notes</a>
</p>

---

## What's different

About 880 commits on top of upstream, spread across ~60 crates in `code-rs/`.
Some are straight ports from OpenAI's `codex-rs`, some were ported then
reshaped, and some are new.

- **Codex port** — pulled from upstream `codex-rs` into `code-rs`
- **Hybrid** — ported, then changed to fit this fork
- **Fork-only** — doesn't exist upstream

> [!NOTE]
> This README only covers what's different. For general install and usage,
> see upstream.
>
> - Upstream remote: `upstream` (just-every/code)
> - Baseline branch: `every-code/main`

---

## Feature matrix

### Core

| Area               | Source     | What it does                                                           | Where to look                                    |
| ------------------ | ---------- | ---------------------------------------------------------------------- | ------------------------------------------------ |
| Tool runtime       | Codex port | Router/registry/handler instead of monolithic streaming dispatch       | `core/src/tools/*`                               |
| Parallel tools     | Codex port | Batch/exclusive scheduling; degrades when ordering metadata is missing | `core/src/tools/scheduler.rs`                    |
| Streaming recovery | Codex port | Retry and auto-compact reconciliation with tool-call correlation       | `core/src/codex/streaming/*`                     |
| App-server exec v2 | Hybrid     | Streaming output, stdin, resize/terminate, connection-scoped PIDs      | `app-server/`, `app-server-protocol/`            |
| Windows sandbox    | Hybrid     | Restricted-token and elevated modes through a real sandbox backend     | `windows-sandbox-rs/`, `core/src/sandboxing.rs`  |
| MCP dispatch       | Fork-only  | Configurable `dispatch=parallel` with server/tool scheduling edits     | `core/src/mcp_connection_manager.rs`             |
| Network mediation  | Hybrid     | Temporary approvals, macOS fail-closed path, compile-gated             | `network-proxy/`, `core/src/network_approval.rs` |

### AI and models

| Area               | Source    | What it does                                                                               | Where to look                                            |
| ------------------ | --------- | ------------------------------------------------------------------------------------------ | -------------------------------------------------------- |
| Personality traits | Fork-only | 7 trait sliders (1–5), archetype presets, tone axis — [details below](#personality-system) | `core/src/personality_traits.rs`, `tui/.../personality/` |
| Model families     | Hybrid    | Per-model context windows, truncation policy, reasoning effort clamping                    | `core/src/model_family.rs`, `core/src/reasoning.rs`      |
| Auto-drive         | Fork-only | Autonomous agent with Launch→Run→Resolve phases, backoff, metrics                          | `code-auto-drive-core/`                                  |
| Ollama             | Fork-only | Local LLM support — auto-pulls models, detects context window                              | `ollama/`                                                |
| ChatGPT connector  | Fork-only | Backend API for apply commands, task submission                                            | `chatgpt/`                                               |
| Feature flags      | Fork-only | Flag registry with lifecycle stages and an experimental menu                               | `features/`                                              |
| Cloud tasks        | Fork-only | Task tracking UI, review coordination, diff preview                                        | `cloud-tasks/`                                           |
| Memories           | Hybrid    | Epoch-based, SQLite-indexed, published as immutable snapshots                              | `core/src/memories/*`, `memories-state/`                 |

### REPL and tools

| Area                 | Source    | What it does                                                                         | Where to look                                 |
| -------------------- | --------- | ------------------------------------------------------------------------------------ | --------------------------------------------- |
| Multi-runtime REPL   | Hybrid    | Node, Deno, Python — per-runtime toggles, permission management, `codex.emitImage()` | `core/src/tools/repl/`                        |
| REPL history linkage | Fork-only | Child execs track their parent call; you can jump between them                       | `tui/src/history_cell/`                       |
| Browser automation   | Fork-only | Headless Chrome via CDP — screenshots, viewport config, asset storage                | `browser/`                                    |
| `search_tool_bm25`   | Hybrid    | MCP tools stay hidden until a selection exists                                       | `core/src/tools/handlers/search_tool_bm25.rs` |
| `apply_patch`        | Hybrid    | Wired through local safety, hook, and diff checks                                    | `core/src/tools/handlers/apply_patch.rs`      |

### TUI

| Area                | Source    | What it does                                                                 | Where to look                            |
| ------------------- | --------- | ---------------------------------------------------------------------------- | ---------------------------------------- |
| 30 settings pages   | Fork-only | Everything from model selection to shell profiles — [listed below](#the-tui) | `tui/src/bottom_pane/settings_pages/`    |
| Intro animation     | Fork-only | Glitch reveal, gradient effects, 10+ variants                                | `tui/src/glitch_animation.rs`            |
| Status line         | Fork-only | 24 items across top/bottom lanes, clickable, configurable via `/statusline`  | `tui/src/chatwidget/status_line_flow.rs` |
| Icon tiers          | Fork-only | Nerd Fonts → Unicode → ASCII fallback, per-icon overrides in config          | `tui/src/icons.rs`                       |
| Session resume/fork | Fork-only | JSONL session catalog — resume or fork any previous conversation             | `core/src/session_catalog.rs`            |
| Rate limit charts   | Fork-only | Weekly/hourly usage gauges and breakdown                                     | `tui/src/rate_limits_view.rs`            |
| Hotkeys             | Fork-only | Per-platform overrides (macOS, Windows, Linux, Android, Termux, BSD)         | `core/src/config_types.rs`               |
| Shell profiles      | Fork-only | Scoped skills, MCP include/exclude, safety overrides                         | `core/src/config/sources.rs`             |
| Auth                | Fork-only | Multi-account with device code + PKCE, file/keyring/ephemeral storage        | `login/`, `core/src/auth/`               |
| Android/Termux      | Fork-only | Browser/CDP paths compiled out; separate build flow                          | `build.zsh`                              |

---

## Personality system

Pick an archetype, a tone, then dial in traits. Or skip the archetype and
just use the sliders.

**Archetypes:** Pragmatic (default) · Friendly · Concise · Enthusiastic · Mentor · None

**Tone:** Neutral · Formal · Casual · Direct · Encouraging

**Traits** (1–5 scale, 3 = neutral — neutral traits emit no instructions):

| Trait        | 1                | 5                       |
| ------------ | ---------------- | ----------------------- |
| Conciseness  | Very detailed    | Extremely terse         |
| Thoroughness | Trust and ship   | Triple-check everything |
| Autonomy     | Always ask first | Act without asking      |
| Pedagogy     | Just answers     | Deep explanations       |
| Enthusiasm   | Reserved         | High energy             |
| Formality    | Very casual      | Very formal             |
| Boldness     | Conservative     | Bold refactoring        |

The instructions sent to the model adapt based on what model you're using —
reasoning models don't get told to "think step by step" (they already do),
and smaller models get shorter instructions to avoid wasting context.

Edit in the TUI (Personality settings page) or in config:

```toml
[personality]
personality = "friendly"
tone = "casual"

[personality.traits]
conciseness = 4
thoroughness = 2
boldness = 5
```

---

## The TUI

### Settings pages

30 pages, accessible via shortcuts or `/settings`:

| Category    | Pages                                                                              |
| ----------- | ---------------------------------------------------------------------------------- |
| AI          | Personality, Model, Planning, Auto-drive, Agents, Prompts                          |
| Tools       | REPL, MCP, Browser, Exec limits, Skills                                            |
| Environment | Shell, Shell profiles, Shell escalation, Network, Secrets                          |
| Session     | Memories, Review, Validation, Notifications, Experimental                          |
| Interface   | Hotkeys, Status line, Theme, Verbosity, Updates, Accounts, Plugins, Apps, Overview |

### Icons

Three tiers with automatic fallback. Set `tui.icon_mode` to `nerd_fonts`,
`unicode` (default), or `ascii`. Override individual icons if you want:

```toml
[tui.icons.gutter_exec]
ascii = "$"
unicode = "❯"
nerd_fonts = ""
```

### Animations

The intro screen has a glitch-reveal effect with character-by-character
rendering, shine bands, and fade transitions. There are 10+ reveal variants
(GlitchSweep, ChromaticScan, AuroraBridge, NeonRoad, etc.) plus a header
wave, shimmer effects, and a JSON-driven spinner registry.

### Status line

Top and bottom lanes, each configurable with items like model name, git
branch, context remaining, rate limits, REPL status, and more. Use
`/statusline` to configure.

---

## Dev notes

Rust sources live in `code-rs/` (~60 crates). `codex-rs/` is a read-only
mirror of upstream OpenAI Codex, kept around for reference.

### Build

```bash
./build-fast.sh                                       # full validation, required before push

cd code-rs && cargo check -p code-tui                 # quick incremental check
cd code-rs && cargo clippy --workspace --all-targets  # all warnings must be clean
cd code-rs && cargo nextest run --no-fail-fast         # tests
```

### Building without the network proxy

```bash
cargo build -p code-cli                              # proxy enabled (default)
cargo build -p code-cli --no-default-features         # proxy compiled out
```

## Compare with upstream

```bash
git diff every-code/main..main
git log --oneline every-code/main..main
```

## Validate

```bash
./build-fast.sh
```
