use super::PlanningSettingsView;

impl_settings_pane!(PlanningSettingsView, handle_key_event_direct,
    height = { 6 },
    complete_field = is_complete
);
