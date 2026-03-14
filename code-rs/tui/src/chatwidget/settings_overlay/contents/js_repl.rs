use crate::bottom_pane::settings_pages::js_repl::JsReplSettingsView;

pub(crate) struct JsReplSettingsContent {
    view: JsReplSettingsView,
}

impl JsReplSettingsContent {
    pub(crate) fn new(view: JsReplSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_with_paste!(JsReplSettingsContent);
