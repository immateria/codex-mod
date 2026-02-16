use crate::history_cell::{AgentRunCell, AgentStatusKind};

#[derive(Default)]
pub(super) struct StatusSummary {
    any_failed: bool,
    any_cancelled: bool,
    any_running: bool,
    any_pending: bool,
    total: usize,
    completed: usize,
}

impl StatusSummary {
    pub(super) fn observe(&mut self, phase: AgentPhase) {
        self.total += 1;
        match phase {
            AgentPhase::Failed => {
                self.any_failed = true;
            }
            AgentPhase::Cancelled => {
                self.any_cancelled = true;
            }
            AgentPhase::Running => {
                self.any_running = true;
            }
            AgentPhase::Pending => {
                self.any_pending = true;
            }
            AgentPhase::Completed => {
                self.completed += 1;
            }
        }
    }

    pub(super) fn apply(self, cell: &mut AgentRunCell) {
        if self.any_failed {
            cell.mark_failed();
            return;
        }
        if self.any_cancelled {
            cell.set_status_label("Cancelled");
            cell.mark_completed();
            return;
        }
        if self.total > 0 && self.completed == self.total {
            cell.set_status_label("Completed");
            cell.mark_completed();
            return;
        }
        if self.any_running {
            cell.set_status_label("Running");
            return;
        }
        if self.any_pending {
            cell.set_status_label("Pending");
            return;
        }
        cell.set_status_label("Running");
    }
}

#[derive(Clone, Copy)]
pub(super) enum AgentPhase {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

pub(super) fn classify_status(status: &str, has_result: bool, has_error: bool) -> AgentPhase {
    if has_error {
        return AgentPhase::Failed;
    }
    if has_result {
        return AgentPhase::Completed;
    }
    let token = status
        .split_whitespace()
        .next()
        .unwrap_or(status)
        .to_ascii_lowercase();
    match token.as_str() {
        "failed" | "error" | "errored" => AgentPhase::Failed,
        "cancelled" | "canceled" => AgentPhase::Cancelled,
        "completed" | "complete" | "done" | "success" | "succeeded" => AgentPhase::Completed,
        "pending" | "queued" | "waiting" | "starting" => AgentPhase::Pending,
        _ => AgentPhase::Running,
    }
}

pub(super) fn phase_to_status_kind(phase: AgentPhase) -> AgentStatusKind {
    match phase {
        AgentPhase::Completed => AgentStatusKind::Completed,
        AgentPhase::Failed => AgentStatusKind::Failed,
        AgentPhase::Cancelled => AgentStatusKind::Cancelled,
        AgentPhase::Pending => AgentStatusKind::Pending,
        AgentPhase::Running => AgentStatusKind::Running,
    }
}
