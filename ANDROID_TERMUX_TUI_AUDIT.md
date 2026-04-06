# Android / Termux TUI Audit

> Audit of the code-rs TUI for narrow-width, short-height, and limited-capability
> terminals typical of **Termux on Android** (portrait mode, on-screen keyboard).
>
> Typical Termux dimensions: **30–40 columns × 12–20 rows** (keyboard visible)

---

## Table of Contents

1. [Layout: Narrow Width Issues](#1-layout-narrow-width-issues)
2. [Layout: Short Height Issues](#2-layout-short-height-issues)
3. [Unicode & Rendering Issues](#3-unicode--rendering-issues)
4. [Input & Interaction Issues](#4-input--interaction-issues)
5. [Recommended Termux Configuration](#5-recommended-termux-configuration)
6. [Fix Priority Matrix](#6-fix-priority-matrix)

---

## 1. Layout: Narrow Width Issues

These issues affect terminals with **20–50 columns** (Android portrait mode).

### Critical

| Issue | File | Details |
|-------|------|---------|
| **Settings sidebar fixed at 22 cols** | `settings_overlay/overlay_render/section.rs:27` | At 30 cols, sidebar consumes 73% of width; content pane gets ~8 cols. Settings are unusable below ~45 cols. |
| **Settings label column 18 cols** | `settings_overlay/types.rs:9` | After the sidebar, labels take 18 more cols. No room for values below ~60 cols total. |
| **Image cards need 52+ cols** | `history_cell/image.rs:33-45` | `IMAGE_MIN_WIDTH(18) + GAP(2) + MIN_TEXT_WIDTH(28) + padding = 52`. Broken layout below 52 cols. |
| **Skills label column 24 cols** | `settings_pages/skills/render.rs:8` | `Constraint::Length(24)` leaves only 6 cols for content at 30 cols. |
| **Overlay rejects < 40 cols** | `settings_pages/interface/hotkeys.rs:7` | `OVERLAY_MIN_WIDTH_MIN = 40`. Overlay settings unavailable on very narrow phones. |
| **Agents sidebar 30-col breakpoint** | `overlay_rendering/agents_terminal_overlay.rs:182` | At ≤ 30 cols, sidebar fills entire width; detail pane is 0 cols. |
| **Composer chrome overhead 6 cols** | `layout_consts.rs:12` | `COMPOSER_CONTENT_WIDTH_OFFSET = 6` (borders + padding). At 20 cols → only 14 for text. |

### Moderate

| Issue | File | Details |
|-------|------|---------|
| Limits overlay needs 110 cols | `settings_overlay/limits/content_impl.rs:2` | `WIDE_LAYOUT_MIN_WIDTH = 110`. Forces single-column on all phones. |
| Accounts two-pane needs 96 cols | `settings_pages/accounts/.../mod.rs:17` | Falls back to single-column below 96 cols. |
| Browser sidebar fixed 24 cols | `overlay_rendering/browser_overlay.rs:188` | Screenshot area only 16 cols at 40-col terminal. |
| Intro animation needs 50+ cols | `glitch_animation.rs:13` | `SMALL_MIN_WIDTH = 50`. Falls back to "Tiny" or nothing below this. |

### Recommendations

- Make settings sidebar width proportional: `min(22, area.width * 40%)`.
- Add a **stacked/single-column mode** for settings on narrow screens (sidebar above content instead of beside it).
- Reduce `COMPOSER_CONTENT_WIDTH_OFFSET` by 2 on terminals < 40 cols (drop outer horizontal padding).
- Image cards: fall back to text-only display below 40 cols.
- All `Constraint::Length(N)` with N > 15 should have a `min(N, area.width - margin)` guard.

---

## 2. Layout: Short Height Issues

These issues affect terminals with **10–20 rows** (on-screen keyboard visible).

### Critical

| Issue | File | Details |
|-------|------|---------|
| **MCP settings needs 25 rows** | `settings_pages/mcp/layout.rs:96-98` | `list(9) + summary(8) + tools(8) = 25` rows minimum in stacked mode. Completely unusable on 12-row terminals. |
| **Approval prompt crushed to 1 row** | `user_approval_widget.rs:646-649` | Options consume most of a 5-8 row bottom pane; command text is unreadable. **Safety risk:** user can't read what they're approving. |
| **Settings overlays fail silently** | `settings_overlay/overlay_render/section.rs:193` | Returns early if `height < 4`. No message to user. |
| **Sectioned panel returns None** | `settings_ui/sectioned_panel.rs:94` | If `content.height < header + 1 + footer`, panels don't render at all. |
| **Accounts settings need 10+ rows** | `settings_pages/accounts/.../mod.rs:18` | `ACCOUNTS_TWO_PANE_MIN_HEIGHT = 10` plus chrome = 12+ rows total. |

### Moderate

| Issue | File | Details |
|-------|------|---------|
| Bottom pane minimum 5 rows | `height_manager.rs:136` | `desired.max(5)`. On 12-row terminal, this is 42% of screen. |
| HUD needs 4 rows minimum | `height_manager.rs:196` | Disappears entirely when vertical budget < 4. |
| History squeezed to 3 rows | `height_manager.rs:225` | `min_history = 3` when bottom ≤ 6 rows. Barely usable. |
| Scrollbar needs 3 rows | `ui_interaction/scrollbar.rs:9` | Disabled below 3 rows — user loses scroll position feedback. |
| Auto-coordinator needs 3 rows | `panes/auto_coordinator/render/prompt.rs:99` | Renders nothing below 3 rows. |
| Intro animation needs 19 rows | `glitch_animation.rs:16` | `SMALL_MIN_HEIGHT = 19`. Never renders with keyboard visible. |

### Recommendations

- Reduce bottom pane minimum to 3 rows on terminals < 16 rows.
- MCP settings: collapse to single scrollable list below 15 rows.
- Approval widget: guarantee at least 2 rows for command text (truncate options instead).
- Show "terminal too small" message instead of silent failure.
- Consider auto-hiding the status bar on very short terminals to reclaim 1 row.

---

## 3. Unicode & Rendering Issues

### Width Calculation Bugs

These use **character count** instead of **display width** and will break with CJK, emoji, or wide Unicode:

| Issue | File | Line(s) | Impact |
|-------|------|---------|--------|
| **Table column widths** | `markdown_renderer/tables.rs` | 118, 122, 138 | Misaligned columns with CJK/emoji content |
| **Celebration art width** | `history_cell/auto_drive.rs` | 1014-1019 | `chars.len()` instead of display width; misalignment |
| **Sparkle occupied range** | `history_cell/auto_drive.rs` | 777-787 | Character index ≠ display position; corrupts double-width chars |
| **Bullet indent** | `history_cell/assistant.rs` | 724 | `chars().count()` for indent; wrong with wide bullets |
| Various layout helpers | `formatting.rs`, `explore.rs`, `reasoning.rs` | multiple | `.chars().count()` used for layout calculations |

### Box-Drawing Characters (83+ instances)

All card borders use Unicode box-drawing (`╭╮╰╯│─`) with **no ASCII fallback** in production:

- `history_cell/auto_drive.rs:32-34` — card borders
- `history_cell/agent/preamble.rs:27-29`
- `history_cell/image.rs:33-35`
- `history_cell/browser/mod.rs:11-13`
- `history_cell/web_search.rs:28-30`
- `auto_drive_style.rs:114-153` — `ButtonGlyphs` (only `light()` uses ASCII)
- `bottom_pane/panes/auto_coordinator/render/prompt.rs:119-126`

Termux renders these correctly with a good font (e.g., Fira Code), but stock fonts
or older Android versions may show boxes or misaligned characters.

### Hardcoded Unicode Symbols

| Symbol | File | Use | Termux Risk |
|--------|------|-----|-------------|
| `✓` checkmark | `history_cell/core.rs:69`, `rate_limits_view.rs` | Status indicators | May be 2-cell wide |
| `•◦·∘⋅☐` bullets | `history_cell/assistant.rs:713` | List rendering | Width varies by font |
| `○◔◑◕●` progress | `history_cell/plan_update.rs:158-165` | Plan progress icons | May be 2-cell wide |
| `✶` star | `history_cell/auto_drive.rs:45` | Celebration sparkles | Width miscalculation |
| `▗▄▐▌▛▚▞▜█` blocks | `history_cell/auto_drive.rs:40-43` | Celebration ASCII art | Likely misaligned |

### Color Handling

**Good:** The TUI correctly detects color support and falls back:
- `theme.rs:360` — `palette_mode()` checks `supports_color` crate
- `shimmer.rs:32` — checks `has_truecolor_terminal()` before using `Color::Rgb`
- Termux NOT in the "known truecolor" list → falls back to ANSI-256/16

**Issue:** A few hardcoded `Color::Rgb()` calls bypass palette detection:
- `history_cell/auto_drive.rs:1048` — `Color::Rgb(255, 255, 255)` without quantization

### Existing Mitigation

- `CODEX_TUI_ASCII=1` env var forces ASCII-only text output (`sanitize.rs:39`)
- But this only affects **content sanitization**, NOT UI chrome (borders, icons, spinners)

### Recommendations

- Replace all `.chars().count()` in layout code with `UnicodeWidthStr::width()`.
- Extend `CODEX_TUI_ASCII` (or add `CODEX_TUI_SIMPLE`) to also replace box-drawing borders with ASCII (`+`, `-`, `|`), and Unicode icons with ASCII fallbacks.
- Add Termux to terminal detection for conservative defaults.
- Fix `occupied_range()` to use display-width positions.

---

## 4. Input & Interaction Issues

### Keyboard Shortcuts

| Shortcut | Purpose | Termux Status |
|----------|---------|---------------|
| `Enter` | Submit message | ✅ Works |
| `Esc` | Cancel / back | ✅ Works |
| `Ctrl+D` | Exit (empty composer) | ✅ Works |
| `Ctrl+B` | Toggle browser overlay | ✅ Works |
| `Ctrl+A` | Toggle agents terminal | ✅ Works |
| `Tab` | Autocomplete / navigate | ✅ Works |
| `PageUp/Down` | Scroll history | ✅ Works |
| `Arrow keys` | Navigate | ✅ Works |
| **`Shift+Tab`** | **Cycle access mode** | **⚠️ Unreliable** — on-screen keyboard often sends Tab |
| **`F2–F6`** | **Auto-drive features** | **⚠️ Requires extra keys bar** config |
| **`F1`** | **Help overlay** | **⚠️ Requires extra keys bar** |

### Mouse-Only Features (No Keyboard Alternative)

| Feature | File | Severity |
|---------|------|----------|
| **History cell fold/collapse** | `history_cell/core.rs:274` | HIGH — no keyboard toggle |
| **Cursor positioning in textarea** | `components/textarea.rs:137` | HIGH — must use arrow keys |
| **Reasoning effort cycling** | Header bar click region | MEDIUM — no keyboard shortcut |
| **Gutter click targets** (fold/jump) | `height_manager.rs:222` | HIGH — click-only |

### Paste Support

- **Bracketed paste** is enabled (`tui.rs:82` — `EnableBracketedPaste`)
- `Ctrl+V` works in Termux via terminal paste
- Long-press paste → unreliable (generates mouse events)

### Recommendations

- Add `Alt+Tab` as alternative to `Shift+Tab` for access mode cycling.
- Add keyboard shortcuts for fold/unfold: `Ctrl+[` / `Ctrl+]` or similar.
- Add `Alt+R` for reasoning effort cycling.
- Document required Termux extra keys bar configuration.
- Consider reducing reliance on function keys for core features; provide `/slash` command alternatives.

---

## 5. Recommended Termux Configuration

### Extra Keys Bar (`~/.termux/termux.properties`)

```properties
extra-keys = [['ESC','Ctrl','Alt','Shift','Tab'],['F1','F2','F3','F4','F5','F6','|']]
```

### Environment Variables

```bash
# Force ASCII-only content sanitization
export CODEX_TUI_ASCII=1

# (Proposed) Force simple UI chrome (ASCII borders, no emoji icons)
# export CODEX_TUI_SIMPLE=1
```

### Font Recommendation

Install a Nerd Font or Fira Code in Termux for proper box-drawing and icon rendering:
```bash
# Example: install via Termux styling
pkg install termux-styling
# Then select a monospace font with good Unicode coverage
```

---

## 6. Fix Priority Matrix

### P0 — Blocks Core Functionality

| Issue | Category | Effort |
|-------|----------|--------|
| Settings sidebar unusable < 45 cols | Layout/Width | Medium — proportional sizing |
| Approval prompt unreadable at short heights | Layout/Height | Low — guarantee 2 rows for command |
| MCP settings needs 25 rows | Layout/Height | Medium — collapsible sections |
| Settings overlays fail silently | Layout/Height | Low — show "too small" message |
| `.chars().count()` in table width | Unicode | Low — replace with `.width()` |

### P1 — Degrades Experience Significantly

| Issue | Category | Effort |
|-------|----------|--------|
| Image cards broken < 52 cols | Layout/Width | Medium — text-only fallback |
| Skills/accounts label columns too wide | Layout/Width | Low — proportional or stacked |
| No keyboard fold/collapse | Input | Low — add shortcut |
| Shift+Tab unreliable | Input | Low — add Alt+Tab alternative |
| Box-drawing with no ASCII mode | Unicode | Medium — conditional border sets |
| Bottom pane min 5 rows | Layout/Height | Low — reduce to 3 on small terminals |

### P2 — Polish & Edge Cases

| Issue | Category | Effort |
|-------|----------|--------|
| Composer chrome overhead | Layout/Width | Low — reduce padding |
| Hardcoded RGB without quantization | Unicode | Low — wrap in palette check |
| Function keys need extra bar | Input | Docs — document config |
| Intro animation height/width limits | Layout | Low — cosmetic only |
| No Termux terminal detection | Unicode | Low — check `$PREFIX` env var |
| Extend `CODEX_TUI_ASCII` to UI chrome | Unicode | Medium — border/icon fallbacks |

---

*Generated by audit of `code-rs/tui/src/` — April 2026*
