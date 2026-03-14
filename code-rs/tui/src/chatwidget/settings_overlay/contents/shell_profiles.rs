use crate::bottom_pane::settings_pages::shell_profiles::ShellProfilesSettingsView;

pub(crate) struct ShellProfilesSettingsContent {
    view: ShellProfilesSettingsView,
}

impl ShellProfilesSettingsContent {
    pub(crate) fn new(view: ShellProfilesSettingsView) -> Self {
        Self { view }
    }

    pub(crate) fn set_current_shell(&mut self, current_shell: Option<&code_core::config_types::ShellConfig>) {
        self.view.set_current_shell(current_shell);
    }

    pub(crate) fn apply_generated_summary(
        &mut self,
        style: code_core::config_types::ShellScriptStyle,
        summary: String,
    ) {
        self.view.apply_generated_summary(style, summary);
    }

    pub(crate) fn apply_summary_generation_error(
        &mut self,
        style: code_core::config_types::ShellScriptStyle,
        error: String,
    ) {
        self.view.set_summary_generation_error(style, error);
    }
}

impl_settings_content_with_paste!(ShellProfilesSettingsContent);
