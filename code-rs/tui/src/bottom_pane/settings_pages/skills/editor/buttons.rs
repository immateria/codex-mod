use super::*;

use crate::bottom_pane::settings_ui::buttons::{
    standard_button_specs,
    SettingsButtonKind,
    StandardButtonSpec,
};

pub(super) fn action_button_specs_inner(
    view: &SkillsSettingsView,
) -> Vec<StandardButtonSpec<ActionButton>> {
    let focused = match view.editor.focus {
        Focus::Generate => Some(ActionButton::Generate),
        Focus::Save => Some(ActionButton::Save),
        Focus::Delete => Some(ActionButton::Delete),
        Focus::Cancel => Some(ActionButton::Cancel),
        _ => None,
    };

    standard_button_specs(
        &[
            (ActionButton::Generate, SettingsButtonKind::GenerateDraft),
            (ActionButton::Save, SettingsButtonKind::Save),
            (ActionButton::Delete, SettingsButtonKind::Delete),
            (ActionButton::Cancel, SettingsButtonKind::Cancel),
        ],
        focused,
        view.editor.hovered_button,
    )
}

