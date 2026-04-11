use super::*;

use ratatui::style::Style;
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
    pub(super) fn render_header_lines(&self) -> Vec<Line<'static>> {
        let dim = Style::default().fg(colors::text_dim());
        match self.current_status() {
            Ok(Some(status)) => {
                let mut lines = vec![
                    Line::from(Span::styled(
                        format!(
                            "Effective: generate {} ({}) · use {} ({}) · skip {} ({})",
                            Self::bool_label(status.effective.generate_memories).to_ascii_lowercase(),
                            Self::source_label(status.sources.generate_memories),
                            Self::bool_label(status.effective.use_memories).to_ascii_lowercase(),
                            Self::source_label(status.sources.use_memories),
                            Self::bool_label(status.effective.no_memories_if_mcp_or_web_search)
                                .to_ascii_lowercase(),
                            Self::source_label(status.sources.no_memories_if_mcp_or_web_search),
                        ),
                        dim,
                    )),
                    Line::from(Span::styled(
                        format!(
                            "Limits: retained {} ({}) · age {}d ({}) · scan {} ({}) · idle {}h ({})",
                            status.effective.max_raw_memories_for_consolidation,
                            Self::source_label(status.sources.max_raw_memories_for_consolidation),
                            status.effective.max_rollout_age_days,
                            Self::source_label(status.sources.max_rollout_age_days),
                            status.effective.max_rollouts_per_startup,
                            Self::source_label(status.sources.max_rollouts_per_startup),
                            status.effective.min_rollout_idle_hours,
                            Self::source_label(status.sources.min_rollout_idle_hours),
                        ),
                        dim,
                    )),
                    Line::from(Span::styled(
                        format!(
                            "Artifacts: summary={} · raw={} · rollout_summaries={} (count={})",
                            if status.artifacts.summary.exists {
                                "present"
                            } else {
                                "missing"
                            },
                            if status.artifacts.raw_memories.exists {
                                "present"
                            } else {
                                "missing"
                            },
                            if status.artifacts.rollout_summaries.exists {
                                "present"
                            } else {
                                "missing"
                            },
                            status.artifacts.rollout_summary_count,
                        ),
                        dim,
                    )),
                    Line::from(Span::styled(
                        format!(
                            "SQLite: {} · threads {} · stage1 {} · pending {} · running {} · dead_lettered {} · dirty {}",
                            if status.db.db_exists { "present" } else { "missing" },
                            status.db.thread_count,
                            status.db.stage1_epoch_count,
                            status.db.pending_stage1_count,
                            status.db.running_stage1_count,
                            status.db.dead_lettered_stage1_count,
                            if status.db.artifact_dirty { "yes" } else { "no" },
                        ),
                        dim,
                    )),
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

        let position = if total > visible {
            format!(" {}/{total} ", scroll + 1)
        } else {
            String::new()
        };

        let dim = Style::default().fg(colors::text_dim());
        let header_lines = if !position.is_empty() {
            vec![Line::from(Span::styled(position, dim))]
        } else {
            Vec::new()
        };

        let body_lines: Vec<Line<'static>> = viewer
            .lines
            .iter()
            .map(|line| Line::from(Span::styled(line.clone(), dim)))
            .collect();

        let back_label = match viewer.parent {
            TextViewerParent::Main => " back to settings",
            TextViewerParent::RolloutList(_) => " back to list",
        };
        let footer_lines = vec![shortcut_line(&[
            hint_nav(" scroll"),
            hint_esc(back_label),
        ])];

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
        list.entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let detail = if entry.size_bytes < 1024 {
                    format!("{}B", entry.size_bytes)
                } else {
                    format!("{:.1}KB", entry.size_bytes as f64 / 1024.0)
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
        let header = vec![Line::from(Span::styled(
            format!("{total} rollout summaries"),
            dim,
        ))];

        let footer = vec![shortcut_line(&[
            hint_nav(" navigate"),
            hint_enter(" view"),
            hint_esc(" back"),
        ])];

        SettingsMenuPage::new(
            " Rollout Summaries ",
            SettingsPanelStyle::bottom_pane(),
            header,
            footer,
        )
        .with_scroll_position(idx + 1, total)
    }
}
