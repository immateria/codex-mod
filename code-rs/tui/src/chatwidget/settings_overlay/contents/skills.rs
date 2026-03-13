use crate::bottom_pane::skills_settings_view::SkillsSettingsView;

pub(crate) struct SkillsSettingsContent {
    view: SkillsSettingsView,
}

impl SkillsSettingsContent {
    pub(crate) fn new(view: SkillsSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_with_paste!(SkillsSettingsContent);
