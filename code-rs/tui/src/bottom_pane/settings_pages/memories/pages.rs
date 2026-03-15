use super::*;

use ratatui::layout::Margin;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::editor_page::SettingsEditorPage;
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
        vec![Line::from(vec![
            Span::styled("↑↓", Style::default().fg(colors::function())),
            Span::styled(" move  ", Style::default().fg(colors::text_dim())),
            Span::styled("←/→", Style::default().fg(colors::function())),
            Span::styled(" cycle  ", Style::default().fg(colors::text_dim())),
            Span::styled("Enter", Style::default().fg(colors::success())),
            Span::styled(
                " edit/activate  ",
                Style::default().fg(colors::text_dim()),
            ),
            Span::styled("Ctrl+S", Style::default().fg(colors::success())),
            Span::styled(" apply  ", Style::default().fg(colors::text_dim())),
            Span::styled("Esc", Style::default().fg(colors::error())),
            Span::styled(" close", Style::default().fg(colors::text_dim())),
        ])]
    }

    fn main_footer_lines(&self) -> Vec<Line<'static>> {
        let footer_text = self
            .status
            .as_ref()
            .map(|(text, _)| text.clone())
            .unwrap_or_else(|| self.row_description(self.selected_row()).to_string());
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
                message.to_string(),
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
        .with_field_margin(Margin::new(2, 0))
    }

    pub(super) fn main_page(&self) -> SettingsRowPage<'_> {
        SettingsRowPage::new(
            " Memories ",
            self.render_header_lines(),
            self.main_footer_lines(),
        )
    }
}

