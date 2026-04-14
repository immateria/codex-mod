# Tagged & User-Created Memories

## Problem

Memories are session replay snippets with no semantic structure. The LLM gets a
chronological list of "what happened" but can't distinguish style preferences from
struct definitions from workflow conventions. Users can't pin important knowledge.

## How the memory pipeline actually works (corrected understanding)

### Phase-1 extraction is LOCAL, not LLM-based

`build_stage1_epochs()` in storage.rs reads rollout JSONL lines and splits them into
epochs purely from local parsing — environment context changes (cwd, branch, shell)
create epoch boundaries. `finalize_epoch()` builds `raw_memory` and `rollout_summary`
from the rollout line data (user snippets, tool outputs, metadata). **No LLM is invoked
in phase-1.**

### The templates are for the consolidation sub-agent (phase-2)

`stage_one_system.md` and `consolidation_system.md` in `core/templates/memories/` are
**not compiled into the binary**. They live on disk and are read by the phase-2
consolidation sub-agent, which is spawned as a real Codex session. That agent reads
`raw_memories.md` and `rollout_summaries/*.md` from the snapshot directory and produces
a consolidated `MEMORY.md` or `memory_summary.md`.

### Implication for tagging

Since phase-1 is pure Rust parsing, auto-tagging needs to happen either:

1. **In `finalize_epoch()`** — heuristic/keyword-based tag inference from the rollout
   content (fast, deterministic, no LLM cost, but limited intelligence)
2. **In the phase-2 consolidation agent** — add tagging instructions to
   `consolidation_system.md` so the LLM tags epochs during consolidation (smart, but
   only runs on refresh, not per-epoch)
3. **Both** — heuristic tags in phase-1, refined by LLM in phase-2

**Recommended: Option 3 (both).** Phase-1 heuristics give immediate coarse tags.
Phase-2 LLM refines when consolidation runs.

---

## Design

### Two memory types

1. **Auto-extracted (enhanced)** — Phase-1 heuristic tagger infers coarse tags from
   rollout content (keywords, file extensions, command patterns). Phase-2 consolidation
   agent can refine tags. Tags stored in `stage1_epochs`, surfaced in `manifest.json`,
   used for relevance boosting in prompt selection.

2. **User-created ("pinned")** — Manual entries via TUI. Each has content + freeform
   tags. Always included in prompt injection at highest priority. Stored in a new
   `user_memories` SQLite table.

### Tag system

- Fully freeform strings. No fixed taxonomy.
- Normalized: lowercase, trimmed, no duplicates per entry.
- Examples: `style`, `convention`, `error-handling`, `struct:User`, `deployment`, `rust`
- Stored as **JSON arrays** in SQLite TEXT columns (not comma-separated — avoids
  delimiter ambiguity, queryable via `json_each()`)
- Serialized as arrays in manifest.json

### Prompt injection

- User-created memories injected first in a dedicated `## Pinned Memories` section
- Auto-extracted epochs fill remaining budget, ranked with tag affinity boost
- Budget: 12KB total. User memories have a soft cap at 4KB to avoid crowding out
  auto-epochs entirely. If user memories exceed 4KB, they still all go in but auto-epoch
  budget shrinks.
- Individual user memory content capped at 2KB (enforced at creation)

### Tag affinity in selection

"Context tags" derived automatically from the current session:
- Workspace root → infer project name tag (e.g., `project:code-rs`)
- File extensions in cwd → language tags (`rust`, `python`, `typescript`)
- Git branch name → branch tag
- These context tags boost matching auto-epochs in the ranking tuple

### LLM prompt format for tagged memories

```
## Pinned Memories
[style] Use imperative present tense in commit messages.
[convention] [rust] Always use Result<T, AppError> for public APIs.
[struct:Config] Fields: model, provider, api_key, temperature, max_tokens.

## Session Memories
[rust] [code-rs @ main] (2026-04-13)
Exhaustive clippy audit: replaced fold patterns with join, fixed ref_option warnings...

[deployment] [infra @ staging] (2026-04-12)
Docker build requires --platform linux/amd64 on M1 Macs...
```

### No backwards compatibility needed

Single-user repo. Schema updated in-place. Delete old snapshots if format changes.

---

## Implementation Phases

### Phase 1: Schema + data model

**`code-rs/memories-state/src/lib.rs`:**
- Bump `STATE_SCHEMA_VERSION` to 7
- `migrate_v6_to_v7()`: `ALTER TABLE stage1_epochs ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'`
- Add to `stage1_epochs` CREATE TABLE: `tags TEXT NOT NULL DEFAULT '[]'`
- Create `user_memories` table:
  ```sql
  CREATE TABLE user_memories (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    scope TEXT NOT NULL DEFAULT 'global',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
  )
  ```
- Add `tags: Vec<String>` to `Stage1EpochInput` and `Stage1EpochRecord`
  - Serialize to/from JSON string for SQLite storage
- Add `UserMemory` struct: `id: String, content: String, tags: Vec<String>,
  scope: String, created_at: i64, updated_at: i64`
- CRUD functions: `insert_user_memory()`, `list_user_memories(scope_filter: Option<&str>)`,
  `update_user_memory()`, `delete_user_memory()`, `list_all_tags() -> Vec<(String, usize)>`
- Update `MemoriesStateStatus` with `user_memory_count: usize`
- Add `user_memory_count` query in `status()`

**`code-rs/core/src/memories/mod.rs`:**
- Add `user_memory_count` to `MemoriesDbStatus`
- Update `empty_db_status()` and `load_memories_db_status()`
- Add sync wrappers for user memory CRUD (TUI calls these)

**`code-rs/core/src/lib.rs`:**
- Re-export: `UserMemory`, `create_user_memory`, `list_user_memories`,
  `update_user_memory`, `delete_user_memory`, `list_all_memory_tags`

### Phase 2: Heuristic auto-tagging in phase-1

**`code-rs/core/src/memories/storage.rs`:**
- Add `fn infer_tags_from_epoch(epoch: &EpochAccumulator, claim: &Stage1Claim) -> Vec<String>`
  - File extension detection from rollout content → language tags
  - Command pattern matching → workflow tags (e.g., `docker` commands → `deployment`)
  - Git branch patterns → branch-based tags (e.g., `feat/auth` → `auth`)
  - Keyword extraction from user snippets (top N most distinctive terms)
- Call in `finalize_epoch()`, pass result into `Stage1EpochInput.tags`
- Update `render_clean_prompt_entry()` to prefix with `[tag1] [tag2]`

**`code-rs/core/templates/memories/stage_one_system.md`:**
- Add section instructing phase-2 consolidation agent to refine/add tags
- Provide tag format guidance: lowercase, specific, colon-namespaced for types

### Phase 3: Manifest + prompt selection

**`code-rs/core/src/memories/manifest.rs`:**
- Add `tags: Vec<String>` to `SnapshotEpochManifestEntry`
- Add `user_memories: Vec<UserMemoryManifestEntry>` to `SnapshotManifest`
  ```rust
  struct UserMemoryManifestEntry {
      id: String,
      content: String,
      tags: Vec<String>,
      scope: String,
  }
  ```
- Bump `MANIFEST_VERSION` to 2
- Add `context_tags: Option<&[String]>` param to `select_prompt_entries()`
- Insert `tag_affinity_rank` into the sort key tuple (between `same_workspace` and
  `branch_rank`): count of matching tags between entry and context, inverted
- Add `fn infer_context_tags(context: &MemoriesCurrentContext) -> Vec<String>`:
  workspace root → project tag, file extensions → language tag
- User memories packed first in a separate loop, then auto-epochs fill remainder

**`code-rs/core/src/memories/storage.rs`:**
- Update `render_artifacts_from_state()` to populate epoch tags from `Stage1EpochRecord`
- Load user memories from DB and include in manifest generation
- Generate `## Pinned Memories` section in `memory_summary.md`

**`code-rs/core/src/memories/prompts.rs`:**
- Update `read_path.md` template to explain pinned memories section
- Update selection to include user memories in the prompt text

### Phase 4: TUI interface

**`code-rs/tui/src/bottom_pane/settings_pages/memories/mod.rs`:**
- Add `RowKind::ManageUserMemories`
- Add `ViewMode::UserMemoryList(Box<UserMemoryListState>)`
- Add `ViewMode::UserMemoryEdit(Box<UserMemoryEditState>)`

**`code-rs/tui/src/bottom_pane/settings_pages/memories/model.rs`:**
- Add `ManageUserMemories` to ROWS arrays (after ViewModelPrompt, before BrowseRollouts)
- `row_label()`: "Manage pinned memories"
- `row_value()`: "{N} memories" from status, or "none"
- `row_description()`: "Create and manage persistent memory entries that are always
  included in the LLM prompt. Use for style preferences, conventions, project
  knowledge, or anything the model should always know."
- `open_user_memory_list()`: calls `list_user_memories()`, opens list view
- `UserMemoryListState`: `entries: Vec<UserMemory>`, `list_state`, `viewport_rows`,
  `pending_delete: Option<String>`, `tag_filter: Option<String>`
- `UserMemoryEditState`: `id: Option<String>` (None=create), content `FormTextField`,
  tags `FormTextField`, `active_field: enum { Content, Tags }`

**`code-rs/tui/src/bottom_pane/settings_pages/memories/input.rs`:**
- Handle Enter on ManageUserMemories → `open_user_memory_list()`
- `UserMemoryList` mode:
  - Up/Down: scroll list
  - Enter: edit selected memory
  - `n`: create new memory
  - `d`: mark for delete, confirm on second press (like rollout delete)
  - `/`: open tag filter input
  - Esc: back to Main
- `UserMemoryEdit` mode:
  - Tab: switch between Content and Tags fields
  - Content field: multi-line text (reuse TextViewer-like editing?)
    Actually — keep it single-line for v1. Content is meant to be concise.
  - Tags field: space-separated freeform tags
  - Enter: save (create or update via core API), back to list
  - Esc: cancel, back to list

**`code-rs/tui/src/bottom_pane/settings_pages/memories/render.rs`:**
- UserMemoryList: scrollable list, each row shows:
  - Left: content preview (truncated to width - tag space)
  - Right: `[tag1] [tag2]` in dim styling
  - Highlighted row: full content in description area
- UserMemoryEdit: two labeled form fields
  - `Content: [__________________________]`
  - `Tags:    [__________________________]`
  - Active field highlighted, inactive dimmed
- Delete confirmation: same pattern as rollout delete (inline prompt)

**`code-rs/tui/src/bottom_pane/settings_pages/memories/pages.rs`:**
- Update header: "Database: N sessions · M epochs · P pinned"

**`code-rs/tui/src/bottom_pane/settings_pages/memories/tests.rs`:**
- Update row index constants for new ManageUserMemories row
- Add tests: navigate to ManageUserMemories, open list, create memory, delete memory

### Phase 5: Polish

- Update "How Memories Work" section in status viewer to explain tagged system
- Show auto-extracted tags in rollout browser (dim badge after content)
- Show tag cloud in status report: "Top tags: rust(12) style(8) deployment(3)"
- Update all affected row descriptions
- Auto-extracted tag display in memory_summary.md viewers

---

## Key decisions

| Decision                                   | Rationale                                                   |
| ------------------------------------------ | ----------------------------------------------------------- |
| JSON arrays for tags in SQLite             | Avoids comma delimiter issues, queryable via `json_each()`  |
| Heuristic + LLM tagging                    | Fast coarse tags immediately, refined on consolidation      |
| User memories soft-capped at 4KB           | Prevents crowding out auto-epochs entirely                  |
| Individual user memory ≤ 2KB               | Forces concise entries; long knowledge goes in project docs |
| Tags normalized to lowercase               | Prevents `Style` vs `style` duplicates                      |
| Context tags inferred from workspace       | No manual tag-per-session configuration needed              |
| User memories in separate manifest section | Clean separation; always injected regardless of ranking     |
| Single-line content in v1 TUI              | Simpler editing; can upgrade to multi-line later            |

## Risks and mitigations

| Risk                                      | Mitigation                                                        |
| ----------------------------------------- | ----------------------------------------------------------------- |
| Heuristic tagger produces garbage tags    | Keep heuristics conservative; only tag when confidence is high    |
| Too many user memories blow budget        | 4KB soft cap + 2KB per-entry limit                                |
| Tag proliferation makes filtering useless | `list_all_tags()` shows counts; user can prune via TUI            |
| Phase-2 consolidation ignores tags        | Update `consolidation_system.md` with tag refinement instructions |
| Schema migration breaks existing DB       | v6→v7 migration adds columns with defaults; safe for ALTER TABLE  |
