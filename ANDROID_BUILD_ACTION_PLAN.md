# Android / Termux — Action Plan

> Companion to `ANDROID_TERMUX_TUI_AUDIT.md`. Four waves of improvements, from
> quick safety wins to deeper architectural work. Each item lists file(s), the
> change, and why it matters.

---

# Wave 1 — Quick Wins (Safety & Core Usability)

Items that fix broken or dangerous behavior in ≤ 15 lines each.

## 1.1 Approval Widget — Guarantee Readable Command Text

**Effort:** ~10 lines · **Priority:** P0 (safety)

The approval prompt can get crushed to **1 row** when the terminal is short,
meaning the user can't read the command they're approving.

**File:** `code-rs/tui/src/user_approval_widget.rs:646-649`

```rust
// Current: prompt gets max(1) row after subtracting all option rows.
let max_prompt_height = area.height.saturating_sub(min_options_height).max(1);
```

**Change:** Guarantee ≥ 2 rows for the command prompt; shrink the options list
instead:

```rust
let max_prompt_height = area.height.saturating_sub(min_options_height).max(2);
```

Also add overflow indication ("…more options") when options get truncated.

---

## 1.2 Settings Overlay — Show "Too Small" Hint Instead of Blank

**Effort:** ~15 lines · **Priority:** P0

When `height < 4` or `width < 4`, the overlay returns early and shows nothing —
users think the UI is broken.

**File:** `code-rs/tui/src/chatwidget/settings_overlay/overlay_render/section.rs:193`

```rust
if area.width < 4 || area.height < 4 {
    return;  // silent blank
}
```

**Change:** Render a centered hint like `"↔ resize terminal"` before returning.

---

## 1.3 Header Touch Targets — Enlarge Click Rects

**Effort:** ~5 lines · **Priority:** P1

Header clickable regions are 1 terminal row tall (~3 mm on a phone). The minimum
mobile touch target is 7 mm (~48 dp).

**File:** `code-rs/tui/src/chatwidget/terminal_surface_header/click_regions.rs:22-27`

**Change:** Extend `height` to `area.height.min(3)` (or at least 2). The hit-test
already uses `Rect`, so enlarging costs nothing visually.

---

## 1.4 Bottom Pane Minimum — Reduce on Short Terminals

**Effort:** ~5 lines · **Priority:** P1

The bottom pane minimum is hardcoded at 5 rows — 42 % of a 12-row terminal.

**File:** `code-rs/tui/src/height_manager.rs:135-136`

**Change:** On terminals < 16 rows, use floor of 3 instead of 5:

```rust
let min_bottom = if area.height < 16 { 3 } else { 5 };
let bottom_cap = percent_cap.max(min_bottom);
let desired = bottom_desired_height.max(min_bottom).min(bottom_cap);
```

---

## 1.5 Fold/Collapse Gutter — Widen Touch Target

**Effort:** ~10 lines · **Priority:** P1

The fold icon is 1–2 chars wide — untappable on a phone.

**File:** `code-rs/tui/src/history_cell/cell_paint.rs` (fold gutter region)

**Change:** Extend the clickable region `width` to at least 3 characters.

---

## 1.6 Composer Padding — Drop Outer Padding on Narrow Screens

**Effort:** ~10 lines · **Priority:** P2

6 columns of chrome on a 30-col terminal leaves only 24 chars for typing.

**Files:** `code-rs/tui/src/layout_consts.rs:4`,
`code-rs/tui/src/bottom_pane/chat_composer/render.rs:37`

**Change:** Add `fn effective_composer_offset(width: u16) -> u16` that returns `4`
instead of `6` when `width < 40`, reclaiming 2 columns.

---

# Wave 2 — Input & Accessibility

Keyboard and slash-command gaps that block Termux users from reaching features.

## 2.1 Access Mode — Add Keyboard + Slash Command Alternative ⚠️

**Effort:** ~40 lines · **Priority:** P0 (security-critical)

Cycling Read Only → Approval → Full Access requires **Shift+Tab** — unreliable
on Android virtual keyboards. No alternative exists.

**Files:**
- `code-rs/tui/src/slash_command.rs` — add `AccessMode` variant
- `code-rs/tui/src/bottom_pane/chat_composer/input/editor/keys.rs:20` — add
  `Alt+A` as backup for `KeyCode::BackTab`
- `code-rs/tui/src/chatwidget/input_pipeline/slash.rs` — handle `/access`

**Change:** Add `/access [read-only|approval|full]` slash command and `Alt+A`
chord as Shift+Tab fallback.

---

## 2.2 Input History — Add Ctrl+P / Ctrl+N Fallback

**Effort:** ~15 lines · **Priority:** P1

Shift+Up/Down for history navigation fails on many Termux keyboards.

**File:** `code-rs/tui/src/bottom_pane/chat_composer/input/editor/keys.rs:76-99`

**Change:** Add `Ctrl+P` (previous) and `Ctrl+N` (next) as Unix-standard
alternatives alongside `Shift+Up` / `Shift+Down`.

---

## 2.3 Help Overlay — Add Keyboard Fallback for F1

**Effort:** ~10 lines · **Priority:** P1

F1 requires the Termux extra-keys bar.

**File:** `code-rs/tui/src/chatwidget/help_handlers.rs:10`

**Change:** Add `?` (when composer is empty and unfocused) or `Ctrl+/` as
alternative bindings for the help overlay.

---

## 2.4 Settings Focus — Add Alternative to Shift+Tab

**Effort:** ~10 lines · **Priority:** P1

Backward focus in settings (content → sidebar) only works with Shift+Tab.

**File:** `code-rs/tui/src/chatwidget/settings_handlers/keys.rs:32-35`

**Change:** Also accept `Esc` (when in content pane, not in sub-editor) as
"back to sidebar".

---

## 2.5 Help Text — Show Alternatives on Small Terminals

**Effort:** ~20 lines · **Priority:** P2

Help popup lists Shift+Tab, Shift+Enter, Shift+Up/Down with no mention of
alternatives or platform caveats.

**File:** `code-rs/tui/src/chatwidget/impl_chunks/popups_config_theme_access.rs:136-285`

**Change:** When `is_termux()` or terminal width < 50, show the alternative
bindings first (e.g., "Alt+A or Shift+Tab — cycle access mode").

---

## 2.6 Browser Overlay — Add Keyboard Screenshot Navigation

**Effort:** ~15 lines · **Priority:** P2

Screenshot gallery scrolling is mouse-scroll only.

**File:** `code-rs/tui/src/chatwidget/overlay_rendering/browser_overlay.rs`

**Change:** Handle `Up` / `Down` arrow keys to change `screenshot_index` in
addition to mouse scroll.

---

# Wave 3 — Layout Resilience

Overlays and panels that break or waste space on narrow/short terminals.

## 3.1 MCP Settings — Stacked Mode Needs Scroll or Accordion

**Effort:** ~50 lines · **Priority:** P1

Below 72 cols the MCP page switches to stacked mode needing 25 rows
(`list 9 + summary 8 + tools 8`). On a 15-row terminal only 1–2 rows show
per section.

**File:** `code-rs/tui/src/bottom_pane/settings_pages/mcp/layout.rs:70-98`

**Change:** Show one section at a time with Tab to rotate, or collapse
inactive sections to a 1-row header. Lower the 80-col hint-row threshold
(line 50) to 60 cols.

---

## 3.2 Image Cards — Text-Only Fallback Below 52 Cols

**Effort:** ~20 lines · **Priority:** P1

Image + text layout needs 51 cols minimum (`1 + 18 + 2 + 28 + 2`). On a 40-col
terminal the entire card vanishes (returns `None`).

**Files:** `code-rs/tui/src/history_cell/image.rs:41-43`,
`code-rs/tui/src/history_cell/browser/mod.rs:27`

**Change:** When `available_width < MIN_TEXT_WIDTH + IMAGE_MIN_WIDTH + gap`,
render a title-only fallback: `"📷 image.png (800×600)"` as a single-line
card row.

---

## 3.3 Agent/Browser Overlay — Full-Width on Narrow Screens

**Effort:** ~25 lines · **Priority:** P1

Agent terminal overlay reserves a 24-col sidebar even at 35 cols, leaving 5–10
cols for the terminal pane. Browser overlay forces a 20-col minimum content pane.

**Files:**
- `code-rs/tui/src/chatwidget/overlay_rendering/agents_terminal_overlay.rs:182`
- `code-rs/tui/src/chatwidget/overlay_rendering/browser_overlay.rs:188`

**Change:** Below 50 cols, switch to full-width single-pane mode — show the
list OR the detail, not both side-by-side.

---

## 3.4 MCP List Width — Clamp Proportionally

**Effort:** ~5 lines · **Priority:** P2

`(content.width / 3).max(30)` forces the MCP list to 30 cols, leaving only 10
for the detail pane on a 40-col terminal.

**File:** `code-rs/tui/src/bottom_pane/settings_pages/mcp/layout.rs:183`

**Change:** `(content.width / 3).clamp(15, 30)` — or switch to stacked below
60 cols (see §3.1).

---

## 3.5 Policy Editor — Lower Label Width Floor

**Effort:** ~5 lines · **Priority:** P2

`label_width.clamp(12, 30)` plus `min_value_width = 14` requires 26+ cols of
inner space.

**File:** `code-rs/tui/src/bottom_pane/settings_pages/mcp/policy_editor/render.rs:317`

**Change:** Use `.clamp(8, 24)` and reduce `min_value_width` to 10 on narrow
screens.

---

## 3.6 Overlay Stack — Lower Width Floor

**Effort:** ~5 lines · **Priority:** P2

`(body_inner.width - 10).max(20)` forces overlays to 20-col minimum, causing
margin squeeze on 35-col terminals.

**File:** `code-rs/tui/src/chatwidget/overlay_rendering/widget_render/overlay_stack.rs:308`

**Change:** Lower to `.max(14)` or remove the floor entirely — the overlay
already clips content that doesn't fit.

---

# Wave 4 — Unicode Width Correctness

Fix `.chars().count()` → `.width()` for display-column calculations.

## Triage Rule

- **Layout/rendering** (padding, column widths, centering) → must use `.width()`
- **Content limits** (max chars for descriptions, API payloads) → `.chars().count()` is fine
- **Secret masking** (`"*".repeat(value.chars().count())`) → fine

## Already Fixed ✅

- `markdown_renderer/tables.rs` — uses `display_width_approx()` now

## 4.1 Header Width Calculations

**Effort:** ~15 lines · **Priority:** P1

Header template and layout use `.chars().count()` for width accumulation. Any
CJK or emoji in model names, directory paths, or shell labels will misalign.

**Files:**
- `chatwidget/terminal_surface_header/template.rs` — lines 41, 149, 163, 178
- `chatwidget/terminal_surface_header/layout.rs` — line 31
- `chatwidget/terminal_surface_render.rs` — lines 524, 540

**Change:** Import `UnicodeWidthStr` and replace `.chars().count()` with
`.width()` at each site.

---

## 4.2 History Cell Width Calculations

**Effort:** ~15 lines · **Priority:** P1

Bullet indentation, explore entry padding, reasoning span length, and
auto-drive art all measure characters instead of display columns.

**Files:**
- `history_cell/assistant.rs:724` — bullet indent depth
- `history_cell/explore.rs:350,415` — explore entry label padding
- `history_cell/reasoning.rs:715` — reasoning span width
- `history_cell/auto_drive.rs:777-787` — `occupied_range()` char index
- `history_cell/auto_drive.rs:1014` — celebration art `chars.len()`

---

## 4.3 Component / Bottom Pane Width Calculations

**Effort:** ~10 lines · **Priority:** P2

Selection list items, auto-coordinator headers, and prompt titles use character
counts for layout.

**Files:**
- `components/list_selection_view.rs:55`
- `bottom_pane/panes/auto_coordinator/render/prompt.rs:118`
- `bottom_pane/panes/auto_coordinator/render/header.rs:40,228`

---

# Wave 5 — Rendering Performance (Low-Power Devices)

Allocation-heavy render paths that hurt frame rate on slow hardware.

## 5.1 Markdown Stream — Eliminate Buffer Clone Per Frame

**Effort:** ~20 lines · **Priority:** P1

`render_preview_lines()` clones the entire markdown buffer string on every
frame during streaming responses.

**File:** `code-rs/tui/src/markdown_stream.rs:377`

**Change:** Take `&str` or `Cow` instead of cloning; pass references through
the rendering pipeline.

---

## 5.2 Theme Split Preview — Reuse Snapshot Buffer

**Effort:** ~15 lines · **Priority:** P2

Theme preview allocates and clones full buffer halves every frame via
`snapshot_left_half()`.

**File:** `code-rs/tui/src/app/render.rs:148,199-228`

**Change:** Keep a pre-allocated `Vec<Cell>` in the render state; resize and
reuse it instead of allocating fresh each frame.

---

## 5.3 Inline Text Processing — Avoid `Vec<char>` Intermediate

**Effort:** ~30 lines · **Priority:** P2

`process_inline_spans()` collects the entire text into a `Vec<char>`, then
creates substrings by re-collecting char slices.

**File:** `code-rs/tui/src/markdown_renderer/preamble.rs:368,403`

**Change:** Use `char_indices()` and `&str` slicing instead of `Vec<char>` +
`.iter().collect()`.

---

## 5.4 Glitch Animation — Cache Frames

**Effort:** ~25 lines · **Priority:** P2

The intro animation allocates multiple `Vec<String>` per frame with repeated
`.to_vec()`, `String::new()`, and `" ".repeat()`.

**File:** `code-rs/tui/src/glitch_animation.rs:116-130,252-274,347-390`

**Change:** Compute animation frames once at start, store in a `Vec<Vec<Line>>`,
index by frame number on each tick.

---

## 5.5 Diff Render — Avoid Repeated Path Clones

**Effort:** ~10 lines · **Priority:** P2

Every diff line clones file paths into `RtSpan::styled(path.clone(), ...)`.

**File:** `code-rs/tui/src/diff_render.rs:244-261`

**Change:** Use `Cow<str>` or borrow from the diff struct instead of cloning.

---

# Wave 6 — Resilience & Recovery

Edge cases that are common on Android but rare on desktop.

## 6.1 SIGHUP / SIGTERM Handlers

**Effort:** ~30 lines · **Priority:** P1

No handler for SIGHUP (terminal disconnect) or SIGTERM (process kill). The TUI
crashes ungracefully on Termux OOM or network drop.

**File:** `code-rs/tui/src/app/terminal/screen.rs:18-35`

**Change:** Add `signal_hook` handlers for SIGHUP, SIGTERM, SIGPIPE. On
SIGHUP/SIGTERM: disable raw mode, restore terminal, exit cleanly. On SIGPIPE:
log and ignore.

---

## 6.2 Color Palette — Detect Termux and Force Conservative

**Effort:** ~15 lines · **Priority:** P2

`supports_color` crate may over-detect truecolor on Termux (COLORTERM is
sometimes misreported).

**File:** `code-rs/tui/src/theme.rs:360-366`

**Change:** When `is_termux()`, force `PaletteMode::Ansi256` regardless of
`COLORTERM`. Add `CODEX_FORCE_ANSI16=1` env override.

---

## 6.3 Termux Auto-Detection Helper

**Effort:** ~15 lines · **Priority:** P2

Central helper used by waves 1–6 to adapt behavior.

**File:** `code-rs/tui/src/platform_caps.rs` (already has `supports_clipboard_image_paste`)

**Change:** Add `pub(crate) fn is_termux() -> bool` checking
`std::env::var("PREFIX")` for `/data/data/com.termux`. Cache in a `OnceLock`.

Use it to:
- Reduce bottom pane minimum (Wave 1)
- Show alternative hotkeys in help (Wave 2)
- Force conservative color palette (Wave 6)
- Default to ASCII mode

---

# Implementation Order

| Step | Wave | Items | Theme |
| ---- | ---- | ----- | ----- |
| 1 | 1 | §1.1–1.2 | Safety — can't approve or configure |
| 2 | 2 | §2.1 | Security — access mode unreachable |
| 3 | 1 | §1.3–1.5 | Touch and height — core phone usability |
| 4 | 2 | §2.2–2.4 | Keyboard — reach all features |
| 5 | 4 | §4.1–4.2 | Unicode width — correctness for i18n |
| 6 | 3 | §3.1–3.3 | Layout — overlays usable on narrow screens |
| 7 | 1+2 | §1.6, §2.5–2.6 | Polish — ergonomic wins |
| 8 | 4 | §4.3 | Unicode width — remaining sites |
| 9 | 5 | §5.1, §5.3 | Perf — highest-impact allocation fixes |
| 10 | 6 | §6.1, §6.3 | Resilience — graceful signal handling |
| 11 | 3 | §3.4–3.6 | Layout polish — secondary overlays |
| 12 | 5 | §5.2, §5.4–5.5 | Perf — remaining allocation wins |
| 13 | 6 | §6.2 | Resilience — color palette |
| 14 | 2 | §2.5 | Polish — platform-aware help text |

---

*Companion to `ANDROID_TERMUX_TUI_AUDIT.md` — April 2026*
