I audited the **`code-rs` memory settings and memory-prompt path only**. I did not treat `codex-rs` as ground truth beyond a quick terminology check.

This is a **static audit** of the forked codebase. I could not run `cargo test` or exercise the TUI because Rust is still unavailable in this container.

## Highest-priority findings

### 1) `phase_1_model` / `phase_2_model` are exposed in the schema, but they are not wired at all

This is the clearest correctness/config issue.

The JSON schema advertises two memory-model settings:

* `phase_1_model` — “Model used for thread summarisation”
* `phase_2_model` — “Model used for memory consolidation”

in `core/config.schema.codex.json:466-472`.

But `MemoriesToml` in `core/src/config_types.rs:121-130` has **no fields** for either setting, and `MemoriesConfig` in `core/src/config_types.rs:133-141` also has no place to store them.

It gets worse: the config persistence layer explicitly **removes** those keys on write:

* `core/src/config/sources.rs:3878-3881`

So today the behavior is:

* users can discover these settings through schema/completions
* the program does not read them
* saving settings will silently delete them from config

That is a real footgun.

**Recommendation:** either fully wire these fields into `MemoriesToml`/`MemoriesConfig` and the consolidation pipeline, or remove them from the schema entirely until implemented.

---

### 2) Pinned memories can blow past the stated 12 KB prompt budget

The prompt system declares a hard budget:

* `core/src/memories/prompts.rs:12-15` → `MAX_MEMORY_PROMPT_BYTES = 12_000`

But in `select_prompt_entries()` the code injects **all pinned memories first** with no byte check:

* `core/src/memories/manifest.rs:142-158`

Only after that does it apply fit logic to auto-extracted epochs:

* `core/src/memories/manifest.rs:185-195`

The TUI also tells users pinned memories are “always injected into LLM prompt”:

* `tui/src/bottom_pane/settings_pages/memories/model.rs:452`
* `tui/src/bottom_pane/settings_pages/memories/pages.rs:497-499`

So the current design is effectively:

* auto memories are budgeted
* pinned memories are not

That means one large pinned note, or several medium ones, can crowd out all auto memories and exceed the intended memory-prompt budget.

**Recommendation:** budget pinned memories too. The least surprising policy is:

1. reserve a capped portion for pinned memories,
2. truncate or rank them,
3. then fill the remaining budget with auto-extracted entries.

---

### 3) If shell style is unknown, memory injection can disappear entirely, including pinned memories

`select_prompt_entries()` starts with:

* `let target_shell = context.shell_style?;`
* `core/src/memories/manifest.rs:132-138`

That means if `shell_style` is `None`, the function returns `None` **before** it even reaches the pinned-memory injection block.

So despite the UI language saying pinned memories are always injected, they are actually contingent on the runtime being able to infer a shell style.

This is subtle, but it is a real correctness/UX mismatch. It will show up most easily in odd shells, partial environment snapshots, embedded runtimes, or future platforms.

**Recommendation:** inject pinned memories even when shell matching for auto memories cannot run. The early return should happen only for the auto-ranked epoch selection, not for pinned memories.

---

### 4) The UI overstates what “always injected” means

The TUI repeatedly says pinned memories are always injected:

* `tui/src/bottom_pane/settings_pages/memories/model.rs:452`
* `tui/src/bottom_pane/settings_pages/memories/pages.rs:497-499`

But prompt injection is globally gated by `use_memories`:

* `core/src/codex/streaming/turn/mod.rs:71-85`

So if `use_memories = false`, pinned memories are not injected at all.

That is probably intended behavior, but the copy is misleading. Right now the UI describes a stronger guarantee than the code actually provides.

**Recommendation:** change the copy to something like:

* “Pinned memories are injected whenever memory usage is enabled.”
  or
* “Pinned memories are prioritized in the memory prompt.”

---

## Medium-priority observations

### 5) The naming/surface area is a bit inconsistent

You currently have both:

* `max_raw_memories_for_consolidation`
* deprecated alias `max_raw_memories_for_global`

The alias handling itself is fine in `core/src/config_types.rs:177-181`, and the settings writer cleans up the legacy name in `core/src/config/sources.rs:3876`. That part is sensible.

But combined with the dead `phase_1_model` / `phase_2_model` keys, the memory config surface feels half-migrated. The risk is that users and future maintainers cannot tell which keys are supported, deprecated, or vestigial.

**Recommendation:** do one cleanup pass:

* keep the alias only for read compatibility,
* never advertise dead keys in schema,
* document the canonical memory keys in one place.

---

### 6) “Skip MCP/web search” is implemented as session pollution, not a per-epoch filter

The description says:

* “Sessions that use MCP tools or web search are skipped during extraction”
* `tui/src/bottom_pane/settings_pages/memories/model.rs:447`

And the actual behavior is that the session gets marked polluted at runtime:

* web search: `core/src/codex/streaming/turn/handle_item.rs:118-126`
* MCP tools: `core/src/tools/handlers/mcp.rs:47`
* mutation logic: `core/src/codex/session.rs:816-857`

That is a perfectly reasonable implementation, but the wording implies a simpler post-hoc extraction filter than what the code actually does.

This is more of a docs/UX precision issue than a bug.

---

## What looks solid

A few parts of the design are actually pretty good:

* Scope precedence is cleanly modeled through global/profile/project resolution in `core/src/config_types.rs:232+`.
* The TUI does a decent job surfacing effective values and their source.
* The polluted/disabled/enabled thread-state model is explicit and persisted.
* The generated snapshot structure (`memory_summary.md`, `raw_memories.md`, manifest, rollout summaries) is easy to reason about.

## My recommended fix order

Start here:

1. **Remove or wire `phase_1_model` / `phase_2_model`**
2. **Enforce byte budgeting for pinned memories**
3. **Allow pinned-memory injection even when shell detection fails**
4. **Fix the UI wording around “always injected”**
5. **Do one config-surface cleanup pass**

## Bottom line

The memory subsystem in `code-rs` is not broken overall, but the **settings surface is ahead of the implementation** in a couple places, and the **prompt-budget story is inconsistent**.

The two issues I’d call real bugs are:

* dead-but-advertised settings (`phase_1_model`, `phase_2_model`)
* pinned memories bypassing the prompt budget

The strongest UX bug is:

* “always injected” is not actually always true





I re-checked the files directly. The main thing that changed from my earlier pass is that the **schema drift is worse than I first said**, and the **prompt-budget bug is more specific**.

Here is the implementation-grade patch plan I would use.

## What the source says today

The current memory config/runtime contract is split across these files:

* `core/src/config_types.rs:121-141` defines the actual Rust memory config types.
* `core/src/config_types.rs:167-205` applies TOML into runtime config and serializes runtime config back to TOML.
* `core/src/config_types.rs:232-247` resolves global/profile/project precedence.
* `core/src/config.rs:1850-1857` applies the feature flag override that forces both `generate_memories` and `use_memories` off.
* `core/src/config/sources.rs:3859-3879` writes the `[memories]` table back to disk.
* `core/config.schema.codex.json:440-475` is the checked-in schema artifact.

The prompt selection path is here:

* `core/src/memories/prompts.rs:12-15` defines the 12 KB prompt budget.
* `core/src/memories/prompts.rs:64-75` calls `select_prompt_entries()` and uses that result directly.
* `core/src/memories/manifest.rs:132-225` does the real prompt-entry selection.
* `core/src/memories/mod.rs:812-860` builds the preview context with `shell_style: None` and calls the same selection logic.

The user-facing wording is here:

* `tui/src/bottom_pane/settings_pages/memories/model.rs:447-452`
* `tui/src/bottom_pane/settings_pages/memories/model.rs:589-592`
* `tui/src/bottom_pane/settings_pages/memories/pages.rs:497-499`
* `tui/src/bottom_pane/settings_pages/memories/pages.rs:581-582`

And the MCP/web-search exclusion mechanism is here:

* `core/src/codex/streaming/turn/handle_item.rs:120-123`
* `core/src/tools/handlers/mcp.rs:47`
* `core/src/codex/session.rs:816-860`

---

## PR 1: repair the config/schema contract

This is the highest-value first PR, because the checked-in schema and the runtime are not describing the same thing.

### The exact mismatch

`MemoriesToml` in Rust currently has these fields:

* `no_memories_if_mcp_or_web_search`
* `generate_memories`
* `use_memories`
* `max_raw_memories_for_consolidation`
* deprecated alias `max_raw_memories_for_global`
* `max_rollout_age_days`
* `max_rollouts_per_startup`
* `min_rollout_idle_hours`

That is in `core/src/config_types.rs:121-130`.

But the checked-in schema at `core/config.schema.codex.json:440-475` only exposes:

* `max_raw_memories_for_global`
* `max_rollout_age_days`
* `max_rollouts_per_startup`
* `min_rollout_idle_hours`
* `phase_1_model`
* `phase_2_model`

So the schema is doing three wrong things at once:

1. it **omits active runtime keys**:

   * `no_memories_if_mcp_or_web_search`
   * `generate_memories`
   * `use_memories`
   * `max_raw_memories_for_consolidation`

2. it **advertises dead keys**:

   * `phase_1_model`
   * `phase_2_model`

3. it advertises the **deprecated alias** but not the canonical key.

That is not just cosmetic. The write path in `core/src/config/sources.rs:3863-3867` persists the canonical key, and `core/src/config/sources.rs:3876-3879` explicitly removes the alias and the dead phase keys.

### Patch steps

#### 1. Make the checked-in schema match the Rust types

Edit `core/config.schema.codex.json` so the `MemoriesToml` properties reflect the actual live config surface.

At minimum, the schema should include:

* `no_memories_if_mcp_or_web_search`
* `generate_memories`
* `use_memories`
* `max_raw_memories_for_consolidation`
* `max_rollout_age_days`
* `max_rollouts_per_startup`
* `min_rollout_idle_hours`

and it should not expose `phase_1_model` or `phase_2_model` unless you are also wiring them end-to-end in runtime code.

#### 2. Decide how the deprecated alias is handled in schema

Runtime already accepts the alias in `core/src/config_types.rs:177-181` and suppresses it on write in `core/src/config_types.rs:200` and `core/src/config/sources.rs:3876`.

I would keep that runtime compatibility exactly as-is.

For the schema, I would do one of two things:

* **Preferred:** remove `max_raw_memories_for_global` from the schema entirely so editors stop suggesting it.
* **Acceptable fallback:** keep it in the schema but mark it deprecated if your schema consumer supports that.

The important thing is that the canonical key must be the one editors and docs point people toward.

#### 3. Remove `phase_1_model` and `phase_2_model` from the schema

Right now they are not present in `MemoriesToml`, not present in `MemoriesConfig`, and are stripped on write. That is a dead config surface.

If you actually want them, that is a separate feature PR. Do not leave them half-advertised.

### Tests to add

There are already useful config tests in `core/src/config_types.rs:250-301`. Extend that area and add one schema-artifact test.

I would add:

* `to_toml_uses_canonical_memory_limit_key_only`
* `schema_artifact_exposes_live_memory_keys`
* `schema_artifact_does_not_expose_dead_phase_model_keys`

That last test should read `core/config.schema.codex.json` as a checked-in artifact and assert on the `MemoriesToml.properties` object. Given the current drift, I would not trust “someone will remember to regenerate it”.

---

## PR 2: repair the prompt-selection contract

This is the correctness PR for the actual memory prompt.

### The exact control-flow bug

`select_prompt_entries()` currently starts with:

* `let target_shell = context.shell_style?;`
* `core/src/memories/manifest.rs:137`

That means the function returns `None` before it ever reaches pinned-memory handling.

But the pinned-memory block is immediately below:

* `core/src/memories/manifest.rs:142-158`

So the current implementation says in code comments and TUI copy that pinned memories are always included, but the actual function only includes them if `context.shell_style` is `Some(...)`.

That is not theoretical. `preview_model_prompt()` and `preview_model_prompt_sync()` intentionally construct a preview context with `shell_style: None`:

* `core/src/memories/mod.rs:814-819`
* `core/src/memories/mod.rs:833-839`

So the preview path is structurally incapable of selecting pinned memories from the manifest path today.

### The exact budget bug

Pinned memories are appended here:

* `core/src/memories/manifest.rs:145-158`

There is no byte check in that block.

The auto entries are budgeted later:

* `core/src/memories/manifest.rs:203-214`

with this rule:

* append if `would_fit`
* or append anyway if `selected_epoch_ids.is_empty()`

That second clause is the subtle problem. `selected_epoch_ids` only tracks **auto epochs**, not pinned memories. So after adding arbitrarily large pinned content, the first auto epoch can still be admitted on the “first oversized entry is allowed” escape hatch because `selected_epoch_ids` is still empty.

So today there are really **three** prompt-selection bugs:

1. pinned memories disappear entirely if `shell_style` is `None`
2. pinned memories do not count against the 12 KB cap
3. pinned memories do not prevent the “oversized first auto entry” exception, because that exception keys off `selected_epoch_ids.is_empty()` instead of “is the prompt already non-empty?”

### Patch steps

#### 1. Remove the early `?` return

Do not do this at function entry:

```rust
let target_shell = context.shell_style?;
```

Instead:

* initialize pinned-memory selection first
* only gate the auto-epoch ranking/filtering on shell style

That means `select_prompt_entries()` should become:

1. collect pinned entries and pinned tags
2. if `context.shell_style` is `Some(target_shell)`, rank and pack auto epochs
3. if the resulting prompt text is empty, return `None`; otherwise return `Some(...)`

#### 2. Replace `selected_epoch_ids.is_empty()` with an actual “prompt already has content” check

The current condition at `core/src/memories/manifest.rs:209-214` should not use `selected_epoch_ids.is_empty()`.

That vector does not represent total prompt content. It only represents admitted auto epochs.

Use either:

* `summary_text.is_empty()`
* or a dedicated `has_any_prompt_content` / `current_bytes` state

The condition you want is:

* the “allow one oversized auto entry” escape hatch may only apply when **no prompt content at all** has been selected yet

not when “no auto entries have been selected yet”.

#### 3. Make pinned entries participate in the same byte budget

Right now `MAX_MEMORY_PROMPT_BYTES` in `core/src/memories/prompts.rs:15` is not actually a hard cap for manifest-based prompt generation.

I would make it a hard cap.

My recommended contract is:

* pack pinned entries first, in stable manifest order
* each candidate is budgeted before append
* if the first pinned entry alone is too large, truncate it to the remaining budget instead of dropping it silently
* then pack auto epochs by existing rank order
* preserve the current “allow one oversized auto epoch if the prompt is otherwise empty” behavior only when there is literally no pinned or auto content yet

That preserves current rank behavior while fixing the incorrect overflow path.

#### 4. Keep ranking logic unchanged

Do not touch this part unless you mean to redesign retrieval:

* workspace match
* tag affinity
* branch affinity
* platform rank
* shell rank
* provenance rank
* usage / recency tie-breakers

Those are all in `core/src/memories/manifest.rs:160-201`.

This PR should be about contract repair, not retrieval policy changes.

### Optional but strong cleanup: collapse duplicate “unknown shell” states

There is already a real `MemoryShellStyle::Unknown` variant:

* `core/src/memories/manifest.rs:295-306`
* `core/src/memories/storage.rs:323`
* `core/src/memories/storage.rs:345`
* `core/src/memories/storage.rs:951`

But `MemoriesCurrentContext` still uses `Option<MemoryShellStyle>`:

* `core/src/memories/manifest.rs:78-84`

That means this codebase currently has **two representations of “unknown shell”**:

* `None`
* `Some(MemoryShellStyle::Unknown)`

That is exactly the kind of duplicate state that caused the current bug.

I would not force that refactor into the same PR unless you want a slightly broader diff, but it is a good follow-up. The stronger shape is:

```rust
pub shell_style: MemoryShellStyle
```

with `Unknown` as the canonical missing-shell value.

That would align with `type-enum-states` and eliminate this class of bug structurally.

### Tests to add

The current manifest tests around `core/src/memories/manifest.rs:419-632` are good ranking tests. Keep them.

Add these:

* `pinned_memories_are_selected_without_shell_style`
* `pinned_memories_count_against_budget`
* `oversized_first_auto_entry_is_not_admitted_after_pinned_content`
* `oversized_first_auto_entry_is_still_admitted_when_prompt_is_otherwise_empty`

That last one preserves the current behavior tested by `oversized_top_ranked_entry_is_still_selected` at `core/src/memories/manifest.rs:556-580`.

Also add a preview regression in `core/src/memories/mod.rs`:

* `preview_model_prompt_sync_can_render_pinned_memories_without_shell_style`

because the preview path explicitly uses `shell_style: None` today.

---

## PR 3: repair user-facing wording

The TUI copy is currently stronger than the code.

### Exact strings to change

These should be updated:

* `tui/.../model.rs:447`
* `tui/.../model.rs:452`
* `tui/.../model.rs:589-592`
* `tui/.../pages.rs:497-499`
* `tui/.../pages.rs:581-582`

### Why

#### “Pinned memories are always injected”

That is currently false for at least two reasons:

1. memory injection is gated by `use_memories` in `core/src/codex/streaming/turn/mod.rs:71-85`
2. today pinned selection is blocked by `shell_style: None`; after PR 2 it still will not be literally “always” if the budget is exhausted

I would change the wording to something like:

* “Pinned memories are prioritized for inclusion in the memory prompt when memory usage is enabled.”
* “Pinned memories are packed before auto-extracted epochs, subject to prompt budget limits.”

That wording is true both before and after the budget fix.

#### “Sessions that use MCP tools or web search are skipped during extraction”

That is directionally true, but it is not how the code works.

What the code actually does is:

* on MCP call: mark session polluted (`core/src/tools/handlers/mcp.rs:47`)
* on web search call: mark session polluted (`core/src/codex/streaming/turn/handle_item.rs:120-123`)
* only do that when the setting is enabled (`core/src/codex/session.rs:816-819`)
* persist the polluted state (`core/src/codex/session.rs:821-860`)

So the wording I would use is:

* “Sessions are marked polluted when they invoke MCP tools or web search, and polluted sessions are excluded from memory extraction.”

That matches the mechanism, not just the outcome.

---

## Recommended order

I would land these in this order:

1. **PR 1:** schema/runtime contract repair
2. **PR 2:** prompt-selection contract repair
3. **PR 3:** wording cleanup

That order matters. PR 3 should not claim fixed semantics until PR 2 exists.

---

## The two most important implementation details not to miss

These are the two places I would watch most carefully in review:

1. **Do not budget auto entries off `selected_epoch_ids.is_empty()` anymore.**
   That is the wrong state variable once pinned content exists.

2. **Do not leave the checked-in schema artifact untested.**
   The current `config.schema.codex.json` has already drifted badly enough that I would treat schema generation as a regression risk unless there is a test reading the artifact directly.


I re-checked the source directly. Here is the patch-ready, file-by-file checklist, with concrete edit targets and diff sketches.

This version assumes the **minimal-risk path**:

* fix the stale schema artifact
* make pinned memories selectable even when `shell_style` is `None`
* make pinned memories participate in the prompt budget
* preserve the current “first auto epoch may overflow the budget” behavior **only** when the prompt is otherwise empty
* fix the TUI wording to match the actual behavior

---

# 1) `core/src/config/schema.rs`

## Why touch this file

This file already defines the generator and the embedded checked-in fixture boundary:

* `config_schema()` builds the schema from Rust types
* `codex_config_schema_json()` embeds `config.schema.codex.json`

Right now there is no visible regression test that the checked-in artifact still matches the Rust types.

## Edit

Add a unit test that compares the generated schema JSON to the checked-in fixture byte-for-byte.

## Diff sketch

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_codex_schema_matches_generated_schema() {
        let generated = config_schema_json().expect("generate config schema");
        let checked_in = codex_config_schema_json();
        assert_eq!(
            generated.as_slice(),
            checked_in,
            "core/config.schema.codex.json is stale; regenerate the fixture from ConfigToml"
        );
    }
}
```

## Why this matters

This is the single best guard against the exact drift that already exists in `MemoriesToml`.

---

# 2) `core/config.schema.codex.json`

## Current problem

The checked-in `MemoriesToml` schema is stale.

It currently exposes:

* `max_raw_memories_for_global`
* `max_rollout_age_days`
* `max_rollouts_per_startup`
* `min_rollout_idle_hours`
* `phase_1_model`
* `phase_2_model`

But the actual Rust type in `core/src/config_types.rs:121-130` defines:

* `no_memories_if_mcp_or_web_search`
* `generate_memories`
* `use_memories`
* `max_raw_memories_for_consolidation`
* deprecated alias `max_raw_memories_for_global`
* `max_rollout_age_days`
* `max_rollouts_per_startup`
* `min_rollout_idle_hours`

## Edit

Do **not** treat the JSON file as the source of truth. The real source of truth is the Rust type. The operational fix is:

1. keep `MemoriesToml` in Rust as the canonical shape
2. regenerate `config.schema.codex.json`
3. commit the regenerated artifact

## Expected `MemoriesToml.properties`

After regeneration, the properties block should contain at least these keys:

```json
"MemoriesToml": {
  "additionalProperties": false,
  "description": "Memories settings loaded from config.toml.",
  "properties": {
    "generate_memories": {
      "type": "boolean"
    },
    "max_raw_memories_for_consolidation": {
      "format": "uint",
      "minimum": 0.0,
      "type": "integer"
    },
    "max_raw_memories_for_global": {
      "format": "uint",
      "minimum": 0.0,
      "type": "integer"
    },
    "max_rollout_age_days": {
      "format": "int64",
      "type": "integer"
    },
    "max_rollouts_per_startup": {
      "format": "uint",
      "minimum": 0.0,
      "type": "integer"
    },
    "min_rollout_idle_hours": {
      "format": "int64",
      "type": "integer"
    },
    "no_memories_if_mcp_or_web_search": {
      "type": "boolean"
    },
    "use_memories": {
      "type": "boolean"
    }
  },
  "type": "object"
}
```

## Keys that should disappear

These should not remain in the checked-in schema unless you also add runtime support:

* `phase_1_model`
* `phase_2_model`

---

# 3) `core/src/config_types.rs`

## Current state

This file is already the canonical source for the memory config surface, and the logic is mostly correct:

* alias handling is correct at `177-181`
* `to_toml()` already writes only the canonical key at `199-200`
* `is_empty()` already knows about the real fields at `221-228`

## Production changes

I would keep production behavior here almost unchanged.

The only production edit I would make is a short clarifying comment above `MemoriesToml` or above the alias field so the intended compatibility policy is explicit.

### Suggested comment

```rust
/// Compatibility note:
/// - `max_raw_memories_for_consolidation` is the canonical key.
/// - `max_raw_memories_for_global` is read-only compatibility for old configs.
/// - stale keys like `phase_1_model` / `phase_2_model` are not part of this type
///   and must not appear in the generated schema.
```

## Test additions

Extend the existing `memories_tests` module.

### Add this test

```rust
#[test]
fn to_toml_uses_canonical_memory_limit_key_only() {
    let cfg = MemoriesConfig {
        max_raw_memories_for_consolidation: 123,
        ..MemoriesConfig::default()
    };

    let toml = cfg.to_toml();
    assert_eq!(toml.max_raw_memories_for_consolidation, Some(123));
    assert_eq!(toml.max_raw_memories_for_global, None);
}
```

### Why here

This locks in the intended write-path behavior independently of the TOML persistence layer.

---

# 4) `core/src/config/sources.rs`

## Current state

`apply_memories_table()` is doing the right thing:

* writes active keys
* removes deprecated/stale ones

at `3859-3881`.

## Edit

Keep the behavior. Add one comment so future maintainers do not “helpfully” re-add the removed keys to the schema.

## Diff sketch

```rust
fn apply_memories_table(table: &mut TomlTable, settings: &MemoriesToml) {
    write_optional_bool(table, "no_memories_if_mcp_or_web_search", settings.no_memories_if_mcp_or_web_search);
    write_optional_bool(table, "generate_memories", settings.generate_memories);
    write_optional_bool(table, "use_memories", settings.use_memories);
    write_optional_usize(
        table,
        "max_raw_memories_for_consolidation",
        settings.max_raw_memories_for_consolidation,
    );
    write_optional_i64(table, "max_rollout_age_days", settings.max_rollout_age_days);
    write_optional_usize(
        table,
        "max_rollouts_per_startup",
        settings.max_rollouts_per_startup,
    );
    write_optional_i64(table, "min_rollout_idle_hours", settings.min_rollout_idle_hours);

    // Compatibility cleanup:
    // - remove deprecated aliases that we still read for backward compatibility
    // - remove stale historical keys that are intentionally unsupported
    table.remove("max_raw_memories_for_global");
    table.remove("max_unused_days");
    table.remove("phase_1_model");
    table.remove("phase_2_model");
    table.remove("extract_model");
    table.remove("consolidation_model");
}
```

No logic change needed here.

---

# 5) `core/src/memories/manifest.rs`

This is the main behavior patch.

## Current problems in this file

### A. Early return drops pinned memories

At `137`:

```rust
let target_shell = context.shell_style?;
```

That returns before pinned-memory handling.

### B. Pinned memories are not budgeted

At `145-158`, pinned memories are appended without checking `max_bytes`.

### C. The auto “first overflow allowed” rule keys off the wrong state

At `209-214`:

```rust
if would_fit || selected_epoch_ids.is_empty()
```

That checks whether any **auto epochs** were selected, not whether the prompt already has content.

If pinned memories were already appended, `selected_epoch_ids` is still empty, so the first oversized auto epoch is incorrectly admitted anyway.

---

## Production refactor

## Step 1: import truncation helper

Add this import near the top:

```rust
use crate::truncate::truncate_middle;
```

That allows the first oversized pinned memory to be truncated to budget instead of blowing past it.

---

## Step 2: update struct comment

Change:

```rust
/// User-created pinned memories. Always injected first in prompt.
```

to:

```rust
/// User-created pinned memories. Packed first into the prompt, subject to prompt-budget limits.
```

---

## Step 3: replace `select_prompt_entries()`

Below is the exact shape I would use.

### Proposed replacement

```rust
pub(crate) fn select_prompt_entries(
    manifest: &SnapshotManifest,
    context: &MemoriesCurrentContext,
    max_bytes: usize,
) -> Option<MemoryPromptSelection> {
    let mut selected_epoch_ids = Vec::new();
    let mut summary_text = String::new();

    // Collect pinned-memory tags first so they can boost epoch ranking later,
    // even when some pinned memories do not fit into the prompt.
    let mut user_tags: std::collections::HashSet<&str> = std::collections::HashSet::new();

    // Phase 1: pack pinned memories first, with budget enforcement.
    for mem in &manifest.user_memories {
        if !mem.pinned {
            continue;
        }

        for tag in &mem.tags {
            user_tags.insert(tag.as_str());
        }

        let chunk = if summary_text.is_empty() {
            format!("[pinned] {}", mem.content)
        } else {
            format!("\n\n[pinned] {}", mem.content)
        };

        let would_fit = summary_text.len().saturating_add(chunk.len()) <= max_bytes;
        if would_fit {
            summary_text.push_str(&chunk);
            continue;
        }

        // Preserve the strongest pinned-memory behavior without letting pinned entries
        // blow past the budget: if the prompt is still empty, include a truncated version
        // of the first pinned memory; otherwise stop packing pinned memories.
        if summary_text.is_empty() && max_bytes > 0 {
            let truncated = truncate_middle(&chunk, max_bytes).0;
            if !truncated.is_empty() {
                summary_text.push_str(&truncated);
            }
        }

        break;
    }

    // Phase 2: auto-extracted epochs are only eligible when shell compatibility
    // can be evaluated.
    if let Some(target_shell) = context.shell_style {
        let mut compatible: Vec<_> = manifest
            .epochs
            .iter()
            .filter_map(|entry| {
                let shell_rank = shell_compatibility_rank(target_shell, entry.shell_style)?;
                let platform_rank =
                    platform_compatibility_rank(context.platform_family, entry.platform_family)?;
                let workspace_rank = workspace_rank(context.workspace_root.as_ref(), entry.workspace_root.as_ref());
                let branch_rank = branch_rank(context.git_branch.as_ref(), entry.git_branch.as_ref());
                let provenance_rank = provenance_rank(entry.provenance);
                let tag_affinity_count = entry
                    .tags
                    .iter()
                    .filter(|tag| user_tags.contains(tag.as_str()))
                    .count();

                Some((
                    workspace_rank,
                    tag_affinity_count,
                    branch_rank,
                    platform_rank,
                    shell_rank,
                    provenance_rank,
                    entry,
                ))
            })
            .collect();

        compatible.sort_by_key(
            |(
                workspace_rank,
                tag_affinity_count,
                branch_rank,
                platform_rank,
                shell_rank,
                provenance_rank,
                entry,
            )| {
                (
                    Reverse(*workspace_rank),
                    Reverse(*tag_affinity_count),
                    Reverse(*branch_rank),
                    Reverse(*platform_rank),
                    Reverse(*shell_rank),
                    Reverse(*provenance_rank),
                    Reverse(entry.usage_count),
                    Reverse(entry.last_usage.unwrap_or(i64::MIN).max(entry.source_updated_at)),
                    Reverse(entry.source_updated_at),
                    entry.id.thread_id,
                    entry.id.epoch_index,
                )
            },
        );

        for (_, _, _, _, _, _, entry) in compatible {
            let chunk = if summary_text.is_empty() {
                entry.prompt_entry.clone()
            } else {
                format!("\n\n{}", entry.prompt_entry)
            };

            let would_fit = summary_text.len().saturating_add(chunk.len()) <= max_bytes;

            // Preserve the current compatibility with the existing oversized-first-entry test,
            // but key it off actual prompt emptiness instead of selected_epoch_ids.
            if would_fit || summary_text.is_empty() {
                summary_text.push_str(&chunk);
                selected_epoch_ids.push(entry.id);
            } else {
                break;
            }
        }
    }

    if summary_text.is_empty() {
        return None;
    }

    Some(MemoryPromptSelection {
        summary_text,
        selected_epoch_ids,
    })
}
```

---

## Why this exact version

It fixes all three actual bugs without broadening the surface area:

1. pinned memories no longer depend on `shell_style`
2. pinned memories no longer bypass the budget
3. the “first overflow allowed” exception is keyed to `summary_text.is_empty()`, which is the correct state variable

It also preserves the current test-backed behavior that one oversized auto epoch may still be selected when the prompt is otherwise empty.

---

## Step 4: add test helper for pinned memories

Inside the test module, add:

```rust
fn user_memory(
    id: &str,
    content: &str,
    tags: &[&str],
    pinned: bool,
) -> UserMemoryManifestEntry {
    UserMemoryManifestEntry {
        id: id.to_string(),
        content: content.to_string(),
        tags: tags.iter().map(|s| s.to_string()).collect(),
        pinned,
    }
}
```

---

## Step 5: add targeted regression tests

Add these to `core/src/memories/manifest.rs` tests.

### 5A. pinned memories are selected without shell style

```rust
#[test]
fn pinned_memories_are_selected_without_shell_style() {
    let manifest = SnapshotManifest::with_user_memories(
        Vec::new(),
        vec![user_memory("u1", "remember this", &["rust"], true)],
    );

    let context = MemoriesCurrentContext {
        platform_family: MemoryPlatformFamily::Unix,
        shell_style: None,
        shell_program: None,
        workspace_root: None,
        git_branch: None,
    };

    let selection = select_prompt_entries(&manifest, &context, 1024).expect("selection");
    assert!(selection.summary_text.contains("[pinned] remember this"));
    assert!(selection.selected_epoch_ids.is_empty());
}
```

### 5B. pinned memories count against budget

```rust
#[test]
fn pinned_memories_count_against_budget() {
    let manifest = SnapshotManifest::with_user_memories(
        Vec::new(),
        vec![user_memory("u1", &"x".repeat(128), &[], true)],
    );

    let context = MemoriesCurrentContext {
        platform_family: MemoryPlatformFamily::Unix,
        shell_style: None,
        shell_program: None,
        workspace_root: None,
        git_branch: None,
    };

    let selection = select_prompt_entries(&manifest, &context, 32).expect("selection");
    assert!(selection.summary_text.len() <= 32);
}
```

### 5C. pinned content suppresses the oversized-first-auto exception

```rust
#[test]
fn oversized_first_auto_entry_is_not_admitted_after_pinned_content() {
    let mut oversized = entry(
        MemoryShellStyle::Zsh,
        None,
        Some("main"),
        Stage1EpochProvenance::Derived,
        1,
        Some(1),
        1,
    );
    oversized.prompt_entry = "x".repeat(64);

    let manifest = SnapshotManifest::with_user_memories(
        vec![oversized],
        vec![user_memory("u1", "pin", &[], true)],
    );

    let context = MemoriesCurrentContext {
        platform_family: MemoryPlatformFamily::Unix,
        shell_style: Some(MemoryShellStyle::Zsh),
        shell_program: Some("zsh".to_string()),
        workspace_root: None,
        git_branch: Some("main".to_string()),
    };

    let selection = select_prompt_entries(&manifest, &context, 16).expect("selection");
    assert!(selection.summary_text.contains("[pinned] pin"));
    assert!(selection.selected_epoch_ids.is_empty());
}
```

### 5D. existing oversized-top-ranked-auto behavior still holds when prompt starts empty

Keep the existing test:

* `oversized_top_ranked_entry_is_still_selected`

unchanged.

That preserves compatibility while fixing the bad interaction with pinned content.

---

# 6) `core/src/memories/prompts.rs`

## Current state

The code itself is fine. It delegates to `select_prompt_entries()` and uses that output.

The only thing I would change here is the comment above `MAX_MEMORY_PROMPT_BYTES`, because after the manifest patch the comment should be more precise.

## Edit

Replace:

```rust
// This budget is applied asymmetrically:
// - manifest selection packs prompt entries until the budget is full
// - summary fallback truncates one canonical summary blob to the budget
```

with:

```rust
// This budget is applied asymmetrically:
// - pinned memories are packed first and must fit within the budget
//   (the first oversized pinned memory is truncated to fit)
// - auto epochs are then packed in rank order until the budget is full
// - for compatibility with existing behavior, a single oversized auto epoch
//   may still be selected only when the prompt is otherwise empty
// - summary fallback truncates one canonical summary blob to the budget
```

## Tests to add here

This file already has manifest-based prompt tests. Add one more, because it exercises the end-to-end prompt builder rather than only `select_prompt_entries()`.

### Proposed test

```rust
#[tokio::test]
async fn prompt_can_render_pinned_memories_when_shell_style_is_absent() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("memories");
    let snapshot_dir = root.join("snapshots").join("20260307T120000Z-test");
    tokio::fs::create_dir_all(&snapshot_dir).await.expect("create snapshot dir");
    tokio::fs::write(root.join("current"), "20260307T120000Z-test\n")
        .await
        .expect("write current pointer");

    let manifest = SnapshotManifest::with_user_memories(
        Vec::new(),
        vec![UserMemoryManifestEntry {
            id: "u1".to_string(),
            content: "Pinned manifest memory".to_string(),
            tags: vec!["rust".to_string()],
            pinned: true,
        }],
    );

    tokio::fs::write(
        snapshot_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("manifest json"),
    )
    .await
    .expect("write manifest");

    let context = MemoriesCurrentContext {
        platform_family: MemoryPlatformFamily::Unknown,
        shell_style: None,
        shell_program: None,
        workspace_root: None,
        git_branch: None,
    };

    let prompt = build_memory_tool_developer_instructions(temp.path(), &context)
        .await
        .expect("prompt should be generated");

    assert!(prompt.instructions.contains("Pinned manifest memory"));
    assert!(prompt.selected_epoch_ids.is_empty());
    assert!(!prompt.used_fallback_summary);
}
```

This is the regression that proves the shell-style bug is actually fixed.

---

# 7) `core/src/memories/mod.rs`

## Production code

No production change is required here once `select_prompt_entries()` is fixed.

The preview functions at:

* `812-824`
* `829-845`

are currently valid callers; the bug is in the callee.

## Optional comment tweak

Add one short comment in both preview constructors:

```rust
// shell_style is intentionally None here; pinned memories must still be previewable
```

That makes the intended contract explicit.

---

# 8) `tui/src/bottom_pane/settings_pages/memories/model.rs`

## Current inaccurate strings

### `RowKind::SkipMcpOrWebSearch`

Current:

```rust
"Sessions that use MCP tools or web search are skipped during extraction ..."
```

This describes the effect, but not the mechanism.

### `RowKind::ManageUserMemories`

Current:

```rust
"These are always injected into the LLM prompt ..."
```

This is too strong.

### status page prose

Current:

```rust
"Pinned memories: User-created entries that are always injected ..."
```

Also too strong.

## Exact replacements

### Replace at `447`

```rust
RowKind::SkipMcpOrWebSearch => "Sessions are marked polluted when they invoke MCP tools or web search, and polluted sessions are excluded from memory extraction. This avoids learning from externally-sourced tool output.",
```

### Replace at `452`

```rust
RowKind::ManageUserMemories => "Create, edit, and delete pinned memories. These are prioritized for inclusion in the LLM prompt when memory usage is enabled, and are packed before auto-extracted epochs subject to the prompt budget.",
```

### Replace status-page lines at `589-592`

Replace those four `lines.push(...)` calls with:

```rust
lines.push("  Pinned memories: User-created entries that are prioritized".to_owned());
lines.push("  for inclusion in the LLM prompt when memory usage is enabled.".to_owned());
lines.push("  They are packed before auto-extracted epochs, subject to".to_owned());
lines.push("  the same prompt-budget limits.".to_owned());
```

---

# 9) `tui/src/bottom_pane/settings_pages/memories/pages.rs`

## Current inaccurate strings

At `497-499` and `581-582`, the UI still says pinned memories are always included.

## Exact replacements

### Replace header text at `497-499`

Current:

```rust
"{total} pinned memor{} — always injected into LLM prompt"
```

Replace with:

```rust
"{total} pinned memor{} — prioritized for prompt inclusion"
```

### Replace editor preamble at `581-582`

Current:

```rust
"Pinned memories are always included in the LLM prompt."
```

Replace with:

```rust
"Pinned memories are prioritized for inclusion in the LLM prompt."
```

That wording stays true after the budget fix.

---

# 10) Tests I would add, in final order

## `core/src/config/schema.rs`

* `checked_in_codex_schema_matches_generated_schema`

## `core/src/config_types.rs`

* `to_toml_uses_canonical_memory_limit_key_only`

## `core/src/memories/manifest.rs`

* `pinned_memories_are_selected_without_shell_style`
* `pinned_memories_count_against_budget`
* `oversized_first_auto_entry_is_not_admitted_after_pinned_content`

Keep:

* `oversized_top_ranked_entry_is_still_selected`

## `core/src/memories/prompts.rs`

* `prompt_can_render_pinned_memories_when_shell_style_is_absent`

---

# Recommended commit split

## Commit 1

**schema: resync MemoriesToml fixture with Rust types**

Files:

* `core/src/config/schema.rs`
* `core/config.schema.codex.json`
* optionally `core/src/config_types.rs`

## Commit 2

**memories: decouple pinned prompt entries from shell-style gating**

Files:

* `core/src/memories/manifest.rs`
* `core/src/memories/prompts.rs`
* optionally `core/src/memories/mod.rs`

## Commit 3

**tui: fix pinned-memory and polluted-session wording**

Files:

* `tui/src/bottom_pane/settings_pages/memories/model.rs`
* `tui/src/bottom_pane/settings_pages/memories/pages.rs`

---

# One design choice I would keep explicit in the PR description

There is an existing test-backed behavior that allows one oversized auto epoch when the prompt starts empty. My plan above preserves that behavior.

If you want the budget to become a **true hard cap**, that is a separate, deliberate change:

* remove or rewrite `oversized_top_ranked_entry_is_still_selected`
* make all content, including the first auto epoch, fit or truncate to `max_bytes`

I would not mix that stronger policy change into the same patch unless the product intent is already settled.
