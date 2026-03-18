use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::settings_pages::agents::{AgentEditorView, SubagentEditorView};

#[derive(Clone, Debug)]
pub(crate) struct AgentOverviewRow {
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) installed: bool,
    pub(crate) description: Option<String>,
}

#[derive(Default)]
pub(super) struct AgentsOverviewState {
    pub(super) rows: Vec<AgentOverviewRow>,
    pub(super) commands: Vec<String>,
    pub(super) selected: usize,
}

impl AgentsOverviewState {
    pub(super) fn total_rows(&self) -> usize {
        self.rows
            .len()
            .saturating_add(self.commands.len())
            .saturating_add(2)
    }

    pub(super) fn clamp_selection(&mut self) {
        let total = self.total_rows();
        if total == 0 {
            self.selected = 0;
        } else if self.selected >= total {
            self.selected = total - 1;
        }
    }
}

pub(super) enum AgentsPane {
    Overview(AgentsOverviewState),
    Subagent(Box<SubagentEditorView>),
    Agent(Box<AgentEditorView>),
}

pub(crate) struct AgentsSettingsContent {
    pub(super) pane: AgentsPane,
    pub(super) app_event_tx: AppEventSender,
}

impl AgentsSettingsContent {
    pub(crate) fn new_overview(
        rows: Vec<AgentOverviewRow>,
        commands: Vec<String>,
        selected: usize,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut overview = AgentsOverviewState {
            rows,
            commands,
            selected,
        };
        overview.clamp_selection();
        Self {
            pane: AgentsPane::Overview(overview),
            app_event_tx,
        }
    }

    pub(crate) fn set_overview(
        &mut self,
        rows: Vec<AgentOverviewRow>,
        commands: Vec<String>,
        selected: usize,
    ) {
        let mut overview = AgentsOverviewState {
            rows,
            commands,
            selected,
        };
        overview.clamp_selection();
        self.pane = AgentsPane::Overview(overview);
    }

    pub(crate) fn set_editor(&mut self, editor: SubagentEditorView) {
        self.pane = AgentsPane::Subagent(Box::new(editor));
    }

    pub(crate) fn set_overview_selection(&mut self, selected: usize) {
        if let AgentsPane::Overview(state) = &mut self.pane {
            state.selected = selected;
            state.clamp_selection();
        }
    }

    pub(crate) fn set_agent_editor(&mut self, editor: AgentEditorView) {
        self.pane = AgentsPane::Agent(Box::new(editor));
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub(crate) fn is_agent_editor_active(&self) -> bool {
        matches!(self.pane, AgentsPane::Agent(_))
    }
}

