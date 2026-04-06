# Android / Termux — Action Plan

> Companion to `ANDROID_TERMUX_TUI_AUDIT.md`. Six waves of improvements, from
> quick safety wins to deeper architectural work. Each item has exact file
> paths, code snippets, and implementation notes.
>
> **Note:** Truecolor works fine on modern Termux. No palette downgrade needed.

---

# Wave 1 — Quick Wins (Safety & Core Usability)

Items that fix broken or dangerous behavior in ≤ 15 lines each.

## 1.1 Approval Widget — Guarantee Readable Command Text

**Effort:** ~10 lines · **Priority:** P0 (safety)

The approval prompt can get crushed to **1 row** when the terminal is short,
meaning the user can't read the command they're approving.

**File:** `code-rs/tui/src/user_approval_widget.rs`

Current logic at line 647:
```rust
let max_prompt_height = area.height.saturating_sub(min_options_height).max(1);
prompt_height = prompt_height.min(max_prompt_height);
```

The `.max(1)` means on a tight screen the entire command text is 1 row — often
just the first few words of a potentially dangerous shell command.

**Implementation:**

1. Change `.max(1)` → `.max(2)` so the prompt always gets at least 2 rows.
2. When `area.height < min_options_height + 2`, cap visible options instead of
   the prompt. The options list already handles variable height (lines 662–689):
   it switches between expanded (3 rows/option) and compact (1 row/option).
   Add a further clamp: if `options_chunk.height < self.select_options.len()`,
   slice visible options to `options_chunk.height` items and append a final
   `"… N more"` line in `text_dim()` style.
3. The `Paragraph::new(lines).wrap(Wrap { trim: false })` at line 695 already
   handles wrapping — the 2-row minimum lets wrapping actually show content.

---

## 1.2 Settings Overlay — Show "Too Small" Hint Instead of Blank

**Effort:** ~15 lines · **Priority:** P0

When `height < 4` or `width < 4`, `render_help_overlay` returns early and
shows a completely blank area — users think the UI is broken.

**File:** `code-rs/tui/src/chatwidget/settings_overlay/overlay_render/section.rs`

Current at line 193:
```rust
if area.width < 4 || area.height < 4 {
    return;
}
```

**Implementation:**

1. Before the early return, render a centered hint using `buf.set_string()`:
   ```rust
   if area.width < 4 || area.height < 4 {
       let hint = if area.width >= 10 { "↔ resize" } else { "…" };
       let x = area.x + area.width.saturating_sub(hint.len() as u16) / 2;
       let y = area.y + area.height / 2;
       buf.set_string(x, y, hint, Style::default().fg(crate::colors::text_dim()));
       return;
   }
   ```
2. This also applies to `sectioned_panel.rs:94` which returns `None` when too
   small — same pattern, add a hint before returning.

---

## 1.3 Header Touch Targets — Enlarge Click Rects

**Effort:** ~5 lines · **Priority:** P1

Header clickable regions are 1 terminal row tall (~3 mm on a phone). The minimum
mobile touch target is 7 mm (~48 dp).

**File:** `code-rs/tui/src/chatwidget/terminal_surface_header/click_regions.rs`

Current at line 22:
```rust
out.push(ClickableRegion {
    rect: Rect {
        x: start_x + visible_start as u16,
        y: area.y,
        width: (visible_end - visible_start) as u16,
        height: 1,  // ← too small for touch
    },
    action: action.clone(),
});
```

**Implementation:**

1. Change `height: 1` to `height: area.height.min(3)`. The `area` passed in is
   the header's full rect (typically 1–3 rows). Using `area.height` means the
   clickable zone extends to fill whatever vertical space the header occupies.
2. This works because the mouse hit-test in `header.rs:handle_click()` already
   does a `rect.contains(Position { x, y })` check — a taller rect catches
   taps that land slightly above or below the label text.
3. No visual change needed — the click rect is invisible; only the hit zone
   grows.

---

## 1.4 Bottom Pane Minimum — Reduce on Short Terminals

**Effort:** ~5 lines · **Priority:** P1

The bottom pane minimum is hardcoded at 5 rows — 42% of a 12-row terminal.
The composer only needs 1 input row + 1 footer + 1 border = 3 rows minimum.

**File:** `code-rs/tui/src/height_manager.rs`

Current at lines 135–136:
```rust
let bottom_cap = percent_cap.max(5);
let desired = bottom_desired_height.max(5).min(bottom_cap);
```

**Implementation:**

1. Replace the hardcoded `5` with a height-dependent floor:
   ```rust
   let min_bottom = if area.height < 16 { 3 } else { 5 };
   let bottom_cap = percent_cap.max(min_bottom);
   let desired = bottom_desired_height.max(min_bottom).min(bottom_cap);
   ```
2. At 12 rows this frees 2 extra rows for history — a 67% increase in visible
   chat content. The composer still renders correctly at height 3: the input
   line (1 row) + footer hint row (1) + top border (1).
3. Update the comment at line 133 to mention the adaptive floor.

---

## 1.5 Fold/Collapse Gutter — Widen Touch Target

**Effort:** ~10 lines · **Priority:** P1

The fold icon in the gutter is 1–2 characters wide — essentially untappable on a
phone.

**File:** `code-rs/tui/src/chatwidget/overlay_rendering/widget_render/history_scroller/render_pass/cell_paint.rs`

At line 474 (inside the fold gutter render), a `ClickableRegion` is created
with `ClickableAction::ToggleFoldAtIndex(idx)`. The width comes from the icon
character width.

**Implementation:**

1. When creating the fold `ClickableRegion`, set `width` to
   `region_width.max(3)` — this extends the invisible tap zone to 3 columns
   while keeping the visual icon at its natural width.
2. Similarly extend `height` to `height.max(2)` to give vertical tolerance.
3. No visual change — the gutter icon stays at 1–2 chars; only the hit rect
   grows.

---

## 1.6 Composer Padding — Drop Outer Padding on Narrow Screens

**Effort:** ~10 lines · **Priority:** P2

The composer uses 6 columns of chrome (outer pad × 2 + border × 2 + inner pad
× 2). On a 30-col terminal that leaves only 24 chars for typing.

**Files:**
- `code-rs/tui/src/layout_consts.rs` — defines `COMPOSER_OUTER_HPAD = 1` (line 4)
  and `COMPOSER_CONTENT_WIDTH_OFFSET = 6` (line 12)
- `code-rs/tui/src/bottom_pane/chat_composer/render.rs:37` — uses the offset

**Implementation:**

1. Add a helper function in `layout_consts.rs`:
   ```rust
   pub fn effective_composer_offset(pane_width: u16) -> u16 {
       if pane_width < 40 {
           // Drop outer horizontal padding on narrow screens
           (COMPOSER_BORDER_WIDTH * 2) + (COMPOSER_INNER_HPAD * 2) // = 4
       } else {
           COMPOSER_CONTENT_WIDTH_OFFSET // = 6
       }
   }
   ```
2. In `render.rs:37`, replace the static `COMPOSER_CONTENT_WIDTH_OFFSET` with
   `effective_composer_offset(area.width)`.
3. Also update the `BottomPane` render to skip the outer `Margin::new(1, 0)`
   when `width < 40` so the border draws edge-to-edge.

---

# Wave 2 — Input & Accessibility

Keyboard and slash-command gaps that block Termux users from reaching features.

## 2.1 Access Mode — Add Keyboard + Slash Command Alternative ⚠️

**Effort:** ~40 lines · **Priority:** P0 (security-critical)

Cycling Read Only → Approval → Full Access requires **Shift+Tab**
(`KeyCode::BackTab`) — unreliable on Android virtual keyboards. **No
alternative exists.** This is a permissions control that must be reachable on
all platforms.

**Files to modify:**

1. **`code-rs/tui/src/bottom_pane/chat_composer/input/editor/keys.rs:20`** —
   The `BackTab` match arm sends `AppEvent::CycleAccessMode`. Add a second arm
   for `Alt+A`:
   ```rust
   KeyEvent { code: KeyCode::BackTab, .. }
   | KeyEvent {
       code: KeyCode::Char('a'),
       modifiers: KeyModifiers::ALT,
       kind: KeyEventKind::Press,
       ..
   } => {
       view.app_event_tx.send(crate::app_event::AppEvent::CycleAccessMode);
       (InputResult::None, true)
   }
   ```

2. **`code-rs/tui/src/slash_command.rs:65`** — Add a new variant before `Quit`:
   ```rust
   #[strum(serialize = "access", serialize = "access-mode")]
   Access,
   ```
   Add description in `impl SlashCommand`:
   ```rust
   SlashCommand::Access => "cycle or set access mode (read-only/approval/full)",
   ```

3. **Slash command handler** — In the dispatch path for slash commands (look for
   the match on `SlashCommand` variants in `chatwidget`), handle `Access` by
   parsing optional args:
   - No arg → send `CycleAccessMode`
   - `"read-only"` / `"ro"` → set read-only
   - `"approval"` / `"write"` → set write-with-approval
   - `"full"` → set full access

---

## 2.2 Input History — Add Ctrl+P / Ctrl+N Fallback

**Effort:** ~15 lines · **Priority:** P1

Shift+Up/Down for history navigation fails on many Termux virtual keyboards
because Shift+Arrow doesn't transmit.

**File:** `code-rs/tui/src/bottom_pane/chat_composer/input/editor/keys.rs`

Current at line 82:
```rust
if modifiers.contains(KeyModifiers::SHIFT) {
    // History navigation with Shift+Up/Down
```

**Implementation:**

Add a new match arm before the Up/Down handler for Ctrl+P and Ctrl+N:
```rust
KeyEvent {
    code: KeyCode::Char('p'),
    modifiers: KeyModifiers::CONTROL,
    kind: KeyEventKind::Press,
    ..
} => {
    if view.history.should_handle_navigation(view.textarea.text(), view.textarea.cursor()) {
        if let Some(text) = view.history.navigate_up(view.textarea.text(), &view.app_event_tx) {
            view.textarea.set_text(&text);
            view.textarea.set_cursor(0);
            return (InputResult::None, true);
        }
    }
    (InputResult::None, true)
}
```

Mirror for `Ctrl+N` calling `navigate_down`. These are standard readline
bindings and won't conflict with existing shortcuts.

---

## 2.3 Help Overlay — Add Keyboard Fallback for F1

**Effort:** ~10 lines · **Priority:** P1

F1 requires the Termux extra-keys bar. Many users won't have it configured.

**File:** `code-rs/tui/src/chatwidget/help_handlers.rs`

Current at line 10:
```rust
if let KeyEvent { code: KeyCode::F(1), .. } = key_event {
    chat.toggle_help_popup();
```

**Implementation:**

Extend the match to also accept `Ctrl+/` (common help binding):
```rust
if matches!(key_event, KeyEvent { code: KeyCode::F(1), .. })
    || matches!(key_event,
        KeyEvent { code: KeyCode::Char('/'), modifiers, .. }
        if modifiers.contains(KeyModifiers::CONTROL)
    )
{
    chat.toggle_help_popup();
    return true;
}
```

Note: `?` is already used in settings overlay for help (`settings_handlers/keys.rs:25,36`),
so we can't reuse it globally. `Ctrl+/` is unambiguous and emacs-standard.

---

## 2.4 Settings Focus — Add Alternative to Shift+Tab

**Effort:** ~10 lines · **Priority:** P1

Backward focus in settings (content → sidebar) only works with Shift+Tab.

**File:** `code-rs/tui/src/chatwidget/settings_handlers/keys.rs`

Current at line 32–35: only `KeyCode::BackTab` moves focus backward.

**Implementation:**

Add `Esc` as "back to sidebar" when the content pane is focused and no
sub-editor is active. This is natural UX — Esc means "go back one level":
```rust
KeyCode::BackTab if content_focused => { /* existing: move to sidebar */ }
KeyCode::Esc if content_focused && !has_active_editor => {
    // Same as BackTab — return to sidebar
    /* ... same focus-change logic ... */
}
```

Be careful not to conflict with the global Esc handler — this must be checked
in the settings key handler before the event bubbles up.

---

## 2.5 Help Text — Show Alternative Bindings

**Effort:** ~20 lines · **Priority:** P2

Help popup lists Shift+Tab, Shift+Enter, Shift+Up/Down with no mention of
alternatives.

**File:** `code-rs/tui/src/chatwidget/impl_chunks/popups_config_theme_access.rs`

At lines 155–231:
```rust
lines.push(kv("Shift+Tab", "Rotate agent between Read Only / ..."));
lines.push(kv("Shift+Enter", "Insert newline"));
lines.push(kv("Shift+Up", "Browse input history"));
```

**Implementation:**

After implementing waves 2.1–2.3, update the help text to list alternatives:
```rust
lines.push(kv("Alt+A / Shift+Tab", "Cycle access mode"));
lines.push(kv("Ctrl+J / Shift+Enter", "Insert newline"));
lines.push(kv("Ctrl+P / Shift+Up", "Previous history"));
lines.push(kv("Ctrl+N / Shift+Down", "Next history"));
lines.push(kv("Ctrl+/ / F1", "Help overlay"));
```

Note: Ctrl+J for newline already works (line 227 says `"Ctrl+J — Insert
newline"`). The Shift+Enter entry just needs the alternative prepended.

---

## 2.6 Browser Overlay — Add Keyboard Screenshot Navigation

**Effort:** ~15 lines · **Priority:** P2

Screenshot gallery scrolling is mouse-scroll only — no keyboard navigation.

**File:** `code-rs/tui/src/chatwidget/overlay_rendering/browser_overlay.rs`

The overlay renders screenshots with an index (`screenshot_index`) that
currently only changes on mouse scroll events.

**Implementation:**

In the key event handler for the browser overlay, add:
```rust
KeyCode::Up | KeyCode::Char('k') => {
    self.screenshot_index = self.screenshot_index.saturating_sub(1);
    true
}
KeyCode::Down | KeyCode::Char('j') => {
    self.screenshot_index = (self.screenshot_index + 1).min(max_index);
    true
}
```

The `j`/`k` bindings are already documented in the overlay header text at line
40: `"Shift+↑/↓ or j/k scroll actions"` — but for screenshots specifically,
plain Up/Down is more discoverable.

---

# Wave 3 — Layout Resilience

Overlays and panels that break or waste space on narrow/short terminals.

## 3.1 MCP Settings — Stacked Mode Needs Scroll or Accordion

**Effort:** ~50 lines · **Priority:** P1

Below 72 cols the MCP page switches to stacked mode needing 25 rows
(`list 9 + summary 8 + tools 8`). On a 15-row terminal only 1–2 rows per
section are visible.

**File:** `code-rs/tui/src/bottom_pane/settings_pages/mcp/layout.rs`

At lines 70–98: wide (≥72 cols) vs stacked layout. At line 50:
`show_hint_row` requires `width >= 80` — never shows on phone.

**Implementation:**

1. When `content.height < 20` in stacked mode, show **one section at a time**
   instead of all three squeezed. Add a `focused_section: usize` (0=list,
   1=summary, 2=tools) to MCP state. Render only the focused section at full
   height, with a 1-row tab bar: `[List] [Summary] [Tools]` where Tab or
   Left/Right switches focus.
2. Lower hint-row threshold from `width >= 80` to `width >= 50`.
3. This is the largest item in Wave 3 — consider deferring if time is tight.

---

## 3.2 Image Cards — Text-Only Fallback Below 52 Cols

**Effort:** ~20 lines · **Priority:** P1

Image + text layout needs 51 cols
(`IMAGE_LEFT_PAD(1) + IMAGE_MIN_WIDTH(18) + IMAGE_GAP(2) + MIN_TEXT_WIDTH(28) + TEXT_RIGHT_PADDING(2) = 51`).
On a 40-col terminal the card returns `None` — the image vanishes entirely.

**Files:**
- `code-rs/tui/src/history_cell/image.rs` — constants at lines 41–43, layout
  function returns `None` when width insufficient
- `code-rs/tui/src/history_cell/browser/mod.rs:27` — same `MIN_TEXT_WIDTH = 28`

**Implementation:**

In the image layout function, before returning `None`, add a text-only
fallback branch:
```rust
if available_width < IMAGE_LEFT_PAD + IMAGE_MIN_WIDTH + IMAGE_GAP + MIN_TEXT_WIDTH + TEXT_RIGHT_PADDING {
    // Text-only fallback: show filename and dimensions as a single card row
    let label = format!("📷 {} ({}×{})", filename, width, height);
    return Some(/* single CardRow with label */);
}
```

This ensures image output is never invisible — users at least see what was
generated.

---

## 3.3 Agent/Browser Overlay — Full-Width on Narrow Screens

**Effort:** ~25 lines · **Priority:** P1

Agent terminal overlay reserves a 24-col sidebar even at 35 cols, leaving 5–10
cols for the terminal pane (unreadable). Browser overlay forces a 20-col
minimum content pane.

**Files:**
- `code-rs/tui/src/chatwidget/overlay_rendering/agents_terminal_overlay.rs:182`
- `code-rs/tui/src/chatwidget/overlay_rendering/browser_overlay.rs:188`

**Implementation — agents overlay:**

At line 182, the check is `body_area.width <= 30`. Change to:
```rust
if body_area.width < 50 {
    // Single-pane mode: show sidebar only (with Enter to switch to terminal)
    // or terminal only (with Esc to go back to sidebar)
    let constraints = [Constraint::Length(body_area.width), Constraint::Length(0)];
    // Use a state flag to toggle which pane is shown
}
```

Add a `single_pane_focus: enum { Sidebar, Terminal }` state to the overlay.
Tab switches between them. This mirrors how mobile apps handle master-detail.

**Implementation — browser overlay:**

At line 188, same pattern: below 50 cols, show screenshots OR action log, not
side-by-side.

---

## 3.4 MCP List Width — Clamp Proportionally

**Effort:** ~5 lines · **Priority:** P2

`(content.width / 3).max(30)` forces the MCP list to 30 cols minimum, leaving
only 10 for the detail pane on a 40-col terminal.

**File:** `code-rs/tui/src/bottom_pane/settings_pages/mcp/layout.rs:183`

**Implementation:**

```rust
// Before:
let list_width = (content.width / 3).max(30u16);
// After:
let list_width = (content.width / 3).clamp(15, 30);
```

At 40 cols: `40/3 = 13` → clamped to 15, detail gets 25 (usable).
At 90 cols: `90/3 = 30` → clamped to 30, detail gets 60 (optimal).

---

## 3.5 Policy Editor — Lower Label Width Floor

**Effort:** ~5 lines · **Priority:** P2

`label_width.clamp(12, 30)` plus `min_value_width = 14` requires 26+ cols of
inner space — tight on narrow terminals.

**File:** `code-rs/tui/src/bottom_pane/settings_pages/mcp/policy_editor/render.rs:317`

**Implementation:**

```rust
// Before:
let min_value_width = 14u16;
let label_width = inner.width.saturating_sub(min_value_width).clamp(12, 30);
// After:
let min_value_width = if inner.width < 30 { 10 } else { 14 };
let label_width = inner.width.saturating_sub(min_value_width).clamp(8, 24);
```

---

## 3.6 Overlay Stack — Lower Width Floor

**Effort:** ~5 lines · **Priority:** P2

`(body_inner.width - 10).max(20)` forces overlays to 20-col minimum, causing
margin squeeze on 35-col terminals.

**File:** `code-rs/tui/src/chatwidget/overlay_rendering/widget_render/overlay_stack.rs:308`

**Implementation:**

```rust
// Before:
let w = (body_inner.width as i16 - 10).max(20) as u16;
// After:
let w = (body_inner.width as i16 - 6).max(14) as u16;
```

Reduces the margin from 10→6 and the floor from 20→14. On a 35-col terminal:
overlay gets 29 cols instead of 25.

---

# Wave 4 — Unicode Width Correctness

Fix `.chars().count()` → `.width()` for display-column calculations.

## Triage Rule

- **Layout/rendering** (padding, column widths, centering) → must use `.width()`
- **Content limits** (max chars for descriptions, API payloads) → `.chars().count()` is correct
- **Secret masking** (`"*".repeat(value.chars().count())`) → correct (stars are 1-wide)

All files below are under `code-rs/tui/src/`. Import `unicode_width::UnicodeWidthStr`
at the top of each file (the crate is already a dependency).

## Already Fixed ✅

- `markdown_renderer/tables.rs` — uses `display_width_approx()` now

## 4.1 Header Width Calculations

**Effort:** ~15 lines · **Priority:** P1

Header template and layout accumulate character counts for centering. Any CJK
or emoji in model names, directory paths, or shell labels will misalign.

**Files and exact sites:**

| File | Line | Current | Replace with |
|------|------|---------|-------------|
| `chatwidget/terminal_surface_header/template.rs` | 41 | `*width += value.chars().count()` | `*width += value.width()` |
| `chatwidget/terminal_surface_header/template.rs` | 149 | `prefix.chars().count()` | `prefix.width()` |
| `chatwidget/terminal_surface_header/template.rs` | 163 | `value.chars().count()` | `value.width()` |
| `chatwidget/terminal_surface_header/template.rs` | 178 | `raw_token.chars().count()` | `raw_token.width()` |
| `chatwidget/terminal_surface_header/layout.rs` | 31 | `*width += text.chars().count()` | `*width += text.width()` |
| `chatwidget/terminal_surface_render.rs` | 524 | `value.chars().count()` | `value.width()` |
| `chatwidget/terminal_surface_render.rs` | 540 | `fallback.chars().count()` | `fallback.width()` |

---

## 4.2 History Cell Width Calculations

**Effort:** ~15 lines · **Priority:** P1

Bullet indentation, explore entry padding, reasoning span length all measure
characters instead of display columns.

| File | Line | Current | Replace with |
|------|------|---------|-------------|
| `history_cell/assistant.rs` | 724 | `t.chars().count()` | `t.width()` |
| `history_cell/explore.rs` | 350 | `label.chars().count()` | `label.width()` |
| `history_cell/explore.rs` | 415 | `entry_label(entry).chars().count()` | `entry_label(entry).width()` |
| `history_cell/reasoning.rs` | 715 | `span.text.chars().count()` | `span.text.width()` |

**auto_drive.rs notes (no change needed):**
- Line 777–787 `occupied_range()` uses `chars().enumerate()` to find non-space
  bounds. This operates on a `Vec<char>` built from ASCII art, so char index =
  display column. The celebration art is ASCII-only.
- Line 1014 `chars.len()` is also on ASCII art chars.

---

## 4.3 Component / Bottom Pane Width Calculations

**Effort:** ~10 lines · **Priority:** P2

Selection list items, auto-coordinator headers, and prompt titles use character
counts for layout.

| File | Line | Current | Replace with |
|------|------|---------|-------------|
| `components/list_selection_view.rs` | 55 | `part.chars().count()` | `part.width()` |
| `bottom_pane/panes/auto_coordinator/render/prompt.rs` | 118 | `title.chars().count()` | `title.width()` |
| `bottom_pane/panes/auto_coordinator/render/header.rs` | 40 | `header_text.chars().count()` | `header_text.width()` |
| `bottom_pane/panes/auto_coordinator/render/header.rs` | 228 | `full_title.chars().count().max(1)` | `full_title.width().max(1)` |

---

# Wave 5 — Rendering Performance (Low-Power Devices)

Allocation-heavy render paths that hurt frame rate on slow hardware. These
matter most on low-end Android SoCs where allocator overhead is proportionally
larger.

## 5.1 Markdown Stream — Eliminate Buffer Clone Per Frame

**Effort:** ~20 lines · **Priority:** P1

`render_preview_lines()` clones the entire markdown buffer string on **every
frame** during streaming responses.

**File:** `code-rs/tui/src/markdown_stream.rs`

At line 377:
```rust
let source = unwrap_markdown_language_fence_if_enabled(self.buffer.clone());
```

**Implementation:**

`unwrap_markdown_language_fence_if_enabled` just strips a leading ` ```lang\n`
fence and returns a `Cow<str>`. Change it to take `&str` and return
`Cow<'_, str>`:
```rust
fn unwrap_markdown_language_fence_if_enabled(buffer: &str) -> Cow<'_, str> { ... }
```
Then `render_preview_lines` can pass `&self.buffer` without cloning.
`strip_empty_fenced_code_blocks` similarly should take `&str` → `Cow`.

This eliminates one full-buffer allocation per frame during streaming — the
hottest render path.

---

## 5.2 Theme Split Preview — Reuse Snapshot Buffer

**Effort:** ~15 lines · **Priority:** P2

Theme preview allocates and clones full buffer halves every frame via
`snapshot_left_half()`.

**File:** `code-rs/tui/src/app/render.rs:148,199-228`

**Implementation:**

Add a `theme_preview_buf: Vec<Cell>` field to the render state (or `App`).
In `snapshot_left_half`, accept `&mut Vec<Cell>` and `.clear()` + `extend()`
instead of creating a new Vec. The capacity stays allocated across frames.

---

## 5.3 Inline Text Processing — Avoid `Vec<char>` Intermediate

**Effort:** ~30 lines · **Priority:** P2

`process_inline_spans()` collects entire text into a `Vec<char>`, then
re-collects char slices into new Strings.

**File:** `code-rs/tui/src/markdown_renderer/preamble.rs:368,403`

```rust
let chars: Vec<char> = text.chars().collect();      // line 368
let rest: String = chars[i..].iter().collect();      // line 403
```

**Implementation:**

Replace with `char_indices()`:
```rust
for (byte_idx, ch) in text.char_indices() {
    // ... pattern matching on ch ...
    let rest = &text[byte_idx..];  // zero-alloc slice
}
```

This eliminates the `Vec<char>` allocation and all the `.iter().collect()`
re-allocations. The logic is identical — just index by byte position instead
of char position.

---

## 5.4 Glitch Animation — Cache Frames

**Effort:** ~25 lines · **Priority:** P2

The intro animation allocates multiple `Vec<String>` per frame with repeated
`.to_vec()`, `String::new()`, and `" ".repeat()`.

**File:** `code-rs/tui/src/glitch_animation.rs:116-130,252-274,347-390`

**Implementation:**

Precompute the full animation sequence on first call:
```rust
struct CachedAnimation {
    frames: Vec<Vec<Line<'static>>>,
    last_size: (u16, u16),
}
```
Invalidate when terminal size changes. On each tick, index into the cache.
This turns per-frame O(n) allocations into O(1) lookups.

---

## 5.5 Diff Render — Avoid Repeated Path Clones

**Effort:** ~10 lines · **Priority:** P2

Every diff line clones file paths into `RtSpan::styled(path.clone(), ...)`.

**File:** `code-rs/tui/src/diff_render.rs:244-261`

**Implementation:**

Since `RtSpan` takes `Cow<'static, str>`, and the diff struct owns the
paths, we can't borrow directly. Two options:
1. Wrap paths in `Arc<str>` and implement `From<Arc<str>>` for the span text.
2. Clone once per file (not per line) by extracting the path clone above the
   line loop and reusing it.

Option 2 is simpler and still eliminates most clones.

---

# Wave 6 — Resilience & Recovery

Edge cases that are common on Android but rare on desktop.

## 6.1 SIGHUP / SIGTERM Handlers

**Effort:** ~30 lines · **Priority:** P1

No handler for SIGHUP (terminal disconnect) or SIGTERM (process kill). The TUI
crashes ungracefully when Termux OOM-kills the process or the network drops.

**File:** `code-rs/tui/src/app/terminal/screen.rs`

Current: only SIGTSTP (Ctrl+Z) is handled (line 28). SIGHUP and SIGTERM
result in unclean terminal state (raw mode left on, alt screen not exited).

**Implementation:**

Use `signal_hook` (already in the dependency tree via crossterm):
```rust
use signal_hook::consts::signal::{SIGHUP, SIGTERM, SIGPIPE};
use signal_hook::flag;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// In app init:
let should_exit = Arc::new(AtomicBool::new(false));
flag::register(SIGHUP, Arc::clone(&should_exit))?;
flag::register(SIGTERM, Arc::clone(&should_exit))?;
flag::register(SIGPIPE, Arc::new(AtomicBool::new(false)))?; // ignore

// In event loop:
if should_exit.load(Ordering::Relaxed) {
    tui::restore()?;
    std::process::exit(0);
}
```

The `flag::register` approach is async-signal-safe — it just sets an atomic
bool. The event loop checks it on each iteration and does a clean shutdown.

---

## 6.2 Termux Auto-Detection Helper

**Effort:** ~15 lines · **Priority:** P2

Central helper used by other waves to adapt behavior on Termux.

**File:** `code-rs/tui/src/platform_caps.rs`

This file already has `supports_clipboard_image_paste()`, `supports_native_picker()`,
etc. — all with the same pattern of `cfg!(target_os = "android")` + env checks.

**Implementation:**

```rust
use std::sync::OnceLock;

pub(crate) fn is_termux() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("PREFIX")
            .ok()
            .is_some_and(|p| p.contains("/com.termux/"))
    })
}
```

Then use `is_termux()` in:
- `height_manager.rs` — adaptive bottom pane minimum (Wave 1.4)
- `click_regions.rs` — larger touch targets (Wave 1.3)
- Help popup — show alternatives first (Wave 2.5)

**Note:** Do NOT use this for color palette detection — truecolor works fine
on modern Termux.

---

# Implementation Order

| Step | Wave | Items          | Theme                                      |
| ---- | ---- | -------------- | ------------------------------------------ |
| 1    | 1    | §1.1–1.2       | Safety — can't approve or configure        |
| 2    | 2    | §2.1           | Security — access mode unreachable         |
| 3    | 1    | §1.3–1.5       | Touch and height — core phone usability    |
| 4    | 2    | §2.2–2.4       | Keyboard — reach all features              |
| 5    | 4    | §4.1–4.2       | Unicode width — correctness for i18n       |
| 6    | 3    | §3.1–3.3       | Layout — overlays usable on narrow screens |
| 7    | 1+2  | §1.6, §2.5–2.6 | Polish — ergonomic wins                    |
| 8    | 4    | §4.3           | Unicode width — remaining sites            |
| 9    | 5    | §5.1, §5.3     | Perf — highest-impact allocation fixes     |
| 10   | 6    | §6.1, §6.2     | Resilience — signal handling + detection   |
| 11   | 3    | §3.4–3.6       | Layout polish — secondary overlays         |
| 12   | 5    | §5.2, §5.4–5.5 | Perf — remaining allocation wins           |
| 13   | 2    | §2.5           | Polish — platform-aware help text          |

---

*Companion to `ANDROID_TERMUX_TUI_AUDIT.md` — April 2026*
