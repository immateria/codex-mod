use code_core::config_types::{validation_tool_category, ValidationCategory};
use code_core::protocol::ValidationGroup;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::cell::Cell;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_ui::hints::{shortcut_line, KeyHint};
use super::settings_ui::line_runs::{selection_id_at, SelectableLineRun};
use super::settings_ui::menu_page::SettingsMenuPage;
use super::settings_ui::panel::SettingsPanelStyle;
use super::settings_ui::toggle;
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
    scroll_top_to_keep_visible,
};
use crate::components::scroll_state::ScrollState;
use super::BottomPane;

#[derive(Clone, Debug)]
pub(crate) struct ToolStatus {
    pub name: &'static str,
    pub description: &'static str,
    pub installed: bool,
    pub install_hint: String,
    pub category: ValidationCategory,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupStatus {
    pub group: ValidationGroup,
    pub name: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct ToolRow {
    pub status: ToolStatus,
    pub enabled: bool,
    pub group_enabled: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SelectionKind {
    Group(usize),
    Tool(usize),
}

enum RowData {
    Header { group_idx: usize },
    Spacer,
    Tool { idx: usize },
}

const DEFAULT_VISIBLE_ROWS: usize = 8;

#[derive(Clone, Debug)]
struct ValidationListModel {
    runs: Vec<SelectableLineRun<'static, usize>>,
    /// Selection index -> semantic kind.
    selection_kinds: Vec<SelectionKind>,
    /// Selection index -> absolute line index within the flattened run list.
    selection_line: Vec<usize>,
    /// Selection index -> inclusive (section_start_line, section_end_line).
    section_bounds: Vec<(usize, usize)>,
    /// Total line count across all runs.
    total_lines: usize,
}

pub(crate) struct ValidationSettingsView {
    groups: Vec<(GroupStatus, bool)>,
    tools: Vec<ToolRow>,
    app_event_tx: AppEventSender,
    state: ScrollState,
    is_complete: bool,
    tool_name_width: usize,
    viewport_rows: Cell<usize>,
    pending_notice: Option<String>,
}

impl ValidationSettingsView {
    pub fn new(
        groups: Vec<(GroupStatus, bool)>,
        tools: Vec<ToolRow>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        let tool_name_width = tools.iter().map(|row| row.status.name.len()).max().unwrap_or(0);
        Self {
            groups,
            tools,
            app_event_tx,
            state,
            is_complete: false,
            tool_name_width,
            viewport_rows: Cell::new(0),
            pending_notice: None,
        }
    }

    fn toggle_group(&mut self, idx: usize) {
        if idx >= self.groups.len() {
            return;
        }
        let group = self.groups[idx].0.group;
        let new_value;
        {
            let (_, enabled) = &mut self.groups[idx];
            new_value = !*enabled;
            *enabled = new_value;
        }
        self.apply_group_to_tools(group, new_value);
        self.app_event_tx
            .send(AppEvent::UpdateValidationGroup { group, enable: new_value });
    }

    fn toggle_tool(&mut self, idx: usize) {
        if let Some(row) = self.tools.get_mut(idx) {
            if !row.status.installed {
                return;
            }
            row.enabled = !row.enabled;
            self.app_event_tx.send(AppEvent::UpdateValidationTool {
                name: row.status.name.to_string(),
                enable: row.enabled,
            });
        }
    }

    fn apply_group_to_tools(&mut self, group: ValidationGroup, enabled: bool) {
        for tool in &mut self.tools {
            if group_for_category(tool.status.category) == group {
                tool.group_enabled = enabled;
            }
        }
    }

    fn build_model(&self, selected_idx: usize) -> ValidationListModel {
        let mut runs = Vec::new();
        let mut selection_kinds = Vec::new();
        let mut selection_line = Vec::new();
        let mut section_bounds = Vec::new();

        let mut current_line = 0usize;

        for (group_idx, (status, enabled)) in self.groups.iter().enumerate() {
            let section_start = current_line;
            let section_selection_start = selection_kinds.len();

            let group_sel_idx = selection_kinds.len();
            selection_kinds.push(SelectionKind::Group(group_idx));
            selection_line.push(current_line);
            section_bounds.push((0, 0));
            runs.push(
                SelectableLineRun::selectable(
                    group_sel_idx,
                    vec![self.render_row(
                        &RowData::Header { group_idx },
                        group_sel_idx == selected_idx,
                    )],
                )
                .with_style(if group_sel_idx == selected_idx {
                    Style::new().bg(colors::selection())
                } else {
                    Style::new()
                }),
            );
            current_line = current_line.saturating_add(1);

            for (idx, row) in self.tools.iter().enumerate() {
                if group_for_category(row.status.category) != status.group {
                    continue;
                }
                if *enabled {
                    let tool_sel_idx = selection_kinds.len();
                    selection_kinds.push(SelectionKind::Tool(idx));
                    selection_line.push(current_line);
                    section_bounds.push((0, 0));
                    runs.push(
                        SelectableLineRun::selectable(
                            tool_sel_idx,
                            vec![self.render_row(
                                &RowData::Tool { idx },
                                tool_sel_idx == selected_idx,
                            )],
                        )
                        .with_style(if tool_sel_idx == selected_idx {
                            Style::new().bg(colors::selection())
                        } else {
                            Style::new()
                        }),
                    );
                } else {
                    runs.push(SelectableLineRun::plain(vec![self.render_row(
                        &RowData::Tool { idx },
                        false,
                    )]));
                }
                current_line = current_line.saturating_add(1);
            }

            let section_end = current_line.saturating_sub(1);
            for idx in section_selection_start..selection_kinds.len() {
                section_bounds[idx] = (section_start, section_end);
            }

            if group_idx + 1 < self.groups.len() {
                runs.push(SelectableLineRun::plain(vec![self.render_row(&RowData::Spacer, false)]));
                current_line = current_line.saturating_add(1);
            }
        }

        ValidationListModel {
            runs,
            selection_kinds,
            selection_line,
            section_bounds,
            total_lines: current_line,
        }
    }

    fn activate_selection(&mut self, pane: Option<&mut BottomPane<'_>>, selection: SelectionKind) {
        match selection {
            SelectionKind::Group(idx) => self.toggle_group(idx),
            SelectionKind::Tool(idx) => {
                if let Some(tool) = self.tools.get(idx) {
                    if !tool.status.installed {
                        let command = tool.status.install_hint.trim().to_string();
                        let tool_name = tool.status.name.to_string();
                        if command.is_empty() {
                            self.flash_notice(
                                pane,
                                format!(
                                    "No install command available for {tool_name}"
                                ),
                            );
                        } else {
                            self.flash_notice(
                                pane,
                                format!(
                                    "Opening terminal to install {tool_name}"
                                ),
                            );
                            self.is_complete = true;
                            self.app_event_tx.send(AppEvent::RequestValidationToolInstall {
                                name: tool_name,
                                command,
                            });
                        }
                    } else {
                        self.toggle_tool(idx);
                    }
                }
            }
        }
    }

    fn flash_notice(&mut self, pane: Option<&mut BottomPane<'_>>, text: String) {
        if let Some(pane) = pane {
            pane.flash_footer_notice(text.clone());
        }
        self.pending_notice = Some(text);
        self.app_event_tx.send(AppEvent::RequestRedraw);
    }

    fn ensure_selected_visible(&mut self, model: &ValidationListModel, body_height: usize) {
        if body_height == 0 || model.total_lines == 0 || model.selection_kinds.is_empty() {
            self.state.scroll_top = 0;
            return;
        }

        let total = model.selection_kinds.len();
        self.state.clamp_selection(total);
        let Some(sel_idx) = self.state.selected_idx else {
            self.state.scroll_top = 0;
            return;
        };
        let sel_idx = sel_idx.min(total.saturating_sub(1));
        self.state.selected_idx = Some(sel_idx);

        let selected_line = model
            .selection_line
            .get(sel_idx)
            .copied()
            .unwrap_or(0)
            .min(model.total_lines.saturating_sub(1));
        let (section_start, section_end) = model
            .section_bounds
            .get(sel_idx)
            .copied()
            .unwrap_or((0, model.total_lines.saturating_sub(1)));
        let section_end = section_end.min(model.total_lines.saturating_sub(1));
        let section_start = section_start.min(section_end);
        let section_len = section_end.saturating_sub(section_start).saturating_add(1);

        // If the full section fits, pin it to the top so the section header is visible.
        if section_len <= body_height {
            self.state.scroll_top = section_start;
            return;
        }

        // If we can show the section header while keeping the selection visible, do so.
        if selected_line <= section_start.saturating_add(body_height.saturating_sub(1)) {
            self.state.scroll_top = section_start;
            return;
        }

        let max_scroll_top = section_end.saturating_add(1).saturating_sub(body_height);
        let current_scroll_top = self.state.scroll_top.clamp(section_start, max_scroll_top);
        let next = scroll_top_to_keep_visible(
            current_scroll_top,
            max_scroll_top,
            body_height,
            selected_line,
            1,
        );

        self.state.scroll_top = next.clamp(section_start, max_scroll_top);
    }

    fn handle_key_event_internal(
        &mut self,
        mut pane: Option<&mut BottomPane<'_>>,
        key_event: KeyEvent,
    ) -> bool {
        let body_height_hint = match self.viewport_rows.get() {
            0 => DEFAULT_VISIBLE_ROWS,
            other => other,
        };

        let mut model = self.build_model(self.state.selected_idx.unwrap_or(0));
        let mut total = model.selection_kinds.len();
        if total == 0 {
            if matches!(key_event.code, KeyCode::Esc) {
                self.is_complete = true;
                return true;
            }
            return false;
        }

        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        model = self.build_model(self.state.selected_idx.unwrap_or(0));
        self.ensure_selected_visible(&model, body_height_hint);

        let current_kind = self
            .state
            .selected_idx
            .and_then(|sel| model.selection_kinds.get(sel))
            .copied();

        let handled = match key_event {
            KeyEvent { code: KeyCode::Up, .. } => {
                self.state.move_up_wrap(total);
                true
            }
            KeyEvent { code: KeyCode::Down, .. } => {
                self.state.move_down_wrap(total);
                true
            }
            KeyEvent { code: KeyCode::Left, .. } | KeyEvent { code: KeyCode::Right, .. } => {
                if let Some(kind) = current_kind {
                    match kind {
                        SelectionKind::Group(idx) => self.toggle_group(idx),
                        SelectionKind::Tool(idx) => {
                            if let Some(tool) = self.tools.get(idx)
                                && tool.status.installed {
                                    self.toggle_tool(idx);
                                }
                        }
                    }
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Char(' '), .. } => {
                if let Some(kind) = current_kind {
                    self.activate_selection(pane.take(), kind);
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                if let Some(kind) = current_kind {
                    self.activate_selection(pane.take(), kind);
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        };

        model = self.build_model(self.state.selected_idx.unwrap_or(0));
        total = model.selection_kinds.len();
        if total == 0 {
            self.state.selected_idx = None;
            self.state.scroll_top = 0;
        } else {
            self.state.clamp_selection(total);
            model = self.build_model(self.state.selected_idx.unwrap_or(0));
            self.ensure_selected_visible(&model, body_height_hint);
        }
        handled
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.handle_key_event_internal(None, key_event)
    }

    fn handle_mouse_event_internal(
        &mut self,
        mut pane: Option<&mut BottomPane<'_>>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let page = self.page();
        let Some(layout) = page.layout(area) else {
            return false;
        };

        let mut model = self.build_model(self.state.selected_idx.unwrap_or(0));
        let total = model.selection_kinds.len();
        if total == 0 {
            return false;
        }

        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        model = self.build_model(self.state.selected_idx.unwrap_or(0));
        self.ensure_selected_visible(&model, layout.body.height as usize);

        let mut selected = self.state.selected_idx.unwrap_or(0);
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            total,
            |x, y| selection_id_at(layout.body, x, y, self.state.scroll_top, &model.runs),
            SelectableListMouseConfig {
                hover_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.state.selected_idx = Some(selected);

        if matches!(result, SelectableListMouseResult::Activated)
            && let Some(kind) = model.selection_kinds.get(selected).copied() {
                self.activate_selection(pane.take(), kind);
            }

        if result.handled() {
            model = self.build_model(self.state.selected_idx.unwrap_or(0));
            let total = model.selection_kinds.len();
            if total == 0 {
                self.state.selected_idx = None;
                self.state.scroll_top = 0;
            } else {
                self.state.clamp_selection(total);
                model = self.build_model(self.state.selected_idx.unwrap_or(0));
                self.ensure_selected_visible(&model, layout.body.height as usize);
            }
        }
        result.handled()
    }

    pub fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.handle_mouse_event_internal(None, mouse_event, area)
    }

    pub fn is_view_complete(&self) -> bool {
        self.is_complete
    }

    fn render_header_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                "Toggle validation groups and installed tools.",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Use ↑↓ to navigate · Enter/Space toggle · Esc close",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(""),
        ]
    }

    fn render_footer_lines(&self) -> Vec<Line<'static>> {
        let shortcuts = shortcut_line(&[
            KeyHint::new("↑↓", " Navigate").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter/Space", " Toggle")
                .with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " Close").with_key_style(Style::new().fg(colors::error())),
        ]);

        let notice_line = match &self.pending_notice {
            Some(notice) => Line::from(Span::styled(
                notice.clone(),
                Style::new().fg(colors::warning()),
            )),
            None => Line::default(),
        };

        vec![shortcuts, notice_line]
    }

    fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Validation Settings",
            SettingsPanelStyle::bottom_pane(),
            self.render_header_lines(),
            self.render_footer_lines(),
        )
    }

    fn render_row(&self, row: &RowData, selected: bool) -> Line<'static> {
        let arrow = if selected { "› " } else { "  " };
        let arrow_style = if selected {
            Style::default().fg(colors::primary())
        } else {
            Style::default().fg(colors::text_dim())
        };
        match row {
            RowData::Header { group_idx } => {
                let Some((group, enabled)) = self.groups.get(*group_idx) else {
                    return Line::from("");
                };
                let description = match group.group {
                    ValidationGroup::Functional => "Compile & structural checks",
                    ValidationGroup::Stylistic => "Formatting and style linting",
                };
                let label_style = if selected {
                    Style::default().fg(colors::primary()).add_modifier(Modifier::BOLD)
                } else if *enabled {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text_dim()).add_modifier(Modifier::BOLD)
                };
                let enabled_state = toggle::enabled_word(*enabled);
                let status_span = Span::styled(enabled_state.text, enabled_state.style);
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled(group.name, label_style),
                    Span::raw("  "),
                    status_span,
                    Span::raw("  "),
                    Span::styled(description, Style::default().fg(colors::text_dim())),
                ];
                if selected {
                    let hint = if *enabled { "(press Enter to disable)" } else { "(press Enter to enable)" };
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(hint, Style::default().fg(colors::text_dim())));
                }
                Line::from(spans)
            }
            RowData::Spacer => Line::from(""),
            RowData::Tool { idx } => {
                let row = &self.tools[*idx];
                let width = self.tool_name_width.max(row.status.name.len());
                let base_style = if row.group_enabled {
                    if selected {
                        Style::default().fg(colors::primary()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(colors::text())
                    }
                } else {
                    Style::default().fg(colors::text_dim())
                };
                let name_span = Span::styled(
                    format!("{name:<width$}", name = row.status.name, width = width),
                    base_style,
                );
                let mut spans = vec![Span::styled(arrow, arrow_style), Span::raw("  "), name_span];
                spans.push(Span::raw("  "));
                if row.group_enabled {
                    if !row.status.installed {
                        spans.push(Span::styled(
                            "missing",
                            Style::default()
                                .fg(colors::warning())
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        let status = toggle::enabled_word_warning_off(row.enabled);
                        spans.push(Span::styled(status.text, status.style));
                    }
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(row.status.description, Style::default().fg(colors::text_dim())));
                    if selected {
                        let hint = if !row.status.installed {
                            "(press Enter to install)"
                        } else {
                            "(press Enter to toggle)"
                        };
                        spans.push(Span::raw("  "));
                        spans.push(Span::styled(hint, Style::default().fg(colors::text_dim())));
                    }
                } else {
                    spans.push(Span::styled(
                        row.status.description,
                        Style::default().fg(colors::text_dim()),
                    ));
                }
                Line::from(spans)
            }
        }
    }
}

impl<'a> BottomPaneView<'a> for ValidationSettingsView {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_internal(Some(pane), key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_internal(Some(pane), key_event))
    }

    fn handle_mouse_event(
        &mut self,
        pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_mouse_event_internal(Some(pane), mouse_event, area))
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let base = 6; // header + footer + padding
        let rows = (self.groups.len() + self.tools.len() + 2) as u16; // section headers and spacing
        base + rows.min(18)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let page = self.page();
        let model = self.build_model(self.state.selected_idx.unwrap_or(0));
        let mut rects = Vec::new();
        let Some(layout) =
            page.render_runs(area, buf, self.state.scroll_top, &model.runs, &mut rects)
        else {
            return;
        };
        let visible_slots = layout.body.height as usize;
        self.viewport_rows.set(visible_slots);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn make_view(groups_enabled: bool) -> ValidationSettingsView {
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let groups = vec![(
            GroupStatus {
                group: ValidationGroup::Functional,
                name: "Functional",
            },
            groups_enabled,
        )];
        let tools = vec![ToolRow {
            status: ToolStatus {
                name: "cargo-check",
                description: "Run cargo check",
                installed: true,
                install_hint: String::new(),
                category: ValidationCategory::Functional,
            },
            enabled: true,
            group_enabled: groups_enabled,
        }];
        ValidationSettingsView::new(groups, tools, AppEventSender::new(tx))
    }

    #[test]
    fn toggling_group_can_drop_tool_selections_and_clamps_selected_idx() {
        let mut view = make_view(true);
        view.state.selected_idx = Some(1);
        view.toggle_group(0);

        let model = view.build_model(view.state.selected_idx.unwrap_or(0));
        assert_eq!(model.selection_kinds.len(), 1);
        view.state.clamp_selection(model.selection_kinds.len());
        assert_eq!(view.state.selected_idx, Some(0));
    }

    #[test]
    fn selection_id_at_matches_selectable_runs() {
        let view = make_view(true);
        let model = view.build_model(0);
        let area = Rect::new(0, 0, 30, 3);

        assert_eq!(selection_id_at(area, 1, 0, 0, &model.runs), Some(0));
        assert_eq!(selection_id_at(area, 1, 1, 0, &model.runs), Some(1));

        let view_disabled = make_view(false);
        let model_disabled = view_disabled.build_model(0);
        assert_eq!(
            selection_id_at(area, 1, 1, 0, &model_disabled.runs),
            None
        );
    }
}

fn group_for_category(category: ValidationCategory) -> ValidationGroup {
    match category {
        ValidationCategory::Functional => ValidationGroup::Functional,
        ValidationCategory::Stylistic => ValidationGroup::Stylistic,
    }
}

pub(crate) fn detect_tools() -> Vec<ToolStatus> {
    vec![
        ToolStatus {
            name: "actionlint",
            description: "Lint GitHub workflows for syntax and logic issues.",
            installed: has("actionlint"),
            install_hint: actionlint_hint(),
            category: validation_tool_category("actionlint"),
        },
        ToolStatus {
            name: "shellcheck",
            description: "Analyze shell scripts for bugs and common pitfalls.",
            installed: has("shellcheck"),
            install_hint: shellcheck_hint(),
            category: validation_tool_category("shellcheck"),
        },
        ToolStatus {
            name: "markdownlint",
            description: "Lint Markdown content for style and formatting problems.",
            installed: has("markdownlint"),
            install_hint: markdownlint_hint(),
            category: validation_tool_category("markdownlint"),
        },
        ToolStatus {
            name: "hadolint",
            description: "Lint Dockerfiles for best practices and mistakes.",
            installed: has("hadolint"),
            install_hint: hadolint_hint(),
            category: validation_tool_category("hadolint"),
        },
        ToolStatus {
            name: "yamllint",
            description: "Validate YAML files for syntax issues.",
            installed: has("yamllint"),
            install_hint: yamllint_hint(),
            category: validation_tool_category("yamllint"),
        },
        ToolStatus {
            name: "cargo-check",
            description: "Run `cargo check` to catch Rust compilation errors quickly.",
            installed: has("cargo"),
            install_hint: cargo_check_hint(),
            category: validation_tool_category("cargo-check"),
        },
        ToolStatus {
            name: "tsc",
            description: "Type-check TypeScript projects with `tsc --noEmit`.",
            installed: has("tsc"),
            install_hint: tsc_hint(),
            category: validation_tool_category("tsc"),
        },
        ToolStatus {
            name: "eslint",
            description: "Lint JavaScript/TypeScript with ESLint (no warnings allowed).",
            installed: has("eslint"),
            install_hint: eslint_hint(),
            category: validation_tool_category("eslint"),
        },
        ToolStatus {
            name: "mypy",
            description: "Static type-check Python files using mypy.",
            installed: has("mypy"),
            install_hint: mypy_hint(),
            category: validation_tool_category("mypy"),
        },
        ToolStatus {
            name: "pyright",
            description: "Run Pyright for fast Python type analysis.",
            installed: has("pyright"),
            install_hint: pyright_hint(),
            category: validation_tool_category("pyright"),
        },
        ToolStatus {
            name: "phpstan",
            description: "Analyze PHP code with phpstan using project rules.",
            installed: has("phpstan"),
            install_hint: phpstan_hint(),
            category: validation_tool_category("phpstan"),
        },
        ToolStatus {
            name: "psalm",
            description: "Run Psalm to detect PHP runtime issues.",
            installed: has("psalm"),
            install_hint: psalm_hint(),
            category: validation_tool_category("psalm"),
        },
        ToolStatus {
            name: "golangci-lint",
            description: "Lint Go modules with golangci-lint.",
            installed: has("golangci-lint"),
            install_hint: golangci_lint_hint(),
            category: validation_tool_category("golangci-lint"),
        },
        ToolStatus {
            name: "shfmt",
            description: "Format shell scripts consistently with shfmt.",
            installed: has("shfmt"),
            install_hint: shfmt_hint(),
            category: validation_tool_category("shfmt"),
        },
        ToolStatus {
            name: "prettier",
            description: "Format web assets (JS/TS/JSON/MD) with Prettier.",
            installed: has("prettier"),
            install_hint: prettier_hint(),
            category: validation_tool_category("prettier"),
        },
    ]
}

fn which(exe: &str) -> Option<std::path::PathBuf> {
    let name = std::ffi::OsStr::new(exe);
    let paths: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    for dir in paths {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn has(cmd: &str) -> bool {
    which(cmd).is_some()
}

fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

pub fn actionlint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install actionlint".to_string();
    }
    if has("brew") {
        return "brew install actionlint".to_string();
    }
    "See: https://github.com/rhysd/actionlint#installation".to_string()
}

pub fn shellcheck_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install shellcheck".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y shellcheck".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y ShellCheck".to_string();
    }
    if has("yum") {
        return "sudo yum install -y ShellCheck".to_string();
    }
    if has("brew") {
        return "brew install shellcheck".to_string();
    }
    "https://www.shellcheck.net/".to_string()
}

pub fn markdownlint_hint() -> String {
    if has("npm") {
        return "npm i -g markdownlint-cli2".to_string();
    }
    if is_macos() && has("brew") {
        return "brew install markdownlint-cli2".to_string();
    }
    "npm i -g markdownlint-cli2".to_string()
}

pub fn hadolint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install hadolint".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y hadolint".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y hadolint".to_string();
    }
    if has("yum") {
        return "sudo yum install -y hadolint".to_string();
    }
    if has("brew") {
        return "brew install hadolint".to_string();
    }
    "https://github.com/hadolint/hadolint".to_string()
}

pub fn yamllint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install yamllint".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y yamllint".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y yamllint".to_string();
    }
    if has("yum") {
        return "sudo yum install -y yamllint".to_string();
    }
    if has("brew") {
        return "brew install yamllint".to_string();
    }
    "https://yamllint.readthedocs.io/".to_string()
}

pub fn cargo_check_hint() -> String {
    if has("cargo") {
        return "cargo check --all-targets".to_string();
    }
    "Install Rust (https://rustup.rs) to enable cargo check".to_string()
}

pub fn shfmt_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install shfmt".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y shfmt".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y shfmt".to_string();
    }
    if has("yum") {
        return "sudo yum install -y shfmt".to_string();
    }
    if has("brew") {
        return "brew install shfmt".to_string();
    }
    "https://github.com/mvdan/sh".to_string()
}

pub fn prettier_hint() -> String {
    if has("npm") {
        return "npx --yes prettier --write <path>".to_string();
    }
    if is_macos() && has("brew") {
        return "brew install prettier".to_string();
    }
    "npm install --global prettier".to_string()
}

pub fn tsc_hint() -> String {
    if has("pnpm") {
        return "pnpm add -D typescript".to_string();
    }
    if has("yarn") {
        return "yarn add --dev typescript".to_string();
    }
    "npm install --save-dev typescript".to_string()
}

pub fn eslint_hint() -> String {
    if has("pnpm") {
        return "pnpm add -D eslint".to_string();
    }
    if has("yarn") {
        return "yarn add --dev eslint".to_string();
    }
    "npm install --save-dev eslint".to_string()
}

pub fn phpstan_hint() -> String {
    if has("composer") {
        return "composer require --dev phpstan/phpstan".to_string();
    }
    "See: https://phpstan.org/user-guide/getting-started".to_string()
}

pub fn psalm_hint() -> String {
    if has("composer") {
        return "composer require --dev vimeo/psalm".to_string();
    }
    "See: https://psalm.dev/docs/install/".to_string()
}

pub fn mypy_hint() -> String {
    if has("pipx") {
        return "pipx install mypy".to_string();
    }
    if has("pip3") {
        return "pip3 install --user mypy".to_string();
    }
    "pip install --user mypy".to_string()
}

pub fn pyright_hint() -> String {
    if has("npm") {
        return "npm install --save-dev pyright".to_string();
    }
    if has("pipx") {
        return "pipx install pyright".to_string();
    }
    "See: https://github.com/microsoft/pyright".to_string()
}

pub fn golangci_lint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install golangci-lint".to_string();
    }
    if has("go") {
        return "go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest".to_string();
    }
    "https://golangci-lint.run/usage/install/".to_string()
}
