use super::*;

use chrono::{DateTime, Utc};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::editor_page::SettingsEditorPage;
use crate::bottom_pane::settings_ui::hints::{hint_esc, hint_enter, hint_nav, hint_nav_horizontal, shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::message_page::SettingsMessagePage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;
use crate::colors;

impl MemoriesSettingsView {
    /// Color-code a boolean value: green for on, dim for off.
    fn bool_span(value: bool) -> Span<'static> {
        if value {
            Span::styled("on", Style::default().fg(colors::success()))
        } else {
            Span::styled("off", Style::default().fg(colors::text_dim()))
        }
    }

    /// Color-code an artifact presence: green for present, warning for missing.
    fn presence_span(exists: bool) -> Span<'static> {
        if exists {
            Span::styled("✓", Style::default().fg(colors::success()))
        } else {
            Span::styled("✗", Style::default().fg(colors::warning()))
        }
    }

    pub(super) fn render_header_lines(&self) -> Vec<Line<'static>> {
        let dim = Style::default().fg(colors::text_dim());
        match self.current_status() {
            Ok(Some(status)) => {
                let src = |s: code_core::MemoriesSettingSource| -> Span<'static> {
                    Span::styled(
                        format!("({})", Self::source_label(s)),
                        dim,
                    )
                };

                let mut lines = vec![
                    Line::from(vec![
                        Span::styled("Generate: ", dim),
                        Self::bool_span(status.effective.generate_memories),
                        Span::styled(" ", dim),
                        src(status.sources.generate_memories),
                        Span::styled("  Use: ", dim),
                        Self::bool_span(status.effective.use_memories),
                        Span::styled(" ", dim),
                        src(status.sources.use_memories),
                        Span::styled("  Skip MCP/web: ", dim),
                        Self::bool_span(status.effective.no_memories_if_mcp_or_web_search),
                        Span::styled(" ", dim),
                        src(status.sources.no_memories_if_mcp_or_web_search),
                    ]),
                    Line::from(Span::styled(
                        format!(
                            "Limits: retained {} · age {}d · scan {} · idle {}h",
                            status.effective.max_raw_memories_for_consolidation,
                            status.effective.max_rollout_age_days,
                            status.effective.max_rollouts_per_startup,
                            status.effective.min_rollout_idle_hours,
                        ),
                        dim,
                    )),
                    Line::from(vec![
                        Span::styled("Artifacts: summary ", dim),
                        Self::presence_span(status.artifacts.summary.exists),
                        Span::styled("  raw ", dim),
                        Self::presence_span(status.artifacts.raw_memories.exists),
                        Span::styled("  rollouts ", dim),
                        Self::presence_span(status.artifacts.rollout_summaries.exists),
                        Span::styled(
                            format!(" ({})", status.artifacts.rollout_summary_count),
                            dim,
                        ),
                    ]),
                    {
                        let total = status.db.stage1_epoch_count;
                        let derived = status.db.derived_epoch_count;
                        let empty = status.db.empty_epoch_count;
                        let fallback = total.saturating_sub(derived).saturating_sub(empty);
                        let pinned = status.db.user_memory_count;
                        let pinned_str = if pinned > 0 {
                            format!(" · {pinned} pinned")
                        } else {
                            String::new()
                        };
                        let quality = if total == 0 {
                            format!(
                                "Database: {} sessions · no memories yet{pinned_str}",
                                status.db.thread_count,
                            )
                        } else {
                            format!(
                                "Database: {} sessions · {} epochs ({} useful · {} empty{}){pinned_str}",
                                status.db.thread_count,
                                total,
                                derived,
                                empty,
                                if fallback > 0 { format!(" · {fallback} fallback") } else { String::new() },
                            )
                        };
                        Line::from(Span::styled(quality, dim))
                    },
                ];
                if self.active_profile.is_none() {
                    lines.push(Line::from(Span::styled(
                        "Active profile scope is unavailable in this session.",
                        dim,
                    )));
                }
                lines.push(Line::from(""));
                lines
            }
            Ok(None) => {
                let mut lines = vec![Line::from(Span::styled(
                    "Memories status loading…",
                    dim,
                ))];
                if self.active_profile.is_none() {
                    lines.push(Line::from(Span::styled(
                        "Active profile scope is unavailable in this session.",
                        dim,
                    )));
                }
                lines.push(Line::from(""));
                lines
            }
            Err(err) => vec![
                Line::from(Span::styled(
                    format!("Memories status unavailable: {err}"),
                    dim,
                )),
                Line::from(""),
            ],
        }
    }

    fn render_footer_lines(&self) -> Vec<Line<'static>> {
        vec![shortcut_line(&[
            hint_nav(" navigate"),
            hint_nav_horizontal(" cycle"),
            hint_enter(" edit/activate"),
            KeyHint::new(crate::bottom_pane::settings_ui::hints::key_ctrl("S"), " apply"),
            KeyHint::new(crate::bottom_pane::settings_ui::hints::key_ctrl("R"), " refresh"),
            hint_esc(" close"),
        ])]
    }

    fn main_footer_lines(&self) -> Vec<Line<'static>> {
        let footer_text = self
            .status
            .as_ref().map_or_else(|| self.row_description(self.selected_row()).to_owned(), |(text, _)| text.clone());
        let footer_style = if self.status.as_ref().is_some_and(|(_, is_error)| *is_error) {
            Style::default().fg(colors::error())
        } else {
            Style::default().fg(colors::text_dim())
        };

        let mut lines = vec![Line::from(Span::styled(footer_text, footer_style))];
        lines.extend(self.render_footer_lines());
        lines
    }

    pub(super) fn edit_page(
        scope: MemoriesScopeChoice,
        target: EditTarget,
        error: Option<&str>,
    ) -> SettingsEditorPage<'static> {
        let label = match target {
            EditTarget::MaxRawMemories => "Max retained memories",
            EditTarget::MaxRolloutAgeDays => "Max rollout age (days)",
            EditTarget::MaxRolloutsPerStartup => "Max rollouts per refresh",
            EditTarget::MinRolloutIdleHours => "Min rollout idle (hours)",
        };
        let scope_note = match scope {
            MemoriesScopeChoice::Global => "Global scope saves a concrete value.",
            MemoriesScopeChoice::Profile | MemoriesScopeChoice::Project => {
                "Leave blank to inherit from the next broader scope."
            }
        };
        let post_field_lines = match error {
            Some(message) => vec![Line::from(Span::styled(
                message.to_owned(),
                Style::default().fg(colors::warning()),
            ))],
            None => vec![Line::from(Span::styled(
                "Ctrl+S or Enter to save. Esc to cancel.",
                Style::default().fg(colors::text_dim()),
            ))],
        };
        SettingsEditorPage::new(
            " Memories ",
            SettingsPanelStyle::bottom_pane(),
            label,
            vec![
                Line::from(Span::styled(
                    scope_note,
                    Style::default().fg(colors::text_dim()),
                )),
                Line::from(""),
            ],
            post_field_lines,
        )
        .with_field_margin(crate::ui_consts::NESTED_HPAD)
    }

    pub(super) fn search_page(viewer_title: &'static str) -> SettingsEditorPage<'static> {
        let dim = Style::default().fg(colors::text_dim());
        SettingsEditorPage::new(
            viewer_title,
            SettingsPanelStyle::bottom_pane(),
            "Search",
            vec![
                Line::from(Span::styled(
                    "Type a search term (case-insensitive).",
                    dim,
                )),
                Line::from(""),
            ],
            vec![Line::from(Span::styled(
                "Enter to search · Esc to cancel",
                dim,
            ))],
        )
        .with_field_margin(crate::ui_consts::NESTED_HPAD)
    }

    pub(super) fn main_page(&self) -> SettingsRowPage<'_> {
        SettingsRowPage::new(
            " Memories ",
            self.render_header_lines(),
            self.main_footer_lines(),
        )
    }

    pub(super) fn text_viewer_page(viewer: &TextViewerState) -> SettingsMessagePage<'static> {
        let total = viewer.lines.len();
        let visible = viewer.viewport_rows.get();
        let scroll = viewer.scroll_top.get();

        let dim = Style::default().fg(colors::text_dim());
        let match_style = Style::default()
            .fg(colors::warning())
            .add_modifier(Modifier::BOLD);
        let current_match_style = Style::default()
            .fg(colors::text_bright())
            .bg(colors::selection())
            .add_modifier(Modifier::BOLD);

        let position = if total > visible {
            format!(" {}/{total} ", scroll + 1)
        } else {
            String::new()
        };

        let search_info = viewer.search.as_ref().map(|s| {
            if s.matches.is_empty() {
                format!("\"{}\" — no matches", s.query)
            } else {
                format!(
                    "\"{}\" — {}/{} matches",
                    s.query,
                    s.current + 1,
                    s.matches.len()
                )
            }
        });

        let mut header_lines = Vec::new();
        if !position.is_empty() || search_info.is_some() {
            let mut parts = Vec::new();
            if !position.is_empty() {
                parts.push(position);
            }
            if let Some(info) = search_info {
                parts.push(info);
            }
            header_lines.push(Line::from(Span::styled(parts.join("  "), dim)));
        }

        // Build highlighted match set for the current search.
        let match_lines: std::collections::HashSet<usize> = viewer
            .search
            .as_ref()
            .map(|s| s.matches.iter().copied().collect())
            .unwrap_or_default();
        let current_match_line = viewer
            .search
            .as_ref()
            .and_then(|s| s.matches.get(s.current).copied());

        let body_lines: Vec<Line<'static>> = viewer
            .lines
            .iter()
            .enumerate()
            .map(|(idx, line)| {
                let line_num_style = Style::default().fg(colors::text_dim()).add_modifier(Modifier::DIM);
                let content_style = if Some(idx) == current_match_line {
                    current_match_style
                } else if match_lines.contains(&idx) {
                    match_style
                } else {
                    dim
                };
                let gutter_width = digit_count(total);
                Line::from(vec![
                    Span::styled(
                        format!("{:>width$} ", idx + 1, width = gutter_width),
                        line_num_style,
                    ),
                    Span::styled(line.clone(), content_style),
                ])
            })
            .collect();

        let back_label = match viewer.parent {
            TextViewerParent::Main => " back",
            TextViewerParent::RolloutList(_) => " back to list",
        };
        let mut hints: Vec<KeyHint<'static>> = vec![
            hint_nav(" scroll"),
            KeyHint::new("g/G", " top/end"),
            KeyHint::new("/", " search"),
        ];
        if viewer.search.as_ref().is_some_and(|s| !s.matches.is_empty()) {
            hints.push(KeyHint::new("n/N", " next/prev"));
        }
        hints.push(hint_esc(back_label));
        let footer_lines = vec![shortcut_line(&hints)];

        SettingsMessagePage::new(
            viewer.title,
            SettingsPanelStyle::bottom_pane(),
            header_lines,
            body_lines,
            footer_lines,
        )
        .with_body_scroll(scroll as u16)
    }

    pub(super) fn rollout_list_menu_rows(list: &RolloutListState) -> Vec<SettingsMenuRow<'static, usize>> {
        let now = Utc::now();
        list.entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let size = if entry.size_bytes < 1024 {
                    format!("{}B", entry.size_bytes)
                } else {
                    format!("{:.1}KB", entry.size_bytes as f64 / 1024.0)
                };
                let age = entry.modified_at.as_deref()
                    .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
                    .map(|dt| format_age(dt.with_timezone(&Utc), now));
                let detail = match age {
                    Some(age) => format!("{size}  {age}"),
                    None => size,
                };
                SettingsMenuRow::new(idx, entry.slug.clone())
                    .with_detail(crate::bottom_pane::settings_ui::rows::StyledText::new(
                        detail,
                        Style::default().fg(colors::text_dim()),
                    ))
            })
            .collect()
    }

    pub(super) fn rollout_list_page(list: &RolloutListState) -> SettingsMenuPage<'static> {
        let total = list.entries.len();
        let state = list.list_state.get();
        let idx = state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));

        let dim = Style::default().fg(colors::text_dim());

        let header_text = if let Some(ref slug) = list.pending_delete {
            format!("Delete {slug}.md? [y]es / [n]o")
        } else {
            format!("{total} rollout summaries")
        };
        let header_style = if list.pending_delete.is_some() {
            Style::default().fg(colors::warning())
        } else {
            dim
        };
        let header = vec![Line::from(Span::styled(header_text, header_style))];

        let mut hints: Vec<KeyHint<'static>> = vec![
            hint_nav(" navigate"),
            hint_enter(" view"),
        ];
        if list.pending_delete.is_none() {
            hints.push(KeyHint::new("d", " delete"));
        }
        hints.push(hint_esc(" back"));
        let footer = vec![shortcut_line(&hints)];

        SettingsMenuPage::new(
            " Rollout Summaries ",
            SettingsPanelStyle::bottom_pane(),
            header,
            footer,
        )
        .with_scroll_position(idx + 1, total)
    }
}

/// Format a past timestamp as a human-friendly relative age.
fn format_age(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - when).num_seconds().max(0);
    if secs < 60 {
        "<1m ago".to_owned()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

// ── User memory list page ────────────────────────────────────────────

impl MemoriesSettingsView {
    pub(super) fn user_memory_list_menu_rows(
        list: &UserMemoryListState,
    ) -> Vec<SettingsMenuRow<'static, usize>> {
        let now = Utc::now();
        let mut rows: Vec<SettingsMenuRow<'static, usize>> = list
            .entries
            .iter()
            .enumerate()
            .map(|(idx, memory)| {
                // Truncate content for preview (first line, max 60 chars).
                let preview: String = memory
                    .content
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(60)
                    .collect();
                let preview = if memory.content.len() > 60 || memory.content.contains('\n') {
                    format!("{preview}…")
                } else {
                    preview
                };

                let tags_str = if memory.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", memory.tags.join(", "))
                };

                let age = {
                    let dt = chrono::DateTime::from_timestamp(memory.updated_at, 0)
                        .unwrap_or_else(Utc::now);
                    format_age(dt, now)
                };

                let detail = format!("{age}{tags_str}");
                SettingsMenuRow::new(idx, preview)
                    .with_detail(crate::bottom_pane::settings_ui::rows::StyledText::new(
                        detail,
                        Style::default().fg(colors::text_dim()),
                    ))
            })
            .collect();

        // "Add new memory" entry at the end.
        rows.push(
            SettingsMenuRow::new(list.entries.len(), "+ Add new pinned memory".to_owned())
                .with_detail(crate::bottom_pane::settings_ui::rows::StyledText::new(
                    String::new(),
                    Style::default().fg(colors::text_dim()),
                )),
        );
        rows
    }

    pub(super) fn user_memory_list_page(list: &UserMemoryListState) -> SettingsMenuPage<'static> {
        let total = list.entries.len();

        let dim = Style::default().fg(colors::text_dim());

        let header_text = if let Some(ref id) = list.pending_delete {
            let short_id = if id.len() > 16 { &id[..16] } else { id.as_str() };
            format!("Delete memory {short_id}…? [y]es / [n]o")
        } else if total == 0 {
            "No pinned memories yet. Press Enter or 'n' to create one.".to_owned()
        } else {
            format!(
                "{total} pinned memor{} — always injected into LLM prompt",
                if total == 1 { "y" } else { "ies" }
            )
        };
        let header_style = if list.pending_delete.is_some() {
            Style::default().fg(colors::warning())
        } else {
            dim
        };
        let header = vec![Line::from(Span::styled(header_text, header_style))];

        let mut hints: Vec<KeyHint<'static>> = vec![
            hint_nav(" navigate"),
            hint_enter(" edit"),
            KeyHint::new("n", " new"),
        ];
        if list.pending_delete.is_none() && total > 0 {
            hints.push(KeyHint::new("d", " delete"));
        }
        hints.push(hint_esc(" back"));
        let footer = vec![shortcut_line(&hints)];

        let display_total = total + 1;
        let state = list.list_state.get();
        let idx = state.selected_idx.unwrap_or(0).min(display_total.saturating_sub(1));

        SettingsMenuPage::new(
            " Pinned Memories ",
            SettingsPanelStyle::bottom_pane(),
            header,
            footer,
        )
        .with_scroll_position(idx + 1, display_total)
    }

    pub(super) fn user_memory_editor_page(
        editor: &UserMemoryEditorState,
    ) -> SettingsEditorPage<'static> {
        let is_new = editor.editing_id.is_none();
        let title = if is_new {
            " New Pinned Memory "
        } else {
            " Edit Pinned Memory "
        };

        let dim = Style::default().fg(colors::text_dim());
        let focus_style = Style::default().fg(colors::text_bright()).add_modifier(Modifier::BOLD);

        let content_label = if editor.focus == UserMemoryEditorFocus::Content {
            Span::styled("▸ Content", focus_style)
        } else {
            Span::styled("  Content", dim)
        };
        let tags_label = if editor.focus == UserMemoryEditorFocus::Tags {
            Span::styled("▸ Tags (comma-separated)", focus_style)
        } else {
            Span::styled("  Tags (comma-separated)", dim)
        };

        // Show the non-focused field as plain text so the user can see both.
        let non_focused_value = match editor.focus {
            UserMemoryEditorFocus::Content => {
                let tags_text = editor.tags_field.text();
                if tags_text.is_empty() {
                    String::new()
                } else {
                    format!("Tags: {tags_text}")
                }
            }
            UserMemoryEditorFocus::Tags => {
                let content_text = editor.content_field.text();
                let preview: String = content_text.chars().take(80).collect();
                if content_text.is_empty() {
                    String::new()
                } else if content_text.len() > 80 {
                    format!("Content: {preview}…")
                } else {
                    format!("Content: {preview}")
                }
            }
        };

        let mut pre_field_lines = vec![
            Line::from(Span::styled(
                "Pinned memories are always included in the LLM prompt.",
                dim,
            )),
            Line::from(Span::styled(
                "Use Tab to switch fields. Ctrl+S to save.",
                dim,
            )),
            Line::from(""),
        ];

        // Show the non-focused field's state.
        if !non_focused_value.is_empty() {
            pre_field_lines.push(Line::from(Span::styled(non_focused_value, dim)));
            pre_field_lines.push(Line::from(""));
        }

        // Show the label for the currently focused field.
        let focused_label = match editor.focus {
            UserMemoryEditorFocus::Content => content_label,
            UserMemoryEditorFocus::Tags => tags_label,
        };
        pre_field_lines.push(Line::from(focused_label));

        let post_field_lines = match &editor.error {
            Some(message) => vec![Line::from(Span::styled(
                message.clone(),
                Style::default().fg(colors::warning()),
            ))],
            None => vec![Line::from(Span::styled(
                "Ctrl+S to save · Esc to cancel",
                dim,
            ))],
        };

        SettingsEditorPage::new(
            title,
            SettingsPanelStyle::bottom_pane(),
            "",
            pre_field_lines,
            post_field_lines,
        )
        .with_field_margin(crate::ui_consts::NESTED_HPAD)
    }
}

/// Number of decimal digits needed to display `n`.
fn digit_count(mut n: usize) -> usize {
    if n == 0 { return 1; }
    let mut count = 0;
    while n > 0 {
        count += 1;
        n /= 10;
    }
    count
}
