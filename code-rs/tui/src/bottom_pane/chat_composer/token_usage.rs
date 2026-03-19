use super::*;

impl ChatComposer {
    /// Update the cached *context-left* percentage and refresh the placeholder
    /// text. The UI relies on the placeholder to convey the remaining
    /// context when the composer is empty.
    pub(crate) fn set_token_usage(
        &mut self,
        last_token_usage: TokenUsage,
        model_context_window: Option<u64>,
        context_mode: Option<ContextMode>,
    ) {
        let initial_prompt_tokens = self
            .token_usage_info
            .as_ref()
            .map(|info| info.initial_prompt_tokens)
            .unwrap_or_else(|| last_token_usage.cached_input_tokens);

        self.token_usage_info = Some(TokenUsageInfo {
            last_token_usage,
            model_context_window,
            context_mode,
            auto_context_phase: self
                .token_usage_info
                .as_ref()
                .and_then(|info| info.auto_context_phase),
            initial_prompt_tokens,
        });
    }

    pub(crate) fn set_auto_context_phase(&mut self, phase: Option<AutoContextPhase>) {
        if let Some(info) = self.token_usage_info.as_mut() {
            info.auto_context_phase = phase;
        }
    }

    /// Record the history metadata advertised by `SessionConfiguredEvent` so
    /// that the composer can navigate cross-session history.
    pub(crate) fn set_history_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.history.set_metadata(log_id, entry_count);
    }
}

