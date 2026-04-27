# Agent/Human Communication Patterns

A design reference for interaction patterns between agent and human in CodexCLI. The goal is
not just more question types, but better mechanisms for communicating:

- **priorities and scope** — what counts, what's out of bounds
- **constraints and hard limits** — guardrails that prevent wasted work
- **approval boundaries and risk tolerance** — keep humans in control without micromanaging
- **confidence and firmness of intent** — distinguish hard requirements from brainstorming
- **preference persistence and scope** — avoid re-litigating the same questions
- **autonomy boundaries** — where the agent continues automatically and where it stops to ask

---

## How to Use This Document

**For designers/architects:** Read the Design Principles table and Non-Question Interaction Patterns. These inform every UI choice.

**For implementers (first-time):** Read **in this order**:
1. **CodexCLI TUI Architecture (Actual State)** — what exists today
2. **Priority Ranking for CodexCLI** — what to build first
3. **Quick Start: Implementing Ranked Choice via Path A** — a concrete day-by-day plan
4. **How to Add a Question Type: Step-by-Step** — the full walkthrough for Paths A + C
5. **Pattern-to-Implementation Cross-Reference** — before starting any new pattern, check this table

**For reviewers:** Use the **Implementation Checklist (CodexCLI-Specific)** near the end — every checkbox must be satisfied before merging.

**For testing:** See **Testing Strategy (Following Actual Patterns)** — follows the existing `RequestUserInputView` test pattern and VT100 snapshot harness.

### What's Accurate vs Aspirational

| Section                                                        | Status                                  |
| -------------------------------------------------------------- | --------------------------------------- |
| Design Principles, Decision Science, Interruption Science      | Universally applicable                  |
| Question Types, Non-Question Patterns, Anti-Patterns           | Design reference (not 1:1 code mapping) |
| **CodexCLI TUI Architecture (Actual State)**                   | ✅ Current code                         |
| **Rust Implementation Patterns (Using Existing Architecture)** | ✅ Current code                         |
| **How to Add a Question Type: Step-by-Step**                   | ✅ Current code                         |
| **Quick Start: Implementing Ranked Choice via Path A**         | ✅ Current code                         |
| **Priority Ranking for CodexCLI**                              | ✅ Based on current code                |
| **Pattern-to-Implementation Cross-Reference**                  | ✅ Current code                         |
| Implementation Architecture (Conceptual Model)                 | ⚠ Design intent; not 1:1 types         |
| Implementation Details by Pattern (Conceptual)                 | ⚠ Design sketches                      |

When in doubt, trust the **"Actual State"** and **"Step-by-Step"** sections. The conceptual sections describe what a fully-realized question system *could* look like, useful for understanding why an extension exists.

---

## Current State

### Already in the codebase

All built on `RequestUserInputQuestion` in [code-rs/protocol/src/request_user_input.rs](code-rs/protocol/src/request_user_input.rs):

| Pattern                             | Status         | Schema field                      |
| ----------------------------------- | -------------- | --------------------------------- |
| Single-choice (radio-style)         | ✅ implemented | `allow_multiple: false` + options |
| Freeform text input                 | ✅ implemented | `options: None`                   |
| Checkbox multi-select               | ✅ implemented | `allow_multiple: true` + options  |
| "Other" freeform alongside options  | ✅ implemented | `is_other: true`                  |
| Secret/password input (masked)      | ✅ implemented | `is_secret: true`                 |
| Multi-question progression          | ✅ implemented | `questions: Vec<...>`             |
| Mouse click selection               | ✅ implemented | `option_hit_test` in model.rs     |
| MCP auth prompt auto-handling       | ✅ implemented | `call_id: mcp_access:*`           |
| Approval modal (exec/network/patch) | ✅ implemented | `UserApprovalWidget`              |
| Approval request queueing           | ✅ implemented | `ApprovalModalView::queue`        |
| Auto Drive continue modes           | ✅ implemented | `AutoDriveContinueMode` enum      |
| Settings overlay                    | ✅ implemented | `settings_pages/` modules         |

### Patterns worth adding (ranked by impact/effort ratio)

**Tier 1 — small schema extensions (Path A):**
1. Ranked choice / drag-to-reorder
2. Required/preferred/optional tagging per option
3. Scope-of-answer (remember this for task / repo / global)
4. Confidence tagging on answers (hard req / strong pref / pref / brainstorm)

**Tier 1 — new history cells (minimal plumbing):**
5. Reversibility status indicator (replaces "Are you sure?")
6. Answer provenance / decision log

**Tier 2 — new panes (Path C):**
7. Per-item approve/reject with batch actions (extend `approval_modal`)
8. Checkpoint configuration (new pane + `AutoDriveSettings` extension)
9. Working-agreement panel (bottom pane chrome)

**Tier 3 — new tool variants (Path B):**
10. Conditional branching between questions (requires DAG semantics)
11. Disambiguation with concrete examples (requires rich option data)
12. Plan-as-artifact (editable multi-step plan — extend `proposed_plan.rs`)

---

## Design Principles

These principles come from HCI research (Nielsen, Norman) and accessibility standards. They inform every pattern.

| Principle                        | Definition                                      | Implication                                                                                             |
| -------------------------------- | ----------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| **Recognition over recall**      | Show options, don't require memory              | Every control self-describing; show context                                                             |
| **Progressive disclosure**       | Show only what's needed now                     | Hide rare options; first screen actionable                                                              |
| **Gulf of execution/evaluation** | Users must see affordances and results          | Controls suggest what they do; every action has visible feedback                                        |
| **Error prevention**             | Prevent bad input, don't clean it up            | Disable submit until valid; confirm destructive actions                                                 |
| **Chunking (4±1)**               | Working memory limit is ~4 items                | Cap visible options at 4; use search/grouping for more                                                  |
| **Escape hatches**               | Every flow needs a clear exit                   | Esc always cancels; always have "do nothing" path                                                       |
| **Feedback loops**               | Every state change gets immediate feedback      | Select → checked; save → confirmation; apply → note in history                                          |
| **Keyboard-first**               | TUI primary path is keyboard                    | All controls fully operable by keyboard                                                                 |
| **Accessible design**            | Don't use color alone; support varied abilities | Symbol + color always; no icon-only; support high-contrast                                              |
| **Mixed-initiative**             | Agent has agency *and* defers appropriately     | Ask when: ambiguous intent, risk medium+, or irreversible. Take when: clear task, low risk, reversible. |
| **Neurodiversity**               | ADHD, autism, dyslexia, fatigue are core users  | Predictable UI, explicit rules, icon+label always, minimize decisions, quiet mode available             |

**Key rules:**
- Predictability > elegance (same key always does same thing)
- Explain *why*, not just *what*
- Minimize back-and-forth questions
- Never ask the same question twice in one session

---

## Decision Science Insights

What behavioral economics and HCI research tell us about how people respond to agent
questions. Ignoring these leads to decisions that feel correct but produce bad outcomes.

| Effect                     | Observable behavior                                     | Design implication                                     |
| -------------------------- | ------------------------------------------------------- | ------------------------------------------------------ |
| **Default bias**           | ~70% of users pick default without reading              | Smart defaults matter more than perfect neutrality     |
| **Anchoring**              | First option seen strongly shapes final choice          | Put recommended option first, not middle               |
| **Cognitive load**         | > 4 chunks overwhelm working memory (Cowan, 2001)       | Cap visible options at 4; search/group for larger sets |
| **Loss aversion**          | Perceived losses feel ~2× heavier than equivalent gains | Frame as "protect X" not "add Y"                       |
| **Sunk cost**              | Users stick with first choice even when wrong           | Make answer revision effortless and visible            |
| **Contrast effect**        | Middle option appears as a safe compromise              | Don't sandwich a recommended option between extremes   |
| **Choice-supportive bias** | After deciding, users rationalize their choice          | Show consequences *after* the choice, not before       |
| **Scarcity/urgency**       | Explicit cost/impact improves decision quality          | "This will delete 12 files" > "continue?"              |

### Practical rules from this research

**Defaults:** Pick the option that is (1) most common for similar users, (2) most reversible
if tied, (3) lowest risk if still tied. Never default to the option with the highest downside.

**Framing:** Frame choices as protecting what exists, not acquiring something new.
`"Preserve test coverage"` lands better than `"add test coverage"` — same meaning, lower resistance.

**Choice set size:** Limit visible options to **3–4 max**. Use search-to-filter for larger sets.
Above 4 unordered options, users start pattern-matching instead of reading (Cowan, 2001).

**Reversibility messaging:** State reversibility explicitly before asking.
`"You can change this later"` reduces choice paralysis and produces faster, better decisions.

**Tradeoffs > feature names:** `"Fast vs. Safe"` resonates more than `"Option A vs. Option B"`.
Label the dimension being traded off, not just the options.

**Avoid "are you sure?":** Users click through 90%+ of these dialogs without reading.
Replace with a reversibility statement (see Reversibility indicator pattern below).

---

## Interruption Science and Question Batching

Research on interruption costs (Gloria Mark & Victor Gonzalez, CHI 2005) establishes:

- After an interruption, it takes an average of **~23 minutes** to fully return to a deep work state
- Interruptions at *natural breakpoints* cost ~3× less recovery than interruptions mid-flow
- External interruptions are more disruptive than self-interruptions at the same point
- Batching low-priority notifications into digests has been shown to reduce interruption overhead 10× (Courier, 2025; Horvitz et al., CSCW 2004)

**For agent design:** An agent that asks questions at arbitrary points during execution
imposes the same cost as an unexpected phone call. Questions must be batched and timed.

### The "ask once" (question batching) pattern

Rather than interrupting with each question as it arises:

1. **Accumulate** low-priority questions silently during execution
2. **Surface** the batch at the next natural breakpoint (phase complete, pre-irreversible step, idle)
3. **Ask once** using a numbered multi-part form

```
Before continuing, I need to clarify 3 things:
  1. Should I also update tests for the renamed symbols?
     [Yes] [No] [Skip tests this pass]
  2. Which files should be excluded from the refactor?
     [Select files...]
  3. Prefer to preserve existing comments or rewrite?
     [Preserve] [Rewrite where unclear]

[Answer all] [Skip optional] [Cancel]
```

The user answers the batch, then the agent resumes uninterrupted.

**Always interrupt immediately for:**
- Discovered information that makes the task impossible or fundamentally changes it
- A constraint violation (e.g., "this change would break the public API")
- Something irreversible about to happen that wasn't covered by the checkpoint config

### Question priority framework

Before deciding whether to interrupt now or queue:

| Priority    | Criterion                                  | Action                         |
| ----------- | ------------------------------------------ | ------------------------------ |
| **Blocker** | Cannot continue without this answer        | Ask immediately                |
| **High**    | Answer materially changes what to do next  | Ask at next breakpoint (< 60s) |
| **Medium**  | Affects approach or style, not correctness | Batch at end of current phase  |
| **Low**     | Preference or optimization, not blocking   | Batch at end of task           |

### Natural breakpoint detection

The system should hold non-urgent questions until a natural completion point:

```
User is mid-sentence (typing)         → queue, don't interrupt
User just approved a plan step        → good time to ask
User is watching test output stream   → wait for tests to finish
User has been idle for 30s            → acceptable to ask
Question is a blocker                 → interrupt immediately
```

### Notification priority tiers and digest

Three tiers of notification, each with a different presentation mode:

```
URGENT (blocking modal)
├── Agent needs a decision to continue
└── Error requiring user input before proceeding

NORMAL (non-blocking toast, auto-dismiss 5s)
├── Task completed
├── File written, tests passed

LOW (quarantined to digest, drain manually)
├── Minor warnings, lint suggestions
└── Optional improvements found

Status bar: [3 items in digest]    ← drain with [d]
```

When digest is drained, items appear as a mini-list with per-item actions:

```
╭── Digest (3 items, last 8 min) ─────────────────────────╮
│  • clippy: 2 warnings in auth.rs      [view] [dismiss]  │
│  • Unused import in token.rs          [fix]  [dismiss]  │
│  • CHANGELOG not updated              [update] [skip]   │
╰─────────────────────────────────────────────────────────╯
```

Key rule: **URGENT items are never quarantined.** LOW items are never shown as modals.

---

## Question Types

### 1. Single choice (radio)
**What it is:** One option from a list; selecting a new one deselects the previous.

**When to use:**
- Mutually exclusive options where exactly one applies
- Short lists (≤ 7 items)
- When order conveys preference (put recommended first)

**UX notes:**
- Auto-advance after selection is acceptable for short/certain choices
- For longer lists, require explicit confirmation (Enter) before submitting
- Show a default pre-selected if there's a clear recommended answer

---

### 2. Multi-select (checkboxes)
**What it is:** Any number of options can be selected independently.

**When to use:**
- Independent toggles with no mutual exclusivity
- "Pick everything that applies" scenarios
- Feature enablement, terminal support, platform selection

**UX notes:**
- Space to toggle, Enter to submit is the most intuitive terminal pattern
- Always show count of selected items in header (e.g., `3 selected`)
- Provide Select All / Deselect All shortcuts for long lists
- If options depend on each other (e.g., A requires B), disable dependents and explain why

---

### 3. Ranked choice / drag-to-reorder
**What it is:** User orders a list of items from most to least preferred.

**When to use:**
- Backlog triage
- Prioritizing cleanup tasks
- Choosing what to attempt first when time is limited
- Setting fallback preferences (e.g., preferred shell order)

**UX notes:**
- In a TUI, `Alt+↑` / `Alt+↓` or `J`/`K` to reorder is standard
- Show position numbers (1, 2, 3…) next to items while reordering
- Allow users to mark items as "excluded" / skip rather than forcing a rank on everything
- For long lists, start with the top 3 and let user expand

---

### 4. Pairwise comparison
**What it is:** Present two options at a time; repeat until winner emerges.

**When to use:**
- Design direction choices where global ranking is hard
- Tradeoff comparisons (approach A vs B)
- Calibrating agent behavior on ambiguous dimensions

**UX notes:**
- Show the total number of rounds up front so users know the cost
- Allow early exit with "no preference, pick for me"
- Best for ≤ 5 items (produces at most 10 pairs)

---

### 5. Required / preferred / optional / exclude tagging
**What it is:** Classify each item by priority level rather than just selecting it.

**When to use:**
- Feature scoping ("which of these must be in v1?")
- Cleanup audits ("which findings must be fixed vs nice to fix")
- Configuring what the agent focuses on

**Levels:**
- **required** — must be addressed
- **preferred** — include if feasible
- **optional** — only if time allows
- **exclude** — explicitly do not include

**UX notes:**
- Render as a 4-state toggle or small inline tag next to each item
- Distinguish visually from a plain checklist (different color or icon per level)
- Default new items to "preferred" to avoid empty states

---

### 6. Budget / tradeoff presets
**What it is:** Discrete presets (not literal sliders) for common tradeoffs.

**When to use:**
- Approach selection when multiple valid strategies exist
- Telling the agent how conservative or aggressive to be

**Examples:**
```
Prioritize:  [ Safety ] [ Balanced ] [ Speed ]
Scope:       [ Minimal ] [ Targeted ] [ Thorough ]
Risk:        [ Conservative ] [ Standard ] [ Aggressive ]
```

**UX notes:**
- Presets are more usable than literal sliders in a TUI
- Show a one-sentence description of each preset when highlighted
- Allow combining dimensions if they're truly independent (e.g., safety=high + speed=high is meaningful)

---

### 7. Matrix / cross-tab questions
**What it is:** Rows × columns grid of toggles or selections.

**When to use:**
- Features by shell/platform/environment
- Styles by terminal type
- Settings that vary per-context

**UX notes:**
- Keep matrices small: ≤ 6 rows × 5 cols before it becomes a table editor
- Allow row/column-level bulk toggle ("enable for all", "disable for row")
- Always show row and column labels clearly — never assume the user will remember what rows mean
- Keyboard: arrow keys to move, Space to toggle, `r` to toggle entire row, `c` for column

---

### 8. Conditional branching
**What it is:** Later questions appear only when earlier answers make them relevant.

**When to use:**
- Multi-step configuration (select runtime → configure runtime-specific options)
- Approval flows (approve? → if no: why? → optional: suggest alternative)
- Expert mode options hidden until user indicates familiarity

**UX notes:**
- Animate transitions when questions appear/disappear — sudden content jumps are disorienting
- Show a breadcrumb or step count so users know where they are
- Allow going back to revise earlier answers (re-evaluates branching)
- Mark conditional questions visually ("this only applies if X")

---

### 9. Fill-in templates
**What it is:** Pre-structured form with labeled fields and optional inline descriptions.

**When to use:**
- Bug reports (title, repro, expected, actual, env)
- Acceptance criteria for features
- Rollout / migration plans
- Agent task scoping (goal, constraints, out-of-scope)

**UX notes:**
- Show field names and format hints before the cursor
- Required vs optional fields should be visually distinct
- Allow partial saves — agent should be able to work with incomplete templates
- Avoid deep nesting (max 2 levels of sub-fields)

---

### 10. Constraint capture
**What it is:** Ask for limits and boundaries, not just choices.

**When to use:**
- Before starting a large refactor or cleanup pass
- When risk appetite is unknown
- When the user has hard technical or time constraints

**Examples:**
- no schema changes
- no new external dependencies
- don't modify test files
- stay within N files
- avoid files in these paths

**UX notes:**
- Make it easy to add new constraints as tags or short phrases
- Allow saving a constraint set for reuse ("use my standard constraints")
- Show active constraints persistently during the task

---

### 11. Scope-of-answer / remember-this prompts
**What it is:** After an answer, ask how long it should apply.

**Levels:**
- this answer only
- this task / session
- this repository
- globally / all future sessions

**When to use:**
- Style, convention, or preference questions
- After a conflict is resolved to prevent recurrence
- When the agent is about to override a default

**UX notes:**
- Default to "this task" — never silently persist to global without confirmation
- Show a small badge or tag on remembered preferences where they affect behavior
- Make revocation easy (one command or keypress)

---

### 12. Confidence and intent capture
**What it is:** Let the user qualify their answer.

**Levels:**
- **hard requirement** — do not deviate
- **strong preference** — default to this, ask before changing
- **preference** — use this if it doesn't cause problems
- **brainstorming** — rough idea, agent should sanity-check before acting on it

**When to use:**
- Any time the user gives a vague or exploratory answer
- Before the agent acts on something irreversible
- When the instruction is ambiguous ("make it better")

**UX notes:**
- Show qualification options as a small inline tag picker, not a full separate question
- Default to "preference" for normal answers
- When confidence = brainstorming, agent should surface a summary for confirmation before acting

---

### 13. Exception / carve-out prompts
**What it is:** Pair a general rule with explicit exceptions.

**Examples:**
- prefer zsh everywhere *except* CI scripts
- use compact labels *unless* accessibility mode is on
- run tests automatically *except* integration tests

**When to use:**
- When the user gives a rule that they know has edge cases
- After a conflict between a general preference and a specific context

**UX notes:**
- Render as a rule + exceptions list
- Allow adding/removing exceptions without re-stating the whole rule
- Highlight exceptions inline where they're active

---

### 14. Checkpoint configuration
**What it is:** Let the user define where the agent continues automatically and where it stops to ask.

**When to use:**
- Before long autonomous runs (Auto Drive)
- When user wants to stay in the loop on specific action types
- When risk levels vary across action categories

**Example:**
```
Continue automatically through:  [x] file reads  [x] tests  [ ] config edits  [ ] deletions
Ask before:  [ ] any file write  [x] schema changes  [x] new dependencies
```

**UX notes:**
- Show this as a grid, not a long list of checkboxes
- Allow presets ("careful", "normal", "autonomous")
- Show the current checkpoint profile in the status bar during a run

---

### 15. Disambiguation and ambiguity flagging
**What it is:** Agent surfaces assumptions it's making and asks for correction.

**Pattern:** "I interpreted your request as X. Is that right?"
- **Yes** → continue
- **No** → let user correct the interpretation
- **Partially** → let user refine with freeform or narrowed choices

**Agent confidence signaling:**
- **High confidence** — proceed with note ("I'm treating this as a hard requirement")
- **Medium confidence** — show interpretation, one-click confirm ("I read this as X — proceed?")
- **Low confidence** — surface 2–3 interpretations, ask user to choose

```
I have low confidence about this instruction:
  "make it better"

I could mean:
  1. Refactor for readability (add names, split long functions)
  2. Optimize for performance (profile and tune hot paths)
  3. Both (if scope allows)

Which did you mean?  [Readability] [Performance] [Both] [Other...]
```

**When to use:**
- When the user's instruction was ambiguous or very general
- Before irreversible actions
- When multiple valid interpretations exist with meaningfully different outcomes

**When NOT to use:**
- Routine instructions that have clear standard interpretations
- When the ambiguity doesn't materially affect the outcome
- Don't ask for disambiguation on every step — alert fatigue erodes trust

**UX notes:**
- Show 2–3 concrete examples of what each interpretation would produce
- Offer an "Other" path so the user isn't forced into pre-built choices
- Log what interpretation was chosen in answer provenance

---

### 16. Typeahead / fuzzy search selection
**What it is:** Free-text input that filters a list in real time (fzf-style).

**When to use:**
- Choosing from large sets (tools, files, models, agents, skills)
- Command palette-style interactions
- Searching for settings by name
- Any selection from > 10 items

**How it should work:**
```
> [user types here]
  ┌──────────────────────────────────────────┐
  │ ● Option A  (match highlighted)          │
  │ ○ Option B                               │
  │ ○ Option C  (also matches)               │
  │ ...                                      │
  │ 47 / 234 items shown                     │
  └──────────────────────────────────────────┘
  j/k to move  Space to select  Enter to confirm  ? help
```

**UX notes:**
- Filter live as-you-type, no delay
- Highlight matched substrings in results (not just the first character)
- Support fuzzy matching (tolerate typos and out-of-order terms)
- Show match count ("47 / 234") so users know if their filter is too narrow
- Offer recent/frequently-used items even before the user types
- Never auto-select the first result — let users see the list first

---

### 17. Per-item approve/reject with batch actions
**What it is:** Checklist of agent-proposed items with accept, reject, and edit per item.

**When to use:**
- Plan steps review before execution
- Code review finding triage
- Audit findings acceptance
- File selection before refactoring

**Batch actions:**
- Accept all
- Reject all
- Accept all low-risk
- Group by file or category

**UX notes:**
- Show impact or risk label per item (low / medium / high)
- Keyboard: J/K to navigate, `a` to accept, `r` to reject, `e` to edit, `A`/`R` for batch
- After batch action, allow undoing the batch as a single operation
- Show a summary count (e.g., "12 accepted, 3 rejected, 2 to review") at the top

---

## Non-Question Interaction Patterns

### Decision cards
A compact card presenting one proposed action with:
- what the agent wants to do (one sentence)
- why it chose this (brief rationale)
- risk level (low/medium/high)
- alternative it didn't pick and why

**When to use:**
- Single high-stakes decisions
- When the agent is choosing between two clearly different paths

**UX notes:**
- Card should fit in ~6 terminal lines
- Primary action on Enter, secondary (pick alternative) on Tab, cancel on Esc
- Don't use cards for routine confirmations — they create alert fatigue

---

### Side-by-side comparison
Show two or more options in parallel columns with labeled attributes:

```
                      Option A        Option B
  Approach:           Incremental     Big-bang rewrite
  Risk:               Low             High
  Time estimate:      2h              8h
  Breaking changes:   No              Yes
  Reversible:         Yes             No
```

**When to use:**
- Architecture or approach decisions
- Choosing between implementations
- Hard scope cuts

**UX notes:**
- Keep rows to ≤ 8 attributes before truncating to most important
- Highlight the winning attribute in each row (or let the user weight them)
- Support keyboard selection: `1` / `2` / `3` for each option

---

### Multi-step wizard
A guided sequence of screens, each covering one logical sub-task.

**When to use:**
- Onboarding / first-time configuration
- Complex setup that needs ordering (e.g., pick runtime → configure paths → set defaults)
- Creating something that has many parts (shell style, agent profile, etc.)

**UX notes:**
- Show step count and current position (Step 2 of 5)
- Allow going back without losing later answers unless they depend on changed data
- Allow skipping optional steps
- Show a summary of all answers before the final submit

---

### Inline progress with interrupt
Show real-time progress for long-running agent actions with clear interrupt controls.

**Controls:**
- `Ctrl+C` → stop
- `p` → pause (continue later)
- `s` → skip current step

**Display:**
- Current action (one line)
- Items done / total
- Elapsed time
- Can toggle detail view for verbose output

**UX notes:**
- Never make the user feel trapped in a long operation
- Paused state should persist across sessions if possible
- Show what was already done so user knows what to undo if they cancel

---

### Contextual help (expandable inline)
Instead of separate help screens, show expandable `?` or `[?]` markers next to
options that need explanation.

**Pattern:**
```
  [x] Enable fused labels  [?]
      ↳ Shows shortcut letters in color merged into label text, e.g. [S]ave
```

**UX notes:**
- Expand on `?` key or click
- Collapse on second press or Esc
- Help text should be ≤ 4 lines — link to docs for more
- Don't show help by default — show indicator that it exists

---

### Preview before apply
Show exactly what will change before executing.

**Levels of preview:**
1. **Summary** — "4 files will be changed, 2 deleted"
2. **File list** — list of affected paths
3. **Full diff** — line-by-line changes

**UX notes:**
- Default to summary; allow drilling to full diff on demand
- Diff should use color where available, character-based fallback where not
- Make it easy to approve from the preview (don't require navigating back)
- Show irreversible steps prominently ("⚠ This deletion cannot be undone")

---

### Editable answer summary
Before final submit on a multi-question form, show all collected answers and allow
any of them to be edited in place.

**UX notes:**
- Show question + answer on one line each where possible
- Navigate with arrow keys or direct selection
- Pressing Enter on an answer reopens that specific question
- Show which answers are still at their default vs explicitly set

---

### Answer provenance / decision log
Show a persistent or on-demand log of decisions the agent is currently honoring.

**Example display:**
```
Active decisions:
  [task]  Avoid schema changes            set 14m ago
  [repo]  Use zsh for all shell examples  set 2d ago
  [task]  Skip integration tests          set this session
```

**UX notes:**
- Show inline in history where a decision was applied
- Allow quick revoke from the log view
- Distinguish task-scoped vs repo-scoped vs global visually (color or icon)

---

### Re-open / revise last question
A command (e.g., `/revise`) that reopens the most recent agent question for correction.

**When to use:**
- User made a selection too quickly
- User changed their mind
- Answer was misunderstood

**UX notes:**
- Pre-fill with the previous answer so user can tweak rather than restart
- Show what the agent already did based on the old answer (so user knows what changed)
- For branched flows, re-evaluating an early question may invalidate later ones — warn the user

---

### Working-agreement panel
A persistent status display showing the current task contract:
- autonomy level
- active constraints
- checkpoint configuration
- persistent preferences in effect

**When to use:**
- During Auto Drive runs
- After complex setup where many preferences were set
- Whenever the user might want to audit what the agent is honoring

**UX notes:**
- Show as a collapsible sidebar or togglable overlay (`?` or `/status`)
- Keep it compact — one line per active rule
- Highlight rules that are overriding a default differently from rules that are setting a new preference

---

### Reversibility status indicator

Replace "Are you sure?" with a statement about what will happen and whether it is reversible.

**Three states:**
```
✓ Reversible — undo with C-z or revert command
⚠ Reversible this session only — cannot recover after restart
✗ Permanent — cannot be undone
```

**Examples:**
```
❌  "Delete file? Are you sure? [Yes] [No]"

✓   "file.ts will be moved to trash  (Undo: C-z)    [Delete] [Cancel]"
✓   "⚠ Permanent — no backup exists.  [Delete] [Move to trash instead] [Cancel]"
✓   "12 files will be deleted. View list?    [Proceed] [Review files] [Cancel]"
```

This works because it gives information instead of asking the user to re-state what they already said.

---

### Undo/redo as substitute for confirmation

If an action is reversible and cheap to undo, skip the confirmation — act immediately
and make undoing easy. This is better UX for confident users without removing the safe path.

**Where this works:**
- File changes (trash or git backup)
- Config changes (prior value stored)
- Plan edits or annotation changes

**Where this breaks:**
- Network calls, deploys, real sends
- Production actions with no backup
- Deletions with no trash/git safety net

**Pattern:**
```
Refactored 14 functions in 3 files.   [Undo this action]
```
Show what changed, keep the undo button visible for at least 10 seconds.

---

### Optimistic UI

Show the expected result immediately, complete the operation in the background.

**Works for:** approval submissions, preference saves, non-destructive file writes.
**Does not work for:** operations with genuine uncertainty (compilation, API calls, deploys).

**Pattern:**
- User presses `[Approve]` → immediately show `✓ Approved` (optimistic)
- Background: confirm the operation
- If failure: revert status, show toast `"Save failed — retrying…"`

Reduces perceived latency for fast operations, eliminates visible waiting for
actions users are already confident about.

---

### Toast vs blocking modal decision tree

```
Does this require user input?
  → YES  → Use modal (blocks)
  → NO   → continue…

Did the user cause this situation?
  → YES  → toast (persistent until dismissed)
  → NO   → continue…

Is this time-sensitive (goes stale in <10s)?
  → YES  → auto-dismiss toast (5s)
  → NO   → persistent toast (manual dismiss)
```

**Examples:**
```
✗ Config file not found.            (5s auto-dismiss)
✓ Refactoring complete: 47 changes. [View changes] [Dismiss]  (persistent)
⚠ This change affects 3 tests.     [OK] [Show tests]          (blocking until acknowledged)
```

Key rule: **never auto-dismiss error toasts.** Users read errors on their own schedule.

---

### Plan-as-artifact

Before executing a multi-step task, the agent produces a *plan* the user can inspect and
edit — not just a progress bar, but an ordered, editable action list:

```
╭──── Plan: Refactor auth module ──────────────────── [Edit] [Run] ╮
│  1. ✎ Extract TokenManager from auth.rs            [edit]        │
│  2. ✎ Update 3 call sites                          [edit]        │
│  3. ✎ Add unit tests for TokenManager              [skip]        │
│  4. ✎ Update CHANGELOG.md                          [skip]        │
│                                                                  │
│  Est. 4 files changed, ~120 lines    [Proceed ↵] [Cancel ESC]    │
╰──────────────────────────────────────────────────────────────────╯
```

**When to use:**
- Before any task touching more than 3 files or 2+ phases
- Before autonomous runs (Auto Drive)
- Any multi-step operation the user hasn't seen before

**UX notes:**
- Users can re-order steps with `Alt+↑` / `Alt+↓`
- `[skip]` marks a step as skipped without removing it from the plan (still visible, greyed)
- Show estimated scope (file count, line changes) at the bottom
- The plan is an artifact — it can be saved, resumed, and shared

---

### Inline action audit trail

Every agent-initiated action is logged in the history with a time-limited undo affordance:

```
  ✓ Modified: src/auth.rs  (+12, -3)  [undo 3m]
  ✓ Created:  tests/auth_test.rs      [undo 3m]
  ✓ Ran:      cargo test              [view output]
  ✗ Failed:   clippy (2 warnings)     [view] [fix automatically]
```

The `[undo Xm]` label greys out when the action becomes irreversible (e.g., after a push,
or when the file has been modified by something else). The timer is the user's signal
that the undo window is closing.

**When to use:** Always. This is the minimal transparency layer for any agent that
writes files, runs commands, or makes changes.

---

### Task timeline / long-running progress

For operations lasting more than ~10 seconds, a spinner is insufficient. Show milestones:

```
╭──── Running: Full test suite ──────────────────────────────────╮
│                                                                │
│  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 34% (2:14 / ~6min)        │
│                                                                │
│  ✓ 0:00  Compiled (dev)                                        │
│  ✓ 1:12  unit tests: 247/247 passed                            │
│  ▶ 2:14  integration tests: 12/56...                           │
│  ○ ~4:00  e2e tests                                            │
│  ○ ~5:30  benchmarks                                 [skip s]  │
│                                                                │
│  [Stop q]  [Skip next phase s]  [Background b]                 │
╰────────────────────────────────────────────────────────────────╯
```

**Key elements:**
- Named milestone labels, not just percentages — easier to re-engage after a distraction
- Estimated remaining time alongside elapsed time
- Skip affordances for optional later phases
- `[Background b]` to detach to background so the user can continue chatting

**Background mode:**
```
  [b] Sent to background: "full test suite" (ETA: ~4min)

  ≫ You can continue chatting. I'll notify you when done.

  Status bar: [1 running: tests 34%]
```

---

### Task resumption and context re-entry

When a session resumes after a break (restart, sleep, multi-day gap), the agent
should offer an explicit re-entry prompt rather than silently dropping context:

```
╭──── Resuming: "Refactor payment module" ────────────────────────╮
│  Started: 2h ago (paused when terminal closed)                  │
│                                                                 │
│  Completed:                                                     │
│    ✓ Extracted PaymentProcessor (src/payment.rs:120)            │
│    ✓ Updated 4 call sites                                       │
│                                                                 │
│  Remaining:                                                     │
│    ○ Write tests (est. 3 files)                                 │
│    ○ Update docs                                                │
│                                                                 │
│  [Resume ↵]  [Review completed first r]  [Start fresh a]        │
╰─────────────────────────────────────────────────────────────────╯
```

A `[See what changed]` option shows files changed externally since last session,
new commits, or any drift from expected state — so the user can decide before committing.

**Design rules:**
- Never silently assume context is still valid — a week may have passed
- Always show what was in progress before asking "continue?"
- If state is ambiguous or context has expired, say so explicitly

---

### Agent narration and think-aloud

The agent should narrate significant reasoning *before* acting on it.
This closes the "gulf of evaluation" — users can see not just *what* the agent did but *why*.

**Levels of narration:**

| Level         | What to narrate                                              |
| ------------- | ------------------------------------------------------------ |
| **Decisions** | "I'm choosing to refactor X before Y because Y depends on X" |
| **Tradeoffs** | "This approach is faster but less safe — defaulting to safe" |
| **Surprises** | "I found something unexpected: [X]. This changes my plan."   |
| **Skips**     | "I'm skipping Z because you said to avoid test files"        |
| **Limits**    | "I'm stopping here — reached the constraint you set"         |

**Implementation:** Reasoning narration is collapsed by default and expandable on demand
(press `r` or `[▸ details]`). This respects focus mode without hiding the capability:

```
  ▶ Working on token.rs...  [▸ show reasoning]

  Expanded:
  ┌ Thinking ─────────────────────────────────────────────────────
  │ Reading auth.rs... found Token struct at line 42
  │ Deciding: mockall is better here (no network in auth tests)
  │ Plan: add 3 test cases for happy/error/expired paths
  └────────────────────────────────────────────────────────────────
```

**`/explain` command** — at any point, the user can type `/explain` to surface:
- what the agent was trying to accomplish with the last action
- what alternative it considered and rejected
- what constraints it is currently honoring

**Context chips** — a compact tag strip below active output showing what the agent
is tracking:

```
considering: src/auth.ts  src/session.ts  │  constraint: no new deps  │  preference: zsh only
```

These update as context shifts and disappear when the task completes.

---

### Pre-task expectation setting

Before a long or ambiguous task, the agent explicitly states what it will do,
what it won't do, and where uncertainty exists:

```
╭──── Before I start: what to expect ───────────────────────────╮
│  I'll refactor the auth module. Here's what I know:           │
│                                                               │
│  ✓ Confident: extracting TokenManager is safe                 │
│  ? Uncertain: 2 call sites in legacy/ may need manual review  │
│    (I'll flag them for you)                                   │
│  ✗ Won't do: change the public API surface (you said so)      │
│                                                               │
│  Estimated: 8 files, ~15 min                                  │
│  [OK ↵]  [Change scope e]  [Cancel ESC]                       │
╰───────────────────────────────────────────────────────────────╯
```

This addresses a critical gap: only ~13% of agentic systems provide any decision
explanation before acting (MIT AI Agent Index, 2025). Setting expectations upfront
lets users catch scope mismatches before the work begins, not after.

---

### Post-task decision log

After completion, show a brief "what I decided and why" summary, distinct from the
action audit trail:

```
── Completed: auth refactor ──────────────────────────────────────

  Key decisions I made:
  • Used mockall (not wiremock) — no HTTP in auth tests
  • Kept original error types — 6 downstream users exist
  • Skipped legacy/compat.rs — too risky, flagged for you

  3 items flagged for your review: [show]
```

**When to use:** After any task with significant autonomous decision-making. Especially
valuable for long Auto Drive runs where the user wasn't watching.

---

### Multi-agent delegation display

When the primary agent delegates to sub-agents, this should be visible — not opaque.

**Delegation card:**
```
Delegating 3 sub-tasks to parallel agents:
  ● [running]  Explore authentication patterns
  ● [running]  Analyze test coverage gaps
  ✓ [done]     List all TODO comments           (2s)
```

**Delegation confirmation (when sub-agent needs context):**
```
Delegating to: Test Writer
Task: "Write unit tests for TokenManager"
Context passed: ✓ auth.rs, ✓ token.rs, ✓ your mockall preference

[Watch] [Background] [Cancel sub-task]
```

The "Context passed" line is critical — it answers "does this sub-agent know what
I already told the orchestrator?"

**Cross-agent preference propagation:** When a preference is applied to a new agent
context, acknowledge it explicitly:
```
── Note: applying your preference ──────────────────────────────
Test Writer will use mockall (you chose this 2 tasks ago)
[Change for this task]
```

**Escalation from sub-agent:**
```
⚠ Sub-agent (authentication) needs input:
  "Found two conflicting auth schemes. Which should be canonical?"
  [Answer now]  [Let orchestrator decide]  [Pause all sub-agents]
```

---

### Classified error communication

Errors should be visually classified so users immediately know the recovery type.
Every error needs two parts: *what went wrong* and *what to do about it*.

**Error taxonomy:**

| Error type          | Examples                                         | Recovery pattern                  |
| ------------------- | ------------------------------------------------ | --------------------------------- |
| **User input**      | Ambiguous prompt, wrong path                     | Disambiguation, inline correction |
| **Agent reasoning** | Wrong approach, hallucinated API                 | Rollback + re-plan                |
| **Environment**     | Network down, file locked, no permissions        | Retry with backoff, workaround    |
| **Ambiguity**       | Agent couldn't choose between 2 valid approaches | Present the fork, ask             |
| **Scope creep**     | Agent did more than asked                        | Show delta, offer rollback        |

**Classified error cards:**
```
╭── ⚠ Ambiguity error ───────────────────────────────────────────╮
│  I found two valid approaches and can't choose without you.    │
│                                                                │
│  What to do: add error handling to parse()                     │
│  Option A: Return Result<T, ParseError>                        │
│  Option B: panic!() with message (matches existing codebase)   │
│                                                                │
│  [Choose A ↵]  [Choose B]  [Let me handle it e]                │
╰────────────────────────────────────────────────────────────────╯

╭── ✗ Environment error ──────────────────────────────────────────╮
│  Cannot write: src/auth.rs is locked by another process         │
│                                                                 │
│  What happened: File lock detected (PID 12847)                  │
│  What to do:   Close the other editor, then retry               │
│                                                                 │
│  [Retry r]  [Write to .rs.new instead n]  [Cancel ESC]          │
╰─────────────────────────────────────────────────────────────────╯
```

**Scope creep diff** — when the agent did more than requested:
```
╭── ⚠ I did more than requested ──────────────────────────────────╮
│  You asked: "add unit tests for TokenManager"                   │
│  I also:   refactored token.rs (seemed related)                 │
│                                                                 │
│  Extra changes: src/token.rs  +23 -11                           │
│  [Keep all ↵]  [Revert extras r]  [Review first v]              │
╰─────────────────────────────────────────────────────────────────╯
```

---

### Streaming output state machine

Agent output passes through four distinct states. Each needs different rendering.

```
THINKING   → agent processing; pulsing spinner; no output yet
DOING      → actions happening; real-time file/command updates; interruptible
REVIEWING  → complete output; stable display; user approval affordance
DONE       → compact summary; timestamp; cursor back in input
```

**Differentiation rules:**
- **Thinking vs Doing**: use different animations. Thinking: `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` (pulsing). Doing: live file/line counter updating.
- **Code blocks**: stream reasoning prose token-by-token; *hold* code blocks until complete to prevent misreading while half-written. Show `[generating… ██████░░░░]` during generation.
- **Interrupt gracefully**: when user presses Esc mid-stream, commit the partial output with a marker rather than discarding it:

```
─── [interrupted by user at 60%] ───────────────────────────────

  [partial output visible here]

── Interrupted. What would you like? ───────────────────────────
[Continue from here ↵]  [Restart with changes e]  [Discard r]
```

**`s` to skip**: skip remaining output without canceling the task. Shows `[output truncated]`.
**Esc**: cancel the task entirely. These must never be the same key.

---

## Preference and Memory Patterns

### Preference scoping
Every preference or setting should be answerable at one of these levels:

| Scope       | Lifetime                           |
| ----------- | ---------------------------------- |
| this answer | affects only the current response  |
| this task   | persists for the current session   |
| this repo   | persists in `.code/` or equivalent |
| global      | persists across all repos          |

**UX notes:**
- Never default to global — it's the most dangerous scope
- Show scope tag wherever a preference is applied
- Group preferences by scope in the review/revoke UI

---

### Preference conflict resolution
When a new answer conflicts with an older preference, show a conflict card:

```
You said: "use compact labels"
This conflicts with an earlier preference: "disable fused labels for accessibility"

[ Replace old ]  [ Override just for this task ]  [ Keep old, ignore new ]
```

**UX notes:**
- Show both the old and new preference in full
- Explain *where* the conflict will surface (which settings, which views)
- Log the resolution choice in answer provenance

---

### Preference review and expiration
Provide a `/preferences` command or settings panel that lists all active preferences:
- what the preference is
- its scope
- when it was set
- where it has been applied

Allow the user to:
- revoke any preference
- narrow its scope (global → repo → task)
- set an expiration (e.g., "expire after this branch is merged")

---

## UX Anti-Patterns to Avoid

### "Are you sure?"
Asking "Are you sure?" with no context adds friction without information. Users
click through 90%+ of these dialogs without reading.

**Replace with:** a reversibility statement + meaningful alternatives:
```
❌  "Delete file? Are you sure? [Yes] [No]"
✓   "file.ts will be moved to trash.  [Delete] [Cancel]    (Undo: C-z)"
✓   "⚠ Permanent — no backup.  [Delete] [Move to trash instead]"
```

### Alert fatigue
Asking for approval or confirmation on every small action trains users to dismiss
without reading. Reserve confirmation for genuinely high-risk or irreversible actions.
Use the reversibility indicator instead.

### Mega-forms
Showing 10+ questions at once overwhelms users and leads to shallow, rushed answers.
Use progressive disclosure: show the first 3, reveal more as needed.

### Invisible defaults
When the agent uses a default, it should say so. `"I used your repo preference for
compact labels"` is better than silently applying it.

### Unreachable cancellation
Every flow must have a visible escape hatch. Esc must always work. Users should
never feel trapped in a flow with no exit.

### Confirmation theater
Asking "confirm?" with no context about what will happen. Confirmations should
always state the consequence, scope, and reversibility.

### State amnesia
The agent should not ask the same question twice in one session. Track what has
already been decided and only ask for re-confirmation when circumstances materially change.

### Option overload
More than 7 unordered options without grouping forces users to scan the full list
every time. Use fuzzy search, grouping, or pagination.

### Hidden freeform
When a question has options but the user's answer is "none of these," there must be
a visible "Other / custom" path. Never hide it.

### Color-only signaling
Never use color as the sole differentiator between states. Always pair color with
a symbol or label. Assume ~8% of users have some form of color blindness.

### Optimistic over-confidence
Don't show `✓ Done` before the operation has actually completed unless the failure
path is handled (revert + notification). Optimistic UI works when failure is rare and
recoverable; not for deploy/send/delete operations.

### First-keystroke validation
Showing validation errors immediately on the first keystroke (before the user has
finished typing) feels aggressive. Validate on blur or after a 500ms pause.

### Stale working-agreement
If the agent's active constraints/preferences haven't been reviewed in a while,
mention it. Stale working agreements are almost as harmful as missing ones.

### Mid-flow interrogation
Asking questions in the middle of an ongoing operation forces context-switches and
breaks the user's flow. Batch questions to natural breakpoints (see Interruption
Science section). Never ask a "by the way" question while something is running.

### Silent agent reasoning
When the agent makes a significant decision — choosing one approach over another,
skipping something, or changing course — it must narrate this choice. Silently
implementing a different approach from what the user described is a trust-breaker.
**Pattern:** "I chose X instead of Y because [brief reason]."

### Error without recovery action
Errors that state only what went wrong — without saying what to do about it — leave
users stranded. Every error needs: (1) what went wrong, and (2) what to do about it.

```
❌  "Error: config not found"
✓   "Config file not found at ~/.code/config.toml
     Run `code init` to create one, or copy config.toml.example to get started."
```

Never blame the user for environment or system errors. Reserve second-person blame
(`"you provided an invalid path"`) only for genuine user input errors.

### Context silence on resumption
Starting a new response as if nothing happened after a session break. Users who
return after a pause expect acknowledgment that things may have changed. Never
assume context is fresh unless the session was continuous.

### Non-stop talker
Narrating every token of reasoning unprompted. Raw chain-of-thought output overwhelms
users who don't need it. **Fix:** collapse reasoning by default; show a `[▸ details]`
affordance for users who want to expand it.

### Synchronous blocker
Long tasks that block the entire interface with no escape or background option.
Any operation expected to take > 10 seconds should offer `[Background b]`.

### Opaque sub-agent
Orchestrator delegates to sub-agents silently; user has no idea what is happening
or why there is latency. Delegation should be visible (see multi-agent delegation
display pattern). Context passed to sub-agents must be visible too.

### Partial code streaming
Streaming code blocks token-by-token creates misleading partial reads — users start
reading incomplete code. Hold code blocks until complete; stream only prose.

---

## TUI/Terminal-Specific Considerations

### Keyboard shortcut reference

Standard TUI patterns that feel natural (mnemonic beats arbitrary — always):

| Key                    | Meaning                            | Source convention   |
| ---------------------- | ---------------------------------- | ------------------- |
| `j` / `k` or `↑` / `↓` | navigate list                      | vim, k9s, lazygit   |
| `Space`                | toggle selection                   | fzf, inquirer       |
| `Enter`                | confirm / submit                   | universal           |
| `Esc`                  | cancel or go back                  | universal           |
| `Tab`                  | cycle sections or secondary action | readline, TUI forms |
| `/` or `C-f`           | enter filter/search mode           | vim, fzf            |
| `?`                    | inline help                        | lazygit, k9s        |
| `a` / `A`              | accept / accept-all                | approve flows       |
| `r` / `R`              | reject / reject-all                | review flows        |
| `e`                    | edit selected item                 | lazygit             |
| `d`                    | delete                             | lazygit, vim        |
| `u`                    | undo                               | vim, C-z fallback   |
| `C-a`                  | select all                         | readline            |
| `C-n` / `C-p`          | next/previous                      | readline            |
| `C-c`                  | hard cancel / exit                 | POSIX universal     |
| `C-z`                  | undo                               | POSIX universal     |
| `C-s`                  | save                               | nano, editors       |
| `C-h`                  | help (alternative to ?)            | readline            |
| `Alt+↑` / `Alt+↓`      | reorder item in list               | ranked choice       |
| `1`–`9`                | quick-access nth item              | numeric shortcuts   |
| `:`                    | command palette                    | vim                 |

**Discoverability rule:** Show a compact hint line after every interactive control.
Always show `[?] help` or equivalent. Don't document all shortcuts — just hint that they exist.

### Color and symbol usage

Always pair color with a symbol. Never use color as the only differentiator.

| Meaning             | Color         | Symbol       | Fallback (no color) |
| ------------------- | ------------- | ------------ | ------------------- |
| Selected            | green or blue | `[x]` or `●` | `[x]`               |
| Unselected          | dim           | `[ ]` or `○` | `[ ]`               |
| Required            | bold/yellow   | `*`          | `*`                 |
| Warning             | yellow        | `⚠`         | `[!]`               |
| Error               | red           | `✗`         | `[X]`               |
| Success             | green         | `✓`         | `[OK]`              |
| Info                | blue/dim      | `ℹ`          | `[i]`               |
| Permanent/dangerous | red bold      | `✗`         | `[!!!]`             |

### Compact vs verbose modes
Short-form controls for power users; full descriptions for first-time or
infrequent flows. Controlled by a density setting.

### Split views
For before/after comparisons or preview-before-apply, use horizontal splits where
terminal width allows (≥ 120 cols). Fall back to sequential above/below for narrower
terminals.

### Scrollable lists with pinned context
When a list requires scrolling, pin the question text and selection count to the
top. Users lose context when the prompt scrolls off screen.

### Status bar
A persistent one-line status bar at the bottom of the TUI showing:
- current mode / state
- active constraints or autonomy level
- any paused actions

### Multi-line and paste input

Multi-line input is common in agent interactions (pastes, code snippets, long prompts)
and needs explicit handling separate from single-line message entry.

**Bracketed paste mode:**
Modern terminals send escape codes around pasted text to distinguish paste from typing
(`\e[?2004h` / `\e[?2004l`; Ratatui/crossterm exposes this as a `Paste(String)` event).
Use this to:
- Expand the input to multi-line mode automatically on paste
- Show `Pasted N lines` indicator above the expanded input
- Prevent immediate submission on pasted newlines

**Multi-line composer behavior:**
```
╭── Message ──────────────────────────────── [Shift+↵ = newline] ─╮
│ Refactor the auth module so that Token is a                     │
│ first-class struct with its own file, and add                   │
│ tests for expired/invalid cases.                 [↵ Send]       │
╰─────────────────────────────────────────────────────────────────╯
```

- `Enter` → submit in single-line mode
- `Shift+Enter` (or `Ctrl+Enter`) → newline without submit
- In multi-line mode: `Enter` always newline; `Ctrl+Enter` submits
- `Ctrl+Z` collapses the entire paste as a single undo action

**Paste intent detection:**
When a large paste is detected (via bracketed paste event), offer intent clarification
before acting:

```
╭── Paste detected (847 chars) ───────────────────────────────────╮
│  Looks like: Rust code (fn main, use std)                       │
│                                                                 │
│  Treat as:                                                      │
│  [a] Code to analyze / review                                   │
│  [b] Code to include in context                                 │
│  [c] Code to replace current file                               │
│  [d] Just plain text                              [ESC cancel]  │
╰─────────────────────────────────────────────────────────────────╯
```

**Code block detection:**
When pasted text starts with ` ``` `, treat as a code snippet and apply syntax
highlighting in the input preview.

### File/path input with completion

When the agent asks for a file path, provide inline completion rather than freeform input:

```
  Which file should I write tests for?

  > src/auth[TAB]
        auth.rs ←
        auth/
        auth_middleware.rs
```

Show recent files and frequently-referenced paths as suggestions before the user types.

### Structured rich output

Where agent output is inherently structured (file lists, test results, diff stats),
render it as a navigable TUI widget rather than raw text:

```
  ┌─ Files changed (↑↓ navigate, ↵ open) ─────────────────────────┐
  │ ▶ src/auth.rs           +45 -12  ████░░░░░  [unstaged]        │
  │   src/auth/token.rs     +89  -0  ██████░░░  [new]             │
  │   tests/auth_test.rs    +134 -0  ████████░  [new]             │
  └────────────────────────────────────────────────────────────────┘
```

This is better than dumping raw `git diff --stat` into the chat stream — users can
navigate and act on items directly.

### Focus mode

A mode that strips all chrome and suppresses non-critical output for users who need
sustained concentration (ADHD, deep work sessions, low-energy states):

- Only current task visible
- No background task updates (queued to digest)
- No ambient status indicators  
- Single primary affordance per screen
- Esc always exits focus mode; never traps

Toggle with a dedicated key (e.g., `F` or `/focus`) visible in normal mode.
Explicit state label on entry: `── Entered: focus mode ──`.

### Explicit state announcements

Every mode change should be announced explicitly, never implied. Users (especially
those with autism-spectrum traits or ADHD) should never have to guess what mode they're in:

```
── Entered: plan review mode ─────────────────────────────────────
You are reviewing a proposed plan. Nothing has changed yet.
Press ↵ to approve, e to edit, ESC to cancel.
──────────────────────────────────────────────────────────────────
```

This applies to: review mode, focus mode, edit mode, approval mode, comparison mode.
The current state label should always be visible in the status bar.

---

## CodexCLI TUI Architecture (Actual State)

Before implementing questions, understand that the codebase **already has a rich question system**.
Any new patterns should extend this system rather than build from scratch.

### Existing Question Protocol

The current question schema is defined in [code-rs/protocol/src/request_user_input.rs](code-rs/protocol/src/request_user_input.rs):

```rust
pub struct RequestUserInputQuestion {
    pub id: String,                                      // stable ID
    pub header: String,                                  // short title
    pub question: String,                                // full prompt text
    pub is_other: bool,                                  // include "Other" freeform option
    pub is_secret: bool,                                 // mask input (password)
    pub allow_multiple: bool,                            // multi-select vs single
    pub options: Option<Vec<RequestUserInputQuestionOption>>,
}

pub struct RequestUserInputQuestionOption {
    pub label: String,
    pub description: String,
}

pub struct RequestUserInputResponse {
    pub answers: HashMap<String, RequestUserInputAnswer>,  // keyed by question_id
}

pub struct RequestUserInputAnswer {
    pub answers: Vec<String>,  // single element for single-select, multi for multi-select
}
```

### Existing UI Flow

1. **Agent** (code-core) calls `request_user_input` tool
2. Handler at [code-rs/core/src/tools/handlers/request_user_input.rs](code-rs/core/src/tools/handlers/request_user_input.rs) processes args
3. Emits `EventMsg::RequestUserInput` via Session
4. **TUI** receives `CodexEvent` → dispatches to chatwidget → opens `RequestUserInputView`
5. View lives in [code-rs/tui/src/bottom_pane/panes/request_user_input/](code-rs/tui/src/bottom_pane/panes/request_user_input/)
   - `mod.rs` — `RequestUserInputView` struct with `AnswerState` per question
   - `model.rs` — state mutations (move_selection, toggle_current_option, submit)
   - `pane_impl.rs` — `BottomPaneView` trait impl (handle_key_event)
   - `render.rs` — ratatui rendering
6. User interacts, presses Enter → `submit()` builds `RequestUserInputResponse`
7. Sends `AppEvent::RequestUserInputAnswer { turn_id, response }`
8. Handler in app routes the answer back to the agent

### Existing UX Features

**Already supported:**
- ✅ Single-choice (radio): default when `allow_multiple = false`
- ✅ Multi-select (checkboxes): when `allow_multiple = true`
- ✅ Freeform text: when `options = None` or `is_other = true`
- ✅ Secret/password input: when `is_secret = true`
- ✅ Multi-question progression: Question N/M with PgUp/PgDn
- ✅ Mouse support: option_hit_test handles clicks
- ✅ Esc escape hatch: closes view, falls back to composer
- ✅ MCP access prompts: special handling for auth

**Keyboard map (current):**
- `↑`/`↓` — move selection
- `Space` — toggle (in multi-select) OR insert space (in freeform)
- `Enter` — next question OR submit on last
- `Esc` — cancel, fall back to composer
- `PgUp`/`PgDn` — previous/next question
- `Backspace` — delete char (in freeform)
- Any char — append to freeform (if applicable)

### Reusable Building Blocks

Don't reinvent these — use what exists:

| Component                          | Location                               | Purpose                             |
| ---------------------------------- | -------------------------------------- | ----------------------------------- |
| `ScrollState`                      | `components/scroll_state.rs`           | Selection + scroll window for lists |
| `GenericDisplayRow`                | `components/selection_popup_common.rs` | Standard row rendering              |
| `render_rows()`                    | `components/selection_popup_common.rs` | List rendering with scroll          |
| `popup_frame::themed_block()`      | `components/popup_frame.rs`            | Consistent modal border             |
| `ui_interaction::contains_point()` | `ui_interaction.rs`                    | Mouse hit-testing                   |
| `ui_interaction::redraw_if()`      | `ui_interaction.rs`                    | Wraps bool → ConditionalUpdate      |
| `icons::checkbox_on/off()`         | `icons.rs`                             | Consistent checkbox symbols         |
| `icons::nav_up_down()`             | `icons.rs`                             | Arrow indicator                     |
| `textwrap::wrap()`                 | external crate                         | Word wrapping                       |
| `popup_frame` components           | `components/popup_frame.rs`            | Shared frame chrome                 |

### Three Implementation Strategies

For new question patterns, pick one:

**A. Extend `RequestUserInputQuestion`** — add new fields (e.g., `ranked: bool`, `excluded: Vec<String>`)
- *Pros*: Agent uses same tool; TUI adds new render branch; minimal plumbing
- *Cons*: Protocol schema grows; need to keep backward-compatible
- *Use for*: Ranked choice, priority tagging (small extensions)

**B. Add new `RequestUserInput*` tool variant** — e.g., `RequestBranchingQuestion`
- *Pros*: Schema stays clean; explicit tool name; independent evolution
- *Cons*: New protocol type; new handler; agent must discover new tool
- *Use for*: Conditional branching, batch forms (different semantics)

**C. Create a separate `BottomPaneView`** — not tied to agent tool
- *Pros*: Full UX control; can be triggered by user or agent
- *Cons*: Separate flow; less discoverable to agent
- *Use for*: Checkpoint configuration (user-initiated), reversibility indicator (event-driven)

**Recommendation:** Start with **A** when possible (smaller change, tested path). Use **B** only when the interaction model is genuinely different. Use **C** for non-agent-driven UX (settings, status).

---

## Implementation Architecture (Conceptual Model)

> **Note:** This section describes an *idealized* data model. The actual codebase uses
> [`RequestUserInputQuestion`](code-rs/protocol/src/request_user_input.rs) — a simpler schema.
> When implementing, see **CodexCLI TUI Architecture (Actual State)** above for what exists today,
> and **How to Add a Question Type** below for the concrete path to extend it.
>
> Use this section for: understanding the design intent when adding new fields or semantics.
> Don't use it for: direct 1:1 type mapping — it doesn't match the code.

### Core Data Model (Aspirational)

Every question/interaction, in the fully-realized model, would carry this metadata:

```rust
pub struct Question {
    id: String,                     // stable identifier for tracking/recovery
    kind: QuestionKind,             // single-choice, multi-select, etc.
    priority: QuestionPriority,     // blocker, high, medium, low
    scope: AnswerScope,             // this-answer, this-task, this-repo, global
    confidence: ConfidenceLevel,    // hard-req, strong-pref, pref, brainstorm
    reversibility: Reversibility,   // reversible, session-only, permanent
    context: ContextChips,          // active constraints/preferences affecting this Q
    provenance: AnswerLog,          // what previous decisions led here
}

pub enum AnswerScope {
    ThisAnswer,
    ThisTask,
    ThisRepo,
    Global,
}

pub enum Reversibility {
    Reversible { undo_window_ms: u64 },
    SessionOnly,
    Permanent,
}
```

**Persistence:**
- Store question history in `.code/session.jsonl` (append-only event log)
- Store preferences in `.code/preferences.json` (keyed by scope + name)
- Store decision provenance in `.code/decision_log.jsonl` for audit trail

### State Machine for Long-Running Tasks

```
[IDLE] --plan-requested--> [PLANNING]
                              |
                              +--confirm-plan--> [APPROVED]
                              |                      |
                              |                      +--waiting-for-questions--> [QUESTION_BATCH]
                              |                                                       |
                              +--cancel-plan------> [CANCELLED]                      +--answers-received--> [EXECUTING]
                                                                                           |
                                                                                           +--question-raised--> [INTERRUPTION]
                                                                                           |
                                                                                           +--complete--> [DONE]
```

Each state has explicit entry/exit animations and narration to prevent mode confusion.

### Question Batching Algorithm

```
1. Accumulate questions in priority queue during execution
2. Check for natural breakpoint:
   - Phase completion (all files read, all tests run)
   - Pre-irreversible step (about to delete/deploy)
   - User idle > 30s (safe to interrupt)
3. Group accumulated questions:
   - Blockers → ask immediately (skip batching)
   - High + Medium → batch together
   - Low → hold until task end
4. Render as numbered multi-part form
5. Block execution until batch answered
```

### Conditional Branching Logic

Store question dependency tree as:

```rust
pub struct QuestionBranch {
    question_id: String,
    depends_on: Option<(question_id, answer_path)>,  // null = show always
    follows: Vec<answer_variant>,                     // which answers reveal this Q
    skip_if: Option<Predicate>,                       // condition that hides this Q
}
```

When an earlier answer changes, re-evaluate all dependent questions:
- If branch becomes invalid, mark dependent Qs as "invalidated"
- Offer rollback ("That answer changes things we already decided on")
- Allow user to revise without losing other answers

### Preference Conflict Detection

```rust
fn check_conflict(new_pref: &Preference, existing: &[Preference]) -> ConflictReport {
    // Three types of conflicts:
    // 1. Direct contradiction (A requires X, B requires NOT X)
    // 2. Scope collision (same setting, different levels)
    // 3. Ordering conflict (order-sensitive rules in incompatible order)
    
    for existing_pref in existing {
        if new_pref.contradicts(existing_pref) {
            return ConflictReport {
                kind: ConflictKind::Direct,
                old: existing_pref,
                new: new_pref,
                suggested_resolution: ["keep_old", "replace", "scope_narrow"],
            };
        }
    }
}
```

---

## Implementation Details by Pattern (Conceptual)

> **Note:** These are design sketches for patterns, showing ideal data models and keyboard layouts.
> In the current codebase, most of these would be **Path A** extensions to `RequestUserInputQuestion`
> rather than standalone types. See the two "How to Add a Question Type" paths above for the concrete
> ways to plug into the existing system.

### 1. Required vs Nice-to-Have Tagging

**Data model (aspirational):**
```rust
pub enum ItemPriority {
    Required,
    Preferred,
    Optional,
    Exclude,
}

pub struct TaggedOption {
    label: String,
    priority: ItemPriority,  // can be edited per-item
    disabled_reason: Option<String>,
}
```

**Codebase integration (Path A):** Extend `RequestUserInputQuestionOption` with a
`priority: Option<ItemPriority>` field. In the view, render the priority as a prefix
tag per row (e.g., `[R]` red, `[P]` yellow). User cycles priority with `1`/`2`/`3`/`4`
keys. On submit, the answer includes items grouped by priority.

**Keyboard controls:**
- Navigate: `j`/`k` or arrows
- Cycle priority: `1` (required), `2` (preferred), `3` (optional), `4` (exclude)
- Or: `[` / `]` to cycle forward/backward
- Show count of each tier in header: "1 required, 3 preferred, 2 optional"

**Rendering:**
- Each option gets a colored tag: `[R]` red, `[P]` yellow, `[O]` gray, `[✗]` strike
- Never use color alone — always include icon/symbol
- Show visual summary bar: `████░░░░░` (required filled, rest empty)

**Validation:**
- If any items are required, enforce: "At least 1 item must be required"
- On submit, validate: "You have 0 required items" → warning, allow override

---

### 2. Ranked Choice / Drag-to-Reorder

**Data model:**
```rust
pub struct RankedChoice {
    items: Vec<RankableItem>,
    current_rank: Vec<usize>,        // indices in preference order
    excluded: Vec<usize>,             // items user marked "skip"
    position_labels: bool,            // show "1.", "2.", "3."
}
```

**Keyboard controls:**
- Navigate: `j`/`k` or arrows move cursor
- Reorder: `Alt+↑` / `Alt+↓` (or `Shift+K`/`Shift+J`)
- Mark excluded: `x` toggles item from included → excluded
- Expand: `>` / `<` to collapse/expand hidden items
- Enter: confirm ranking

**Visual feedback:**
- Position numbers update live as items reorder
- Excluded items show strikethrough or dimmed
- Show count: "3 ranked, 2 excluded" in header

**Validation:**
```rust
fn validate_ranking(items: &[RankedItem]) -> Result<()> {
    // Must have at least 1 item ranked (not all excluded)
    if items.iter().all(|i| i.is_excluded) {
        return Err("At least one item must be ranked");
    }
    Ok(())
}
```

---

### 3. Per-Item Approve/Reject + Batch

**Data model:**
```rust
pub struct ApprovableItem {
    id: String,
    label: String,
    description: String,
    risk: RiskLevel,                 // low, medium, high
    status: ApprovalStatus,          // pending, approved, rejected, edited
    edit_history: Vec<String>,       // track user modifications
}

pub enum ApprovalStatus {
    Pending,
    Approved { approved_at: Instant },
    Rejected { reason: Option<String> },
    Edited { new_value: String },
}
```

**Keyboard controls:**
- `j`/`k`: navigate items
- `a`: approve current item
- `r`: reject current item
- `e`: edit current item (opens inline editor or separate form)
- `A`: approve all remaining
- `R`: reject all remaining
- `A-l`: approve all low-risk
- `A-m`: approve all medium-risk
- `Ctrl+z`: undo last batch action

**Rendering:**
```
 Approve these changes? [12/47 pending]

  ✓ [a] src/auth.rs: +45 -12            [medium]
  ✓ [a] tests/auth_test.rs: new file    [low]
  ? [ ] src/legacy/compat.rs: +8 -34    [high]  ← current cursor
  ✗ [r] config.toml: no mutable statics [high]

 [A]pprove all  [R]eject all  [L]ow-risk only  [u]ndo last
```

**Summary line:**
```
12 approved  3 rejected  2 pending (1 high-risk)  [review pending?]
```

**After batch action:**
```
── Batch action: approved 8 low-risk changes ──────────────────
Undo this as one? [u]  or continue individually
```

---

### 4. Conditional Branching

**Architecture:**
- Question tree is built as DAG (directed acyclic graph)
- Root questions: no dependencies
- Child questions: depend on specific parent answers
- Loop detection: validate at tree construction time

**Implementation:**

```rust
pub struct QuestionTree {
    questions: Vec<Question>,
    edges: Vec<(usize, usize, AnswerCondition)>,  // (parent_q, child_q, when_answer_is)
}

pub enum AnswerCondition {
    Equals(String),
    In(Vec<String>),
    Not(String),
    And(Vec<AnswerCondition>),
}

fn evaluate_visible_questions(tree: &QuestionTree, answers: &AnswerMap) -> Vec<usize> {
    let mut visible = vec![]; // root Qs
    let mut to_process = vec![root_question_indices];
    
    while let Some(q_idx) = to_process.pop() {
        if matches_condition(q_idx, answers) {
            visible.push(q_idx);
            to_process.extend(children_of(q_idx, tree));
        }
    }
    visible
}
```

**UI state:**
```rust
pub struct ConditionalFlowState {
    current_question: usize,
    all_answers: AnswerMap,
    visible_questions: Vec<usize>,
    depth: usize,                    // which "branch" the user is on
    breadcrumb: Vec<String>,         // "Q1: Option A > Q3 > You are here"
}
```

**Rendering:**
- Show breadcrumb: `Q1: Runtime Selection > Q2: Configure Paths > Current`
- When a question appears: fade-in animation, narration `"Based on your answer, I need to know..."`
- When a question disappears: fade-out, note `"That question no longer applies"`
- "Back" button only goes back one level (not all the way to root)

**Validation on submit:**
- Invalidated branches: if user changes Q1, any Q3+ that depended on Q1's old answer is now invalid
- Offer choices: `[keep answers, will be reviewed]` / `[discard Q3+] [cancel]`

---

### 5. Scope-of-Answer / Remember-This

**Data model:**
```rust
pub struct RememberedPreference {
    id: String,
    name: String,                   // human-readable label
    rule: String,                   // what was decided
    scope: PreferenceScope,
    confidence: ConfidenceLevel,    // was this a firm choice or tentative?
    set_at: Instant,
    expires: Option<Instant>,       // optional auto-expiry
    contexts: Vec<String>,          // which parts of the system apply this
}

pub enum PreferenceScope {
    ThisAnswer,
    ThisTask,
    ThisRepo { path: PathBuf },
    Global,
}
```

**Trigger point:**
After the user answers a style/preference question, surface:

```
You said: "use compact labels"

Remember this:  [Only this time]  [This task]  [This repo]  [Everywhere]

 [← details]  [Proceed ↵]
```

**Persistence:**
- ThisTask: in-memory, cleared when task ends
- ThisRepo: `.code/preferences.json`
- Global: `~/.code/preferences.json`

**On application:**
When a preference is applied, show inline note:
```
Using compact labels (you chose this for this repo)
[Change just for this task]
```

**Revocation:**
```
/preferences  → list all active preferences
                select one → [keep] [narrow scope] [revoke]
```

---

### 6. Disambiguation / Ambiguity Flagging

**Confidence scoring:**
```rust
pub enum ConfidenceLevel {
    High { reason: String },        // "This matches a known pattern"
    Medium { ambiguities: Vec<String> },  // "Could mean X or Y"
    Low { interpretations: Vec<String> }, // "Multiple valid readings"
}

fn score_confidence(instruction: &str, context: &TaskContext) -> ConfidenceLevel {
    // Heuristics:
    // - Known task patterns → High
    // - Vague descriptors ("make it better") → Low
    // - Overloaded terms ("clean up") → Medium
    // - Contradictory signals → Low
}
```

**Display by confidence level:**

High confidence:
```
✓ I'm treating "add unit tests" as: write tests for public API only
  [Proceed] [Clarify]
```

Medium confidence:
```
? "Refactor this" could mean:
  1. Reorganize code (same behavior)
  2. Performance optimization
  
  Which did you mean?  [1] [2] [Both] [Other...]
```

Low confidence:
```
✗ I have low confidence about: "make it better"

  I could interpret this as:
  1. Readability improvements (extract functions, add names)
  2. Performance optimization (profile and tune)
  3. Test coverage (add missing tests)
  4. All of the above
  5. Something else
  
  [1] [2] [3] [4] [5 - let me explain]
```

**Answer selection:**
- Show 2–3 example changes for each interpretation
- Allow user to pick or write custom
- Log interpretation in answer provenance

---

### 7. Checkpoint Configuration

**Data model:**
```rust
pub struct CheckpointConfig {
    name: String,                      // "careful", "balanced", "autonomous"
    continue_auto: Vec<ActionKind>,   // file read, test run, etc.
    ask_before: Vec<ActionKind>,      // file write, schema change, etc.
    never_without_approval: Vec<ActionKind>,  // deploy, delete, etc.
}

pub enum ActionKind {
    FileRead,
    FileWrite { impact: ImpactLevel },
    FileDelete,
    SchemaChange,
    NewDependency,
    TestRun,
    CommandExec,
    Deploy,
}

pub enum ImpactLevel {
    Low,
    Medium,
    High,
}
```

**UI (before Auto Drive):**
```
How autonomous should I be?

Continue without asking:  [✓] file reads  [ ] tests  [ ] config  [ ] deletes
Ask before:              [ ] any file write  [✓] schema changes  [✓] new deps
Never without approval:  [✓] deploys  [✓] branch pushes

[← use preset]  [Custom ↵]
```

**Presets:**
```
[Careful]     Continue: reads only. Ask: writes, tests. Never: deletes, deploys.
[Balanced]    Continue: reads, tests. Ask: writes, schema. Never: deletes, deploys.
[Autonomous]  Continue: all except deploys/deletes. Ask: delete. Never: (nothing)
```

**Display during run:**
```
Status bar: [Autonomous mode: file writes auto] [1 pending approval: schema change]
```

---

### 8. Reversibility Status Indicator

**Data model:**
```rust
pub enum Reversibility {
    Reversible {
        via: String,  // "Undo with C-z", "Git revert", "Restore from trash"
        window: Option<Duration>,  // None = forever, Some(x) = x seconds
    },
    SessionOnly {
        reason: String,  // "Session state, lost on restart"
    },
    Permanent {
        reason: String,  // "No backup exists"
        alternatives: Vec<String>,  // ["Move to trash instead", ...]
    },
}
```

**Rendering:**
```
✓ Reversible  file.ts will be deleted and moved to trash.  [Undo: C-z ↵]  [Cancel ESC]

⚠ Session only  Config change. Lost after restart.  [Proceed ↵] [Cancel ESC]

✗ Permanent  12 files will be deleted permanently (no backup).  
             [Delete] [Move to trash instead t] [Cancel ESC]
```

**Implementation:**
- Never say "Are you sure?" — communicate what will happen instead
- Always show the undo affordance on success
- Automatic undo button greys out after window closes

---

### 9. Question Batching (Ask Once)

**Algorithm:**
```rust
fn should_batch_question(q: &Question, execution_state: &ExecState) -> bool {
    match q.priority {
        // Blockers are never batched
        QuestionPriority::Blocker => false,
        
        // High: ask if it changes "what do next"
        QuestionPriority::High => {
            // Only batch if we're already in a batch window
            execution_state.is_at_breakpoint()
        }
        
        // Medium/Low: always batch
        _ => true,
    }
}

fn detect_natural_breakpoint(exec: &ExecutionProgress) -> bool {
    exec.phase_complete()      // "reading files" → "analyzing"
        || exec.about_to_irreversible()  // next action is delete/deploy
        || exec.user_idle_ms() > 30_000
}
```

**UI state:**
```rust
pub struct BatchedQuestionForm {
    questions: Vec<(usize, Question)>,  // (question_idx_in_batch, question)
    accumulated_at: Instant,
    answers: HashMap<usize, Answer>,
    focus: usize,                        // which question has cursor
}
```

**Rendering:**
```
Before continuing, I need to clarify 3 things:

  1. Should I also update tests for the renamed symbols?
     [Yes]  [No]  [Skip tests this pass]

  2. Which files should be excluded from the refactor?
     [Select files...]

  3. Prefer to preserve existing comments or rewrite?
     [Preserve where clear]  [Rewrite consistently]

[↵ Answer all] [Tab to next] [? help] [ESC cancel]
```

**Key behaviors:**
- Focus cycles: Tab moves to next Q, Shift+Tab to previous
- Enter on last Q submits the batch
- Esc cancels the entire batch (and pauses execution)
- Any Q can be skipped with `[Skip]` button if it's optional

---

### 10. Notification Digest / Priority Tiers

**Data model:**
```rust
pub enum NotificationPriority {
    Urgent,  // blocks execution, modal
    Normal,  // toast, auto-dismiss 5s
    Low,     // digest only, manual drain
}

pub struct Notification {
    priority: NotificationPriority,
    title: String,
    message: String,
    timestamp: Instant,
    actions: Vec<NotificationAction>,
    auto_dismiss_ms: Option<u64>,
}
```

**Display:**

Urgent (modal):
```
╭── ⚠ Cannot continue ──────────────────╮
│  src/auth.rs is locked (PID 12847)   │
│                                       │
│  [Retry]  [Skip]  [Cancel all] [ESC] │
╰───────────────────────────────────────╯
```

Normal (toast):
```
✓ Tests passed: 247/247  [View results]   (auto-dismiss in 5s)
```

Low (digest status bar):
```
Status: [3 items in digest]  [d]rain  [4 warnings in auth.rs] [2 unused imports]
```

Digest view:
```
╭── Digest (3 items, last 8 min) ────────────────────╮
│  • clippy: 2 warnings in auth.rs    [view] [fix]   │
│  • Unused import in token.rs        [fix] [skip]   │
│  • CHANGELOG not updated            [done] [skip]  │
╰────────────────────────────────────────────────────╯
```

---

### 11. Pre-Task Expectation Setting

**Data model:**
```rust
pub struct TaskExpectation {
    what_i_will_do: String,
    what_i_wont_do: Vec<String>,
    uncertainties: Vec<Uncertainty>,
    constraints_active: Vec<String>,
    estimated_scope: TaskScope,
}

pub struct Uncertainty {
    what: String,
    why: String,
    how_handled: String,  // "I'll flag it for you", "I'll ask when I hit it"
}
```

**Rendering:**
```
╭─ Before I start: what to expect ──────────────────────────╮
│  I'll refactor the auth module. Here's what I know:       │
│                                                            │
│  ✓ Confident:                                              │
│    Extract TokenManager is safe (no downstream deps)      │
│                                                            │
│  ? Uncertain:                                              │
│    2 call sites in legacy/ may need manual review         │
│    (I'll flag them for you)                               │
│                                                            │
│  ✗ Won't do:                                               │
│    Change the public API surface (you said to avoid)      │
│    Add new dependencies                                    │
│                                                            │
│  Estimated: 8 files, ~15 min, will ask 1-2 questions     │
│  [OK ↵]  [Change scope e]  [Cancel ESC]                  │
╰────────────────────────────────────────────────────────────╯
```

---

## Testing and Validation

### Unit-level testing:

```rust
#[test]
fn test_conditional_branching_invalid_answers() {
    // Q1 → (answer A → Q2, answer B → Q3)
    // If user changes Q1 from A to B, Q2 should invalidate
    let mut tree = build_test_tree();
    let mut answers = default_answers();
    
    answers.set("q1", "A");
    assert!(is_visible(&tree, "q2", &answers));
    
    answers.set("q1", "B");
    assert!(!is_visible(&tree, "q2", &answers));
    assert!(is_visible(&tree, "q3", &answers));
}

#[test]
fn test_batch_question_ordering() {
    let blockers = vec![q_blocker];
    let highs = vec![q_high1, q_high2];
    let meds = vec![q_med];
    
    let batch = build_batch_form(&blockers, &highs, &meds);
    // Blocker should be asked immediately
    assert_eq!(batch.current_question, blockers[0].id);
    // Then batch of high + med
}

#[test]
fn test_reversibility_window_expiry() {
    let action = Action::FileDelete { 
        file: "test.rs",
        reversible: Reversible { window: Duration::from_secs(30) }
    };
    
    // Within window: undo button visible
    assert!(undo_button_visible(&action, elapsed: 20s));
    
    // After window: undo button grey
    assert!(!undo_button_active(&action, elapsed: 40s));
}
```

### Integration-level testing (VT100 snapshot):

```rust
#[test]
fn test_ranked_choice_reordering_visual() {
    let harness = ChatWidgetHarness::new();
    harness.push_event(AskQuestion {
        kind: RankedChoice,
        items: ["auth", "token", "middleware"].into(),
    });
    
    harness.send_key(Key::Char('2'));  // move to "token"
    harness.send_key(Key::Alt(Key::Up));  // move up
    
    let frame = harness.render_to_vt100(120, 30);
    // Snapshot should show: [1. token] [2. auth] [3. middleware]
    insta::assert_snapshot!(frame);
}

#[test]
fn test_batch_questions_focus_cycle() {
    let harness = ChatWidgetHarness::new();
    harness.push_event(AskBatch(vec![q1, q2, q3]));
    
    // Tab cycles focus
    harness.send_key(Key::Tab);
    assert_eq!(harness.focus_index(), 1);
    harness.send_key(Key::Tab);
    assert_eq!(harness.focus_index(), 2);
    
    // Shift+Tab cycles backward
    harness.send_key(Key::ShiftTab);
    assert_eq!(harness.focus_index(), 1);
}
```

### Accessibility testing:

```rust
#[test]
fn test_no_color_only_signaling() {
    // Render with NO_COLOR env var set
    let frame = render_with(env("NO_COLOR", "1"));
    
    // Must still see:
    // - [R] for required, [P] for preferred (not just color)
    // - ✓ for success, ✗ for error (not just color)
    // - Reversibility statement (not just a color-coded icon)
    
    assert!(frame.contains("[R]"));
    assert!(frame.contains("✓"));
}

#[test]
fn test_screen_reader_landmark() {
    // When rendering "Are you sure?", must have:
    // - Explicit consequence statement
    // - All buttons labeled
    // - Not hidden by ARIA
    
    let widget = render_confirmation("Delete file?");
    assert!(widget.aria_label().contains("permanent"));
    assert!(widget.button_labels().contains("Delete"));
}
```

---

## Suggested Implementation Order

Ordered by estimated value / effort ratio:

1. **required vs nice-to-have tagging** — fits existing multiselect; small delta
2. **ranked choice** — solves many backlog/priority questions
3. **per-item approve/reject + batch actions** — critical for plan and finding review
4. **conditional branching** — reduces question noise significantly
5. **scope-of-answer / remember-this** — high value for long-running alignment
6. **disambiguation / ambiguity flagging** — prevents entire classes of misunderstanding
7. **preview before apply** — trust and correctness
8. **question batching (ask-once)** — natural breakpoint detection + batch form; logic only
9. **notification digest / priority tiers** — new widget, high value for long runs
10. **classified error display** — message templates; low complexity, high impact
11. **agent narration (collapsed by default)** — existing stream + toggle; medium effort
12. **inline action audit trail** — always-on transparency layer; medium effort
13. **plan-as-artifact** — new state machine + editable list widget; high complexity
14. **streaming state machine (four phases)** — formalize states; medium effort
15. **task resumption / context re-entry** — requires session state persistence
16. **pre-task expectation setting** — medium effort, very high trust value
17. **background task mode** — new concurrency model; high complexity
18. **multi-line composer + paste handling** — ratatui bracketed-paste API available
19. **matrix questions** — strong for cross-context config
20. **checkpoint configuration** — important for Auto Drive
21. **multi-agent delegation display** — enables trust in parallel agent work
22. **editable answer summary + provenance log** — improves debuggability

---

## Rust Implementation Patterns (Using Existing Architecture)

The codebase already has a clean abstraction for modal/overlay views: the **`BottomPaneView` trait**.
This is exactly where question/interaction UX should live.

### Existing Architecture

**BottomPane** is the container for modals. It can show:
- `ChatComposer` (normal input)
- A `BottomPaneView` (e.g., ApprovalModalView, ModelSelectionView, etc.)

Key files:
- `code-rs/tui/src/bottom_pane/mod.rs` — container
- `code-rs/tui/src/bottom_pane/bottom_pane_view.rs` — trait definition
- `code-rs/tui/src/bottom_pane/panes/` — implementations
- `code-rs/tui/src/app_event.rs` — central event bus

### Implement Questions as BottomPaneView Traits

Each question/interaction type becomes a view in `bottom_pane/panes/`:

```rust
// code-rs/tui/src/bottom_pane/panes/ranked_choice_view.rs
pub struct RankedChoiceView<'a> {
    items: Vec<RankableItem>,
    current_rank: Vec<usize>,
    excluded: Vec<usize>,
    focused_idx: usize,
    app_event_tx: AppEventSender,
}

impl<'a> BottomPaneView<'a> for RankedChoiceView<'a> {
    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        let handled = match key_event.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.focused_idx = (self.focused_idx + 1).min(self.items.len() - 1);
                true
            },
            KeyCode::Char('k') | KeyCode::Up => {
                self.focused_idx = self.focused_idx.saturating_sub(1);
                true
            },
            KeyCode::Char('K') | KeyCode::AltUp => {
                // Move item up in ranking
                if self.focused_idx > 0 {
                    self.current_rank.swap(self.focused_idx, self.focused_idx - 1);
                }
                true
            },
            KeyCode::Enter => {
                self.submit();
                true
            },
            KeyCode::Esc => {
                self.cancel();
                true
            },
            _ => false,
        };
        
        if handled { ConditionalUpdate::NeedsRedraw } else { ConditionalUpdate::NoRedraw }
    }

    fn is_complete(&self) -> bool {
        self.completed
    }

    fn desired_height(&self, width: u16) -> u16 {
        // Return line count: headers + items + footer
        (self.items.len() as u16) + 4
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Render ranked choice list with position numbers
        for (idx, item) in self.items.iter().enumerate() {
            let is_focused = idx == self.focused_idx;
            let position = self.current_rank.iter().position(|&p| p == idx);
            // ... render logic
        }
    }
}

impl<'a> RankedChoiceView<'a> {
    fn submit(&mut self) {
        let answer = Answer::RankedChoice {
            ranked: self.current_rank.iter().map(|&idx| self.items[idx].id.clone()).collect(),
            excluded: self.excluded.iter().map(|&idx| self.items[idx].id.clone()).collect(),
        };
        
        // Send answer back via AppEvent
        let _ = self.app_event_tx.send(AppEvent::QuestionAnswered {
            question_id: self.question_id.clone(),
            answer,
        });
        self.completed = true;
    }
}
```

**Integration flow:**
1. Agent needs ranked choice → sends `AppEvent::AskQuestion`
2. App receives event → creates `RankedChoiceView` and sets it as `bottom_pane.active_view`
3. User interacts with view (keyboard/mouse) → view handles events
4. User presses Enter → view sends `AppEvent::QuestionAnswered`
5. App receives answer → passes to agent execution context

### Module Structure (Actual)

```
code-rs/tui/src/
├── bottom_pane/
│   ├── panes/
│   │   ├── approval_modal/  (already exists)
│   │   ├── ranked_choice/   (NEW)
│   │   │   ├── mod.rs
│   │   │   ├── view.rs
│   │   │   └── render.rs
│   │   ├── batch_form/      (NEW)
│   │   ├── conditional_branching/  (NEW)
│   │   └── ...
│   ├── bottom_pane_view.rs  (trait — already exists)
│   ├── mod.rs               (container — already exists)
│   └── state.rs             (could extend for question state)
│
├── chatwidget/
│   ├── history_cell.rs      (add AnswerCell variant)
│   └── mod.rs               (unchanged)
│
├── questions/               (NEW — logic, not UI)
│   ├── mod.rs
│   ├── types.rs             (Question, Answer enums)
│   ├── coordinator.rs       (batching, condition logic)
│   └── preferences.rs       (persistence)
│
└── app_event.rs             (extend AppEvent enum)
```

**Rationale:**
- UI rendering stays in `bottom_pane/panes/` (follows existing pattern)
- Logic/state goes in `questions/` (separate from UI)
- Events routed through `app_event_tx` (matches existing architecture)
- No new top-level modules needed

### Type Safety and Serialization

```rust
// Stable question IDs for recovery
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuestionId(String);

impl QuestionId {
    pub fn new(scope: &str, name: &str) -> Self {
        Self(format!("{}::{}", scope, name))
    }
}

// Answer must be serializable for persistence
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Answer {
    SingleChoice(String),
    MultiSelect(Vec<String>),
    RankedChoice { ranked: Vec<String>, excluded: Vec<String> },
    FreeForm(String),
    Tagged { choices: Vec<(String, ItemPriority)> },
}

impl Answer {
    pub fn validate(&self, against: &Question) -> Result<()> {
        match (&self, &question.kind) {
            (Answer::SingleChoice(s), QuestionKind::SingleChoice { options }) => {
                if options.contains(s) { Ok(()) } else { Err("invalid option") }
            },
            // ... other variants
        }
    }
}

// Preferences live in .code/preferences.json (top-level key = scope)
#[derive(Serialize, Deserialize)]
pub struct PreferencesFile {
    global: Vec<Preference>,
    repos: HashMap<PathBuf, Vec<Preference>>,
}
```

### Answer Provenance and Audit Trail

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnswerRecord {
    pub question_id: QuestionId,
    pub answered_at: Instant,
    pub answer: Answer,
    pub confidence: ConfidenceLevel,
    pub scope: AnswerScope,
    pub context: TaskContext,         // what task/files/constraints were active
    pub prevision: Option<String>,    // what the agent said before asking
}

// Append-only session log: ~/.code/session.jsonl
pub fn log_answer(record: &AnswerRecord) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(".code/session.jsonl")?;
    
    let json = serde_json::to_string(record)?;
    writeln!(file, "{}", json)?;
    Ok(())
}

// Recovery: replay session log to reconstruct state
pub fn recover_session_state(session_log: &Path) -> Result<AnswerMap> {
    let file = BufReader::new(File::open(session_log)?);
    let mut answers = AnswerMap::new();
    
    for line in file.lines() {
        let record: AnswerRecord = serde_json::from_str(&line?)?;
        answers.insert(record.question_id, record.answer);
    }
    
    Ok(answers)
}
```

### Async and Concurrency

Question coordination must not block execution. Use channels:

```rust
// In code-rs/core (or wherever execution happens):
pub struct ExecutionContext {
    question_tx: Sender<QuestionEvent>,
    question_rx: Receiver<QuestionEvent>,  // listen for approvals
}

impl ExecutionContext {
    pub async fn ask_question(&self, q: Question) -> Result<Answer> {
        // Send question to TUI
        self.question_tx.send(QuestionEvent::QuestionAsked(q))?;
        
        // Wait for answer (with timeout)
        match timeout(Duration::from_secs(300), self.wait_for_answer(&q.id)).await {
            Ok(Ok(answer)) => Ok(answer),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                // User didn't answer in 5min, pause execution
                self.question_tx.send(QuestionEvent::PauseExecution)?;
                Err("Question timeout")
            }
        }
    }
}
```

---

## Integration Points with Existing Code

### AppEvent (Central Bus)

Extend `code-rs/tui/src/app_event.rs` with question events:

```rust
pub enum AppEvent {
    // ... existing variants
    
    /// Agent asking a question (question type determines which view opens)
    AskQuestion {
        question_id: String,
        kind: QuestionKind,
        options: Vec<QuestionOption>,
        priority: QuestionPriority,
        reason: Option<String>,
    },
    
    /// Batch of questions to ask together
    AskBatch {
        questions: Vec<Question>,
    },
    
    /// User answered a question
    QuestionAnswered {
        question_id: String,
        answer: Answer,
        scope: AnswerScope,
    },
    
    /// Question was cancelled/skipped
    QuestionCancelled {
        question_id: String,
    },
}
```

### BottomPane Integration

Questions open as views using existing `ActiveViewKind`:

```rust
// In code-rs/tui/src/bottom_pane/mod.rs
pub enum ActiveViewKind {
    None,
    AutoCoordinator,
    ModelSelection,
    RequestUserInput,      // NEW: reuse for freeform questions
    ShellSelection,
    RankedChoice,          // NEW
    BatchForm,             // NEW
    ConditionalBranching,  // NEW
    ApprovableList,        // NEW (extend approval_modal)
    Other,
}

// In BottomPane::try_consume_approval_request or similar:
pub fn show_question(&mut self, question: Question, ticket: BackgroundOrderTicket) {
    let view: Box<dyn BottomPaneView> = match question.kind {
        QuestionKind::RankedChoice { items } => {
            Box::new(RankedChoiceView::new(items, question.id, self.app_event_tx.clone()))
        },
        QuestionKind::BatchForm { questions } => {
            Box::new(BatchFormView::new(questions, self.app_event_tx.clone()))
        },
        // ...
    };
    
    self.active_view = Some(view);
    self.active_view_kind = ActiveViewKind::RankedChoice; // or appropriate kind
}
```

### History Integration

Questions appear as special history cells (follow ApprovalRequest pattern):

```rust
// In code-rs/tui/src/history/history_cell.rs
pub enum HistoryCell {
    // ... existing variants
    Question {           // NEW
        question: Question,
        answer: Answer,
        timestamp: Instant,
        scope_tag: AnswerScope,
    },
}
```

### Settings Integration

Preferences stored using existing `.code/config.toml`:

```toml
# .code/config.toml
[question_behavior]
default_scope = "this_task"         # scope-of-answer default
batch_at_natural_breakpoint = true
show_reversibility_always = true
reversibility_window_seconds = 30

[accessibility]
no_color_fallback = true            # already exists
use_text_labels = true              # already exists
```

### Agent-to-TUI Communication

Agent (code-core) should have async channel for questions:

```rust
// In code-core execution context
pub struct ExecutionContext {
    pub question_tx: Sender<QuestionRequest>,
    // ... other fields
}

impl ExecutionContext {
    pub async fn ask_question(&self, q: Question) -> Result<Answer> {
        self.question_tx.send(QuestionRequest::Ask(q))?;
        // Wait for answer from TUI
        timeout(Duration::from_secs(300), self.answer_rx.recv()).await??
    }
}
```

The TUI app receives questions via CodexEvent → translates to AppEvent::AskQuestion.

---

## Testing Strategy (Following Actual Patterns)

The codebase already has excellent test patterns — follow them exactly.

### Reference: Existing Unit Tests

The `RequestUserInputView` test suite at [code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs:391](code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs#L391) is the template:

**Pattern:**
1. Create a mock `AppEventSender` via `std::sync::mpsc::channel()`
2. Build the view directly (no harness needed for unit tests)
3. Drive state via public/super methods: `toggle_current_option`, `move_selection`, `push_freeform_char`
4. Call `submit()`
5. Assert on the `AppEvent` received from the channel

```rust
fn make_view(question: RequestUserInputQuestion) -> (RequestUserInputView, Receiver<AppEvent>) {
    let (tx, rx) = channel();
    (
        RequestUserInputView::new(
            "turn-1".to_owned(),
            "call-1".to_owned(),
            vec![question],
            AppEventSender::new(tx),
        ),
        rx,
    )
}

#[test]
fn ranked_question_submits_in_rank_order() {
    let question = RequestUserInputQuestion { is_ranked: true, /* ... */ };
    let (mut view, rx) = make_view(question);

    view.move_selection(false);       // focus index 1
    view.move_ranked_item_up();       // swap to position 0
    view.submit();

    let AppEvent::RequestUserInputAnswer { response, .. } = rx.recv().unwrap() else {
        panic!("expected answer event");
    };
    assert_eq!(response.answers["q"].answers[0], "originally_index_1");
}
```

**What to test (per new feature):**
- Happy path: user makes valid selection and submits
- Order preservation: multi-select items come back in option order (see existing `submit_multiselect_returns_checked_labels_in_order`)
- Edge case: empty freeform in "Other" (see existing `submit_multiselect_omits_blank_other_answer`)
- Cancellation: Esc produces no `RequestUserInputAnswer` (the view becomes complete instead)

### Reference: VT100 Snapshot Tests

The canonical VT100 harness is in [code-rs/tui/tests/vt100_chatwidget_snapshot.rs](code-rs/tui/tests/vt100_chatwidget_snapshot.rs). It uses:

- `ChatWidgetHarness` from `code_tui::test_helpers` (gated behind `test-helpers` feature)
- `render_chat_widget_to_vt100(width, height)` for a single frame
- `render_chat_widget_frames_to_vt100(&[(w,h), ...])` for multi-frame streaming
- `CODEX_TUI_FAKE_HOUR=12` env var is auto-set for deterministic greetings

**Running:**
```bash
cargo test -p code-tui --test vt100_chatwidget_snapshot --features test-helpers -- --nocapture
```

**Accepting updated snapshots (only after intentional changes):**
```bash
cargo insta review --workspace-root code-rs
# or to accept specific ones:
cargo insta accept --workspace-root code-rs
```

**Snapshot location:** `code-rs/tui/tests/snapshots/`

### Regression Checklist (from CLAUDE.md)

After any UI change, run at minimum:
```bash
./build-fast.sh                                     # required; fixes all warnings
cargo test -p code-tui --features test-helpers --lib  # unit tests
cargo test -p code-tui --test vt100_chatwidget_snapshot --features test-helpers
```

Before pushing to main:
```bash
./pre-release.sh                                    # full preflight
```

### Writing New UI Regression Tests (from CLAUDE.md)

Follow this sequence:

1. **Build a `ChatWidget` in isolation** using `make_chatwidget_manual()` or `make_chatwidget_manual_with_sender()` from `test_helpers`
2. **Define a `ScriptStep` enum** describing the scripted interaction (key presses, events)
3. **Feed events** via `chat.handle_key_event()` using `run_script()` from `tests.rs`
4. **Render with `ratatui::Terminal`/`TestBackend`**
5. **Normalize with `buffer_to_string()`** (wraps `strip_ansi_escapes`) before asserting
6. **Prefer `insta::assert_snapshot!`** over string compare
7. **Gate snapshot updates behind `UPDATE_IDEAL=1`** so baseline refreshes stay explicit

### What NOT to Do

- ❌ **Never run `cargo fmt` / `rustfmt`** (per CLAUDE.md policy)
- ❌ Never commit `.snap.new` files — review and accept instead
- ❌ Never use absolute times/random in snapshots (use `CODEX_TUI_FAKE_HOUR`)
- ❌ Never test keyboard events directly without also testing the outgoing `AppEvent`

---

## Performance Considerations

### Question Rendering

- Cache compiled question widgets (QuestionRenderer trait objects)
- Lazy-render large lists: only render visible + 1 off-screen
- Debounce search filtering (wait 100ms before re-rendering)

```rust
pub trait QuestionRenderer: Send {
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn handle_input(&mut self, key: KeyEvent) -> bool;
    fn extract_answer(&self) -> Option<Answer>;
}

// Lazy list for large ranked/approval items
pub struct LazyQuestionList {
    items: Vec<Item>,
    viewport: Range<usize>,
    scroll_pos: usize,
    filtered_indices: Vec<usize>,  // only visible items
}
```

### Preference Lookup

Preferences are looked up frequently. Use layered caching:

```rust
pub struct PreferenceCache {
    // L1: task-scoped prefs (in-memory, cleared per task)
    task_scope: HashMap<String, Preference>,
    
    // L2: repo-scoped (loaded once per repo, mmap-friendly)
    repo_scope: RwLock<HashMap<String, Preference>>,
    
    // L3: global (loaded once, ~50KB expected size)
    global_scope: OnceLock<Vec<Preference>>,
}

impl PreferenceCache {
    pub fn lookup(&self, name: &str) -> Option<Preference> {
        // Lookup order: task → repo → global
        self.task_scope.get(name)
            .or_else(|| self.repo_scope.read().ok()?.get(name))
            .or_else(|| self.global_scope.get()?.iter().find(|p| p.name == name))
            .cloned()
    }
}
```

---

## Priority Ranking for CodexCLI

Given the existing `RequestUserInputQuestion` system and Auto Drive focus, rank by **Path** (A=extend, B=new tool, C=new pane) to estimate real effort:

### Tier 1 (Path A — Schema Extensions)

Backward-compatible additions to `RequestUserInputQuestion`.

| Pattern                                     | Schema Change                                    | Render Change                           |
| ------------------------------------------- | ------------------------------------------------ | --------------------------------------- |
| **Ranked choice**                           | `is_ranked: bool`, `allow_exclude: bool`         | Position numbers, reorder keys          |
| **Required / Preferred / Optional tagging** | Per-option `ItemPriority` enum                   | Priority tag prefix on each row         |
| **Scope-of-answer tag**                     | `remember_as: Option<AnswerScope>`               | Scope picker after submit               |
| **Confidence tagging**                      | Per-answer `confidence: Option<ConfidenceLevel>` | Inline chip picker                      |
| **Reversibility indicator**                 | N/A (new history_cell variant)                   | New card in history with action buttons |

### Tier 2 (Path C — New Panes)

Standalone panes. Pattern = copy [request_user_input/](code-rs/tui/src/bottom_pane/panes/request_user_input/) directory structure.

| Pattern                           | Dependency                                                                           | Notes                        |
| --------------------------------- | ------------------------------------------------------------------------------------ | ---------------------------- |
| **Checkpoint config**             | Extends `AutoDriveSettings` in [config_types.rs](code-rs/core/src/config_types.rs)   | Settings-style pane          |
| **Per-item approve/reject batch** | Extends [approval_modal/](code-rs/tui/src/bottom_pane/panes/approval_modal/)         | Already has queuing          |
| **Notification digest**           | New `ActiveViewKind::Digest`                                                         | Priority queue in chatwidget |
| **Working-agreement panel**       | Uses `chrome` slot in [bottom_pane/chrome.rs](code-rs/tui/src/bottom_pane/chrome.rs) | Persistent overlay           |

### Tier 3 (Path B — New Protocol)

Significant additions requiring new protocol design.

| Pattern                          | Why New Protocol                                |
| -------------------------------- | ----------------------------------------------- |
| **Conditional branching**        | Needs question DAG with re-evaluation semantics |
| **Matrix questions**             | 2D navigation; `ScrollState` is 1D only         |
| **Plan-as-artifact**             | Multi-step editable structure                   |
| **Disambiguation with examples** | Rich option data (code snippets, diffs)         |

### Tier 4 (Defer)

| Pattern                                          | Reason to Defer                                                    |
| ------------------------------------------------ | ------------------------------------------------------------------ |
| **Pairwise comparison**                          | Path B; rare use case                                              |
| **Streaming state machine (formal four phases)** | Large refactor; current streaming is "good enough"                 |
| **Multi-agent delegation UI**                    | Codebase uses sub-agent system differently; needs product decision |
| **Focus mode**                                   | Can ship via theme/density setting; no new pane needed             |
| **Editable answer summary**                      | Users can already revise via `/revise` on request                  |

---

## Real-World Integration Points

### Auto Drive Flow (End-to-End)

The pipeline already exists. Questions plug into step 5-9 without touching 1-4 or 10-11:

```
┌─────────────────────────────────────────────────────────────────────────┐
│ code-core (agent side)                                                  │
│ 1. Tool handler (handlers/request_user_input.rs) receives call          │
│ 2. Deserializes RequestUserInputToolArgs from JSON                      │
│ 3. Builds RequestUserInputQuestion objects                              │
│ 4. Emits EventMsg::RequestUserInput via Session                         │
│    (agent awaits answer on async channel)                               │
└──────────────────────────────────┬──────────────────────────────────────┘
                                   │ CodexEvent
                                   ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ code-tui (chatwidget/code_event_pipeline/)                              │
│ 5. Processes CodexEvent::RequestUserInput                               │
│ 6. Constructs RequestUserInputView with questions + turn_id + call_id   │
│ 7. Sets bottom_pane.active_view = Box::new(view)                        │
│    Sets active_view_kind = ActiveViewKind::RequestUserInput             │
└──────────────────────────────────┬──────────────────────────────────────┘
                                   │ (user keypress events)
                                   ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ code-tui (bottom_pane/panes/request_user_input/)                        │
│ 8. pane_impl.rs handles keys; model.rs mutates AnswerState              │
│ 9. On Enter at last question: model.submit()                            │
│    Builds HashMap<question_id, RequestUserInputAnswer>                  │
│    Sends AppEvent::RequestUserInputAnswer { turn_id, response }         │
└──────────────────────────────────┬──────────────────────────────────────┘
                                   │ AppEvent::RequestUserInputAnswer
                                   ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ code-tui (app event loop)                                               │
│ 10. Processes AppEvent::RequestUserInputAnswer                          │
│ 11. Forwards to agent via ConversationManager / Session                 │
│     (fulfills the async channel from step 4)                            │
└─────────────────────────────────────────────────────────────────────────┘
```

### Specific File Entry Points

When extending the existing question system:

| What you want to change      | File                                                                                                                                                     |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Question protocol schema     | [code-rs/protocol/src/request_user_input.rs](code-rs/protocol/src/request_user_input.rs)                                                                 |
| Agent tool handling          | [code-rs/core/src/tools/handlers/request_user_input.rs](code-rs/core/src/tools/handlers/request_user_input.rs)                                           |
| TUI input pipeline dispatch  | [code-rs/tui/src/chatwidget/input_pipeline/user_input/request_user_input.rs](code-rs/tui/src/chatwidget/input_pipeline/user_input/request_user_input.rs) |
| View struct + AnswerState    | [code-rs/tui/src/bottom_pane/panes/request_user_input/mod.rs](code-rs/tui/src/bottom_pane/panes/request_user_input/mod.rs)                               |
| State transitions + submit   | [code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs](code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs)                           |
| Key/mouse handling           | [code-rs/tui/src/bottom_pane/panes/request_user_input/pane_impl.rs](code-rs/tui/src/bottom_pane/panes/request_user_input/pane_impl.rs)                   |
| Rendering                    | [code-rs/tui/src/bottom_pane/panes/request_user_input/render.rs](code-rs/tui/src/bottom_pane/panes/request_user_input/render.rs)                         |
| Auto Drive settings          | [code-rs/core/src/config_types.rs](code-rs/core/src/config_types.rs) `AutoDriveSettings`                                                                 |
| Auto Drive continue timing   | Same file, `AutoDriveContinueMode` enum (Immediate/TenSeconds/SixtySeconds/Manual)                                                                       |
| Bottom pane container        | [code-rs/tui/src/bottom_pane/mod.rs](code-rs/tui/src/bottom_pane/mod.rs) (`ActiveViewKind`)                                                              |
| BottomPaneView trait         | [code-rs/tui/src/bottom_pane/bottom_pane_view.rs](code-rs/tui/src/bottom_pane/bottom_pane_view.rs)                                                       |
| App event enum               | [code-rs/tui/src/app_event.rs](code-rs/tui/src/app_event.rs)                                                                                             |
| History cells (for past Q&A) | [code-rs/tui/src/history_cell/](code-rs/tui/src/history_cell/)                                                                                           |
| Reusable list/selection UI   | [code-rs/tui/src/components/](code-rs/tui/src/components/)                                                                                               |

### Example: Agent Asks Ranked Choice (Post-Path-A)

Agent-side tool call JSON:

```json
{
  "tool": "request_user_input",
  "questions": [{
    "id": "test_scope",
    "header": "Tests to run",
    "question": "Which test types should I run? Rank by priority.",
    "isRanked": true,
    "allowExclude": true,
    "options": [
      {"label": "unit", "description": "Fast feedback"},
      {"label": "integration", "description": "Realistic"},
      {"label": "e2e", "description": "Slowest"}
    ]
  }]
}
```

TUI renders:
```
┌─ User input ────────────────────────────────────────┐
│ Question 1/1                                        │
│ Tests to run                                        │
│ Which test types should I run? Rank by priority.    │
│                                                     │
│  1. unit           Fast feedback                    │
│  2. integration    Realistic                        │
│   ✗ e2e            Slowest                          │
│                                                     │
│ ↑↓ select │ Alt+↑↓ reorder │ x exclude │ Enter submit│
└─────────────────────────────────────────────────────┘
```

Response back to agent:

```json
{
  "answers": {
    "test_scope": {
      "answers": ["unit", "integration"]
    }
  }
}
```

The agent gets ordered labels (excluded `e2e` omitted). Existing response schema; no new tool; one turn of agent time.

---

## Why This Matters

The fundamental problem is a communication asymmetry: the agent can produce very
specific, consequential actions, but the human's input is usually coarse and
ambiguous. Better interaction patterns close that gap by making it cheap to be
precise.

The biggest gains come from letting the human communicate:

| What to communicate        | Why it matters                                |
| -------------------------- | --------------------------------------------- |
| **priority**               | agent focuses on what counts                  |
| **constraints**            | prevents wasted work on invalid paths         |
| **approval boundaries**    | keeps human in control without micromanaging  |
| **preference persistence** | avoids re-litigating the same questions       |
| **confidence / firmness**  | agent doesn't overfit to offhand comments     |
| **autonomy boundaries**    | reduces anxiety during long runs              |
| **task resumption state**  | makes multi-session collaboration sustainable |
| **agent reasoning**        | users trust what they can understand          |
| **question batch timing**  | minimizes flow interruption, respects focus   |
| **error recovery path**    | reduces frustration and stuck states          |

Done well, these patterns reduce back-and-forth, improve correctness, make agent
behavior transparent and predictable, and make collaboration feel intentional
rather than improvisational.

---

## Implementation Checklist (CodexCLI-Specific)

### Before Starting
- [ ] Read `bottom_pane_view.rs` — understand the BottomPaneView trait
- [ ] Study `user_approval_widget.rs` — see how an existing modal works
- [ ] Review one test in `vt100_chatwidget_snapshot.rs` — understand snapshot testing
- [ ] Know the difference between `handle_key_event` and `handle_key_event_with_result`

### Architecture & Data
- [ ] Question has stable `id` (String, UUID, or similar) for recovery
- [ ] All answer types derive `Serialize + Deserialize`
- [ ] AppEvent variants added (AskQuestion, QuestionAnswered, QuestionCancelled)
- [ ] ActiveViewKind enum extended with new question type
- [ ] View struct derives Debug and implements BottomPaneView trait

### View Implementation
- [ ] Implements all BottomPaneView methods (even if no-op)
- [ ] `handle_key_event_with_result` returns ConditionalUpdate (not bool)
- [ ] Esc key cancels and sends QuestionCancelled event
- [ ] Enter/Ctrl-C handled explicitly
- [ ] `is_complete()` returns true only after submit/cancel
- [ ] `desired_height()` calculates height correctly (avoid overflow)
- [ ] `render()` uses ratatui widgets (Paragraph, Block, etc.)
- [ ] Focus always visible (cursor, highlight, or inverted colors)

### Event Handling
- [ ] AppEvent::AskQuestion routed through app.rs
- [ ] AppEvent::QuestionAnswered goes to agent execution context
- [ ] AppEvent::QuestionCancelled unblocks execution gracefully
- [ ] No direct pointers between views; all communication via AppEvent

### UX & Accessibility
- [ ] All controls keyboard-operable (no mouse-only features)
- [ ] Color + symbol always used together (test with NO_COLOR=1)
- [ ] Focus indicator works in both color and monochrome modes
- [ ] Status text (headers, footers) clearly states what's happening
- [ ] Error messages don't blame user
- [ ] Reversibility stated explicitly if applicable
- [ ] Batch operations show summary: "X items pending" style

### History Integration
- [ ] HistoryCell enum extended with Question variant (optional but good)
- [ ] Question + answer recorded in history for replay
- [ ] Timestamp and scope tracked for provenance

### Testing (Required)
- [ ] Unit tests in view module (focus movement, validation)
- [ ] At least one VT100 snapshot test (initial state + one interaction)
- [ ] Accessibility test: verify NO_COLOR rendering
- [ ] Edge case: empty list, single item, very long labels
- [ ] All tests pass: `cargo test -p code-tui --lib <view_name>`

### Performance
- [ ] Views with >20 items use lazy rendering (don't render off-screen)
- [ ] No allocations in hot paths (render per frame)
- [ ] Serialization/deserialization tested with large payloads

### Code Quality
- [ ] No new compiler warnings: `./build-fast.sh` passes cleanly
- [ ] Follows code-rs naming conventions (snake_case, no abbreviations)
- [ ] Comments explain non-obvious logic (but not every line)
- [ ] Trait methods documented (doc comments on pub functions)

### Before Shipping
- [ ] Branch passes `./build-fast.sh` cleanly
- [ ] All snapshots reviewed and accepted
- [ ] Manual test in terminal (Alt-Tab with other windows, resize, paste)
- [ ] Test with real agent running AutoDrive
- [ ] Review diff: only view + types + tests, no unrelated changes

---

## Implementation Checklist: Per-Pattern

### Ranked Choice
- [ ] Position numbers update live during reordering
- [ ] Alt+↑/↓ or J/K both work
- [ ] Excluded items visually distinct
- [ ] Summary bar shows ratio (3 ranked / 2 excluded)

### Batch Questions
- [ ] Tab cycles focus to next Q
- [ ] Last Q → Enter submits batch (not just any Q)
- [ ] Esc cancels entire batch
- [ ] Blocker questions never batched

### Conditional Branching
- [ ] Breadcrumb shows path: "Q1: X > Q2 > Current"
- [ ] Invalidated Qs show "This no longer applies" not silent removal
- [ ] Back button goes back 1 level only
- [ ] Re-evaluation shows diff: "Q3 changed because..."

### Scope-of-Answer
- [ ] Default scope is this-task (never silent global)
- [ ] Scope tag visible where preference applied
- [ ] Revocation one command (`/preferences`)

### Reversibility
- [ ] Permanent actions show alternatives (delete vs. trash)
- [ ] Reversible actions show undo button for window duration
- [ ] Window greys out (not removed) after expiry
- [ ] Session-only actions marked with ⚠

### Approval List
- [ ] Risk level per item (low/medium/high)
- [ ] Batch actions: A/R for approve/reject all, A-l for low-risk only
- [ ] Summary: "12 approved, 3 rejected, 2 pending"
- [ ] Undo shows as single operation

### Notification Digest
- [ ] Urgent = modal (blocks)
- [ ] Normal = toast (5s auto-dismiss)
- [ ] Low = digest only (manual drain with 'd')
- [ ] Never auto-dismiss errors

---

## Quick Start: Implementing Ranked Choice via Path A

### Goal: Add ranked-choice questions in a single branch

Leverage the existing `RequestUserInputQuestion` — no new pane, no new tool.

### File-by-File Change List

| File                                                                    | Change                                                                                                 |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `code-rs/protocol/src/request_user_input.rs`                            | Add `is_ranked` and `allow_exclude` fields to `RequestUserInputQuestion`                               |
| `code-rs/core/src/tools/handlers/request_user_input.rs`                 | Pass new fields through tool args deserialization                                                      |
| `code-rs/tui/src/bottom_pane/panes/request_user_input/mod.rs`           | Add `rank_order: Vec<usize>` and `excluded_options: BTreeSet<usize>` to `AnswerState`                  |
| `code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs`         | Add `current_is_ranked()`, `move_ranked_item_up/down()`, `toggle_exclude_current()`; extend `submit()` |
| `code-rs/tui/src/bottom_pane/panes/request_user_input/pane_impl.rs`     | Add Alt+Up/Down and `x` key handling                                                                   |
| `code-rs/tui/src/bottom_pane/panes/request_user_input/render.rs`        | Branch on `is_ranked` for row rendering and footer hints                                               |
| `code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs` (tests) | Add 3-4 unit tests for rank/exclude/submit                                                             |
| `code-rs/tui/tests/vt100_ranked_input.rs` (new)                         | VT100 snapshot tests for new render branch                                                             |

The existing tests in `model.rs` stay green; only new tests are added.

### Step-by-Step Plan

**Phase 1 — Protocol + Handler**

1. Read [request_user_input.rs](code-rs/protocol/src/request_user_input.rs) end-to-end
2. Add `is_ranked` and `allow_exclude` fields (with `#[serde(default)]` for back-compat)
3. Update the tool handler's deserialization struct
4. Run: `./build-fast.sh` — verify no warnings
5. Commit: `feat(protocol/request_user_input): add is_ranked and allow_exclude fields`

**Phase 2 — View State**

1. Read [model.rs](code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs) end-to-end — understand `submit()`
2. Add `rank_order` and `excluded_options` to `AnswerState`
3. Initialize them in `new()` based on question options length
4. Add `current_is_ranked()` and `current_allows_exclude()` helpers
5. Write unit test: `ranked_question_defaults_to_option_order`
6. Commit: `feat(tui/request_user_input): track rank order and excluded options`

**Phase 3 — Submit Logic + Movement**

1. Implement `move_ranked_item_up/down()` and `toggle_exclude_current()`
2. Extend `submit()` with the `if question.is_ranked` branch
3. Add unit tests:
   - `ranked_question_submits_in_rank_order`
   - `ranked_question_excluded_items_omitted`
   - `ranked_move_up_at_top_is_noop`
   - `ranked_move_down_at_bottom_is_noop`
4. Commit: `feat(tui/request_user_input): implement rank reorder and exclude submit`

**Phase 4 — Key Handling**

1. Read [pane_impl.rs](code-rs/tui/src/bottom_pane/panes/request_user_input/pane_impl.rs) — note the match order matters
2. Add Alt+Up/Down branches (**before** the existing Up/Down branches)
3. Add `x` key branch when `allow_exclude`
4. Test manually with `code-dev` (requires a scripted question that sets `is_ranked: true`)
5. Commit: `feat(tui/request_user_input): add alt+up/down reorder and x exclude keys`

**Phase 5 — Rendering + Footer**

1. Read [render.rs](code-rs/tui/src/bottom_pane/panes/request_user_input/render.rs) end-to-end
2. Branch inside the options-rendering block on `current_is_ranked()`
3. Build position-numbered `GenericDisplayRow`s from `rank_order`
4. Update the footer hint to show "Alt+↑/↓ reorder" and "x exclude" when applicable
5. Visual test: run in `code-dev`, verify rendering at 80/120/200 cols
6. Commit: `feat(tui/request_user_input): render rank positions and exclude state`

**Phase 6 — VT100 Snapshots**

1. Read an existing VT100 test (e.g., `vt100_chatwidget_snapshot.rs`) to see the harness API
2. Create `code-rs/tui/tests/vt100_ranked_input.rs` with 4-5 scenarios:
   - Initial render
   - After one Alt+Down reorder
   - After excluding item 2
   - Narrow terminal (80 cols)
   - Wide terminal (200 cols)
3. Accept snapshots: `cargo insta review --workspace-root code-rs`
4. Commit: `test(tui/request_user_input): add vt100 snapshots for ranked choice`

**Phase 7 — Documentation + PR**

1. Update the tool schema doc comment in [handlers/request_user_input.rs](code-rs/core/src/tools/handlers/request_user_input.rs) if the agent prompt references tool args
2. Run `./build-fast.sh` one final time
3. Open PR with clear description of the schema change and UI additions
4. Link this doc as the design reference

### Checkpoints (Do Not Merge Without)

- [ ] `./build-fast.sh` passes with no new warnings
- [ ] `cargo test -p code-tui --features test-helpers --lib bottom_pane::panes::request_user_input`
- [ ] `cargo test -p code-tui --test vt100_ranked_input --features test-helpers`
- [ ] All new snapshots accepted (no `.snap.new` files)
- [ ] Manual test in `code-dev`: ranked question renders, reorders, submits correct order
- [ ] Schema back-compat: run an existing multi-select question, verify unchanged behavior
- [ ] Footer hint legible at 80-column width
- [ ] No direct mutation of outer `BottomPane` state from view (stays self-contained)

---

## How to Add a Question Type: Step-by-Step

This walkthrough shows **two paths** — extending RequestUserInput (simpler) and creating a new pane (more flexible).

### Path A: Extend `RequestUserInputQuestion` with Ranked Choice

This is the **recommended approach** for patterns that can be expressed as ordered options.

#### A.1 Extend the Protocol Schema

[code-rs/protocol/src/request_user_input.rs:15](code-rs/protocol/src/request_user_input.rs#L15)

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
pub struct RequestUserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    #[serde(rename = "isOther", default)]
    pub is_other: bool,
    #[serde(rename = "isSecret", default)]
    pub is_secret: bool,
    #[serde(rename = "allowMultiple", default)]
    pub allow_multiple: bool,
    // NEW FIELDS (backward compatible via #[serde(default)]):
    #[serde(rename = "isRanked", default)]
    #[schemars(rename = "isRanked")]
    #[ts(rename = "isRanked")]
    pub is_ranked: bool,                              // NEW: user ranks options 1..N
    #[serde(rename = "allowExclude", default)]
    #[schemars(rename = "allowExclude")]
    #[ts(rename = "allowExclude")]
    pub allow_exclude: bool,                          // NEW: items can be marked "skip"
    pub options: Option<Vec<RequestUserInputQuestionOption>>,
}
```

**Why this way:** All existing callers work unchanged (new fields default to false).
The TUI branches on `is_ranked` to swap render logic; answer serialization uses existing `Vec<String>` with rank order.

#### A.2 Extend Tool Handler Deserialization

[code-rs/core/src/tools/handlers/request_user_input.rs:19](code-rs/core/src/tools/handlers/request_user_input.rs#L19)

```rust
#[derive(Debug, Deserialize)]
struct RequestUserInputToolQuestion {
    id: String,
    header: String,
    question: String,
    #[serde(default = "default_allow_freeform")]
    allow_freeform: bool,
    #[serde(default)]
    allow_multiple: bool,
    #[serde(default)]
    is_secret: bool,
    #[serde(default)]
    is_ranked: bool,            // NEW
    #[serde(default)]
    allow_exclude: bool,        // NEW
    #[serde(default)]
    options: Option<Vec<RequestUserInputToolOption>>,
}
```

Then pass these through when building `RequestUserInputQuestion`.

#### A.3 Extend `AnswerState` for Ranking

[code-rs/tui/src/bottom_pane/panes/request_user_input/mod.rs:12](code-rs/tui/src/bottom_pane/panes/request_user_input/mod.rs#L12)

```rust
#[derive(Debug, Clone)]
struct AnswerState {
    option_state: ScrollState,
    checked_options: BTreeSet<usize>,
    hover_option_idx: Option<usize>,
    freeform: String,
    // NEW:
    rank_order: Vec<usize>,          // indices in user-defined order (when is_ranked)
    excluded_options: BTreeSet<usize>,  // indices marked "skip" (when allow_exclude)
}
```

Initialize `rank_order` in `new()` based on the question options length.

#### A.4 Add Reorder Operations to the Model

[code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs](code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs)

```rust
impl RequestUserInputView {
    pub(super) fn current_is_ranked(&self) -> bool {
        self.current_question().is_some_and(|q| q.is_ranked)
    }

    pub(super) fn current_allows_exclude(&self) -> bool {
        self.current_question().is_some_and(|q| q.allow_exclude)
    }

    pub(super) fn move_ranked_item_up(&mut self) {
        let Some(answer) = self.current_answer_mut() else { return; };
        let Some(selected) = answer.option_state.selected_idx else { return; };
        if selected == 0 { return; }
        answer.rank_order.swap(selected, selected - 1);
        answer.option_state.selected_idx = Some(selected - 1);
    }

    pub(super) fn move_ranked_item_down(&mut self) {
        let Some(answer) = self.current_answer_mut() else { return; };
        let Some(selected) = answer.option_state.selected_idx else { return; };
        if selected + 1 >= answer.rank_order.len() { return; }
        answer.rank_order.swap(selected, selected + 1);
        answer.option_state.selected_idx = Some(selected + 1);
    }

    pub(super) fn toggle_exclude_current(&mut self) {
        let Some(answer) = self.current_answer_mut() else { return; };
        let Some(selected) = answer.option_state.selected_idx else { return; };
        let Some(&item_idx) = answer.rank_order.get(selected) else { return; };
        if !answer.excluded_options.insert(item_idx) {
            answer.excluded_options.remove(&item_idx);
        }
    }
}
```

#### A.5 Extend `submit()` to Emit Ranked Answers

[code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs:324](code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs#L324)

Inside `submit()`, add a branch for ranked questions:

```rust
if question.is_ranked {
    // Answer is option labels in rank order (excluded items omitted)
    for &item_idx in &answer_state.rank_order {
        if answer_state.excluded_options.contains(&item_idx) {
            continue;
        }
        if let Some(opt) = question.options.as_ref().and_then(|opts| opts.get(item_idx)) {
            answer_list.push(opt.label.clone());
        }
    }
} else if let Some(options) = options {
    // ... existing multi-select / single-select handling
}
```

The agent gets back a `Vec<String>` of labels in preference order — simple and schema-compatible.

#### A.6 Add Key Handlers

[code-rs/tui/src/bottom_pane/panes/request_user_input/pane_impl.rs:33](code-rs/tui/src/bottom_pane/panes/request_user_input/pane_impl.rs#L33)

Add these branches to `handle_key_event`:

```rust
// Inside the match on key_event.code:
KeyCode::Up if self.current_is_ranked() && key_event.modifiers.contains(KeyModifiers::ALT) => {
    self.move_ranked_item_up();
}
KeyCode::Down if self.current_is_ranked() && key_event.modifiers.contains(KeyModifiers::ALT) => {
    self.move_ranked_item_down();
}
KeyCode::Char('x') if self.current_allows_exclude() => {
    self.toggle_exclude_current();
}
```

**Check key order** — `KeyCode::Up` with Alt modifier must come *before* the existing `KeyCode::Up` branch or it won't match.

#### A.7 Extend Render Logic

[code-rs/tui/src/bottom_pane/panes/request_user_input/render.rs:138](code-rs/tui/src/bottom_pane/panes/request_user_input/render.rs#L138)

In the options-rendering block, branch when `is_ranked`:

```rust
if let Some(options) = options.filter(|opts| !opts.is_empty()) {
    if self.current_is_ranked() {
        // Render items in rank_order with position numbers
        let rank_order = self.current_answer()
            .map(|a| a.rank_order.clone())
            .unwrap_or_else(|| (0..options.len()).collect());
        let excluded = self.current_answer()
            .map(|a| a.excluded_options.clone())
            .unwrap_or_default();

        let rows = rank_order.iter().enumerate().map(|(pos, &item_idx)| {
            let opt = &options[item_idx];
            let is_excluded = excluded.contains(&item_idx);
            let position_prefix = if is_excluded { "  ✗".to_string() } else { format!("{:>2}.", pos + 1) };
            GenericDisplayRow {
                name: format!("{} {}", position_prefix, opt.label),
                description: Some(opt.description.clone()),
                match_indices: None,
                is_current: false,
                name_color: if is_excluded { Some(Color::DarkGray) } else { None },
            }
        }).collect::<Vec<_>>();

        render_rows(content_rect, buf, &rows, &state, rows.len().max(1), false);
    } else {
        // Existing multi-select / single-select render path (unchanged)
    }
}
```

Also update the **footer hint** to show ranking shortcuts when `is_ranked`:

```rust
let footer = if self.current_is_ranked() {
    format!(
        "{ud} select | Alt+{ud} reorder | x exclude | Enter {enter_label} | Esc cancel",
        ud = crate::icons::nav_up_down(),
    )
} else if has_options {
    // ... existing variants
};
```

#### A.8 Update `desired_height_for_width`

[code-rs/tui/src/bottom_pane/panes/request_user_input/render.rs:13](code-rs/tui/src/bottom_pane/panes/request_user_input/render.rs#L13)

No change required — ranked options use the same row count as the existing options path.
Verify `options_len` computation handles the new case (it does; it just counts options).

#### A.9 Add Tests

Extend the tests in [code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs:391](code-rs/tui/src/bottom_pane/panes/request_user_input/model.rs#L391):

```rust
#[test]
fn ranked_question_submits_in_rank_order() {
    let (mut view, rx) = make_view(RequestUserInputQuestion {
        id: "priorities".to_owned(),
        header: "Priorities".to_owned(),
        question: "Rank these tasks".to_owned(),
        is_other: false,
        is_secret: false,
        allow_multiple: false,
        is_ranked: true,
        allow_exclude: false,
        options: Some(vec![
            option("auth", "Auth module"),
            option("token", "Token handling"),
            option("middleware", "Middleware"),
        ]),
    });

    // Move "token" to position 0 (before "auth")
    view.move_selection(false); // select index 1 = "token"
    view.move_ranked_item_up();

    view.submit();
    let AppEvent::RequestUserInputAnswer { response, .. } = rx.recv().unwrap() else {
        panic!("expected answer event");
    };
    assert_eq!(
        response.answers["priorities"].answers,
        vec!["token".to_owned(), "auth".to_owned(), "middleware".to_owned()],
    );
}

#[test]
fn ranked_question_excluded_items_omitted() {
    let (mut view, rx) = make_view(RequestUserInputQuestion {
        is_ranked: true,
        allow_exclude: true,
        // ...
    });
    view.toggle_exclude_current();
    view.submit();
    // Assert that excluded item label is NOT in the answer list
}
```

#### A.10 Add VT100 Snapshot Test

Create [code-rs/tui/tests/vt100_ranked_input.rs](code-rs/tui/tests/vt100_ranked_input.rs) following the pattern in [code-rs/tui/tests/vt100_chatwidget_snapshot.rs](code-rs/tui/tests/vt100_chatwidget_snapshot.rs):

```rust
use code_tui::test_helpers::ChatWidgetHarness;
use code_protocol::request_user_input::{RequestUserInputEvent, RequestUserInputQuestion, RequestUserInputQuestionOption};

#[test]
fn ranked_choice_renders_position_numbers() {
    let mut harness = ChatWidgetHarness::new();
    harness.emit_request_user_input(RequestUserInputEvent {
        call_id: "test".to_owned(),
        turn_id: "t1".to_owned(),
        questions: vec![RequestUserInputQuestion {
            id: "rank".to_owned(),
            header: "Rank Tasks".to_owned(),
            question: "Order these by priority".to_owned(),
            is_other: false,
            is_secret: false,
            allow_multiple: false,
            is_ranked: true,
            allow_exclude: false,
            options: Some(vec![
                RequestUserInputQuestionOption {
                    label: "auth".to_owned(),
                    description: "Auth module".to_owned(),
                },
                RequestUserInputQuestionOption {
                    label: "token".to_owned(),
                    description: "Token handling".to_owned(),
                },
            ]),
        }],
    });

    let frame = harness.render_to_vt100(80, 20);
    insta::assert_snapshot!("ranked_choice_initial", frame);
}
```

### Path B: Create a Separate Pane (Checkpoint Configuration)

This is the approach for **non-agent-driven UX** — like checkpoint config, reversibility indicator.
The user triggers these via slash commands or automatically on Auto Drive start.

#### B.1 Create Directory Structure

```
code-rs/tui/src/bottom_pane/panes/checkpoint_config/
├── mod.rs           (struct + BottomPaneView impl; export new())
├── model.rs         (state transitions: toggle, save, load from config)
├── pane_impl.rs     (key/mouse event handling)
└── render.rs        (grid rendering for action-kind × auto-or-ask matrix)
```

Follow the pattern of [approval_modal/](code-rs/tui/src/bottom_pane/panes/approval_modal/) or [request_user_input/](code-rs/tui/src/bottom_pane/panes/request_user_input/).

#### B.2 Reuse Existing Components

```rust
use crate::components::scroll_state::ScrollState;
use crate::components::selection_popup_common::{render_rows, GenericDisplayRow};
use crate::components::popup_frame::themed_block;

pub(crate) struct CheckpointConfigView {
    app_event_tx: AppEventSender,
    // Checkpoint config structure:
    continue_auto: BTreeSet<ActionKind>,     // auto-continue without asking
    ask_before: BTreeSet<ActionKind>,        // ask before doing
    never_approve: BTreeSet<ActionKind>,     // never do without explicit per-action approval
    focused_row: ScrollState,
    focused_col: usize,  // 0=continue, 1=ask, 2=never
    complete: bool,
}
```

#### B.3 Extend `ActiveViewKind`

[code-rs/tui/src/bottom_pane/mod.rs:46](code-rs/tui/src/bottom_pane/mod.rs#L46)

```rust
pub(crate) enum ActiveViewKind {
    None,
    AutoCoordinator,
    ModelSelection,
    RequestUserInput,
    ShellSelection,
    CheckpointConfig,  // NEW
    Other,
}
```

#### B.4 Save/Load from Config

Extend [code-rs/core/src/config_types.rs](code-rs/core/src/config_types.rs) `AutoDriveSettings`:

```rust
pub struct AutoDriveSettings {
    // ... existing fields ...
    #[serde(default)]
    pub checkpoint_config: CheckpointConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct CheckpointConfig {
    pub continue_auto: Vec<ActionKind>,
    pub ask_before: Vec<ActionKind>,
    pub never_approve: Vec<ActionKind>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ActionKind {
    FileRead,
    FileWrite,
    FileDelete,
    SchemaChange,
    NewDependency,
    TestRun,
    CommandExec,
    Deploy,
}
```

#### B.5 Wire via AppEvent

Extend [code-rs/tui/src/app_event.rs:206](code-rs/tui/src/app_event.rs#L206) (AppEvent enum):

```rust
pub(crate) enum AppEvent {
    // ... existing variants ...
    ShowCheckpointConfig,                     // user invoked settings
    CloseCheckpointConfig,                    // user closed the pane
    CheckpointConfigSaved(CheckpointConfig),  // save + persist to config
}
```

Then handle in the app event loop to open/close the pane.

---

---

## Pattern-to-Implementation Cross-Reference

Quick lookup: for each pattern in this document, where to start in code.

| Pattern                     | Best Path             | Entry file                              | Existing Analog               |
| --------------------------- | --------------------- | --------------------------------------- | ----------------------------- |
| Single choice               | Already works         | `request_user_input/render.rs`          | `allow_multiple: false`       |
| Multi-select                | Already works         | `request_user_input/render.rs`          | `allow_multiple: true`        |
| Freeform text               | Already works         | `request_user_input/render.rs`          | `options: None`               |
| Secret input                | Already works         | `request_user_input/render.rs`          | `is_secret: true`             |
| Ranked choice               | **A**                 | `request_user_input/*` (extend)         | N/A — add `is_ranked`         |
| Pairwise comparison         | B                     | New pane + new tool                     | N/A                           |
| Required/Preferred tagging  | **A**                 | Extend `RequestUserInputQuestionOption` | N/A                           |
| Budget/tradeoff presets     | **A**                 | Use existing single-choice              | Options as labels             |
| Matrix questions            | B                     | New pane (2D grid widget)               | `ScrollState` is 1D           |
| Conditional branching       | B                     | New tool + coordinator logic            | N/A                           |
| Fill-in templates           | **A**                 | Multi-question with freeform            | Already works                 |
| Constraint capture          | **A**                 | Multi-question with `is_other`          | Already works                 |
| Scope-of-answer             | **A**                 | Add `remember_as` field                 | N/A                           |
| Confidence tagging          | **A**                 | Per-answer `confidence` field           | N/A                           |
| Exception/carve-out         | B                     | New tool for rule+exception pairs       | N/A                           |
| Checkpoint config           | **C**                 | New pane + `AutoDriveSettings`          | `AutoDriveContinueMode`       |
| Disambiguation              | **A**                 | Use single-choice with context          | Already works                 |
| Typeahead/fuzzy             | Integrate existing    | `components/` search widgets            | file_popup has pattern        |
| Per-item approve/reject     | Extend                | `bottom_pane/panes/approval_modal/`     | Already has queue             |
| Decision cards              | New history_cell      | `history_cell/`                         | `proposed_plan.rs` pattern    |
| Side-by-side compare        | New history_cell      | `history_cell/`                         | N/A                           |
| Multi-step wizard           | Extend                | Multi-question flow already works       | PgUp/PgDn nav exists          |
| Inline progress + interrupt | Already works         | Status bar in `chat_composer/status.rs` | `task_running.rs`             |
| Contextual help             | Extend                | Inline `?` key handler                  | Footer already has `?` hint   |
| Preview before apply        | Extend                | `history_cell/patch.rs`                 | Diff rendering exists         |
| Editable answer summary     | Extend                | Multi-question review screen            | Would need new view           |
| Provenance log              | New history_cell      | `history_cell/`                         | Use append-only JSON          |
| `/revise` command           | Extend slash commands | `slash_command.rs`                      | Pattern for `/revise`         |
| Working-agreement panel     | **C**                 | `bottom_pane/chrome.rs`                 | Chrome slot exists            |
| Reversibility indicator     | New history_cell      | `history_cell/`                         | N/A                           |
| Undo/redo as confirmation   | Extend                | `history_cell/` + ghost commits         | `GhostCommit` exists          |
| Optimistic UI               | Already works         | N/A                                     | Widely used                   |
| Toast vs modal              | Already distinct      | `notifications/` settings               | `EmitTuiNotification` event   |
| Plan-as-artifact            | Extend                | `history_cell/proposed_plan.rs`         | Starting point exists         |
| Inline audit trail          | Extend                | `history_cell/exec.rs`                  | Exec cells track undos        |
| Task timeline               | Extend                | `history_cell/`                         | `auto_drive.rs` tracks phases |
| Task resumption             | Already works         | `resume/` module                        | `ResumeCandidate` exists      |
| Agent narration             | Already works         | `history_cell/reasoning.rs`             | Reasoning cells exist         |
| Pre-task expectation        | **C**                 | New "plan card" before execution        | N/A                           |
| Post-task decision log      | New history_cell      | `history_cell/`                         | Could reuse summary cells     |
| Multi-agent delegation      | Existing              | `agent/` in chatwidget                  | Already shown                 |
| Classified errors           | Extend                | `history_cell/` error variants          | Error rendering exists        |
| Streaming state machine     | Already works         | `chatwidget/streaming.rs`               | Four states present           |

**Convention:** **Bold letter** = recommended path. `A` = extend RequestUserInput schema. `B` = new tool. `C` = new pane.

---

## Key Research Sources

| Area                    | Source                                                                                                 |
| ----------------------- | ------------------------------------------------------------------------------------------------------ |
| Mixed-initiative design | Hearst (1999) "Mixed-initiative interaction" *IEEE Intelligent Systems*; Horvitz (1999) CHI '99        |
| Interruption science    | Mark & Gonzalez (2005) CHI 2005; Horvitz, Koch & Apacible (2004) CSCW; Gopher et al. (2000)            |
| Neurodiversity          | ResearchGate (2025) "Adaptive UX Frameworks for Neurodivergent Users"; devqube.com; arXiv:2507.06864   |
| Mental model alignment  | Norman (1988) *The Design of Everyday Things*; uxmatters.com (2025); MIT AI Agent Index (2025)         |
| Error taxonomy          | Nielsen (1994) Heuristic #9; logrocket.com; smashingmagazine.com                                       |
| Multi-agent UX          | Dibia (2025) AI.Engineer keynote; Google Cloud multi-agent architecture docs; WEF (2025)               |
| Streaming UX            | proagenticworkflows.ai (2025) "Best Practices: Streaming LLM Responses"                                |
| TUI input               | ratatui.rs / crossterm bracketed-paste docs; tui-textarea crate; tui-chat crate                        |
| Decision science        | Kahneman (2011) *Thinking, Fast and Slow*; Thaler & Sunstein (2008) *Nudge*; Cowan (2001) "4±1 chunks" |
