use super::*;

use crate::bottom_pane::settings_ui::hit_test::line_has_non_whitespace_at;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test;

impl SkillsSettingsView {
    fn list_area_from_inner(inner: Rect) -> Rect {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(inner);
        chunks[1]
    }

    pub(super) fn list_area_framed(area: Rect) -> Rect {
        let outer = Block::default().borders(Borders::ALL);
        let inner = outer.inner(area);
        Self::list_area_from_inner(inner)
    }

    pub(super) fn list_area_content_only(area: Rect) -> Rect {
        Self::list_area_from_inner(area)
    }

    fn list_selection_at_over_text(
        list_area: Rect,
        x: u16,
        y: u16,
        scroll_top: usize,
        skills_len: usize,
        hit_lines: &[Line<'static>],
    ) -> Option<usize> {
        if !list_area.contains(Position { x, y }) {
            return None;
        }

        debug_assert!(y >= list_area.y, "y={y} is above list_area.y={}", list_area.y);
        let visible_row = (y - list_area.y) as usize;

        if skills_len == 0 {
            // Empty lists render an informational line above the "Add new..." row, so treat
            // either of the first two visible lines as selecting the sole "Add new..." item.
            if visible_row > 1 {
                return None;
            }
            let line = hit_lines.get(visible_row)?;
            return line_has_non_whitespace_at(line, list_area.x, list_area.width, x).then_some(0);
        }

        let row = scroll_top.saturating_add(visible_row);
        if row > skills_len {
            return None;
        }
        let line = hit_lines.get(row)?;
        line_has_non_whitespace_at(line, list_area.x, list_area.width, x).then_some(row)
    }

    pub(super) fn handle_list_mouse_event_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let list_area = Self::list_area_framed(area);
        self.handle_list_mouse_event_with_area(mouse_event, list_area)
    }

    pub(super) fn handle_list_mouse_event_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let list_area = Self::list_area_content_only(area);
        self.handle_list_mouse_event_with_area(mouse_event, list_area)
    }

    fn handle_list_mouse_event_with_area(
        &mut self,
        mouse_event: MouseEvent,
        list_area: Rect,
    ) -> bool {
        let item_count = self.list_item_count();
        let selected = self.selected_list_index();
        let skills_len = self.skills.len();
        let hit_lines = if skills_len == 0 {
            vec![
                Line::from("No skills yet. Press Enter or Ctrl+N to create one."),
                Line::from(format!(
                    "{arrow} Add new...",
                    arrow = if selected == 0 { ">" } else { " " },
                )),
            ]
        } else {
            let mut lines = Vec::with_capacity(skills_len.saturating_add(1));
            for (idx, skill) in self.skills.iter().enumerate() {
                let arrow = if idx == selected { ">" } else { " " };
                let scope_text = match skill.scope {
                    SkillScope::Repo => " [repo]",
                    SkillScope::User => " [user]",
                    SkillScope::System => " [system]",
                };
                lines.push(Line::from(format!(
                    "{arrow} {name}{scope_text}  {desc}",
                    name = skill.name.as_str(),
                    desc = skill.description.as_str(),
                )));
            }
            lines.push(Line::from(format!(
                "{arrow} Add new...",
                arrow = if selected == skills_len { ">" } else { " " },
            )));
            lines
        };

        let kind = mouse_event.kind;
        let outcome = route_scroll_state_mouse_with_hit_test(
            mouse_event,
            &mut self.list_state,
            item_count,
            list_area.height.max(1) as usize,
            |x, y, scroll_top| {
                if matches!(kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) {
                    if !list_area.contains(Position { x, y }) {
                        return None;
                    }
                    let rel = y.saturating_sub(list_area.y) as usize;
                    Some(scroll_top.saturating_add(rel).min(item_count.saturating_sub(1)))
                } else {
                    Self::list_selection_at_over_text(
                        list_area,
                        x,
                        y,
                        scroll_top,
                        skills_len,
                        &hit_lines,
                    )
                }
            },
            SelectableListMouseConfig {
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );
        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            self.enter_editor();
        }
        outcome.changed
    }

    pub(super) fn handle_list_key(&mut self, key: KeyEvent) -> bool {
        let item_count = self.list_item_count();
        self.list_state.clamp_selection(item_count);

        match key.code {
            KeyCode::Up => {
                if let Some(selected) = self.list_state.selected_idx
                    && selected > 0
                {
                    self.list_state.selected_idx = Some(selected.saturating_sub(1));
                }
                self.list_state.ensure_visible(
                    item_count,
                    self.list_viewport_rows.get().max(1),
                );
                true
            }
            KeyCode::Down => {
                if let Some(selected) = self.list_state.selected_idx
                    && selected.saturating_add(1) < item_count
                {
                    self.list_state.selected_idx = Some(selected.saturating_add(1));
                }
                self.list_state.ensure_visible(
                    item_count,
                    self.list_viewport_rows.get().max(1),
                );
                true
            }
            _ => false,
        }
    }

    pub(super) fn start_new_skill(&mut self) {
        self.list_state.selected_idx = Some(self.skills.len());
        self.editor.name_field.set_text("");
        self.editor.description_field.set_text("");
        self.editor.style_field.set_text("");
        self.set_style_resource_fields_from_profile(None);
        self.editor.style_profile_mode = StyleProfileMode::Inherit;
        self.editor.examples_field.set_text("");
        self.editor.body_field.set_text("");
        self.editor.focus = Focus::Name;
        self.editor.edit_scroll_top = 0;
        self.editor.hovered_button = None;
        self.status = Some((
            "New skill. Fill fields, then Generate draft or Save.".to_string(),
            Style::default().fg(colors::info()),
        ));
        self.mode = Mode::Edit;
    }

    fn load_selected_into_form(&mut self) {
        let selected = self.selected_list_index();
        if let Some(skill) = self.skills.get(selected).cloned() {
            let slug = skill_slug(&skill);
            self.status = None;
            self.editor.name_field.set_text(&slug);
            self.editor
                .description_field
                .set_text(&frontmatter_value(&skill.content, "description").unwrap_or_default());
            let shell_style = frontmatter_value(&skill.content, "shell_style").unwrap_or_default();
            self.editor.style_field.set_text(&shell_style);
            self.editor.style_profile_mode =
                self.infer_style_profile_mode(&shell_style, &slug, &skill.name);
            // Style resource and MCP override fields are profile-derived editor helpers,
            // not per-skill frontmatter state.
            self.set_style_resource_fields_from_profile(ShellScriptStyle::parse(&shell_style));
            // Trigger examples are used only as a draft-generation helper and are not
            // restored from an existing skill document.
            self.editor.examples_field.set_text("");
            self.editor.body_field.set_text(&strip_frontmatter(&skill.content));
            self.editor.focus = Focus::Name;
            self.editor.edit_scroll_top = 0;
            self.editor.hovered_button = None;
            self.mode = Mode::Edit;
        }
    }

    pub(super) fn enter_editor(&mut self) {
        let selected = self.selected_list_index();
        if selected >= self.skills.len() {
            self.start_new_skill();
        } else {
            self.load_selected_into_form();
        }
    }
}
