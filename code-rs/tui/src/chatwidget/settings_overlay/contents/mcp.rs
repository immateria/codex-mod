use crate::bottom_pane::{McpSettingsView, mcp_settings_view::McpSettingsViewState};

pub(crate) struct McpSettingsContent {
    view: McpSettingsView,
}

impl McpSettingsContent {
    pub(crate) fn new(view: McpSettingsView) -> Self {
        Self { view }
    }

    pub(crate) fn snapshot_state(&self) -> McpSettingsViewState {
        self.view.snapshot_state()
    }

    pub(crate) fn restore_state(&mut self, state: &McpSettingsViewState) {
        self.view.restore_state(state);
    }
}

impl_settings_content!(McpSettingsContent);
