use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use code_core::config_types::{ContextMode, ServiceTier};

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::hints::{shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::editor_page::SettingsEditorPage;
use crate::bottom_pane::settings_ui::line_runs::SelectableLineRun;
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::toggle;
use crate::colors;

use super::super::model_selection_state::{reasoning_effort_label, EntryKind, ModelSelectionData};
use super::{EditTarget, ModelSelectionView, ViewMode};

impl ModelSelectionView {
    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            self.data.target.panel_title(),
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            self.header_lines(),
            self.footer_lines(),
        )
    }

    fn dim_style() -> Style {
        Style::new().fg(colors::text_dim())
    }

    fn section_header_style() -> Style {
        Style::new().fg(colors::text_bright()).bold()
    }

    fn highlighted(base: Style, is_highlighted: bool) -> Style {
        if is_highlighted {
            base.bg(colors::selection()).bold()
        } else {
            base
        }
    }

    fn push_blank_line<'a>(lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        lines.push(SelectableLineRun::plain(vec![Line::from("")]));
    }

    fn push_fast_mode_section<'a>(&self, lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        let fast_index = 0;
        let is_selected = self.selected_index == fast_index;
        let fast_enabled = matches!(self.data.current.current_service_tier, Some(ServiceTier::Fast));
        let status = toggle::enabled_word(fast_enabled);

        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Fast mode",
            Self::section_header_style(),
        )])]));
        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Same model, but 1.5x faster responses (uses 2x credits)",
            Self::dim_style(),
        )])]));

        let label_style = {
            let base = Self::highlighted(Style::new().fg(colors::text()), is_selected);
            if fast_enabled {
                base.fg(colors::success())
            } else {
                base
            }
        };
        let arrow_style = if is_selected {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new().fg(colors::text_dim())
        };

        lines.push(SelectableLineRun::selectable(
            fast_index,
            vec![Line::from(vec![
                Span::styled(if is_selected { "› " } else { "  " }, arrow_style),
                Span::styled("Fast mode: ", label_style),
                Span::styled(
                    status.text,
                    label_style.fg(status.style.fg.unwrap_or(colors::text())),
                ),
            ])],
        ));
        // Keep fast-mode height aligned with `FAST_MODE_SECTION_HEIGHT` for scroll math.
        Self::push_blank_line(lines);
        Self::push_blank_line(lines);
    }

    fn push_context_mode_section<'a>(&self, lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        let Some(context_index) = self.data.context_mode_entry_index() else {
            return;
        };
        let is_selected = self.selected_index == context_index;
        let context_status = match self.data.current.current_context_mode {
            Some(ContextMode::OneM) => "1M",
            Some(ContextMode::Auto) => "auto",
            Some(ContextMode::Disabled) | None => "disabled",
        };
        let Some(context_window_index) = self
            .data
            .context_window_entry_index() else { return };
        let Some(auto_compact_index) = self
            .data
            .auto_compact_entry_index() else { return };
        let context_available = self.data.supports_extended_context();

        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Mode Settings",
            Self::section_header_style(),
        )])]));
        for info_line in ModelSelectionData::context_mode_intro_lines() {
            lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
                info_line,
                Self::dim_style(),
            )])]));
        }

        let label_style = {
            let base = Self::highlighted(Style::new().fg(colors::text()), is_selected);
            let with_mode = if self.data.current.current_context_mode.is_some() {
                base.fg(colors::success())
            } else {
                base
            };
            if !context_available {
                with_mode.fg(colors::text_dim())
            } else {
                with_mode
            }
        };
        let arrow_style = if is_selected {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new().fg(colors::text_dim())
        };

        lines.push(SelectableLineRun::selectable(
            context_index,
            vec![Line::from(vec![
                Span::styled(if is_selected { "› " } else { "  " }, arrow_style),
                Span::styled(format!("Context preset: {context_status}"), label_style),
            ])],
        ));

        let is_window_selected = self.selected_index == context_window_index;
        let window_label_style = Self::highlighted(Style::new().fg(colors::text()), is_window_selected);
        let window_arrow_style = if is_window_selected {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new().fg(colors::text_dim())
        };
        let mut window_value = self.current_context_window_label();
        if self.data.context_window_is_default() {
            window_value.push_str(" (auto)");
        }
        lines.push(SelectableLineRun::selectable(
            context_window_index,
            vec![Line::from(vec![
                Span::styled(
                    if is_window_selected { "› " } else { "  " },
                    window_arrow_style,
                ),
                Span::styled("Context window: ", window_label_style),
                Span::styled(window_value, window_label_style.fg(colors::success())),
            ])],
        ));

        let is_compact_selected = self.selected_index == auto_compact_index;
        let compact_label_style =
            Self::highlighted(Style::new().fg(colors::text()), is_compact_selected);
        let compact_arrow_style = if is_compact_selected {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new().fg(colors::text_dim())
        };
        let mut compact_value = self.current_auto_compact_label();
        if self.data.auto_compact_is_default() {
            compact_value.push_str(" (auto)");
        }
        lines.push(SelectableLineRun::selectable(
            auto_compact_index,
            vec![Line::from(vec![
                Span::styled(
                    if is_compact_selected { "› " } else { "  " },
                    compact_arrow_style,
                ),
                Span::styled("Auto-compact at: ", compact_label_style),
                Span::styled(compact_value, compact_label_style.fg(colors::success())),
            ])],
        ));

        if !context_available {
            lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
                "Unavailable for this model. Saved settings apply automatically on supported models.",
                Self::dim_style(),
            )])]));
        }

        Self::push_blank_line(lines);
    }

    fn push_follow_chat_section<'a>(&self, lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        let Some(follow_chat_index) = self.data.follow_chat_entry_index() else {
            return;
        };
        let is_selected = self.selected_index == follow_chat_index;

        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Follow Chat Mode",
            Self::section_header_style(),
        )])]));
        lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            "Use the active chat model and reasoning; stays in sync as chat changes.",
            Self::dim_style(),
        )])]));

        let label_style = Self::highlighted(Style::new().fg(colors::text()), is_selected);
        let arrow_style = if is_selected {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new().fg(colors::text_dim())
        };
        let indent_style = if is_selected {
            Style::new().bg(colors::selection()).bold()
        } else {
            Style::new()
        };

        let mut spans = vec![
            Span::styled(if is_selected { "› " } else { "  " }, arrow_style),
            Span::styled("   ", indent_style),
            Span::styled("Use chat model", label_style),
        ];
        if self.data.current.use_chat_model {
            spans.push(Span::raw("  (current)"));
        }

        lines.push(SelectableLineRun::selectable(
            follow_chat_index,
            vec![Line::from(spans)],
        ));
        Self::push_blank_line(lines);
    }

    fn push_preset_lines<'a>(&self, lines: &mut Vec<SelectableLineRun<'a, usize>>) {
        let mut previous_model: Option<&str> = None;
        let entries = self.data.entries();
        for (entry_idx, entry) in entries.iter().enumerate() {
            if matches!(
                entry,
                EntryKind::FastMode | EntryKind::ContextMode | EntryKind::FollowChat
            ) {
                continue;
            }
            let EntryKind::Preset(preset_index) = entry else {
                continue;
            };
            let flat_preset = &self.data.flat_presets[*preset_index];
            let is_new_model = previous_model
                .map(|model| !model.eq_ignore_ascii_case(&flat_preset.model))
                .unwrap_or(true);

            if is_new_model {
                if previous_model.is_some() {
                    Self::push_blank_line(lines);
                }
                lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
                    flat_preset.display_name.clone(),
                    Self::section_header_style(),
                )])]));
                if !flat_preset.model_description.trim().is_empty() {
                    lines.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
                        flat_preset.model_description.clone(),
                        Self::dim_style(),
                    )])]));
                }
                previous_model = Some(&flat_preset.model);
            }

            let is_selected = entry_idx == self.selected_index;
            let is_current = !self.data.current.use_chat_model
                && flat_preset.model.eq_ignore_ascii_case(&self.data.current.current_model)
                && flat_preset.effort == self.data.current.current_effort;

            let mut row_text = reasoning_effort_label(flat_preset.effort).to_string();
            if is_current {
                row_text.push_str(" (current)");
            }

            let indent_style = if is_selected {
                Style::new().bg(colors::selection()).bold()
            } else {
                Style::new()
            };
            let label_style = {
                let base = Self::highlighted(Style::new().fg(colors::text()), is_selected);
                if is_current {
                    base.fg(colors::success())
                } else {
                    base
                }
            };
            let divider_style = Self::highlighted(Style::new().fg(colors::text_dim()), is_selected);
            let description_style = Self::highlighted(Style::new().fg(colors::dim()), is_selected);

            lines.push(SelectableLineRun::selectable(
                entry_idx,
                vec![Line::from(vec![
                    Span::styled("   ", indent_style),
                    Span::styled(row_text, label_style),
                    Span::styled(" - ", divider_style),
                    Span::styled(flat_preset.description.clone(), description_style),
                ])],
            ));
        }
    }

    pub(super) fn build_render_runs<'a>(&'a self) -> Vec<SelectableLineRun<'a, usize>> {
        let mut runs = Vec::new();
        if self.data.supports_fast_mode() {
            self.push_fast_mode_section(&mut runs);
        }
        if self.data.target.supports_context_mode() {
            self.push_context_mode_section(&mut runs);
        }
        if self.data.target.supports_follow_chat() {
            self.push_follow_chat_section(&mut runs);
        }
        self.push_preset_lines(&mut runs);
        runs
    }

    fn header_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(vec![
                Span::styled(
                    format!("{}: ", self.data.target.current_label()),
                    Self::dim_style(),
                ),
                Span::styled(
                    if self.data.target.supports_follow_chat() && self.data.current.use_chat_model {
                        "Follow Chat Mode".to_string()
                    } else {
                        self.data.current_model_display_name()
                    },
                    Style::new().fg(colors::warning()).bold(),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    format!("{}: ", self.data.target.reasoning_label()),
                    Self::dim_style(),
                ),
                Span::styled(
                    if self.data.target.supports_follow_chat() && self.data.current.use_chat_model {
                        "From chat".to_string()
                    } else {
                        reasoning_effort_label(self.data.current.current_effort).to_string()
                    },
                    Style::new().fg(colors::warning()).bold(),
                ),
            ]),
            Line::from(""),
        ]
    }

    fn footer_lines(&self) -> Vec<Line<'static>> {
        let mut hints = vec![
            KeyHint::new("↑↓", " Navigate").with_key_style(Style::new().fg(colors::light_blue())),
            KeyHint::new("←→ +/-", " Adjust").with_key_style(Style::new().fg(colors::warning())),
            KeyHint::new("Enter", " Select/Edit").with_key_style(Style::new().fg(colors::success())),
        ];
        if self.data.target.supports_context_mode() {
            hints.push(
                KeyHint::new("Ctrl+S", " Save default")
                    .with_key_style(Style::new().fg(colors::primary())),
            );
        }
        hints.push(KeyHint::new("Esc", " Cancel").with_key_style(Style::new().fg(colors::error())));

        vec![
            Line::from(""),
            shortcut_line(&hints),
        ]
    }

    pub(super) fn edit_page(target: EditTarget, error: Option<&str>) -> SettingsEditorPage<'static> {
        let (title, help_text) = match target {
            EditTarget::ContextWindow => (
                "Context window",
                "Enter an integer like 500000, or 500k / 1m. Leave blank for the preset-derived value.",
            ),
            EditTarget::AutoCompact => (
                "Auto-compact threshold",
                "Enter the token threshold where pre-turn compaction should trigger. Leave blank for the preset-derived value.",
            ),
        };
        let post_field_lines = match error {
            Some(message) => vec![Line::from(Span::styled(
                message.to_string(),
                Style::new().fg(colors::error()),
            ))],
            None => vec![Line::from(Span::styled(
                "Enter to apply. Ctrl+S to save default. Esc to cancel. Use auto or blank to reset to the derived value.",
                Self::dim_style(),
            ))],
        };

        SettingsEditorPage::new(
            " Select Model & Reasoning ",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            title,
            vec![
                Line::from(Span::styled(help_text, Self::dim_style())),
                Line::from(""),
            ],
            post_field_lines,
        )
        .with_field_margin(Margin::new(2, 0))
    }

    fn render_main_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        let runs = self.build_render_runs();
        let layout = self
            .page()
            .render_runs_in_chrome(chrome, area, buf, self.scroll_offset, &runs);
        if let Some(layout) = layout {
            self.visible_body_rows.set(layout.body.height as usize);
        }
    }

    fn render_edit_in_chrome(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        target: EditTarget,
        field: &crate::components::form_text_field::FormTextField,
        error: Option<&str>,
    ) {
        let _ = Self::edit_page(target, error).render_in_chrome(chrome, area, buf, field);
    }

    pub(super) fn render_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main | ViewMode::Transition => self.render_main_in_chrome(chrome, area, buf),
            ViewMode::Edit {
                target,
                field,
                error,
            } => self.render_edit_in_chrome(chrome, area, buf, *target, field, error.as_deref()),
        }
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::ContentOnly, area, buf);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::Framed, area, buf);
    }
}
