# Android / Termux — Quick-Win Action Plan

> Extracted from `ANDROID_TERMUX_TUI_AUDIT.md`. Items ordered by effort (lowest
> first), grouped by category. Each item lists the file(s) to touch, the change,
> and why it matters.

---

## 1. Approval Widget — Guarantee Readable Command Text

**Effort:** ~10 lines · **Priority:** P0 (safety)

The approval prompt can get crushed to **1 row** when the terminal is short,
meaning the user can't read the command they're approving.

**File:** `code-rs/tui/src/user_approval_widget.rs:646-649`

```rust
// Current: prompt gets max(1) row after subtracting all option rows.
let max_prompt_height = area.height.saturating_sub(min_options_height).max(1);
```

**Change:** Guarantee ≥ 2 rows for the command prompt. Shrink the options list
instead:

```rust
let max_prompt_height = area.height.saturating_sub(min_options_height).max(2);
```

Also add overflow indication ("…more options") when options get truncated.

---

## 2. Settings Overlay — Show "Too Small" Hint Instead of Blank

**Effort:** ~15 lines · **Priority:** P0

When `height < 4` or `width < 4`, the overlay returns early and shows nothing —
users think the UI is broken.

**File:** `code-rs/tui/src/chatwidget/settings_overlay/overlay_render/section.rs:193`

```rust
// Current: silent early return
if area.width < 4 || area.height < 4 {
    return;
}
```

**Change:** Render a centered hint like `"↔ resize terminal"` before returning.

---

## 3. Header Touch Targets — Enlarge Click Rects

**Effort:** ~5 lines · **Priority:** P1

Header clickable regions are 1 terminal row tall (~3mm on a phone). The minimum
mobile touch target is 7mm (~48dp).

**File:** `code-rs/tui/src/chatwidget/terminal_surface_header/click_regions.rs:22-27`

```rust
// Current:
height: 1,
```

**Change:** Extend `height` to `area.height.min(3)` (or at least 2). The hit-test
already uses `Rect`, so enlarging height costs nothing visually — the clickable
area just extends below the visible label.

---

## 4. Bottom Pane Minimum — Reduce on Short Terminals

**Effort:** ~5 lines · **Priority:** P1

The bottom pane minimum is hardcoded at 5 rows. On a 12-row terminal, that's
42% of the screen.

**File:** `code-rs/tui/src/height_manager.rs:135-136`

```rust
// Current:
let bottom_cap = percent_cap.max(5);
let desired = bottom_desired_height.max(5).min(bottom_cap);
```

**Change:** On terminals < 16 rows, reduce the floor from 5 to 3:

```rust
let min_bottom = if area.height < 16 { 3 } else { 5 };
let bottom_cap = percent_cap.max(min_bottom);
let desired = bottom_desired_height.max(min_bottom).min(bottom_cap);
```

---

## 5. Composer Padding — Drop Outer Padding on Narrow Screens

**Effort:** ~10 lines · **Priority:** P2

The composer uses 6 columns of chrome (outer pad + border + inner pad × 2). On a
30-col terminal that leaves only 24 chars for typing.

**File:** `code-rs/tui/src/layout_consts.rs:4`  
**File:** `code-rs/tui/src/bottom_pane/chat_composer/render.rs:37`

**Change:** Make `COMPOSER_OUTER_HPAD` conditional — use `0` when area width < 40.
Either pass width into the render or add a `fn effective_composer_offset(width)`
helper that returns `4` instead of `6` on small screens, reclaiming 2 columns.

---

## 6. Unicode Width — Fix `.chars().count()` in Layout Code

**Effort:** ~20 lines across files · **Priority:** P0/P1

Many layout calculations use `.chars().count()` (character count) instead of
display width. This breaks alignment with CJK, emoji, and other wide characters.

### Already fixed ✅

- `markdown_renderer/tables.rs` — uses `display_width_approx()` now.

### Still needs fixing

| File                                        | Line(s)  | Context                          |
| ------------------------------------------- | -------- | -------------------------------- |
| `history_cell/assistant.rs`                 | 724      | Bullet indent depth              |
| `history_cell/auto_drive.rs`                | 777-787  | `occupied_range()` char index    |
| `history_cell/auto_drive.rs`                | 1014     | Celebration art `chars.len()`    |
| `chatwidget/terminal_surface_header/template.rs` | 41,149,163,178 | Header width calculation   |
| `chatwidget/terminal_surface_header/layout.rs`   | 31       | Header width accumulation  |
| `chatwidget/terminal_surface_render.rs`     | 524, 540 | Segment width in header render   |
| `history_cell/explore.rs`                   | 350, 415 | Explore entry label padding      |
| `history_cell/reasoning.rs`                 | 715      | Reasoning span length            |
| `components/list_selection_view.rs`         | 55       | Selection list item width        |
| `bottom_pane/panes/auto_coordinator/render/prompt.rs` | 118 | Title width              |
| `bottom_pane/panes/auto_coordinator/render/header.rs` | 40, 228 | Header char count      |

**Change:** Import `unicode_width::UnicodeWidthStr` and replace `.chars().count()`
with `.width()` in each rendering/layout call site. Non-rendering uses (character
limits for content truncation, input validation) can stay as-is since they're
counting logical characters, not display columns.

### Triage rule

- **Layout/rendering** (padding, column widths, centering) → must use `.width()`
- **Content limits** (max chars for descriptions, API payloads) → `.chars().count()` is fine
- **Secret masking** (`"*".repeat(value.chars().count())`) → fine, stars are 1-wide

---

## 7. Fold/Collapse Gutter — Widen Touch Target

**Effort:** ~10 lines · **Priority:** P1

The fold icon in the gutter is 1–2 characters wide — essentially untappable on a
phone.

**File:** `code-rs/tui/src/history_cell/cell_paint.rs` (fold gutter rendering)

**Change:** When registering the fold/collapse clickable region, extend its `width`
to at least 3 characters. This widens the tap zone without changing the visual —
the hit-test area just becomes a bit wider than the visible icon.

---

## 8. Detect Termux for Conservative Defaults

**Effort:** ~15 lines · **Priority:** P2

Termux sets `$PREFIX=/data/data/com.termux/files/usr`. Detecting this lets us
auto-tune defaults without user config.

**Change:** Add a `fn is_termux() -> bool` helper that checks
`std::env::var("PREFIX")` for the Termux path. Use it to:
- Default to ASCII mode
- Reduce bottom pane minimum
- Enlarge click target heights
- Skip function-key-dependent hotkeys in help text

---

## Implementation Order (Suggested)

| Step | Items | Why first                                        |
| ---- | ----- | ------------------------------------------------ |
| 1    | §1–§2 | Safety and UX — users can't approve or configure |
| 2    | §3–§4 | Touch and height — core phone usability          |
| 3    | §6    | Unicode width — correctness for i18n users       |
| 4    | §5,§7 | Polish — small ergonomic wins                    |
| 5    | §8    | Auto-detection — ties everything together        |

---

*Companion to `ANDROID_TERMUX_TUI_AUDIT.md` — April 2026*
