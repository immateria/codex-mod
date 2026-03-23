impl ChatWidget<'_> {
    pub(in super::super) fn try_coordinator_route(
        &mut self,
        original_text: &str,
    ) -> Option<CoordinatorRouterResponse> {
        let trimmed = original_text.trim();
        if trimmed.is_empty() {
            return None;
        }
        if !self.auto_state.is_active() {
            return None;
        }
        if self.auto_state.is_paused_manual()
            && self.auto_state.should_bypass_coordinator_next_submit()
        {
            return None;
        }
        if !self.config.auto_drive.coordinator_routing {
            return None;
        }
        if trimmed.starts_with('/') {
            return None;
        }

        let mut updates = Vec::new();
        if let Some(summary) = self.auto_state.last_decision_summary.clone()
            && !summary.trim().is_empty() {
                updates.push(summary);
            }
        if let Some(current) = self.auto_state.current_summary.clone()
            && !current.trim().is_empty() && updates.iter().all(|existing| existing != &current) {
                updates.push(current);
            }

        let context = CoordinatorContext::new(self.auto_state.pending_agent_actions.len(), updates);
        let response = route_user_message(trimmed, &context);
        if response.user_response.is_some() || response.cli_command.is_some() {
            Some(response)
        } else {
            None
        }
    }
}
