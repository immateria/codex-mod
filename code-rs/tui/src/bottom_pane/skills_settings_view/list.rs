use super::*;

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

    fn list_selection_at(&self, list_area: Rect, x: u16, y: u16) -> Option<usize> {
        if !list_area.contains(Position { x, y }) {
            return None;
        }

        debug_assert!(y >= list_area.y, "y={y} is above list_area.y={}", list_area.y);
        let row = (y - list_area.y) as usize;
        if row <= self.skills.len() {
            Some(row)
        } else {
            None
        }
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
        let mut selected = self.selected;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.skills.len().saturating_add(1),
            |x, y| self.list_selection_at(list_area, x, y),
            SelectableListMouseConfig {
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );
        self.selected = selected;
        if matches!(result, SelectableListMouseResult::Activated) {
            if self.selected < self.skills.len() {
                self.enter_editor();
            } else {
                self.start_new_skill();
            }
        }
        result.handled()
    }

    pub(super) fn handle_list_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                true
            }
            KeyCode::Down => {
                let max = self.skills.len();
                if self.selected < max {
                    self.selected += 1;
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn start_new_skill(&mut self) {
        self.selected = self.skills.len();
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
        if let Some(skill) = self.skills.get(self.selected).cloned() {
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
        if self.selected >= self.skills.len() {
            self.start_new_skill();
        } else {
            self.load_selected_into_form();
        }
    }
}
