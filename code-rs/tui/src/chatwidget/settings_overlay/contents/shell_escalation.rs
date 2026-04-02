use crate::bottom_pane::settings_pages::shell_escalation::ShellEscalationSettingsView;

pub(crate) struct ShellEscalationSettingsContent {
    view: ShellEscalationSettingsView,
}

impl ShellEscalationSettingsContent {
    pub(crate) fn new(view: ShellEscalationSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_with_paste!(ShellEscalationSettingsContent);

