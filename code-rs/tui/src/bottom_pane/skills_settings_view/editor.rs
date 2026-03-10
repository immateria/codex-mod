use super::*;
use crate::bottom_pane::settings_ui::buttons::{text_button_at, TextButton};

impl SkillsSettingsView {
    pub fn handle_key_event_direct(&mut self, key: KeyEvent) -> bool {
        if self.complete {
            return true;
        }
        let handled = match self.mode {
            Mode::List => match key {
                KeyEvent { code: KeyCode::Esc, .. } => {
                    self.complete = true;
                    true
                }
                KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                    self.enter_editor();
                    true
                }
                KeyEvent { code: KeyCode::Char('n'), modifiers, .. }
                    if modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.start_new_skill();
                    true
                }
                other => self.handle_list_key(other),
            },
            Mode::Edit => match key {
                KeyEvent { code: KeyCode::Esc, .. } => {
                    self.cancel_edit();
                    true
                }
                KeyEvent { code: KeyCode::Tab, .. } => {
                    self.cycle_focus(true);
                    true
                }
                KeyEvent { code: KeyCode::BackTab, .. } => {
                    self.cycle_focus(false);
                    true
                }
                KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
                    if matches!(
                        self.editor.focus,
                        Focus::StyleProfile
                            | Focus::Generate
                            | Focus::Save
                            | Focus::Delete
                            | Focus::Cancel
                    ) =>
                {
                    match self.editor.focus {
                        Focus::StyleProfile => self.cycle_style_profile_mode(true),
                        Focus::Generate => self.generate_draft(),
                        Focus::Save => self.save_current(),
                        Focus::Delete => self.delete_current(),
                        Focus::Cancel => self.cancel_edit(),
                        Focus::List
                        | Focus::Name
                        | Focus::Description
                        | Focus::Style
                        | Focus::StyleReferences
                        | Focus::StyleSkillRoots
                        | Focus::StyleMcpInclude
                        | Focus::StyleMcpExclude
                        | Focus::Examples
                        | Focus::Body => {}
                    }
                    true
                }
                KeyEvent { code: KeyCode::Char('n'), modifiers, .. }
                    if modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.start_new_skill();
                    true
                }
                KeyEvent { code: KeyCode::Char('g'), modifiers, .. }
                    if modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.generate_draft();
                    true
                }
                _ => match self.editor.focus {
                    Focus::Name => {
                        self.editor.name_field.handle_key(key);
                        true
                    }
                    Focus::Description => {
                        self.editor.description_field.handle_key(key);
                        true
                    }
                    Focus::Style => {
                        let previous_style =
                            ShellScriptStyle::parse(self.editor.style_field.text().trim());
                        self.editor.style_field.handle_key(key);
                        self.sync_style_profile_fields_if_needed(previous_style);
                        true
                    }
                    Focus::StyleProfile => match key.code {
                        KeyCode::Left => {
                            self.cycle_style_profile_mode(false);
                            true
                        }
                        KeyCode::Right | KeyCode::Char(' ') => {
                            self.cycle_style_profile_mode(true);
                            true
                        }
                        _ => false,
                    },
                    Focus::StyleReferences => {
                        let before = self.editor.style_references_field.text().to_string();
                        self.editor.style_references_field.handle_key(key);
                        if self.editor.style_references_field.text() != before {
                            self.editor.style_references_dirty = true;
                        }
                        true
                    }
                    Focus::StyleSkillRoots => {
                        let before = self.editor.style_skill_roots_field.text().to_string();
                        self.editor.style_skill_roots_field.handle_key(key);
                        if self.editor.style_skill_roots_field.text() != before {
                            self.editor.style_skill_roots_dirty = true;
                        }
                        true
                    }
                    Focus::StyleMcpInclude => {
                        let before = self.editor.style_mcp_include_field.text().to_string();
                        self.editor.style_mcp_include_field.handle_key(key);
                        if self.editor.style_mcp_include_field.text() != before {
                            self.editor.style_mcp_include_dirty = true;
                        }
                        true
                    }
                    Focus::StyleMcpExclude => {
                        let before = self.editor.style_mcp_exclude_field.text().to_string();
                        self.editor.style_mcp_exclude_field.handle_key(key);
                        if self.editor.style_mcp_exclude_field.text() != before {
                            self.editor.style_mcp_exclude_dirty = true;
                        }
                        true
                    }
                    Focus::Examples => {
                        self.editor.examples_field.handle_key(key);
                        true
                    }
                    Focus::Body => {
                        self.editor.body_field.handle_key(key);
                        true
                    }
                    Focus::Generate | Focus::Save | Focus::Delete | Focus::Cancel => false,
                    Focus::List => self.handle_list_key(key),
                },
            },
        };

        if handled && matches!(self.mode, Mode::Edit) {
            self.ensure_edit_focus_visible_from_last_render();
        }

        handled
    }

    pub fn handle_paste_direct(&mut self, text: String) -> bool {
        if self.complete {
            return false;
        }

        if !matches!(self.mode, Mode::Edit) {
            return false;
        }

        match self.editor.focus {
            Focus::Name => {
                self.editor.name_field.handle_paste(text);
                true
            }
            Focus::Description => {
                self.editor.description_field.handle_paste(text);
                true
            }
            Focus::Style => {
                let previous_style =
                    ShellScriptStyle::parse(self.editor.style_field.text().trim());
                self.editor.style_field.handle_paste(text);
                self.sync_style_profile_fields_if_needed(previous_style);
                true
            }
            Focus::StyleReferences => {
                let before = self.editor.style_references_field.text().to_string();
                self.editor.style_references_field.handle_paste(text);
                if self.editor.style_references_field.text() != before {
                    self.editor.style_references_dirty = true;
                }
                true
            }
            Focus::StyleSkillRoots => {
                let before = self.editor.style_skill_roots_field.text().to_string();
                self.editor.style_skill_roots_field.handle_paste(text);
                if self.editor.style_skill_roots_field.text() != before {
                    self.editor.style_skill_roots_dirty = true;
                }
                true
            }
            Focus::StyleMcpInclude => {
                let before = self.editor.style_mcp_include_field.text().to_string();
                self.editor.style_mcp_include_field.handle_paste(text);
                if self.editor.style_mcp_include_field.text() != before {
                    self.editor.style_mcp_include_dirty = true;
                }
                true
            }
            Focus::StyleMcpExclude => {
                let before = self.editor.style_mcp_exclude_field.text().to_string();
                self.editor.style_mcp_exclude_field.handle_paste(text);
                if self.editor.style_mcp_exclude_field.text() != before {
                    self.editor.style_mcp_exclude_dirty = true;
                }
                true
            }
            Focus::Examples => {
                self.editor.examples_field.handle_paste(text);
                true
            }
            Focus::Body => {
                self.editor.body_field.handle_paste(text);
                true
            }
            Focus::StyleProfile
            | Focus::Generate
            | Focus::Save
            | Focus::Delete
            | Focus::Cancel
            | Focus::List => false,
        }
    }

    fn scroll_edit_container_by(&mut self, delta: isize, max_scroll: usize) -> bool {
        if max_scroll == 0 || delta == 0 {
            return false;
        }
        let next = next_scroll_top_with_delta(self.editor.edit_scroll_top, max_scroll, delta);
        if next == self.editor.edit_scroll_top {
            false
        } else {
            self.editor.edit_scroll_top = next;
            true
        }
    }

    fn ensure_edit_focus_visible(&mut self, layout: &SkillsFormLayout) -> bool {
        if layout.max_scroll == 0 || layout.viewport_inner.height == 0 {
            return false;
        }
        let Some((focus_top, focus_h)) = layout.focus_bounds(self.editor.focus) else {
            return false;
        };
        if focus_h == 0 {
            return false;
        }

        let viewport_h = layout.viewport_inner.height as usize;
        let next = scroll_top_to_keep_visible(
            self.editor.edit_scroll_top,
            layout.max_scroll,
            viewport_h,
            focus_top,
            focus_h,
        );
        if next == self.editor.edit_scroll_top {
            false
        } else {
            self.editor.edit_scroll_top = next;
            true
        }
    }

    fn ensure_edit_focus_visible_from_last_render(&mut self) -> bool {
        let Some(area) = self.last_render_area.get() else {
            return false;
        };
        let Some(layout) = self.compute_form_layout(area) else {
            return false;
        };
        self.ensure_edit_focus_visible(&layout)
    }

    pub fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        if self.mode == Mode::List {
            return self.handle_list_mouse_event(mouse_event, area);
        }

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if !area.contains(Position {
                    x: mouse_event.column,
                    y: mouse_event.row,
                }) {
                    return false;
                }
                self.handle_edit_click(mouse_event, area)
            }
            MouseEventKind::Moved => {
                if !area.contains(Position {
                    x: mouse_event.column,
                    y: mouse_event.row,
                }) {
                    return self.set_hovered_button(None);
                }
                self.handle_edit_mouse_move(mouse_event, area)
            }
            MouseEventKind::ScrollUp => {
                if !area.contains(Position {
                    x: mouse_event.column,
                    y: mouse_event.row,
                }) {
                    return false;
                }
                self.handle_edit_scroll(mouse_event, area, false)
            }
            MouseEventKind::ScrollDown => {
                if !area.contains(Position {
                    x: mouse_event.column,
                    y: mouse_event.row,
                }) {
                    return false;
                }
                self.handle_edit_scroll(mouse_event, area, true)
            }
            _ => false,
        }
    }

    fn handle_edit_click(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let Some(layout) = self.compute_form_layout(area) else {
            return false;
        };
        self.set_hovered_button(None);

        if layout.name_field.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            self.editor.focus = Focus::Name;
            self.editor.name_field
                .handle_mouse_click(mouse_event.column, mouse_event.row, layout.name_field);
            return true;
        }
        if layout.description_field.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            self.editor.focus = Focus::Description;
            self.editor.description_field.handle_mouse_click(
                mouse_event.column,
                mouse_event.row,
                layout.description_field,
            );
            return true;
        }
        if layout.style_field.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            self.editor.focus = Focus::Style;
            self.editor.style_field
                .handle_mouse_click(mouse_event.column, mouse_event.row, layout.style_field);
            return true;
        }
        if layout.style_profile_row.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            self.editor.focus = Focus::StyleProfile;
            self.cycle_style_profile_mode(true);
            return true;
        }
        if layout.style_references_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            self.editor.focus = Focus::StyleReferences;
            if layout.style_references_inner.contains(Position {
                x: mouse_event.column,
                y: mouse_event.row,
            }) {
                self.editor.style_references_field.handle_mouse_click(
                    mouse_event.column,
                    mouse_event.row,
                    layout.style_references_inner,
                );
            }
            return true;
        }
        if layout.style_skill_roots_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            self.editor.focus = Focus::StyleSkillRoots;
            if layout.style_skill_roots_inner.contains(Position {
                x: mouse_event.column,
                y: mouse_event.row,
            }) {
                self.editor.style_skill_roots_field.handle_mouse_click(
                    mouse_event.column,
                    mouse_event.row,
                    layout.style_skill_roots_inner,
                );
            }
            return true;
        }
        if layout.style_mcp_include_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            self.editor.focus = Focus::StyleMcpInclude;
            if layout.style_mcp_include_inner.contains(Position {
                x: mouse_event.column,
                y: mouse_event.row,
            }) {
                self.editor.style_mcp_include_field.handle_mouse_click(
                    mouse_event.column,
                    mouse_event.row,
                    layout.style_mcp_include_inner,
                );
            }
            return true;
        }
        if layout.style_mcp_exclude_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            self.editor.focus = Focus::StyleMcpExclude;
            if layout.style_mcp_exclude_inner.contains(Position {
                x: mouse_event.column,
                y: mouse_event.row,
            }) {
                self.editor.style_mcp_exclude_field.handle_mouse_click(
                    mouse_event.column,
                    mouse_event.row,
                    layout.style_mcp_exclude_inner,
                );
            }
            return true;
        }
        if layout.examples_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            self.editor.focus = Focus::Examples;
            if layout.examples_inner.contains(Position {
                x: mouse_event.column,
                y: mouse_event.row,
            }) {
                self.editor.examples_field.handle_mouse_click(
                    mouse_event.column,
                    mouse_event.row,
                    layout.examples_inner,
                );
            }
            return true;
        }
        if layout.body_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            self.editor.focus = Focus::Body;
            if layout.body_inner.contains(Position {
                x: mouse_event.column,
                y: mouse_event.row,
            }) {
                self.editor.body_field.handle_mouse_click(
                    mouse_event.column,
                    mouse_event.row,
                    layout.body_inner,
                );
            }
            return true;
        }

        self.handle_edit_button_click(mouse_event, layout.buttons_row)
    }

    fn handle_edit_mouse_move(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let Some(layout) = self.compute_form_layout(area) else {
            return self.set_hovered_button(None);
        };
        self.set_hovered_button(self.edit_button_at(
            mouse_event.column,
            mouse_event.row,
            layout.buttons_row,
        ))
    }

    fn handle_edit_scroll(&mut self, mouse_event: MouseEvent, area: Rect, scroll_down: bool) -> bool {
        let Some(layout) = self.compute_form_layout(area) else {
            return false;
        };
        let container_delta = if scroll_down { 3 } else { -3 };

        if layout.style_references_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            let previous_focus = self.editor.focus;
            self.editor.focus = Focus::StyleReferences;
            let moved = self.editor.style_references_field.handle_mouse_scroll(scroll_down);
            let focus_changed = previous_focus != self.editor.focus;
            let scrolled = self.scroll_edit_container_by(container_delta, layout.max_scroll);
            return moved || focus_changed || scrolled;
        }
        if layout.style_skill_roots_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            let previous_focus = self.editor.focus;
            self.editor.focus = Focus::StyleSkillRoots;
            let moved = self.editor.style_skill_roots_field.handle_mouse_scroll(scroll_down);
            let focus_changed = previous_focus != self.editor.focus;
            let scrolled = self.scroll_edit_container_by(container_delta, layout.max_scroll);
            return moved || focus_changed || scrolled;
        }
        if layout.style_mcp_include_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            let previous_focus = self.editor.focus;
            self.editor.focus = Focus::StyleMcpInclude;
            let moved = self.editor.style_mcp_include_field.handle_mouse_scroll(scroll_down);
            let focus_changed = previous_focus != self.editor.focus;
            let scrolled = self.scroll_edit_container_by(container_delta, layout.max_scroll);
            return moved || focus_changed || scrolled;
        }
        if layout.style_mcp_exclude_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            let previous_focus = self.editor.focus;
            self.editor.focus = Focus::StyleMcpExclude;
            let moved = self.editor.style_mcp_exclude_field.handle_mouse_scroll(scroll_down);
            let focus_changed = previous_focus != self.editor.focus;
            let scrolled = self.scroll_edit_container_by(container_delta, layout.max_scroll);
            return moved || focus_changed || scrolled;
        }
        if layout.examples_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            let previous_focus = self.editor.focus;
            self.editor.focus = Focus::Examples;
            let moved = self.editor.examples_field.handle_mouse_scroll(scroll_down);
            let focus_changed = previous_focus != self.editor.focus;
            let scrolled = self.scroll_edit_container_by(container_delta, layout.max_scroll);
            return moved || focus_changed || scrolled;
        }
        if layout.body_outer.contains(Position {
            x: mouse_event.column,
            y: mouse_event.row,
        }) {
            let previous_focus = self.editor.focus;
            self.editor.focus = Focus::Body;
            let moved = self.editor.body_field.handle_mouse_scroll(scroll_down);
            let focus_changed = previous_focus != self.editor.focus;
            let scrolled = self.scroll_edit_container_by(container_delta, layout.max_scroll);
            return moved || focus_changed || scrolled;
        }

        self.scroll_edit_container_by(container_delta, layout.max_scroll)
    }

    fn handle_edit_button_click(&mut self, mouse_event: MouseEvent, row: Rect) -> bool {
        let Some(button) = self.edit_button_at(mouse_event.column, mouse_event.row, row) else {
            return false;
        };
        self.set_hovered_button(Some(button));
        self.editor.focus = button.focus();

        match button {
            ActionButton::Generate => self.generate_draft(),
            ActionButton::Save => self.save_current(),
            ActionButton::Delete => self.delete_current(),
            ActionButton::Cancel => self.cancel_edit(),
        }
        true
    }

    fn edit_button_at(&self, x: u16, y: u16, row: Rect) -> Option<ActionButton> {
        text_button_at(
            x,
            y,
            row,
            &[
                TextButton::new(ActionButton::Generate, GENERATE_BUTTON_LABEL, false, false, Style::new()),
                TextButton::new(ActionButton::Save, SAVE_BUTTON_LABEL, false, false, Style::new()),
                TextButton::new(ActionButton::Delete, DELETE_BUTTON_LABEL, false, false, Style::new()),
                TextButton::new(ActionButton::Cancel, CANCEL_BUTTON_LABEL, false, false, Style::new()),
            ],
        )
    }

    fn set_hovered_button(&mut self, hovered: Option<ActionButton>) -> bool {
        if self.editor.hovered_button == hovered {
            return false;
        }
        self.editor.hovered_button = hovered;
        true
    }

    fn cycle_focus(&mut self, forward: bool) {
        let order = [
            Focus::Name,
            Focus::Description,
            Focus::Style,
            Focus::StyleProfile,
            Focus::StyleReferences,
            Focus::StyleSkillRoots,
            Focus::StyleMcpInclude,
            Focus::StyleMcpExclude,
            Focus::Examples,
            Focus::Body,
            Focus::Generate,
            Focus::Save,
            Focus::Delete,
            Focus::Cancel,
        ];
        debug_assert!(
            self.editor.focus != Focus::List,
            "cycle_focus called with Focus::List while in edit mode"
        );
        let mut idx = order
            .iter()
            .position(|f| *f == self.editor.focus)
            .unwrap_or_else(|| if forward { 0 } else { order.len() - 1 });
        if forward {
            idx = (idx + 1) % order.len();
        } else {
            idx = idx.checked_sub(1).unwrap_or(order.len() - 1);
        }
        self.editor.focus = order[idx];
    }

    fn cycle_style_profile_mode(&mut self, forward: bool) {
        self.editor.style_profile_mode = if forward {
            self.editor.style_profile_mode.next()
        } else {
            self.editor.style_profile_mode.previous()
        };
    }

    fn cancel_edit(&mut self) {
        self.mode = Mode::List;
        self.editor.focus = Focus::List;
        self.editor.hovered_button = None;
        self.status = None;
    }

    fn sync_style_profile_fields_if_needed(&mut self, previous_style: Option<ShellScriptStyle>) {
        let next_style = ShellScriptStyle::parse(self.editor.style_field.text().trim());
        if next_style != previous_style
            && next_style.is_some()
            && !self.editor.style_profile_fields_dirty()
        {
            self.set_style_resource_fields_from_profile(next_style);
        }
    }

    pub(super) fn set_style_resource_fields_from_profile(&mut self, style: Option<ShellScriptStyle>) {
        let profile = style.and_then(|shell_style| self.shell_style_profiles.get(&shell_style));
        let (references, skill_roots, mcp_include, mcp_exclude) = match profile {
            Some(profile) => (
                profile.references.clone(),
                profile.skill_roots.clone(),
                profile.mcp_servers.include.clone(),
                profile.mcp_servers.exclude.clone(),
            ),
            None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        };

        self.editor.style_references_field
            .set_text(&format_path_list(&references));
        self.editor.style_skill_roots_field
            .set_text(&format_path_list(&skill_roots));
        self.editor.style_mcp_include_field
            .set_text(&format_string_list(&mcp_include));
        self.editor.style_mcp_exclude_field
            .set_text(&format_string_list(&mcp_exclude));
        self.editor.style_references_dirty = false;
        self.editor.style_skill_roots_dirty = false;
        self.editor.style_mcp_include_dirty = false;
        self.editor.style_mcp_exclude_dirty = false;
    }

    pub(super) fn infer_style_profile_mode(
        &self,
        shell_style: &str,
        slug: &str,
        display_name: &str,
    ) -> StyleProfileMode {
        let Some(style) = ShellScriptStyle::parse(shell_style) else {
            return StyleProfileMode::Inherit;
        };

        let Some(profile) = self.shell_style_profiles.get(&style) else {
            return StyleProfileMode::Inherit;
        };

        let identifiers = [slug, display_name];
        if profile_list_contains_any(&profile.disabled_skills, &identifiers) {
            return StyleProfileMode::Disable;
        }
        if profile_list_contains_any(&profile.skills, &identifiers) {
            return StyleProfileMode::Enable;
        }
        StyleProfileMode::Inherit
    }

    pub(super) fn parse_shell_style(&self, shell_style_raw: &str) -> Result<Option<ShellScriptStyle>, String> {
        let trimmed = shell_style_raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        ShellScriptStyle::parse(trimmed)
            .ok_or_else(|| "Invalid shell style. Use: posix-sh, bash-zsh-compatible, or zsh.".to_string())
            .map(Some)
    }
}
