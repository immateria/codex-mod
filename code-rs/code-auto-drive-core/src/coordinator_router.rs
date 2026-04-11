//! Lightweight, keyword-based router for Auto Drive user prompts.
//!
//! This module offers a tiny heuristic bridge so the TUI can hand user
//! questions to the Auto Drive coordinator before they are sent to the CLI.
//! The real coordinator integration can later replace these heuristics
//! without touching call sites.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoordinatorRouterResponse {
    pub user_response: Option<String>,
    pub cli_command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoordinatorContext {
    pub active_agents: usize,
    pub recent_updates: Vec<String>,
}

impl CoordinatorContext {
    pub fn new(active_agents: usize, recent_updates: Vec<String>) -> Self {
        Self {
            active_agents,
            recent_updates,
        }
    }

    pub fn latest_update(&self) -> Option<&str> {
        self.recent_updates.last().map(String::as_str)
    }
}

pub fn route_user_message(msg: &str, ctx: &CoordinatorContext) -> CoordinatorRouterResponse {
    let normalized = msg.trim().to_ascii_lowercase();

    if normalized.is_empty() {
        return CoordinatorRouterResponse::default();
    }

    if contains_any(&normalized, &STATUS_PHRASES) {
        return status_response(ctx);
    }

    if contains_any(&normalized, &PLAN_PHRASES) {
        return plan_response();
    }

    if contains_any(&normalized, &STOP_PHRASES) {
        return stop_response();
    }

    CoordinatorRouterResponse::default()
}

const STATUS_PHRASES: [&str; 5] = [
    "what work has been done",
    "what have you done",
    "status update",
    "progress update",
    "current status",
];

const PLAN_PHRASES: [&str; 4] = [
    "start more agents",
    "spin up more agents",
    "launch more agents",
    "create a plan",
];

const STOP_PHRASES: [&str; 3] = [
    "stop all agents",
    "halt agents",
    "cancel the plan",
];

fn contains_any(message: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| message.contains(phrase))
}

fn status_response(ctx: &CoordinatorContext) -> CoordinatorRouterResponse {
    let mut summary = format!(
        "We currently have {} active agent{}",
        ctx.active_agents,
        if ctx.active_agents == 1 { "" } else { "s" }
    );

    if let Some(update) = ctx.latest_update() {
        summary.push_str("; most recently: ");
        summary.push_str(update);
    } else {
        summary.push('.');
    }

    CoordinatorRouterResponse {
        user_response: Some(summary),
        cli_command: None,
    }
}

fn plan_response() -> CoordinatorRouterResponse {
    CoordinatorRouterResponse {
        user_response: Some("Starting a fresh plan via the planner.".to_owned()),
        cli_command: Some("/plan".to_owned()),
    }
}

fn stop_response() -> CoordinatorRouterResponse {
    CoordinatorRouterResponse {
        user_response: Some("Stopping all active automation.".to_owned()),
        cli_command: Some("/stop".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_status_queries() {
        let ctx = CoordinatorContext::new(
            2,
            vec!["Finished lint pass on the CLI crate".to_string()],
        );

        let response = route_user_message(
            "Can you tell me what work has been done so far?",
            &ctx,
        );

        let user_response = response.user_response.expect("expected a status message");
        assert!(user_response.contains("2 active agents"));
        assert!(user_response.contains("Finished lint pass on the CLI crate"));
        assert!(response.cli_command.is_none());
    }

    #[test]
    fn routes_plan_requests() {
        let ctx = CoordinatorContext::default();
        let response = route_user_message("Please start more agents to handle this.", &ctx);

        assert_eq!(response.cli_command.as_deref(), Some("/plan"));
        assert!(response
            .user_response
            .as_deref()
            .unwrap_or_default()
            .contains("Starting a fresh plan"));
    }

    #[test]
    fn returns_default_for_unmatched_input() {
        let ctx = CoordinatorContext::default();
        let response = route_user_message("Hello there!", &ctx);

        assert!(response.user_response.is_none());
        assert!(response.cli_command.is_none());
    }
}
