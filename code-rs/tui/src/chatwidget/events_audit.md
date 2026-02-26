# ChatWidget History Event Audit (2025-09-27)

This note summarizes the places where `ChatWidget` still mutates `history_cells`
directly so we can migrate them onto the `HistoryState::apply_event` pipeline.
Line numbers reference `code-rs/tui/src/chatwidget.rs` as of 2025-09-27.

## Structural mutations

- **Global insert path** – `history_insert_with_key_global_tagged`
  (`chatwidget.rs:4638`) now accepts optional `HistoryDomainRecord` inputs. When
  present (e.g., background/system notices as of 2025-09-27) it routes through
  `HistoryState::apply_domain_event(Insert)` and assigns IDs before the cell is
  stored. Other callers still fall back to `history_record_from_cell`.
- **Indexed replacement** – `history_replace_at` (`chatwidget.rs:4715`) rebuilds a
  record from the incoming cell, applies `HistoryEvent::Replace`, then writes
  `history_cells[idx] = cell`. Several call sites mutate the cell before it flows
  here (e.g., plan/patch updates).
- **Removal path** – `history_remove_at` (`chatwidget.rs:4781`) mirrors
  `HistoryEvent::Remove` and then calls `history_cells.remove(idx)` alongside the
  auxiliary ID/order vectors.
- **Patch apply success** – `handle_patch_apply_end_now`
  (`chatwidget.rs:2240-2338`) finds a prior patch cell, mutates its state in
  place (including manual text span edits), and calls `history_record_from_cell`
  + `HistoryEvent::Replace`. This is a prime candidate for a dedicated
  `HistoryDomainEvent::PatchOutcome` handler.
- **Streaming updates** – the streaming lifecycle functions repeatedly locate
  cells via `rposition` and mutate them directly:
  - tail update                       (`chatwidget.rs:11390-11436`)
  - indexed update                    (`chatwidget.rs:11417-11446`)
  - final message hydration           (`chatwidget.rs:11782-11860`)
  - review-flow variants              (`chatwidget.rs:11660-11747`)
  These still rely on cell-owned caches and in-place mutation.
- **Exec lifecycle** –
  - [done] Streaming deltas now flow through `HistoryDomainEvent::UpdateExecStream`
    (`chatwidget.rs:6996-7034`), and the returned record hydrates the cached
    `ExecCell` state.
  - [todo] End/update: merge logic still downcasts and mutates cells directly
    (`chatwidget.rs:7159-7184`, `7233-7242`).
- **Background/system notices** – `push_system_cell` (`chatwidget.rs:1254`) now
  uses `HistoryDomainRecord::BackgroundEvent` for inserts/replacements, so these
  paths no longer reconstruct records from existing cells (migrated 2025-09-27).
- **Plain/wait/loading inserts** – `history_insert_plain_cell_with_key`
  (`chatwidget.rs:4660`) and wait-tool completions (`chatwidget.rs:7358`) now
  emit `HistoryDomainRecord` variants; the inserted cells get hydrated from the
  returned `HistoryRecord`, so `assign_history_id` no longer sets IDs for these
  types (2025-09-27).
- **Reasoning merge** – reasoning stream handling mutates the stored
  `CollapsibleReasoningCell` state (`chatwidget.rs:1440-2058`,
  `11288-11336`).

## Non-structural traversal (stateful toggles)

The following loops walk `history_cells` to update cached state or trigger
animations; they should eventually derive from `HistoryRecord` once the renderer
cache lands, but they are less urgent than the structural mutations above.

- `clear_reasoning_in_progress` / `set_show_reasoning`
  (`chatwidget.rs:1991-2034`).
- `on_composer_expanded`, `toggle_reasoning_visibility`, export helpers, etc.
- Animation checks (`chatwidget.rs:3174`, `18153`, `18309`).

## Migration plan mapping

- **Phase A (inventory complete)** – this document captures all direct mutation
  sites that need event coverage.
- **Phase B (API surface)** – introduce `HistoryDomainEvent` enums per category:
  `Plain`, `Tool`, `Exec`, `Streaming`, `Patch`, `Diff`, etc. Each enum variant
  carries the payload currently embedded in the cell mutation.
- **Phase C (migration waves)** – replace call-site logic:
  1. Plain/background/loading/wait/tool/plan/upgrade – already flow through
     insert/replace; only need to move pre-insert mutation into domain event
     constructors.
  2. Exec/stream/assistant – emit domain events so state changes happen inside
     `HistoryState`, then render from the resulting record.
  3. Diff/patch/image/explore/rate-limit – replace bespoke `history_cells`
     mutations with domain events and remove the remaining uses of
     `history_record_from_cell`.
- **Phase D (ownership flip)** – once all events route through
  `HistoryState::apply_event`, collapse `history_cells` into a derived cache
  (fed by the Step 6 renderer cache) and guard with debug asserts to prevent
  new direct mutations.
