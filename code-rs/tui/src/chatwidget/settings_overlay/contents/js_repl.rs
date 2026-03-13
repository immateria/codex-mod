use crate::bottom_pane::JsReplSettingsView;

pub(crate) struct JsReplSettingsContent {
    view: JsReplSettingsView,
}

impl JsReplSettingsContent {
    pub(crate) fn new(view: JsReplSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_with_paste!(JsReplSettingsContent);
