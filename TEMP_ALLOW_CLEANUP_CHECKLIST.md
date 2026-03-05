# Temporary Allow Cleanup Checklist

This is the execution checklist derived from `TEMP_ALLOW_CLEANUP_AUDIT.md`.
It is intentionally basic and temporary.

## Batch Rules

- [ ] Work in small batches.
- [ ] Remove stale `#[allow(dead_code)]` / `#![allow(dead_code)]` first.
- [ ] Prefer deleting dead code over adding narrower suppressions.
- [ ] If an item is test-only, consider `#[cfg(test)]` before keeping a dead-code allow.
- [ ] After each cleanup batch, run `./build-fast.sh` and fix all warnings/errors before moving on.

## Batch 1: High-Confidence Cleanup

- [x] `code-rs/core/src/function_tool.rs`
  Remove the unused module/type if still unreferenced, and remove `mod function_tool;` from `code-rs/core/src/lib.rs`.
- [x] `code-rs/core/src/history/state.rs`
  Remove the current dead-code suppressions and keep only anything the compiler proves still needs one.
- [x] `code-rs/core/src/tools/spec.rs`
  Remove stale allows from `ToolsConfigParams` and active fields; decide whether `new_from_params` should stay.
- [x] `code-rs/tui/src/app_event.rs`
  Remove stale dead-code allows from variants with verified live callsites.
- [x] `code-rs/tui/src/diff_render.rs`
  Remove clearly dead helpers first, then reassess the compatibility wrappers.

## Batch 2: Small Mixed Cleanups

- [x] `code-rs/tui/src/bottom_pane/chat_composer.rs`
  Removed dead `use_shift_enter_hint` and stale dead-code annotation on `flush_paste_burst_if_due`.
  Kept `is_empty()` because it is still used through `public_widgets/composer_input.rs`.
- [x] `code-rs/tui/src/chatwidget.rs`
  Remove dead items like `add_agents_output` / `on_esc` if still superseded.
- [x] `code-rs/tui/src/insert_history.rs`
  Remove `SetUnderlineColor` if there is still no real caller.

## Batch 3: Broader TUI Cleanup

- [x] `code-rs/tui/src/bottom_pane/mod.rs`
  Removed vestigial wrappers/stubs and deleted the orphaned `diff_popup` module.
- [x] `code-rs/tui/src/bottom_pane/paste_burst.rs`
  Deleted the dormant burst-buffer API and kept the live timing-window logic only.
- [x] `code-rs/tui/src/bottom_pane/theme_selection_view/mod.rs`
  Removed the blanket dead-code allow and cleaned up now-unused imports.
- [x] `code-rs/tui/src/markdown.rs`
  Removed the blanket dead-code allow.
- [x] `code-rs/tui/src/markdown_stream.rs`
  Removed the blanket dead-code allow and trimmed dead helper methods.
- [x] `code-rs/tui/src/streaming/controller.rs`
  Removed the blanket dead-code allow and deleted unused compatibility helpers.

## Batch 4: Core Mixed/Intentional Surfaces

- [x] `code-rs/core/src/acp.rs`
  Removed the blanket dead-code allow and deleted the orphaned ACP permission helper while keeping the live filesystem/tool-call helpers.
- [x] `code-rs/core/src/bridge_client.rs`
  Removed dead bridge mutators/getters and dropped unused metadata fields while keeping the live subscription/control path.
- [x] `code-rs/core/src/client.rs`
  Removed stale dead-code allows from active fields/getters and deleted genuinely unused client accessors.
- [x] `code-rs/core/src/rollout/catalog.rs`
  Narrowed the surface by making test-only helpers test-only and removed stale dead-code suppressions from the live runtime path.
- [x] `code-rs/code-auto-drive-diagnostics/src/lib.rs`
  Trimmed the crate to the live `CompletionCheck` + schema surface instead of carrying placeholder config/runtime APIs.

## Batch 5: Test/Helper Surface Cleanup

- [x] `code-rs/core/src/exec_command/session_manager.rs`
  Tightened the PTY session-manager surface to `pub(crate)` and removed stale re-export allowances in `code-rs/core/src/exec_command/mod.rs`.
- [x] `code-rs/tui/src/chatwidget/smoke_helpers.rs`
  Removed the blanket dead-code allow, deleted orphan harness helpers with no callsites, and moved unit-test-only helpers behind `#[cfg(test)]`.
- [x] `code-rs/core/tests/common/mod.rs`
  Replaced the blanket dead-code allow with crate-scoped visibility plus narrow item-level allowances for helpers that are shared across multiple separate integration-test binaries.

## Batch 6: Core Mixed Follow-Up

- [x] `code-rs/core/src/event_mapping.rs`
  Deleted the unhooked response-item mapping module and removed its module declaration from `code-rs/core/src/lib.rs`.
- [x] `code-rs/core/src/client_common.rs`
  Removed the stale dead-code allow from `REVIEW_PROMPT` and deleted the orphan `format_user_instructions_message` helper.
- [x] `code-rs/core/src/rollout/mod.rs`
  Dropped stale dead-code/unused-import allowances and removed the unused `RolloutRecorderParams` re-export.

## Batch 7: Core/TUI Live Surface Follow-Up

- [x] `code-rs/core/src/util.rs`
  Deleted the orphan `notify_on_sigint` and `is_inside_git_repo` helpers.
- [x] `code-rs/core/src/openai_tools/types.rs`
  Removed the stale dead-code allow from the live `OpenAiTool` enum.
- [x] `code-rs/tui/src/util/list_window.rs`
  Removed the stale dead-code allow from the live `anchored_window` helper.
- [x] `code-rs/tui/src/markdown_render.rs`
  Removed the stale dead-code allow from the live exported markdown renderer.

## Batch 8: TUI Popup/Input Follow-Up

- [x] `code-rs/tui/src/bottom_pane/command_popup.rs`
  Removed the stale dead-code allow from `set_prompts`.
- [x] `code-rs/tui/src/bottom_pane/bottom_pane_view.rs`
  Removed stale dead-code allows from the live `ConditionalUpdate::NeedsRedraw` variant and `as_any` trait hook.
- [x] `code-rs/tui/src/bottom_pane/prompt_args.rs`
  Deleted the unused positional-expansion helper chain instead of keeping dead-code suppressions on it.

## Batch 9: TUI Utility/Orphan Follow-Up

- [x] `code-rs/tui/src/status_indicator_widget.rs`
  Deleted the unreferenced orphan widget plus its stale snapshot files and removed the dead `foundation::status` re-export.
- [x] `code-rs/tui/src/text_processing.rs`
  Deleted the fully unused markdown helper module and removed its `mod` declaration from `code-rs/tui/src/lib.rs`.
- [x] `code-rs/tui/src/render/line_utils.rs`
  Deleted the uncalled line-cloning helpers and removed the stale dead-code allow from `is_blank_line_trim`.
- [x] `code-rs/tui/src/clipboard_paste.rs`
  Removed dead-code suppressions by deleting the unused encoded-format surface and gating `NoImage` to clipboard-enabled builds only.

## Batch 10: Core/TUI Small Live-Surface Cleanup

- [x] `code-rs/core/src/codex.rs`
  Deleted the unreferenced `MutexExt::lock_unchecked` shim instead of keeping dead-code suppressions on it.
- [x] `code-rs/tui/src/session_log.rs`
  Deleted the orphan `log_outbound_op` helper.
- [x] `code-rs/tui/src/app_event_sender.rs`
  Removed the stale dead-code allow from the live single-channel constructor.
- [x] `code-rs/core/src/rollout/list.rs`
  Removed the stale dead-code allow from `get_conversation`, which is used by rollout tests.
- [x] `code-rs/core/src/truncate.rs`
  Removed the stale dead-code allow from the live `truncate_middle` helper.

## Batch 11: TUI Formatting/Spinner Follow-Up

- [x] `code-rs/tui/src/spinner.rs`
  Deleted the unused `spinner_group_for` helper and removed stale dead-code allows from live spinner-management functions.
- [x] `code-rs/tui/src/text_formatting.rs`
  Deleted the orphan grapheme-based tool-result truncation path while keeping the live JSON-compaction and display-width helpers.

## Batch 12: TUI Auto Drive Style Cleanup

- [x] `code-rs/tui/src/auto_drive_style.rs`
  Removed unread frame/footer fields, collapsed the unused `AccentStyle` wrapper to `Option<Style>`, and dropped stale struct-level dead-code allows.
- [x] `code-rs/tui/src/bottom_pane/auto_coordinator_view.rs`
  Updated the Beacon accent mutation path to match the simplified `FrameStyle::accent` type.

## Batch 13: TUI Auto Drive/Spinner Follow-Up

- [x] `code-rs/tui/src/spinner.rs`
  Removed the stale dead-code allow from the live `frame_at_time` helper.
- [x] `code-rs/tui/src/bottom_pane/auto_coordinator_view.rs`
  Deleted the unread `goal` field from `AutoActiveViewModel` and removed dead helper methods that had no runtime or test callsites.
- [x] `code-rs/tui/src/chatwidget/auto_drive_flow.rs`
  Dropped the now-unused `goal` field from the Auto Drive goal-entry view model construction.
- [x] `code-rs/tui/src/chatwidget/auto_drive_flow/presentation.rs`
  Dropped the now-unused `goal` field from live Auto Drive view model construction.

## Batch 14: TUI Small Cell/Strings Cleanup

- [x] `code-rs/tui/src/auto_drive_strings.rs`
  Removed the stale dead-code allow from the live placeholder-phrase helper.
- [x] `code-rs/tui/src/history_cell/loading.rs`
  Removed the stale dead-code allow from `from_state` and deleted the dead `new_loading_cell` helper.
- [x] `code-rs/tui/src/history_cell/plan_update.rs`
  Removed the stale dead-code allow from the live `from_state` constructor.
- [x] `code-rs/tui/src/history_cell/upgrade.rs`
  Deleted the unused `from_state` constructor.

## Lower-Priority Follow-Up

- [ ] Re-scan the workspace for remaining `#[allow(dead_code)]` / `#![allow(dead_code)]`.
- [ ] Re-scan for other narrow allows that may now be stale because of earlier refactors.
- [ ] Remove this checklist and the audit file before any final commit that should not carry temporary docs.
