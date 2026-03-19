use super::*;

mod draft;
mod save;
mod delete;
mod validation;
mod style_profiles;

impl SkillsSettingsView {
    pub(super) fn generate_draft(&mut self) {
        draft::generate_draft_inner(self);
    }

    pub(super) fn save_current(&mut self) {
        save::save_current_inner(self);
    }

    pub(super) fn delete_current(&mut self) {
        delete::delete_current_inner(self);
    }
}
