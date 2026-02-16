use super::ToolCardSlot;
use crate::history_cell::{AgentRunCell, AgentStatusKind};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub(in super::super) struct AgentRunTracker {
    pub slot: ToolCardSlot,
    pub cell: AgentRunCell,
    pub batch_id: Option<String>,
    pub batch_label: Option<String>,
    pub agent_ids: HashSet<String>,
    pub models: HashSet<String>,
    pub task: Option<String>,
    pub context: Option<String>,
    pub has_custom_name: bool,
    pub call_ids: HashSet<String>,
    pub agent_started_at: HashMap<String, Instant>,
    pub agent_elapsed: HashMap<String, Duration>,
    pub agent_token_counts: HashMap<String, u64>,
    pub agent_announced_status: HashMap<String, AgentStatusKind>,
    pub anchor_inserted: bool,
    pub write_enabled: Option<bool>,
}

impl AgentRunTracker {
    pub(super) fn new(order_key: super::OrderKey) -> Self {
        Self {
            slot: ToolCardSlot::new(order_key),
            cell: AgentRunCell::new("(pending)".to_string()),
            batch_id: None,
            batch_label: None,
            agent_ids: HashSet::new(),
            models: HashSet::new(),
            task: None,
            context: None,
            has_custom_name: false,
            call_ids: HashSet::new(),
            agent_started_at: HashMap::new(),
            agent_elapsed: HashMap::new(),
            agent_token_counts: HashMap::new(),
            agent_announced_status: HashMap::new(),
            anchor_inserted: false,
            write_enabled: None,
        }
    }

    pub(super) fn merge_agent_ids<I>(&mut self, ids: I)
    where
        I: IntoIterator<Item = String>,
    {
        for id in ids {
            self.agent_ids.insert(id);
        }
    }

    pub(super) fn merge_models<I>(&mut self, models: I)
    where
        I: IntoIterator<Item = String>,
    {
        for model in models {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.models.insert(trimmed.to_string());
        }
    }

    pub(super) fn effective_label(&self) -> Option<String> {
        if let Some(label) = self.batch_label.as_ref() {
            let trimmed = label.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        self.cell.display_title()
    }

    pub(super) fn set_task(&mut self, task: Option<String>) {
        if let Some(value) = task {
            self.task = Some(value);
        }
        self.cell.set_task(self.task.clone());
    }

    pub(super) fn set_context(&mut self, context: Option<String>) {
        if let Some(value) = context {
            self.context = Some(value);
        }
        self.cell.set_context(self.context.clone());
    }

    pub(super) fn set_agent_name(&mut self, name: Option<String>, override_existing: bool) {
        if let Some(name) = name
            && (override_existing || !self.has_custom_name)
        {
            self.cell.set_agent_name(name);
            self.has_custom_name = true;
        }
    }

    pub(super) fn set_write_mode(&mut self, write_flag: Option<bool>) {
        if let Some(flag) = write_flag {
            self.write_enabled = Some(flag);
            self.cell.set_write_mode(Some(flag));
        }
    }

    pub(crate) fn overlay_display_label(&self) -> Option<String> {
        self.effective_label()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| self.batch_id.as_ref().map(std::string::ToString::to_string))
    }

    pub(crate) fn overlay_task(&self) -> Option<String> {
        self.task.as_ref().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    pub(crate) fn overlay_context(&self) -> Option<String> {
        self.context.as_ref().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }
}
