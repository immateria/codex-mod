use crate::bottom_pane::ExecLimitsSettingsView;

pub(crate) struct ExecLimitsSettingsContent {
    view: ExecLimitsSettingsView,
}

impl ExecLimitsSettingsContent {
    pub(crate) fn new(view: ExecLimitsSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_with_paste!(ExecLimitsSettingsContent);
