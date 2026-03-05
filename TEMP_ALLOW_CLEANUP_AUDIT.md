# Temporary Allow-Lint Cleanup Audit

This is a temporary planning file for lint-suppression cleanup in code-rs.
Focus is on suppressions likely masking real dead/unused code, not test-only convenience allows.

Generated: 2026-03-05 12:37:07 CST

## How To Use This File

This file is a reference, not an execution checklist.

- Use it to decide whether a suppression is:
  - stale and removable now
  - mixed and needs a small follow-up read before editing
  - intentional/test-only and lower priority
- When a file says "mostly stale", that is a good candidate for immediate cleanup.
- When a file says "mixed", expect a narrower edit: remove obviously dead items, then re-run the compiler and re-check what remains.

## Triage Guide

- `High confidence stale/dead`
  - good first-batch cleanup target
- `Mostly stale suppressions`
  - likely remove the existing allow(s), then fix whatever the compiler surfaces
- `Mixed`
  - some items are live, some are not; do not delete blindly
- `Intentional API/test surface`
  - lower priority unless the surface should be redesigned

## Recommended Execution Order

Start with the files where the audit signal is strongest and the likely blast radius is low:

1. `code-rs/core/src/function_tool.rs`
2. `code-rs/core/src/history/state.rs`
3. `code-rs/core/src/tools/spec.rs`
4. `code-rs/tui/src/app_event.rs`
5. `code-rs/tui/src/diff_render.rs`
6. `code-rs/tui/src/bottom_pane/chat_composer.rs`
7. `code-rs/tui/src/chatwidget.rs`

After that, move to the broader or more conditional modules:

1. `code-rs/tui/src/bottom_pane/mod.rs`
2. `code-rs/tui/src/bottom_pane/paste_burst.rs`
3. `code-rs/tui/src/bottom_pane/theme_selection_view/mod.rs`
4. `code-rs/core/src/acp.rs`
5. `code-rs/core/src/bridge_client.rs`
6. `code-rs/core/src/client.rs`
7. `code-rs/core/src/rollout/catalog.rs`

## Highest Priority (broad file-level dead_code allows)

- code-rs/core/tests/common/mod.rs:1:#![allow(dead_code)]
- code-rs/core/src/acp.rs:1:#![allow(dead_code)]
- code-rs/core/src/exec_command/session_manager.rs:1:#![allow(dead_code)]
- code-rs/code-auto-drive-diagnostics/src/lib.rs:8:#![allow(dead_code)]
- code-rs/tui/src/markdown.rs:1:#![allow(dead_code)]
- code-rs/tui/src/markdown_stream.rs:1:#![allow(dead_code)]
- code-rs/tui/src/streaming/controller.rs:1:#![allow(dead_code)]
- code-rs/tui/src/bottom_pane/theme_selection_view/mod.rs:1:#![allow(dead_code)]
- code-rs/tui/src/chatwidget/smoke_helpers.rs:1:#![allow(dead_code)]

## Likely Dead Module/Type Candidates

- `code-rs/core/src/function_tool.rs`
  - `FunctionCallError` appears to be defined but not referenced outside this file/module.
  - Candidate action: remove module and `mod function_tool;` from `core/src/lib.rs` if truly unused.

## First Investigation Pass (selected items)

### 1) `code-rs/core/src/function_tool.rs` (`#[allow(dead_code)]`)

Status: **High confidence stale/dead**

Findings:
- `FunctionCallError` is only referenced in `code-rs/core/src/function_tool.rs`.
- `core/src/lib.rs` still has `mod function_tool;`, but no call sites use the type.

Thoughts:
- Best cleanup is to delete `function_tool.rs` and remove `mod function_tool;` from `core/src/lib.rs`.
- This should be low-risk and shrinks dead surface immediately.

### 2) `code-rs/core/src/acp.rs` (`#![allow(dead_code)]`)

Status: **Partially active, partially stale**

Findings:
- Active/used:
  - `AcpFileSystem` is used by `core/src/apply_patch.rs` (`AcpFileSystem::new(...)`).
  - `acp` module is publicly exposed via `pub mod acp;` in `core/src/lib.rs`.
- Likely dead inside the module:
  - `request_permission(...)` has no call sites.
  - `new_execute_tool_call(...)` has no call sites.

Thoughts:
- Keep module, remove blanket `#![allow(dead_code)]`.
- Either:
  - remove the two unused functions now, or
  - add narrow item-level `#[allow(dead_code)]` only on those two if they are intentionally parked.
- This is a good candidate for replacing broad suppression with precise intent.

### 3) `code-rs/core/src/exec_command/session_manager.rs` (`#![allow(dead_code)]`)

Status: **Likely active; blanket allow appears stale**

Findings:
- Core runtime APIs in this file are used via the streamable shell path:
  - `handle_exec_command_request(...)`
  - `handle_write_stdin_request(...)`
  - `kill_all(...)`
  - `result_into_payload(...)`
- The file already has gated tests (`#[cfg(test)]`, plus `#[cfg(unix)]` where needed).
- No obvious reason a module-wide dead_code suppression should still be necessary.

Thoughts:
- Remove `#![allow(dead_code)]` and let compiler/clippy identify any truly dead symbols.
- If anything remains intentionally parked, re-add only item-level `#[allow(dead_code)]`.
- This should be done as a dedicated cleanup commit because the file is large and safety-sensitive.

### 4) `code-rs/tui/src/markdown.rs` (`#![allow(dead_code)]`)

Status: **Active module; blanket allow likely stale**

Findings:
- The module is used across history rendering and streaming:
  - `append_markdown(...)` and `append_markdown_with_bold_first(...)` are called from multiple TUI modules.
  - `append_markdown_with_opener_and_cwd_and_bold(...)` is used by assistant/stream cells.
- No obvious evidence this file needs a module-wide dead_code suppression.

Thoughts:
- Remove blanket allow and see which symbols (if any) are actually dead.
- Keep any truly intentional dormant helper behind item-level `#[allow(dead_code)]` with a short comment.

### 5) `code-rs/tui/src/markdown_stream.rs` (`#![allow(dead_code)]`)

Status: **Active module with a few likely dead methods**

Findings:
- `MarkdownStreamCollector` is actively used by `tui/src/streaming/mod.rs` + `streaming/controller.rs`.
- Many methods are used (`commit_complete_lines`, `has_buffered_content`, `insert_section_break`, etc.).
- One likely dead method:
  - `set_bold_first_sentence(...)` has no call sites.

Thoughts:
- Remove blanket allow.
- Remove dead methods like `set_bold_first_sentence` (or annotate narrowly if intentionally retained).
- This is a good “narrowing” pass: active core remains, dormant edges get pruned.

### 6) `code-rs/tui/src/streaming/controller.rs` (`#![allow(dead_code)]`)

Status: **Active module; blanket allow likely stale**

Findings:
- `StreamController` and `AppEventHistorySink` are used by chatwidget runtime and replay flow.
- Multiple call sites exist in:
  - `chatwidget/session_flow.rs`
  - `chatwidget/streaming.rs`
  - `chatwidget/code_event_pipeline/stream_events.rs`
  - `chatwidget/history_pipeline/replay_render.rs`

Thoughts:
- Similar to markdown modules: remove blanket allow and force explicit dead-code accounting.
- Any leftovers should be justified at item level, not module level.

### 7) `code-rs/code-auto-drive-diagnostics/src/lib.rs` (`#![allow(dead_code)]`)

Status: **Partially wired placeholder crate**

Findings:
- Used today:
  - `AutoDriveDiagnostics::completion_schema()` (TUI decision runtime).
  - `CompletionCheck` (parsed in chatwidget flow).
- Not used:
  - `DiagnosticsConfig`
  - `AutoDriveDiagnostics::new()`
  - `AutoDriveDiagnostics::run_check(...)` (currently `unimplemented!`)

Thoughts:
- This crate is not fully integrated yet; blanket allow is currently masking placeholder API.
- Recommended short-term:
  - keep only minimal exposed pieces actually used now,
  - gate planned API behind explicit TODO + item-level allow,
  - or defer and keep this crate as the “intentional placeholder” exception.

## Second Investigation Pass (selected hotspots)

### 8) `code-rs/tui/src/app_event.rs` (`#[allow(dead_code)]` on enum variants)

Status: **Mostly stale suppressions**

Findings:
- Verified active usage for currently allow-marked variants:
  - `ModelPresetsUpdated` (app init/events flows)
  - `UpdateMcpServer` (MCP settings view state/events)
  - `PrefillComposer` (MCP settings flows)
  - `UpdateTheme` / `PreviewTheme` (theme selection)
  - `UpdateSpinner` / `PreviewSpinner` (theme/spinner selection)
  - `DiffResult` (events + diff flows)
  - `StartCommitAnimation` / `StopCommitAnimation` (streaming controller)
  - `ShowChromeOptions` (chrome connect flow)

Thoughts:
- These `#[allow(dead_code)]` annotations should be removable in a focused cleanup pass.
- Keep only item-level allows where a variant is truly compile-target dependent.

### 9) `code-rs/tui/src/bottom_pane/mod.rs` (many `#[allow(dead_code)]`)

Status: **Mixed (some stale, some intentional wrappers)**

Findings:
- Definitely used:
  - `show_notifications_settings`, `has_active_view`, `composer_is_empty`
  - `show_theme_selection`, `show_mcp_settings`
  - paste-burst wrappers are used via composer/public widget pathways.
- Likely dead or vestigial:
  - `show_diff_popup` (no current callsites found)
  - `set_footer_hints` (no current callsites found)
  - `render_auto_coordinator_footer` is an explicit empty stub.

Thoughts:
- Remove stale allows on methods with active callsites.
- For true stubs/wrappers: either delete, or keep with narrow allow + short “reserved/compat” comment.

### 10) `code-rs/tui/src/bottom_pane/paste_burst.rs` (`#[allow(dead_code)]` heavy)

Status: **Split between active enter-window path and dormant burst-buffer API**

Findings:
- Actively used now (chat composer path):
  - `record_plain_char_for_enter_window`, `enter_should_insert_newline`, `recent_plain_char`
  - `extend_enter_window`, `clear_enter_window`, `flush_if_due`, `recommended_flush_delay`, `is_active`
- Mostly not externally used now:
  - `CharDecision`, `RetroGrab`, `on_plain_char`, `decide_begin_buffer`
  - `append_newline_if_active`, `newline_should_insert_instead_of_submit`
  - `extend_window`, `begin_with_retro_grabbed`, `append_char_to_buffer`

Thoughts:
- Decide if legacy burst-buffer mode is still intended.
- If not intended: remove dormant API and the attached allows.
- If intended: add one integration path + tests so these are genuinely live.

### 11) `code-rs/tui/src/diff_render.rs` (multiple `#[allow(dead_code)]`)

Status: **Mixed; several clear cleanup wins**

Findings:
- Likely dead helpers:
  - `expand_tabs_to_spaces`
  - `strip_control_sequences`
- Likely compatibility wrappers:
  - `create_diff_summary` delegates to `create_diff_summary_with_width`
  - `render_patch_details` delegates to `_with_width`
- Confirmed active:
  - `create_diff_summary_with_width`
  - `create_diff_details_only` (used in chatwidget diff flow)

Thoughts:
- Remove truly dead helpers first (`expand_tabs_to_spaces`, `strip_control_sequences`).
- Keep wrappers only if preserving external call ergonomics; otherwise inline callers to width-aware variants.

### 12) `code-rs/core/src/rollout/catalog.rs` (9 dead_code allows)

Status: **Mostly intentional API/test surface**

Findings:
- Runtime usage exists via `session_catalog` for:
  - `by_cwd`, `by_git_root`, `reconcile`
- Many other allow-marked APIs are used in `rollout/catalog.rs` tests:
  - `get`, `remove`, `resolve_rollout_path`, `resolve_snapshot_path`

Thoughts:
- Not a prime candidate for blanket deletion.
- Better approach: keep API, reduce allows by exposing only required methods or gating test-only utilities behind `#[cfg(test)]` helpers where feasible.

### 13) `code-rs/core/src/bridge_client.rs` (9 dead_code allows)

Status: **Mixed; some dormant control APIs**

Findings:
- Active:
  - `send_bridge_control` is used by bridge tool handlers.
- Likely unused now:
  - `set_bridge_levels`
  - `set_bridge_subscription`
  - `set_bridge_filter`
  - `get_workspace_subscription`

Thoughts:
- Decide if these are future-facing API hooks or dead leftovers.
- If future-facing, keep with narrow allow + explicit comment on intended caller.
- If not, remove now to reduce drift.

### 14) `code-rs/core/src/client.rs` (7 dead_code allows)

Status: **Mixed with obvious stale allows**

Findings:
- Active getters:
  - `get_model`, `get_model_context_window`, `get_auth_manager`
- Likely unused getters:
  - `get_model_family`
  - `get_text_verbosity`
- `Error` optional fields are not dead in practice (consumed via helper predicates); some field-level allows appear stale.

Thoughts:
- Remove stale allows on actively used fields/getters.
- For truly unused getters, either remove or annotate as planned API with reason.

## Third Investigation Pass (selected hotspots)

### 15) `code-rs/tui/src/insert_history.rs`

Status: **Active module with one obvious dead compatibility artifact**

Findings:
- Active:
  - `insert_history_lines` is used by app event handling.
  - `word_wrap_lines` is used broadly by history cells/rendering.
- Likely intentional test/helper surface:
  - `insert_history_lines_to_writer`
  - `insert_history_lines_to_writer_above`
- Likely dead:
  - `SetUnderlineColor` has no callsites.

Thoughts:
- Keep the module.
- Remove `SetUnderlineColor` unless there is a concrete near-term underline-color use.
- The writer-targeted insertion helpers should either stay as explicit test/ANSI helpers with comments, or be narrowed if no external test actually needs them directly.

### 16) `code-rs/tui/src/bottom_pane/chat_composer.rs`

Status: **Mixed; mostly active with a few stale item-level allows**

Findings:
- Active:
  - `flush_paste_burst_if_due`
  - `is_in_paste_burst`
  - `recommended_paste_flush_delay`
- Likely dead/stale:
  - `use_shift_enter_hint` field does not appear to have live reads.
  - `is_empty()` appears unused; the real callsites route through `composer_text().trim().is_empty()` or `bottom_pane.composer_is_empty()`.

Thoughts:
- Remove dead field/methods rather than keeping item-level allows.
- This file is not a good candidate for broad suppression; it wants small surgical cleanup only.

### 17) `code-rs/core/src/tools/spec.rs`

Status: **Mostly stale allows**

Findings:
- Active:
  - `ToolsConfigParams` is used by client/setup/openai tools/router tests.
  - `apply_patch_tool_type` and `include_view_image_tool` are used by tool registry/build config.
- Mixed:
  - `new_from_params` is mostly a convenience wrapper and appears to be used primarily in tests.

Thoughts:
- Remove stale allows from the fields and `ToolsConfigParams`.
- For `new_from_params`, either:
  - keep as test/helper API and gate more explicitly, or
  - delete it and update tests to call `ToolsConfig::new(...)` directly.

### 18) `code-rs/core/src/history/state.rs`

Status: **Mostly stale allows**

Findings:
- `HistoryEvent`, `HistoryMutation`, and `WithId` are all used by the TUI/history hydration pipeline.
- `HistoryState` itself is used broadly in runtime and tests.
- The `Default` impl allow looks suspiciously stale; this file is no longer “parked” infrastructure.

Thoughts:
- This file should likely lose all four dead-code suppressions.
- If any specific item still triggers after removal, that would be the right moment to narrow it, but the current broad annotations look outdated.

### 19) `code-rs/tui/src/chatwidget.rs`

Status: **Mixed**

Findings:
- Active:
  - `debug_notice` is used from app init.
  - `composer_is_empty` is used by Esc/input routing.
- Likely dead:
  - `add_agents_output` has no current callsites.
  - `on_esc` has no current callsites and appears superseded by newer Esc routing.

Thoughts:
- Remove `add_agents_output` and `on_esc` if they are truly superseded.
- Keep `debug_notice` and `composer_is_empty`; their dead-code suppressions appear stale.

### 20) `code-rs/tui/src/bottom_pane/theme_selection_view/mod.rs`

Status: **Active module; blanket allow likely stale**

Findings:
- `ThemeSelectionView` and its internal state types are actively used by bottom-pane settings and overlay settings.
- The submodule tree (`core`, `input`, `render`, `pane_impl`, etc.) is fully wired.
- The top-level `#![allow(dead_code)]` looks like leftover scaffolding from an earlier split/refactor rather than something still justified.

Thoughts:
- Remove the blanket allow and let the compiler identify any real leftovers.
- Any remaining dead item should get a narrow annotation or be deleted, but the whole module does not look dormant.

## dead_code Hotspots (file counts)

Top files containing #[allow(dead_code)] or #![allow(dead_code)]:
- code-rs/tui/src/bottom_pane/paste_burst.rs (`17` occurrences)
- code-rs/tui/src/diff_render.rs (`12` occurrences)
- code-rs/tui/src/app_event.rs (`11` occurrences)
- code-rs/tui/src/bottom_pane/mod.rs (`10` occurrences)
- code-rs/core/src/rollout/catalog.rs (`9` occurrences)
- code-rs/core/src/bridge_client.rs (`9` occurrences)
- code-rs/core/src/client.rs (`7` occurrences)
- code-rs/tui/src/insert_history.rs (`5` occurrences)
- code-rs/tui/src/chatwidget.rs (`5` occurrences)
- code-rs/tui/src/bottom_pane/chat_composer.rs (`5` occurrences)
- code-rs/tui/src/auto_drive_style.rs (`5` occurrences)
- code-rs/tui/src/theme.rs (`4` occurrences)
- code-rs/tui/src/spinner.rs (`4` occurrences)
- code-rs/tui/src/colors.rs (`4` occurrences)
- code-rs/tui/src/bottom_pane/prompt_args.rs (`4` occurrences)
- code-rs/core/src/tools/spec.rs (`4` occurrences)
- code-rs/core/src/history/state.rs (`4` occurrences)
- code-rs/code-auto-drive-core/src/auto_coordinator.rs (`4` occurrences)
- code-rs/tui/src/text_processing.rs (`3` occurrences)
- code-rs/tui/src/render/line_utils.rs (`3` occurrences)
- code-rs/tui/src/chatwidget/shared_defs.rs (`3` occurrences)
- code-rs/tui/src/bottom_pane/auto_coordinator_view.rs (`3` occurrences)
- code-rs/tui/src/text_formatting.rs (`2` occurrences)
- code-rs/tui/src/status_indicator_widget.rs (`2` occurrences)
- code-rs/tui/src/markdown_renderer.rs (`2` occurrences)
- code-rs/tui/src/history_cell/tool.rs (`2` occurrences)
- code-rs/tui/src/history_cell/loading.rs (`2` occurrences)
- code-rs/tui/src/history_cell/diff.rs (`2` occurrences)
- code-rs/tui/src/history_cell/auto_drive.rs (`2` occurrences)
- code-rs/tui/src/clipboard_paste.rs (`2` occurrences)
- code-rs/tui/src/chatwidget/tests/mod.rs (`2` occurrences)
- code-rs/tui/src/bottom_pane/diff_popup.rs (`2` occurrences)
- code-rs/tui/src/bottom_pane/bottom_pane_view.rs (`2` occurrences)
- code-rs/core/src/util.rs (`2` occurrences)
- code-rs/core/src/tool_apply_patch.rs (`2` occurrences)
- code-rs/core/src/codex.rs (`2` occurrences)
- code-rs/core/src/client_common.rs (`2` occurrences)
- code-rs/core/src/agent_tool/manager.rs (`2` occurrences)
- code-rs/cli/src/bridge.rs (`2` occurrences)
- code-rs/tui/src/util/list_window.rs (`1` occurrences)
- code-rs/tui/src/streaming/controller.rs (`1` occurrences)
- code-rs/tui/src/slash_command.rs (`1` occurrences)
- code-rs/tui/src/session_log.rs (`1` occurrences)
- code-rs/tui/src/markdown_stream.rs (`1` occurrences)
- code-rs/tui/src/markdown_render.rs (`1` occurrences)
- code-rs/tui/src/markdown.rs (`1` occurrences)
- code-rs/tui/src/history_cell/wait_status.rs (`1` occurrences)
- code-rs/tui/src/history_cell/upgrade.rs (`1` occurrences)
- code-rs/tui/src/history_cell/plan_update.rs (`1` occurrences)
- code-rs/tui/src/history_cell/plain.rs (`1` occurrences)
- code-rs/tui/src/history_cell/core.rs (`1` occurrences)
- code-rs/tui/src/history_cell/agent.rs (`1` occurrences)
- code-rs/tui/src/components/textarea.rs (`1` occurrences)
- code-rs/tui/src/components/form_text_field.rs (`1` occurrences)
- code-rs/tui/src/chatwidget/smoke_helpers.rs (`1` occurrences)
- code-rs/tui/src/chatwidget/settings_routing.rs (`1` occurrences)
- code-rs/tui/src/chatwidget/session_flow.rs (`1` occurrences)
- code-rs/tui/src/chatwidget/perf.rs (`1` occurrences)
- code-rs/tui/src/chatwidget/overlay_rendering/widget_render/widget_helpers.rs (`1` occurrences)
- code-rs/tui/src/chatwidget/overlay_rendering/agents_terminal_overlay.rs (`1` occurrences)
- code-rs/tui/src/chatwidget/input_pipeline/mouse.rs (`1` occurrences)
- code-rs/tui/src/chatwidget/input_pipeline/history/operations.rs (`1` occurrences)
- code-rs/tui/src/chatwidget/chrome_connection/connect.rs (`1` occurrences)
- code-rs/tui/src/bottom_pane/theme_selection_view/mod.rs (`1` occurrences)
- code-rs/tui/src/bottom_pane/command_popup.rs (`1` occurrences)
- code-rs/tui/src/auto_drive_strings.rs (`1` occurrences)
- code-rs/tui/src/app_event_sender.rs (`1` occurrences)
- code-rs/rmcp-client/src/bin/test_streamable_http_server.rs (`1` occurrences)
- code-rs/rmcp-client/src/bin/test_stdio_server.rs (`1` occurrences)
- code-rs/rmcp-client/src/bin/rmcp_test_server.rs (`1` occurrences)
- code-rs/mcp-test-server/src/main.rs (`1` occurrences)
- code-rs/mcp-client/src/mcp_client.rs (`1` occurrences)
- code-rs/core/tests/common/mod.rs (`1` occurrences)
- code-rs/core/src/truncate.rs (`1` occurrences)
- code-rs/core/src/rollout/recorder.rs (`1` occurrences)
- code-rs/core/src/rollout/mod.rs (`1` occurrences)
- code-rs/core/src/rollout/list.rs (`1` occurrences)
- code-rs/core/src/openai_tools/types.rs (`1` occurrences)
- code-rs/core/src/function_tool.rs (`1` occurrences)
- code-rs/core/src/exec_command/session_manager.rs (`1` occurrences)
- code-rs/core/src/event_mapping.rs (`1` occurrences)
- code-rs/core/src/acp.rs (`1` occurrences)
- code-rs/code-auto-drive-diagnostics/src/lib.rs (`1` occurrences)
- code-rs/browser/src/page/viewport.rs (`1` occurrences)
- code-rs/browser/src/assets.rs (`1` occurrences)
- code-rs/app-server-protocol/src/experimental_api.rs (`1` occurrences)

## Non-test file-level allow(...) suppressions worth auditing

- code-rs/execpolicy/src/arg_type.rs:1:#![allow(clippy::needless_lifetimes)]
- code-rs/execpolicy/src/arg_matcher.rs:1:#![allow(clippy::needless_lifetimes)]
- code-rs/execpolicy/src/policy_parser.rs:1:#![allow(clippy::needless_lifetimes)]
- code-rs/execpolicy/src/lib.rs:1:#![allow(clippy::type_complexity)]
- code-rs/execpolicy/src/lib.rs:2:#![allow(clippy::too_many_arguments)]
- code-rs/execpolicy/src/opt.rs:1:#![allow(clippy::needless_lifetimes)]
- code-rs/core/src/acp.rs:1:#![allow(dead_code)]
- code-rs/tui/src/markdown.rs:1:#![allow(dead_code)]
- code-rs/tui/src/markdown_stream.rs:1:#![allow(dead_code)]
- code-rs/tui/src/syntax_highlight.rs:1:#![allow(clippy::disallowed_methods)]
- code-rs/tui/src/colors.rs:1:#![allow(clippy::disallowed_methods)]
- code-rs/code-auto-drive-diagnostics/src/lib.rs:8:#![allow(dead_code)]
- code-rs/core/src/rollout/tests.rs:1:#![allow(clippy::unwrap_used, clippy::expect_used)]
- code-rs/core/src/codex.rs:2:#![allow(clippy::unwrap_used)]
- code-rs/tui/src/streaming/controller.rs:1:#![allow(dead_code)]
- code-rs/core/src/exec_command/session_manager.rs:1:#![allow(dead_code)]
- code-rs/tui/src/history_cell/auto_drive.rs:1:#![allow(clippy::disallowed_methods)]
- code-rs/tui/src/gradient_background.rs:1:#![allow(clippy::disallowed_methods)]
- code-rs/tui/src/foundation.rs:1:#![allow(unused_imports)]
- code-rs/tui/src/auto_drive_style.rs:1:#![allow(clippy::disallowed_methods)]
- code-rs/tui/src/card_theme.rs:1:#![allow(clippy::disallowed_methods)]
- code-rs/tui/src/theme.rs:1:#![allow(clippy::disallowed_methods)]
- code-rs/tui/src/glitch_animation.rs:1:#![allow(clippy::disallowed_methods)]
- code-rs/tui/src/header_wave.rs:1:#![allow(clippy::disallowed_methods)]
- code-rs/tui/src/chatwidget/smoke_helpers.rs:1:#![allow(dead_code)]
- code-rs/tui/src/resume/tests.rs:1:#![allow(clippy::unwrap_used, clippy::expect_used)]
- code-rs/tui/src/bottom_pane/chat_composer.rs:1:#![allow(clippy::disallowed_methods)]
- code-rs/code-backend-openapi-models/src/lib.rs:1:#![allow(clippy::unwrap_used, clippy::expect_used)]
- code-rs/tui/src/bottom_pane/theme_selection_view/mod.rs:1:#![allow(dead_code)]
- code-rs/tui/src/bottom_pane/auto_coordinator_view.rs:1:#![allow(clippy::disallowed_methods)]

## Non-test item-level allow(...) suppressions worth auditing

- code-rs/shell-command/src/parse_command.rs:88:#[allow(clippy::items_after_test_module)]
- code-rs/mcp-client/src/mcp_client.rs:89:    #[allow(dead_code)]
- code-rs/core/src/function_tool.rs:3:#[allow(dead_code)]
- code-rs/tui/src/markdown_render.rs:35:#[allow(dead_code)]
- code-rs/app-server/src/transport.rs:49:#[allow(clippy::print_stderr)]
- code-rs/app-server/src/transport.rs:68:#[allow(clippy::print_stderr)]
- code-rs/core/src/openai_tools/types.rs:33:#[allow(dead_code)]
- code-rs/tui/src/onboarding/onboarding_screen.rs:24:#[allow(clippy::large_enum_variant)]
- code-rs/rmcp-client/src/rmcp_client.rs:158:    #[allow(clippy::too_many_arguments)]
- code-rs/tui/src/diff_render.rs:20:#[allow(dead_code)]
- code-rs/tui/src/diff_render.rs:43:#[allow(dead_code)]
- code-rs/tui/src/diff_render.rs:176:#[allow(dead_code)]
- code-rs/tui/src/diff_render.rs:183:#[allow(dead_code)]
- code-rs/tui/src/diff_render.rs:190:#[allow(dead_code)]
- code-rs/tui/src/diff_render.rs:534:#[allow(dead_code)]
- code-rs/tui/src/diff_render.rs:539:#[allow(dead_code)]
- code-rs/tui/src/diff_render.rs:663:#[allow(dead_code)]
- code-rs/tui/src/diff_render.rs:670:#[allow(dead_code)]
- code-rs/tui/src/diff_render.rs:786:#[allow(dead_code)]
- code-rs/tui/src/diff_render.rs:791:#[allow(dead_code)]
- code-rs/tui/src/diff_render.rs:797:#[allow(dead_code)]
- code-rs/protocol/src/protocol.rs:94:#[allow(clippy::large_enum_variant)]
- code-rs/protocol/src/protocol.rs:605:                    #[allow(clippy::expect_used)]
- code-rs/protocol/src/protocol.rs:642:                        #[allow(clippy::expect_used)]
- code-rs/protocol/src/protocol.rs:667:                            #[allow(clippy::expect_used)]
- code-rs/rmcp-client/src/bin/rmcp_test_server.rs:59:    #[allow(dead_code)]
- code-rs/tui/src/shimmer.rs:52:            #[allow(clippy::disallowed_methods)]
- code-rs/core/src/bridge_client.rs:28:    #[allow(dead_code)]
- code-rs/core/src/bridge_client.rs:30:    #[allow(dead_code)]
- code-rs/core/src/bridge_client.rs:32:    #[allow(dead_code)]
- code-rs/core/src/bridge_client.rs:34:    #[allow(dead_code)]
- code-rs/core/src/bridge_client.rs:323:#[allow(dead_code)]
- code-rs/core/src/bridge_client.rs:335:#[allow(dead_code)]
- code-rs/core/src/bridge_client.rs:348:#[allow(dead_code)]
- code-rs/core/src/bridge_client.rs:360:#[allow(dead_code)]
- code-rs/core/src/bridge_client.rs:499:#[allow(dead_code)]
- code-rs/rmcp-client/src/bin/test_stdio_server.rs:59:    #[allow(dead_code)]
- code-rs/core/src/tools/spec.rs:18:    #[allow(dead_code)]
- code-rs/core/src/tools/spec.rs:24:    #[allow(dead_code)]
- code-rs/core/src/tools/spec.rs:30:#[allow(dead_code)]
- code-rs/core/src/tools/spec.rs:97:    #[allow(dead_code)]
- code-rs/rmcp-client/src/bin/test_streamable_http_server.rs:62:    #[allow(dead_code)]
- code-rs/core/src/codex.rs:246:#[allow(dead_code)]
- code-rs/core/src/codex.rs:251:#[allow(dead_code)]
- code-rs/core/src/event_mapping.rs:25:#[allow(dead_code)]
- code-rs/core/src/client.rs:113:    #[allow(dead_code)]
- code-rs/core/src/client.rs:116:    #[allow(dead_code)]
- code-rs/core/src/client.rs:381:    #[allow(dead_code)]
- code-rs/core/src/client.rs:1750:    #[allow(dead_code)]
- code-rs/core/src/client.rs:1764:    #[allow(dead_code)]
- code-rs/core/src/client.rs:1769:    #[allow(dead_code)]
- code-rs/core/src/client.rs:1774:    #[allow(dead_code)]
- code-rs/core/src/client.rs:1959:#[allow(clippy::expect_used, clippy::unwrap_used)]
- code-rs/exec/src/event_processor_with_human_output.rs:796:        #[allow(clippy::print_stdout)]
- code-rs/core/src/protocol.rs:103:#[allow(clippy::large_enum_variant)]
- code-rs/core/src/util.rs:76:#[allow(dead_code)]
- code-rs/core/src/util.rs:94:#[allow(dead_code)]
- code-rs/core/src/exec.rs:221:            #[allow(unused_mut)]
- code-rs/core/src/exec.rs:224:            #[allow(unused_variables)]
- code-rs/core/src/exec.rs:404:    #[allow(unused_variables)]
- code-rs/core/src/exec.rs:654:    #[allow(clippy::let_unit_value)]
- code-rs/core/src/rollout/mod.rs:6:#[allow(dead_code)]
- code-rs/core/src/rollout/mod.rs:18:#[allow(unused_imports)]
- code-rs/core/src/rollout/mod.rs:21:#[allow(unused_imports)]
- code-rs/otel/src/otel_event_manager.rs:119:    #[allow(clippy::too_many_arguments)]
- code-rs/core/src/history/state.rs:1406:#[allow(dead_code)]
- code-rs/core/src/history/state.rs:2243:#[allow(dead_code)]
- code-rs/core/src/history/state.rs:2367:#[allow(dead_code)]
- code-rs/core/src/history/state.rs:2375:#[allow(dead_code)]
- code-rs/core/src/client_common.rs:39:#[allow(dead_code)]
- code-rs/core/src/client_common.rs:239:    #[allow(dead_code)]
- code-rs/core/src/rollout/catalog.rs:206:    #[allow(dead_code)]
- code-rs/core/src/rollout/catalog.rs:219:    #[allow(dead_code)]
- code-rs/core/src/rollout/catalog.rs:236:    #[allow(dead_code)]
- code-rs/core/src/rollout/catalog.rs:289:    #[allow(dead_code)]
- code-rs/core/src/rollout/catalog.rs:301:    #[allow(dead_code)]
- code-rs/core/src/rollout/catalog.rs:359:    #[allow(dead_code)]
- code-rs/core/src/rollout/catalog.rs:367:    #[allow(dead_code)]
- code-rs/core/src/rollout/catalog.rs:380:#[allow(dead_code)]
- code-rs/core/src/rollout/catalog.rs:388:#[allow(dead_code)]
- code-rs/exec/src/event_processor_with_json_output.rs:27:    #[allow(clippy::print_stdout)]
- code-rs/exec/src/event_processor_with_json_output.rs:44:    #[allow(clippy::print_stdout)]
- code-rs/tui/src/spinner.rs:149:#[allow(dead_code)]
- code-rs/tui/src/spinner.rs:162:#[allow(dead_code)]
- code-rs/tui/src/spinner.rs:227:#[allow(dead_code)]
- code-rs/tui/src/spinner.rs:237:#[allow(dead_code)]
- code-rs/core/src/rollout/list.rs:138:#[allow(dead_code)]
- code-rs/core/src/rollout/list.rs:449:    #[allow(clippy::unwrap_used)]
- code-rs/core/src/rollout/list.rs:452:    #[allow(clippy::unwrap_used)]
- code-rs/core/src/openai_tools.rs:15:#[allow(clippy::expect_used)]
- code-rs/tui/src/auto_drive_strings.rs:66:#[allow(dead_code)]
- code-rs/core/src/config.rs:129:#[allow(deprecated)]
- code-rs/core/src/rollout/recorder.rs:70:    #[allow(dead_code)]
- code-rs/apply-patch/src/parser.rs:59:#[allow(clippy::enum_variant_names)]
- code-rs/core/src/truncate.rs:8:#[allow(dead_code)]
- code-rs/core/src/exec_command/mod.rs:8:#[allow(unused_imports)]
- code-rs/core/src/exec_command/mod.rs:10:#[allow(unused_imports)]
- code-rs/core/src/exec_command/mod.rs:12:#[allow(unused_imports)]
- code-rs/core/src/exec_command/mod.rs:14:#[allow(unused_imports)]
- code-rs/core/src/exec_command/mod.rs:16:#[allow(unused_imports)]
- code-rs/core/src/exec_command/mod.rs:18:#[allow(unused_imports)]
- code-rs/core/src/exec_command/mod.rs:20:#[allow(unused_imports)]
- code-rs/core/src/exec_command/mod.rs:22:#[allow(unused_imports)]
- code-rs/cli/src/bridge.rs:40:    #[allow(dead_code)]
- code-rs/cli/src/bridge.rs:43:    #[allow(dead_code)]
- code-rs/core/src/tool_apply_patch.rs:22:#[allow(dead_code)]
- code-rs/core/src/tool_apply_patch.rs:36:#[allow(dead_code)]
- code-rs/core/src/exec_command/session_manager.rs:657:    #[allow(clippy::print_stderr)]
- code-rs/tui/src/session_log.rs:201:#[allow(dead_code)]
- code-rs/apply-patch/src/lib.rs:35:#[allow(async_fn_in_trait)]
- code-rs/app-server-protocol/src/protocol/common.rs:45:    #[allow(non_upper_case_globals)]
- code-rs/app-server-protocol/src/protocol/common.rs:162:        #[allow(clippy::vec_init_then_push)]
- code-rs/app-server-protocol/src/protocol/common.rs:173:        #[allow(clippy::vec_init_then_push)]
- code-rs/app-server-protocol/src/protocol/common.rs:551:        #[allow(clippy::vec_init_then_push)]
- code-rs/app-server-protocol/src/protocol/common.rs:565:        #[allow(clippy::vec_init_then_push)]
- code-rs/app-server-protocol/src/protocol/common.rs:618:        #[allow(clippy::vec_init_then_push)]
- code-rs/tui/src/slash_command.rs:320:    #[allow(dead_code)]
- code-rs/tui/src/app_event_sender.rs:23:    #[allow(dead_code)]
- code-rs/tui/src/colors.rs:32:#[allow(dead_code)]
- code-rs/tui/src/colors.rs:66:#[allow(dead_code)]
- code-rs/tui/src/colors.rs:118:#[allow(dead_code)]
- code-rs/tui/src/colors.rs:215:#[allow(dead_code)]
- code-rs/tui/src/util/list_window.rs:5:#[allow(dead_code)]
- code-rs/core/src/agent_tool/manager.rs:43:    #[allow(dead_code)]
- code-rs/core/src/agent_tool/manager.rs:288:    #[allow(dead_code)]
- code-rs/tui/src/components/form_text_field.rs:153:    #[allow(dead_code)]
- code-rs/app-server-protocol/src/experimental_api.rs:37:    #[allow(dead_code)]
- code-rs/tui/src/components/textarea.rs:188:    #[allow(dead_code)]
- code-rs/tui/src/components/textarea.rs:905:    #[allow(clippy::unwrap_used)]
- code-rs/tui/src/app/state.rs:58:#[allow(clippy::large_enum_variant)]
- code-rs/mcp-test-server/src/main.rs:19:    #[allow(dead_code)]
- code-rs/tui/src/app/render.rs:23:    #[allow(clippy::unwrap_used)]
- code-rs/tui/src/markdown_renderer.rs:15:    #[allow(dead_code)]
- code-rs/tui/src/markdown_renderer.rs:1418:#[allow(dead_code)]
- code-rs/tui/src/text_formatting.rs:8:#[allow(dead_code)]
- code-rs/tui/src/text_formatting.rs:71:#[allow(dead_code)]
- code-rs/code-auto-drive-core/src/auto_coordinator.rs:585:    #[allow(dead_code)]
- code-rs/code-auto-drive-core/src/auto_coordinator.rs:603:#[allow(dead_code)]
- code-rs/code-auto-drive-core/src/auto_coordinator.rs:625:#[allow(dead_code)]
- code-rs/code-auto-drive-core/src/auto_coordinator.rs:646:#[allow(dead_code)]
- code-rs/tui/src/chatwidget.rs:2466:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget.rs:4121:        #[allow(unreachable_code)]
- code-rs/tui/src/chatwidget.rs:4812:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget.rs:5198:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget.rs:5235:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget.rs:8266:#[allow(dead_code)]
- code-rs/tui/src/history_cell/plain.rs:634:#[allow(dead_code)]
- code-rs/tui/src/history_cell/diff.rs:127:#[allow(dead_code)]
- code-rs/tui/src/history_cell/diff.rs:132:#[allow(dead_code)]
- code-rs/tui/src/history_cell/agent.rs:68:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget/overlay_rendering/agents_terminal_overlay.rs:575:    #[allow(dead_code)]
- code-rs/tui/src/history_cell/wait_status.rs:21:    #[allow(dead_code)]
- code-rs/tui/src/history_cell/plan_update.rs:23:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget/overlay_rendering/widget_render/widget_helpers.rs:3:#[allow(dead_code)]
- code-rs/tui/src/history_cell/auto_drive.rs:141:    #[allow(dead_code)]
- code-rs/tui/src/history_cell/auto_drive.rs:147:    #[allow(dead_code)]
- code-rs/tui/src/history_cell/tool.rs:40:    #[allow(dead_code)]
- code-rs/tui/src/history_cell/tool.rs:269:    #[allow(dead_code)]
- code-rs/tui/src/history_cell/upgrade.rs:21:    #[allow(dead_code)]
- code-rs/tui/src/history_cell/core.rs:259:    #[allow(dead_code)]
- code-rs/tui/src/history_cell/loading.rs:18:    #[allow(dead_code)]
- code-rs/tui/src/history_cell/loading.rs:67:#[allow(dead_code)]
- code-rs/tui/src/lib.rs:546:        #[allow(clippy::print_stderr)]
- code-rs/tui/src/lib.rs:558:        #[allow(clippy::print_stderr)]
- code-rs/tui/src/lib.rs:609:        #[allow(clippy::print_stderr)]
- code-rs/tui/src/lib.rs:752:    #[allow(clippy::print_stderr)]
- code-rs/tui/src/lib.rs:1017:#[allow(clippy::print_stderr)]
- code-rs/tui/src/lib.rs:1022:#[allow(clippy::print_stdout, clippy::print_stderr)]
- code-rs/tui/src/chatwidget/overlay_rendering/widget_render/history_scroller/render_pass/cell_paint.rs:4:    #[allow(clippy::too_many_arguments)]
- code-rs/tui/src/theme.rs:112:#[allow(dead_code)]
- code-rs/tui/src/theme.rs:123:#[allow(dead_code)]
- code-rs/tui/src/theme.rs:129:#[allow(dead_code)]
- code-rs/tui/src/theme.rs:139:#[allow(dead_code)]
- code-rs/tui/src/status_indicator_widget.rs:21:#[allow(dead_code)]
- code-rs/tui/src/status_indicator_widget.rs:36:#[allow(dead_code)]
- code-rs/tui/src/auto_drive_style.rs:82:#[allow(dead_code)]
- code-rs/tui/src/auto_drive_style.rs:93:#[allow(dead_code)]
- code-rs/tui/src/auto_drive_style.rs:105:#[allow(dead_code)]
- code-rs/tui/src/auto_drive_style.rs:113:#[allow(dead_code)]
- code-rs/tui/src/auto_drive_style.rs:121:#[allow(dead_code)]
- code-rs/tui/src/clipboard_paste.rs:10:    #[allow(dead_code)]
- code-rs/tui/src/clipboard_paste.rs:39:    #[allow(dead_code)]
- code-rs/tui/src/app_event.rs:141:#[allow(clippy::large_enum_variant)]
- code-rs/tui/src/app_event.rs:158:    #[allow(dead_code)]
- code-rs/tui/src/app_event.rs:446:    #[allow(dead_code)]
- code-rs/tui/src/app_event.rs:467:    #[allow(dead_code)]
- code-rs/tui/src/app_event.rs:571:    #[allow(dead_code)]
- code-rs/tui/src/app_event.rs:592:    #[allow(dead_code)]
- code-rs/tui/src/app_event.rs:602:    #[allow(dead_code)]
- code-rs/tui/src/app_event.rs:605:    #[allow(dead_code)]
- code-rs/tui/src/app_event.rs:633:    #[allow(dead_code)]
- code-rs/tui/src/app_event.rs:655:    #[allow(dead_code)]
- code-rs/tui/src/app_event.rs:657:    #[allow(dead_code)]
- code-rs/tui/src/app_event.rs:683:    #[allow(dead_code)]
- code-rs/tui/src/insert_history.rs:26:#[allow(dead_code)]
- code-rs/tui/src/insert_history.rs:34:#[allow(dead_code)]
- code-rs/tui/src/insert_history.rs:129:#[allow(dead_code)]
- code-rs/tui/src/insert_history.rs:135:#[allow(dead_code)]
- code-rs/tui/src/insert_history.rs:397:#[allow(dead_code)]
- code-rs/tui/src/text_processing.rs:6:#[allow(dead_code)]
- code-rs/tui/src/text_processing.rs:19:#[allow(dead_code)]
- code-rs/tui/src/text_processing.rs:70:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/chat_composer.rs:141:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/chat_composer.rs:628:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/chat_composer.rs:826:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/chat_composer.rs:1395:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/chat_composer.rs:2093:    #[allow(dead_code)]
- code-rs/browser/src/assets.rs:28:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/review_settings_view.rs:136:    #[allow(clippy::too_many_arguments)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:6:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:9:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:15:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:17:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:25:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:38:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:111:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:117:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:191:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:203:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:210:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:216:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:226:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:243:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:280:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:300:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/paste_burst.rs:311:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/command_popup.rs:51:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/bottom_pane_view.rs:14:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/bottom_pane_view.rs:87:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget/chrome_connection/connect.rs:209:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/diff_popup.rs:12:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/diff_popup.rs:19:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/auto_coordinator_view.rs:41:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/auto_coordinator_view.rs:132:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/auto_coordinator_view.rs:144:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/prompt_args.rs:81:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/prompt_args.rs:248:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/prompt_args.rs:272:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/prompt_args.rs:335:#[allow(dead_code)]
- code-rs/tui/src/render/line_utils.rs:4:#[allow(dead_code)]
- code-rs/tui/src/render/line_utils.rs:21:#[allow(dead_code)]
- code-rs/tui/src/render/line_utils.rs:41:#[allow(dead_code)]
- code-rs/tui/src/bottom_pane/mod.rs:285:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/mod.rs:453:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/mod.rs:900:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/mod.rs:993:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/mod.rs:1015:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/mod.rs:1124:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/mod.rs:1252:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/mod.rs:1285:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/mod.rs:1290:    #[allow(dead_code)]
- code-rs/tui/src/bottom_pane/mod.rs:1375:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget/perf.rs:179:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget/shared_defs.rs:394:#[allow(dead_code)]
- code-rs/tui/src/chatwidget/shared_defs.rs:401:#[allow(dead_code)]
- code-rs/tui/src/chatwidget/shared_defs.rs:676:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget/settings_routing.rs:57:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget/session_flow.rs:1324:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget/input_pipeline/mouse.rs:4:    #[allow(dead_code)]
- code-rs/tui/src/chatwidget/input_pipeline/history/operations.rs:146:    #[allow(dead_code)]
- code-rs/browser/src/page/viewport.rs:12:    #[allow(dead_code)]
- code-rs/utils/cargo-bin/src/lib.rs:38:#[allow(deprecated)]
- code-rs/mcp-server/src/exec_approval.rs:49:#[allow(clippy::too_many_arguments)]
- code-rs/mcp-server/src/patch_approval.rs:43:#[allow(clippy::too_many_arguments)]

## Suggested Cleanup Order

1. Remove obviously unused modules/types (start with `core/src/function_tool.rs`).
2. Replace `#![allow(dead_code)]` with narrow item-level `#[allow(dead_code)]` only where justified.
3. For TUI/core hotspots with many dead_code allows, either wire code paths or delete stale branches.
4. Audit `clippy::unwrap_used` / `clippy::expect_used` outside tests (especially crate-level allows).
5. Re-run required validation after each batch: `./build-fast.sh`.
argo metadata guard…
OK: No forbidden ../codex-rs dependencies detected in code-rs/.
Cache bucket: main-0d6e4079e367-bd2f9194c9b3 (branch/worktree)
Using rustup toolchain: 1.93.0-aarch64-apple-darwin
rustc 1.93.0 (254b59607 2026-01-19)
Building code binary (dev-fast mode)...
Building bins: code
OK: Build successful!
Binary location: /Users/immateria/Codex-CLI-Mod/code-termux/.code/working/_target-cache/code-termux/main-0d6e4079e367-bd2f9194c9b3/code-rs/dev-fast/code

Binary Hash: cbeba4288436caded18b754088450a5129c211ea6902c575bee3323679b2b865 (176M).
