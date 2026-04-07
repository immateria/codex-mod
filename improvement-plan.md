# code-rs DRY & Quality Improvement Plan

## Completed (prior session)
- KeyHint helpers, SETTINGS_LIST_MOUSE_CONFIG, clone elimination, AtomicBool safety fix
- Upstream merge (just-every/code main -> codex-mod)

## How to use this plan
- Treat each round as a standalone, bisectable change set (ideally 1-3 commits).
- Keep refactors mechanical first, then tighten behavior with tests once the surface is smaller.
- Run `./build-fast.sh` at the end of each round; warnings are failures.
- Avoid churn: keep public APIs stable unless the round explicitly targets API cleanup.

Notes on dependencies:
- Round 4 is easiest once Round 1 exists (the macro can generate the wrappers).
- Round 16 benefits from Round 8 (one width implementation makes renderer work simpler).

## Round 1 - pane_impl macro (HIGH - ~2000 LOC saved)
Goal: replace the near-identical settings-page `pane_impl.rs` boilerplate with a single
macro invocation per page.

Where:
- `code-rs/tui/src/bottom_pane/settings_pages/*/pane_impl.rs`
- Each file typically implements:
  - `ChromeRenderable` (framed vs content-only forwarding)
  - `ChromeMouseHandler`
  - `BottomPaneView`
- Quick inventory:
  - `rg -n \"^impl BottomPaneView for\" code-rs/tui/src/bottom_pane/settings_pages -S`
  - `rg -n \"pane_impl\\.rs\" code-rs/tui/src/bottom_pane/settings_pages -S`

How:
1. Add `impl_settings_pane!` in a shared module, e.g.:
   - `code-rs/tui/src/bottom_pane/settings_pages/pane_impl_macro.rs`
2. Macro inputs should be boring and explicit:
   - view type name (`ShellEscalationSettingsView`)
   - title string or `fn title(&self) -> String`
   - the page's `render_in_chrome(...)` entrypoint (most pages already have one)
   - key/mouse handlers (`handle_key_event`, `handle_mouse_event`)
3. Macro output should generate the three trait impl blocks with consistent wiring.
4. Migrate pages mechanically:
   - Delete content-heavy `pane_impl.rs` files.
   - Replace with a tiny `pane_impl.rs` that only calls the macro, or move the macro
     invocation into the page `mod.rs`.

Acceptance:
- Only "weird" pages keep hand-written `pane_impl.rs` (ideally none).
- `git diff` should show mostly mechanical deletions, not logic changes.

## Round 2 - Generic view factory (HIGH - ~160 LOC saved)
Goal: collapse the `show_settings_*` forwarding methods into one generic method.

Where:
- `code-rs/tui/src/bottom_pane/views.rs` (look for `show_settings_*` functions)

How:
1. Add:
   - `pub fn show_settings<V: BottomPaneView + 'static>(&mut self, view: V)`
2. Internally, convert to whatever the pane stack expects today (usually a boxed trait
   object): `self.push_view(Box::new(view))`.
3. Replace call sites:
   - Most call sites can switch from `show_settings_network(...)` to
     `show_settings(NetworkSettingsView::new(...))`.
4. Optionally keep the old names as thin wrappers for a short deprecation window, but
   don't keep 30+ copies forever.

Acceptance:
- `views.rs` no longer has a wall of `show_settings_*` wrappers.

## Round 3 - Scroll handling trait (HIGH - ~600 LOC saved)
Goal: make "menu scrolling + selection movement" a shared trait instead of duplicated
key/mouse logic in each settings page.

Where:
- Most settings pages have similar `Up/Down/PageUp/PageDown/Home/End` handling in their
  `input.rs` (or `mod.rs`) and store scroll state like:
  - `code-rs/tui/src/components/scroll_state.rs`
  - Examples: `code-rs/tui/src/bottom_pane/settings_pages/exec_limits/*`,
    `code-rs/tui/src/bottom_pane/settings_pages/secrets/mod.rs`,
    `code-rs/tui/src/bottom_pane/settings_pages/shell_profiles/model.rs`

How:
1. Define `ScrollableMenu` with the minimum hooks needed:
   - `fn scroll_state(&self) -> &ScrollState` / `fn scroll_state_mut(&mut self) -> &mut ScrollState`
   - `fn item_count(&self) -> usize`
   - `fn selected_index(&self) -> usize` / `fn set_selected_index(&mut self, idx: usize)`
   - `fn viewport_rows(&self) -> usize` (some pages track this via `Cell<usize>`)
2. Provide default implementations:
   - `fn handle_scroll_keys(&mut self, key: KeyEvent) -> bool`
   - `fn handle_mouse_wheel(&mut self, delta: i16) -> bool`
3. Migrate pages incrementally:
   - Start with a "simple list" settings page (Experimental, Secrets).
   - Move the shared behavior first, then delete local duplicates.

Acceptance:
- Selection/scroll behavior stays identical (same keybindings, same wrap/clamp rules).
- Pages stop re-implementing the same `Up/Down` math.

## Round 4 - Dual-render wrapper consolidation (MED - ~210 LOC saved)
Goal: remove repeated `render_framed` / `render_content_only` wrappers.

Where:
- Many settings pages implement:
  - `render_framed(frame, area, ...)`
  - `render_content_only(frame, area, ...)`
  that both forward into `render_in_chrome(ChromeMode::{Framed,ContentOnly}, ...)`.

How:
- Prefer folding this into Round 1's macro so each page only provides
  `render_in_chrome(...)`.
- If a page truly needs special logic, keep it local and document why in a short
  comment.

Acceptance:
- Fewer per-page render entrypoints; more consistent chrome behavior.

## Round 5 - JSON file I/O helpers (MED - ~100 LOC saved)
Goal: stop re-copying "read JSON / write JSON with secure perms / atomic replace" code.

Where:
- Common patterns show up in:
  - `code-rs/core/src/auth.rs`
  - `code-rs/core/src/auth_accounts.rs`
  - `code-rs/core/src/auth/storage.rs`
  - `code-rs/core/src/account_usage.rs` (and similar small state files)
- Look for repeated sequences like:
  - `tokio::fs::read_to_string(...)` + `serde_json::from_str`
  - create parent dir + write + flush + `chmod 0o600`

How:
1. Add `code_core::util::json_file` (or `code-rs/core/src/util/json_file.rs`) with:
   - `read_json_file<T: DeserializeOwned>(path) -> Result<Option<T>>`
   - `write_json_file_secure<T: Serialize>(path, value) -> Result<()>`
2. Make writes safe by default:
   - write temp file next to target
   - flush (`sync_all`) then atomic rename
   - best-effort `0o600` on unix (don't break Windows)
3. Convert call sites one by one; keep error messages stable (tests/snapshots).

Acceptance:
- No new behavior differences besides fewer code paths.
- New helper has unit tests for:
  - missing file -> `Ok(None)`
  - invalid JSON -> useful error
  - secure write sets perms on unix
  - atomic replace doesn't leave partial files behind

## Round 6 - header_map_to_json dedup (LOW - ~20 LOC saved)
Goal: remove the tiny but annoying duplication.

Where:
- `code-rs/core/src/client.rs` (has a local `fn header_map_to_json(...)`)
- `code-rs/core/src/chat_completions.rs` (same helper)

How:
- Move to a shared util module, e.g. `code-rs/core/src/util/http.rs`, and re-export it.
- Keep signatures identical to avoid churn.

Acceptance:
- Only one implementation exists in core.

## Round 7 - Cleanup pass
Goal: keep the workspace warning-free as the surface grows.

Where / how:
- Duplicate `#[test]` attributes:
  - `rg -n \"#\\[test\\]\\s*\\n\\s*#\\[test\\]\" code-rs -S`
- Unused vars/consts:
  - Fix at the call site first (`_foo`), then consider deleting dead code.
- `#[allow(dead_code)]`:
  - `rg -n \"allow\\(dead_code\\)\" code-rs -S`
  - Either delete the code, add a narrow `cfg(test)` use, or document why it's kept.
- Public surface audit:
  - tighten `pub` to `pub(crate)` when it's not used outside the crate.

Acceptance:
- `./build-fast.sh` stays clean after each round (warnings treated as failures).

## Round 8 - Display width + wrapping consolidation (HIGH - correctness + DRY)
Right now we have multiple "how wide is this text in the terminal?" implementations
spread across wrapping, markdown rendering (especially tables), and history rendering.
This shows up as misaligned tables, truncation, and inconsistent wrapping across panes.

Where:
- Width math is already scattered across TUI:
  - `code-rs/tui/src/live_wrap.rs`
  - `code-rs/tui/src/text_formatting.rs`
  - `code-rs/tui/src/syntax_highlight.rs`
  - `code-rs/tui/src/history_cell/*`
  - `code-rs/tui/src/markdown_renderer/tables.rs` (tables are the most visible bug)

How:
1. Create `code-rs/tui/src/text_width.rs`:
   - `display_width(&str) -> usize` (fast ASCII path)
   - `display_width_graphemes(&[&str]) -> usize` (for pre-split content)
   - Optional: `strip_ansi_and_width(&str)` for styled strings, when needed
2. Decide the baseline algorithm:
   - Use `unicode-segmentation` graphemes + `unicode_width` as the starting point.
   - Keep the behavior deterministic (don't depend on terminal quirks).
3. Migrate the "worst offenders" first:
   - markdown tables (`tables.rs`)
   - truncation helpers that currently cut mid-grapheme
4. Add one VT100-based regression test that renders a table containing:
   - CJK, combining marks, emoji ZWJ sequences, and ANSI-styled spans
   - Assert: the `|` column boundaries line up across rows.

Acceptance:
- Table alignment improves on real terminals (macOS Terminal + iTerm2 + Termux).
- No more mid-grapheme truncation in table cells unless a single grapheme is wider
  than the whole column.

## Round 9 - Platform caps as a first-class module (MED - fewer dead UI paths)
We keep re-learning which desktop affordances are not available on Android/Termux
(pickers, reveal-in-file-manager, clipboard image paste, browser open, etc).

Where:
- Platform capability checks should live in:
  - `code-rs/tui/src/platform_caps.rs`
- Existing call sites to consolidate:
  - header mouse actions: `code-rs/tui/src/chatwidget/input_pipeline/mouse/header.rs`
  - picker usage: settings pages that advertise `p` (path pickers)
  - clipboard paste routing: `code-rs/tui/src/app/events/priority.rs`

How:
1. Add/extend helpers in `platform_caps.rs`:
   - `supports_native_picker()`
   - `supports_reveal_in_file_manager()`
   - `supports_clipboard_image_paste()`
   - `supports_open_url()` (or: always true but android uses `termux-open-url`)
2. Add a test override env var (mirrors other caps patterns) so this can be exercised on
   desktop without cross-compiling.
3. Remove dead affordances:
   - hide rows/hints when unsupported (preferred)
   - or disable + show a short "Not supported on Android" footer notice

Acceptance:
- Android/Termux builds don't advertise dead actions.
- Desktop builds keep current UX.

## Round 10 - AppEvent async-task helpers (MED - fewer one-off patterns)
We have a growing set of "spawn task -> send Loaded event -> update shared state -> redraw"
handlers. The mechanics are repetitive and it's easy to get inconsistent stale-response
handling.

Where:
- Most async event handlers live under:
  - `code-rs/tui/src/app/events/run/*`
  - Settings/tools in particular: `code-rs/tui/src/app/events/run/tools_and_ui_settings.rs`

How:
1. Introduce a small internal helper, e.g. `code-rs/tui/src/app/events/task_helpers.rs`:
   - `spawn_and_send(sender, async move { ... }, |result| AppEvent::...Loaded { result })`
   - standardize `Err(anyhow)` -> `String`
2. Standardize stale-response handling:
   - Plugins already track roots/cwd to ignore stale loads; make the pattern easy to
     reuse so every new flow doesn't invent its own variant.
3. Migrate a couple flows first:
   - Plugins list/detail
   - Apps directory/status

Acceptance:
- New async flows are 5-10 lines of glue instead of 30-40 lines of boilerplate.
- Stale responses are consistently ignored (no UI flicker).

## Round 11 - TOML edit helpers (MED - safer config persistence)
Our `config_edit` functions share a lot of structure:
"load TOML -> ensure tables -> update keys -> preserve unrelated tables -> write if changed".

Where:
- `code-rs/core/src/config_edit.rs` contains a bunch of "TOML edit" functions that all
  do the same dance with `toml_edit`.
- The most complex call sites today:
  - plugin sources, apps sources, feature flags (profile-scoped)

How:
1. Add a small internal helper struct (private) inside `config_edit`:
   - loads `<CODE_HOME>/config.toml`
   - provides:
     - `ensure_table(path: &[&str]) -> &mut Table`
     - `remove_key(path: &[&str])`
    - `set_bool(path: &[&str], value: bool)`
    - `set_string_opt(path: &[&str], Option<&str>)`
     - "array-of-tables" helpers for `[[plugins.marketplace_repos]]`-style edits
2. Keep the high-level public APIs unchanged; only refactor internals.
3. Add/keep unit tests that ensure we preserve unrelated tables/subtables.

Acceptance:
- Config edits become harder to break (fewer hand-rolled path manipulations).
- Tests cover "preserve plugin subtables" and "profile scoping" invariants.

## Round 12 - Small-build regression guard (LOW - prevents accidental bloat)
We've put real effort into compile-time gating (`--no-default-features`) but it's easy
to regress by accidentally enabling a default feature or pulling in a heavy dependency.

Where:
- We already rely on build scripts like `build.zsh`; this should live either as:
  - a documented manual check, or
  - a tiny helper script in `scripts/` that developers can run before pushing.

How:
1. Add `scripts/check-small-build.sh` (or document the commands in one place):
   - `cargo check -p code-cli --no-default-features`
   - `cargo tree -p code-cli --no-default-features | rg '(aws-lc-sys|chromiumoxide)'`
2. (Optional) If we add CI for this fork, make this a dedicated job so accidental
   dependency regressions get caught early.

Acceptance:
- A single, repeatable recipe proves the "small build" stays small over time.

## Round 13 - Config deserialization resilience (HIGH - fewer startup hard-fails)
Goal: avoid "one bad config value makes the app not start" for non-critical settings.
If a value is invalid, fall back to defaults and emit a warning that tells the user
what key was ignored and why.

Where:
- Config load/validation:
  - `code-rs/core/src/config.rs`
  - `code-rs/core/src/config_types.rs`
- The most obvious pain points we have already hit:
  - theme config (`tui.theme`) accepting only a struct when users often write a string
  - path-like fields on Android (Termux) where defaults differ

How:
1. Identify which config keys should be "soft" vs "hard":
   - soft: UI/theme, layout, hotkeys, statusline templates
   - hard: auth storage mode, code_home/cwd invariants, security toggles that would
     violate policy if misread
2. For soft keys, use one of these patterns:
   - `#[serde(default)]` + `deserialize_with` that catches type mismatches and returns
     default while recording a warning
   - `#[serde(untagged)]` to accept both the "new" structured form and an older/simple
     string form (e.g. `tui.theme = \"dracula\"`)
3. Thread warnings into the existing "warnings at startup" surface:
   - log via `tracing::warn!`
   - also emit a single `EventMsg::Warning` during session configure so the TUI shows
     it in history
4. Add targeted unit tests that parse TOML snippets and assert:
   - invalid values do not error the whole config load
   - a warning is produced (string contains the key name)

Acceptance:
- The app starts with a bad theme value and falls back to default.
- Warnings are visible once (not spammed every turn).

## Round 14 - Settings routing + location preference tests (MED - prevents regressions)
Goal: lock down the "where does settings open" behavior (overlay vs bottom pane),
because it is easy to regress when adding new sections.

Where:
- Routing logic:
  - `code-rs/tui/src/chatwidget/settings_routing.rs`
  - `code-rs/tui/src/chatwidget/settings_routing/builders.rs`
  - `code-rs/tui/src/bottom_pane/settings_overlay/*`
- Settings location preference (auto / always overlay / always bottom pane) is usually
  stored under the TUI config.

How:
1. Add unit tests using `ChatWidgetHarness` that:
   - set settings location preference to each mode
   - open settings via:
     - `/settings` (big overlay)
     - "open section in bottom pane" action
     - direct `AppEvent::OpenSettings { section: ... }`
   - assert which surface is active after the action
2. Add a couple "width threshold" tests for `auto`:
   - narrow width -> bottom pane
   - wide width -> overlay
3. Keep the tests focused on behavior, not exact rendering (no snapshots required).

Acceptance:
- A future section addition can't accidentally ignore the user's preference without
  breaking a test.

## Round 15 - Mouse hit testing for menu rows (MED - less accidental focus)
Goal: menu rows should only highlight/select when the pointer is over the actual text
or an explicit button region, not the entire row's empty padding.

Where:
- Settings menu rendering and mouse routing:
  - `code-rs/tui/src/bottom_pane/settings_menu/*`
  - settings pages that use `SettingsMenuPage` + `SettingsMenuRow`
  - mouse router helpers in `code-rs/tui/src/chatwidget/input_pipeline/mouse/*`

How:
1. Extend `SettingsMenuRow` to record a "click target rect" for the label text:
   - compute it at render time (based on label start x + label width)
   - store it alongside the existing row rect
2. Update mouse hover/activation logic to use the click target rect:
   - hover only when inside the label rect (or inside a button rect)
   - clicks outside do nothing (no focus change)
3. Add one unit test for a representative menu page:
   - hover on padding: no focused row change
   - hover on label: focused row changes

Acceptance:
- The UI feels less "twitchy" and accidental clicks don't shift selection.

## Round 16 - Markdown rendering consolidation (MED - fewer parallel implementations)
Goal: reduce the number of Markdown render entrypoints so fixes (wrapping/width, tables,
performance) land in one place.

Where:
- Current split:
  - `code-rs/tui/src/markdown_renderer/*`
  - `code-rs/tui/src/markdown_render.rs`
  - `code-rs/tui/src/markdown.rs` (if still present)
- Call sites that render assistant markdown into TUI lines.

How:
1. Pick one module tree as "the renderer" (prefer the custom renderer we are keeping).
2. Move shared helpers (wrapping, first-sentence extraction, blockquotes, tables) under
   that module, and delete the dead path.
3. Add a small regression suite for common markdown constructs:
   - headings, lists, blockquotes
   - code blocks (syntax highlight on/off)
   - tables (with unicode width edge cases)
4. Keep perf in mind:
   - avoid allocations in the hot path (rendering large streaming outputs)

Acceptance:
- There is exactly one place to fix markdown wrapping and table width bugs.

## Round 17 - Warn-once + notice plumbing (MED - less spam, clearer UX)
Goal: stop repeating the same warning every turn/tool call. Emit it once with enough
context to act on, then stay quiet.

Where:
- Core warnings emitted during configure-session:
  - `code-rs/core/src/codex/streaming/submission/configure_session/*`
- TUI warning surfaces:
  - history warning cells (driven by `EventMsg::Warning`)
  - footer notices and background notices (reconnecting, etc.)

How:
1. Add a small "warn once" helper in core (or session state):
   - keyed by a stable string, e.g. `\"shell-zsh-fork-not-ready\"`
   - stores a `HashSet<String>` on the session/configure context
2. Use it for known spammy warnings:
   - feature enabled but compiled out (Android gating)
   - shell escalation enabled but missing prerequisites
   - managed network mediation enabled but not supported in this build
3. Keep the message actionable:
   - include the exact config key and what value was ignored
   - include the single next step ("set zsh_path", "build with --features ...", etc.)

Acceptance:
- Warnings show up once per session configure, not once per attempted use.

## Round 18 - Shared async-state types for settings pages (MED - fewer bespoke enums)
Goal: avoid re-implementing the same `Uninitialized/Loading/Ready/Failed` enums with
slightly different semantics across Plugins, Apps, and future pages.

Where:
- Existing patterns:
  - `code-rs/tui/src/chatwidget/plugins_shared_state.rs`
  - `code-rs/tui/src/chatwidget/apps_shared_state.rs`
  - other stateful pages (skills/tools overlays, validation, etc.)

How:
1. Add a generic helper in TUI, e.g. `code-rs/tui/src/async_state.rs`:
   - `enum AsyncState<T> { Uninitialized, Loading, Ready(T), Failed(String) }`
   - Optional: `AsyncStateWithKey<K, T>` for stale-response suppression
2. Replace per-feature enums where it improves clarity:
   - keep domain-specific fields inside the `T` payload
3. Add a couple small helpers:
   - `is_loading()`, `error()`, `as_ready()`

Acceptance:
- Fewer bespoke enums and fewer "almost the same" match arms in settings pages.

## Round 19 - TUI test helpers for mouse/clickable regions (LOW - faster test writing)
Goal: reduce the amount of boilerplate in TUI tests that need to click things.

Where:
- Clickable actions/regions:
  - `code-rs/tui/src/chatwidget/shared_defs/preamble.rs`
  - `code-rs/tui/src/chatwidget/input_pipeline/mouse/*`
- Tests that currently re-implement "render once, then scan clickable regions":
  - `code-rs/tui/src/chatwidget/tests/*`
  - `code-rs/tui/tests/vt100_chatwidget_snapshot.rs`

How:
1. Add helpers on `ChatWidgetHarness`:
   - `render_once(width, height)` (already exists in some places; unify)
   - `find_clickable_rect(|action| ...) -> Rect`
   - `click_action(|action| ...)` that:
     - renders once
     - finds the region
     - clicks the region center
2. Keep it deterministic by always using the same backend (`VT100Backend`) and area.

Acceptance:
- New UI tests can click a button in ~3 lines instead of ~30 lines.
