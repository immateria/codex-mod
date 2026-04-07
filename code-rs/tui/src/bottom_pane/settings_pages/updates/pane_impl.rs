use super::UpdateSettingsView;

impl_settings_pane!(UpdateSettingsView, handle_key_event_direct,
    height = {
        let rows = Self::HEADER_LINE_COUNT
            .saturating_add(Self::ROW_COUNT)
            .saturating_add(Self::FOOTER_LINE_COUNT)
            .saturating_add(2);
        u16::try_from(rows).unwrap_or(u16::MAX)
    },
    complete_field = is_complete
);
