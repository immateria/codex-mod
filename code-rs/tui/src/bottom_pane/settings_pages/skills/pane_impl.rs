use super::{SkillsSettingsView, model::SKILLS_SETTINGS_VIEW_HEIGHT};

impl_settings_pane!(SkillsSettingsView, handle_key_event_direct,
    height = { SKILLS_SETTINGS_VIEW_HEIGHT },
    complete_fn = is_complete
);
