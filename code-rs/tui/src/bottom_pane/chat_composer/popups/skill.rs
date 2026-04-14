use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use crate::bottom_pane::popup_consts::MAX_POPUP_ROWS;
use crate::components::scroll_state::ScrollState;
use crate::components::selection_popup_common::GenericDisplayRow;
use crate::components::selection_popup_common::render_rows;
use code_common::fuzzy_match::fuzzy_match;

pub(crate) struct SkillPopup {
    filter: String,
    skills: Vec<(String, String)>,
    state: ScrollState,
}

impl SkillPopup {
    pub(crate) fn new() -> Self {
        Self {
            filter: String::new(),
            skills: Vec::new(),
            state: ScrollState::new(),
        }
    }

    pub(crate) fn set_skills(&mut self, skills: Vec<(String, String)>) {
        self.skills = skills;
        self.state.clamp_selection(self.filtered_items().len());
    }

    pub(crate) fn on_text_change(&mut self, token: &str) {
        self.filter = token.to_owned();
        let count = self.filtered_items().len();
        self.state.clamp_selection(count);
        self.state.ensure_visible(count, MAX_POPUP_ROWS.min(count));
    }

    fn filtered_items(&self) -> Vec<(usize, Option<Vec<usize>>)> {
        if self.filter.is_empty() {
            return self
                .skills
                .iter()
                .enumerate()
                .map(|(i, _)| (i, None))
                .collect();
        }
        let mut out: Vec<(usize, Option<Vec<usize>>, i32)> = Vec::new();
        for (i, (name, _)) in self.skills.iter().enumerate() {
            if let Some((indices, score)) = fuzzy_match(name, &self.filter) {
                out.push((i, Some(indices), score));
            }
        }
        out.sort_by_key(|(_, _, score)| *score);
        out.into_iter().map(|(i, idx, _)| (i, idx)).collect()
    }

    pub(crate) fn match_count(&self) -> usize {
        self.filtered_items().len()
    }

    pub(crate) fn selected_item(&self) -> Option<String> {
        let items = self.filtered_items();
        self.state
            .selected_idx
            .and_then(|idx| items.get(idx))
            .and_then(|(skill_idx, _)| self.skills.get(*skill_idx))
            .map(|(name, _)| name.clone())
    }

    pub(crate) fn first_match(&self) -> Option<String> {
        self.filtered_items()
            .into_iter()
            .next()
            .and_then(|(skill_idx, _)| self.skills.get(skill_idx))
            .map(|(name, _)| name.clone())
    }

    pub(crate) fn move_up(&mut self) {
        let len = self.filtered_items().len();
        self.state.move_up_wrap_visible(len, MAX_POPUP_ROWS);
    }

    pub(crate) fn move_down(&mut self) {
        let len = self.filtered_items().len();
        self.state.move_down_wrap_visible(len, MAX_POPUP_ROWS);
    }

    pub(crate) fn select_visible_index(&mut self, visible_row: usize) -> bool {
        self.state
            .select_visible_row(self.filtered_items().len(), visible_row)
    }

    pub(crate) fn calculate_required_height(&self) -> u16 {
        ScrollState::popup_required_height(self.filtered_items().len(), MAX_POPUP_ROWS)
    }
}

impl WidgetRef for SkillPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let indented_area = area.inner(crate::ui_consts::NESTED_HPAD);
        let items = self.filtered_items();
        if items.is_empty() {
            return;
        }
        let rows: Vec<GenericDisplayRow> = items
            .into_iter()
            .map(|(skill_idx, indices)| {
                let (name, desc) = &self.skills[skill_idx];
                GenericDisplayRow {
                    name: format!("${name}"),
                    match_indices: indices
                        .map(|v| v.into_iter().map(|i| i + 1).collect()),
                    is_current: false,
                    description: Some(desc.clone()),
                    name_color: Some(crate::colors::primary()),
                }
            })
            .collect();
        render_rows(indented_area, buf, &rows, &self.state, MAX_POPUP_ROWS, false);
    }
}
