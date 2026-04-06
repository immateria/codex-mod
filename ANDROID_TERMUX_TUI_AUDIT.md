# Android / Termux TUI Audit

> Audit of the code-rs TUI for narrow-width, short-height, and touch-driven
> terminals typical of **Termux on Android** (portrait mode, on-screen keyboard).
>
> Typical Termux dimensions: **30–40 columns × 12–20 rows** (keyboard visible)

---

## Table of Contents

1. [Layout: Narrow Width](#1-layout-narrow-width)
2. [Layout: Short Height](#2-layout-short-height)
3. [Touch & Tap Targets](#3-touch--tap-targets)
4. [Input & Hotkey Flexibility](#4-input--hotkey-flexibility)
5. [Unicode Width Bugs](#5-unicode-width-bugs)
6. [Fix Priority Matrix](#6-fix-priority-matrix)

---

## 1. Layout: Narrow Width

At 30–40 columns, several UI elements break or become unusable.

### Settings System (Broken Below ~45 cols)

The settings overlay uses a **fixed 22-column sidebar** (`section.rs:27`) plus an
**18-column label column** (`types.rs:9`). That's 40 cols of chrome before any
content appears. On a 30-col screen the content pane gets **~8 columns** — unusable.

**Affected files and constants:**

| Constant / Logic | File | Value | Impact at 30 cols |
|------------------|------|-------|--------------------|
| Sidebar width | `settings_overlay/overlay_render/section.rs:27` | `Length(22)` | 73% of screen |
| Label column | `settings_overlay/types.rs:9` | `18` chars | 0 cols for values |
| Skills labels | `settings_pages/skills/render.rs:8` | `Length(24)` | 6 cols for content |
| Overlay min width | `settings_pages/interface/hotkeys.rs:7` | `40` | Overlay unavailable |
| Accounts two-pane | `settings_pages/accounts/.../mod.rs:17` | `96` cols min | Single-col only |

**Fix:** Make the sidebar proportional (`min(22, width * 40%)`) and add a
**stacked mode** that puts sidebar above content instead of beside it when
`width < 45`. Most settings pages already handle single-column form layouts.

### Card / History Cell Rendering

| Element | File | Min Width | Issue |
|---------|------|-----------|-------|
| Image cards | `history_cell/image.rs:33-45` | 52 cols | `MIN_WIDTH(18) + GAP(2) + TEXT(28) + pad`. No text-only fallback. |
| Agents sidebar | `agents_terminal_overlay.rs:182` | 30 col breakpoint | At ≤ 30 cols, detail pane = 0 cols. |
| Browser sidebar | `browser_overlay.rs:188` | Fixed 24 cols | Screenshot area only 16 cols at 40-col. |
| Limits wide layout | `limits/content_impl.rs:2` | 110 cols | Single-column only on phones (acceptable). |

**Fix:** Image cards should drop to a **title + text-only** layout below 40 cols.
Agent/browser overlays should go **full-width single-pane** instead of
sidebar-plus-detail.

### Composer Input Area

The chat composer has **6 columns of chrome** (`layout_consts.rs:12`:
`COMPOSER_CONTENT_WIDTH_OFFSET = 6` for outer pad + border + inner pad on each
side). At 30 cols that's only 24 chars of typing area; at 20 cols, only 14.

**Fix:** Drop outer horizontal padding (`COMPOSER_OUTER_HPAD`) on terminals
< 40 cols, saving 2 cols. The border itself provides sufficient visual
separation.

---

## 2. Layout: Short Height

With the on-screen keyboard visible, Termux typically has **12–15 rows**. The TUI
must fit status bar + history + composer into that budget.

### Critical Vertical Budget Conflicts

| Component | Min Rows | File | Issue |
|-----------|----------|------|-------|
| Bottom pane | 5 | `height_manager.rs:136` | 42% of a 12-row screen |
| History | 3 | `height_manager.rs:225` | Only 3 lines visible |
| Status bar | 1 | — | Always shown |
| HUD preview | 4 | `height_manager.rs:196` | Disappears entirely at budget < 4 |
| **Total minimum** | **9** | | Only 3 rows spare at 12 |

At 12 rows: `status(1) + history(3) + bottom(5) + borders(≥2) = 11+`. There's
barely room for **anything**.

**Fix:** On terminals < 16 rows, reduce bottom pane minimum from 5 to **3**. The
composer only needs 1 row for the input line + 1 for the footer hint + 1 for
the prompt/border.

### Settings Pages Refuse to Render

| Page | Min Height | File | Behavior |
|------|-----------|------|----------|
| MCP (stacked) | **25 rows** | `mcp/layout.rs:96-98` | `list(9)+summary(8)+tools(8)`. Unusable. |
| Settings overlay | 4 rows | `section.rs:193` | Returns early — **silently blank**. |
| Sectioned panel | header+1+footer | `sectioned_panel.rs:94` | Returns `None` — panel vanishes. |
| Accounts | 10 rows | `accounts/.../mod.rs:18` | Two-pane blocked. |

**Fix:** MCP settings need a **collapsible accordion** that shows one section at a
time. Settings overlay should display a "resize terminal" hint instead of going
blank.

### Approval Widget — Safety Risk

`user_approval_widget.rs:646-649`: When height is tight, the command prompt
gets crushed to **1 row** while option buttons consume the rest. The user
**can't read the command they're approving**.

**Fix:** Guarantee ≥ 2 rows for the command text; truncate the options list
instead.

---

## 3. Touch & Tap Targets

Termux translates screen taps into mouse click events. The TUI already has a
clickable region system (`ClickableRegion` in `chatwidget`), but the current
targets are **1 terminal row tall and as narrow as the label text**. On a phone
screen, that's roughly 3–5mm tall — below the recommended 7mm (48dp) minimum
for mobile touch targets.

### Current Clickable Regions

| Region | Height | Width | File | Touch Friendly? |
|--------|--------|-------|------|-----------------|
| Header "Model:" | 1 row | ~10 chars | `terminal_surface_header/click_regions.rs` | ❌ Too small |
| Header "Shell:" | 1 row | ~8 chars | same | ❌ Too small |
| Header "Reasoning:" | 1 row | ~12 chars | same | ❌ Too small |
| Fold/collapse gutter | 1 row | 1-2 chars | `cell_paint.rs:379,450` | ❌ Tiny |
| Bottom pane items | 1 row each | varies | `selectable_list_mouse.rs` | ⚠️ Marginal |

### Proposed Improvements

1. **Increase header click target height to 2-3 rows** by extending the
   clickable rect vertically (the hit-test already uses `Rect`, so enlarging
   `height` is trivial). Even if the visual label is 1 row, the tappable area
   should extend above and below.

2. **Add dedicated touch-action rows** on small screens: a row of tappable
   buttons below the header (e.g., `[Model] [Shell] [Reasoning] [Settings]`)
   that replaces the compact header. This is similar to the Termux extra keys
   bar concept but rendered inside the TUI.

3. **Fold/collapse targets** should expand to at least 3 chars wide. Currently
   the gutter fold icon is 1–2 chars, which is nearly impossible to tap
   accurately on a phone.

4. **Approval buttons** should be large, full-width rows rather than inline
   options. On a phone, "Yes / No / Edit" should each be a separate tappable
   row.

5. **Slash command popup items** are currently mouse-inaccessible (documented as
   deferred in `MOUSE_CLICKS.md`). On touch screens this is a significant gap —
   users can't tap to select from the autocomplete popup.

### Implementation Notes

The `ClickableRegion` system (`chatwidget/input_pipeline/mouse/header.rs`)
already supports arbitrary `Rect` dimensions. Enlarging touch targets requires:
- Extending `rect.height` when registering regions (no rendering change needed)
- For the "touch action bar" idea: a new 1-row widget rendered between header
  and history when `area.width < 50 || area.height < 20`

---

## 4. Input & Hotkey Flexibility

### What Works on Termux

| Shortcut | Purpose | Status |
|----------|---------|--------|
| `Enter` | Submit message | ✅ |
| `Esc` | Cancel / back / stop auto-drive | ✅ |
| `Ctrl+D` | Exit (empty composer) | ✅ |
| `Ctrl+B` | Toggle browser overlay | ✅ |
| `Ctrl+A` | Toggle agents terminal | ✅ |
| `Tab` | Autocomplete / navigate | ✅ |
| `PageUp/Down` | Scroll history | ✅ |
| `Arrow keys` | Navigate | ✅ |
| `Ctrl+V` / bracketed paste | Paste | ✅ |

### What's Problematic

| Shortcut | Purpose | Issue |
|----------|---------|-------|
| `Shift+Tab` | Cycle access mode (RO → Write → Full) | On-screen keyboards often send plain Tab |
| `F1` | Help overlay | Needs Termux extra keys bar |
| `F2–F6` | Auto-drive features / configurable hotkeys | Needs extra keys bar |
| Mouse click on header | Model/Shell/Reasoning selectors | Touch targets too small (see §3) |

### Hotkey System Is Already Flexible

The `TuiHotkey` type (`core/config_types.rs:1544`) supports:
- **Function keys** (`F1`–`F24`)
- **Chords** (`Ctrl+X`, `Alt+X`, `Ctrl+Alt+X`)
- **Legacy** single-key bindings
- **Disabled**

Users can remap hotkeys in their config. This is good — but the **defaults**
assume function keys are available, which they aren't on Termux without extra
keys bar config.

### Recommended Default Adjustments for Small Terminals

- **Shift+Tab fallback**: Add `Alt+A` (or any Ctrl chord) as alternative for
  access mode cycling. `Alt` works in Termux.
- **Reasoning cycling**: Add a keyboard shortcut (currently mouse-only).
  `Alt+R` or a configurable hotkey.
- **Fold/collapse**: Add keyboard shortcut. Currently mouse-only via gutter
  click (`ToggleFoldAtIndex`). Suggest `[` / `]` keys when focus is on history
  (the `Legacy` hotkey slot).
- Provide `/` slash command equivalents for anything that currently requires a
  function key or mouse click. `/reasoning`, `/fold`, `/access` would cover
  the gaps.

### Mouse-Only Features That Need Keyboard Alternatives

| Feature | Current Access | Suggested Alternative |
|---------|---------------|----------------------|
| History fold/collapse | Gutter click | `[` / `]` or `/fold` |
| Reasoning cycling | Header click | `Alt+R` or `/reasoning` |
| Cursor positioning | Mouse click in textarea | Arrow keys work (acceptable) |
| Directory picker | Header click | `/cd` command exists ✅ |

---

## 5. Unicode Width Bugs

Several layout calculations use **character count** (`.chars().count()`) instead
of **display width** (`UnicodeWidthStr::width()`). This causes misalignment
with CJK characters, emoji, or any double-width Unicode.

### Confirmed Bugs

| Location | File | Line(s) | Bug |
|----------|------|---------|-----|
| **Table column widths** | `markdown_renderer/tables.rs` | 118, 122, 138 | `widths[i] = cell.chars().count()` — wrong for CJK |
| **Celebration art layout** | `history_cell/auto_drive.rs` | 1014-1019 | `chars.len()` for width calculation |
| **Sparkle position range** | `history_cell/auto_drive.rs` | 777-787 | Char index ≠ display position |
| **Bullet indent depth** | `history_cell/assistant.rs` | 724 | `chars().count()` for indent |

### What's Already Correct

- Spinner frame width: `spinner.rs:204` uses `UnicodeWidthStr::width()` ✅
- Card truncation: `card_style.rs` uses `UnicodeWidthStr` ✅
- Color detection: falls back to ANSI-256/16 on Termux ✅
- `CODEX_TUI_ASCII=1` forces ASCII content (but not UI chrome) ✅

### Recommended Fix

Replace every `.chars().count()` in layout/rendering code with `.width()` from
`unicode_width`. The most impactful fix is the **table renderer** — markdown
tables with CJK content are visibly broken.

---

## 6. Fix Priority Matrix

### P0 — Blocks Core Functionality

| Issue | Category | Effort |
|-------|----------|--------|
| Settings sidebar breaks < 45 cols → stacked mode | Layout | Medium |
| Approval prompt unreadable at short heights | Layout | Low |
| MCP settings needs 25 rows → accordion mode | Layout | Medium |
| Settings overlays go blank silently → show hint | Layout | Low |
| `.chars().count()` in table width → `.width()` | Unicode | Low |

### P1 — Major Experience Gaps

| Issue | Category | Effort |
|-------|----------|--------|
| Touch targets too small (header, gutter) | Touch | Low |
| No keyboard shortcut for fold/collapse | Input | Low |
| No keyboard shortcut for reasoning cycling | Input | Low |
| Shift+Tab unreliable → add Alt chord alternative | Input | Low |
| Image cards broken < 52 cols → text-only fallback | Layout | Medium |
| Bottom pane min 5 rows → reduce to 3 on small screens | Layout | Low |

### P2 — Polish

| Issue | Category | Effort |
|-------|----------|--------|
| Composer padding reduction on narrow screens | Layout | Low |
| Touch action bar for small screens | Touch | Medium |
| Slash command popup not tappable | Touch | High (deferred) |
| Extend `CODEX_TUI_ASCII` to UI chrome (borders, icons) | Unicode | Medium |
| Detect Termux via `$PREFIX` env for conservative defaults | Config | Low |
| Document recommended Termux extra keys bar | Docs | Low |

### Recommended Termux Extra Keys Bar

```properties
# ~/.termux/termux.properties
extra-keys = [['ESC','Ctrl','Alt','Shift','Tab'],['F1','F2','F3','F4','F5','F6','|']]
```

---

*Audit of `code-rs/tui/src/` — April 2026*
