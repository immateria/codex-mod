use std::sync::atomic::Ordering;

use super::{App, AppState};

impl App<'_> {
    pub(crate) const DEFAULT_TERMINAL_TITLE: &'static str = "Code";

    #[cfg(unix)]
    pub(crate) fn sigterm_triggered(&self) -> bool {
        self.sigterm_flag.load(Ordering::Relaxed)
    }

    #[cfg(unix)]
    pub(crate) fn clear_sigterm_guard(&mut self) {
        self.sigterm_guard.take();
    }

    pub(crate) fn token_usage(&self) -> code_core::protocol::TokenUsage {
        let usage = match &self.app_state {
            AppState::Chat { widget } => widget.token_usage().clone(),
            AppState::Onboarding { .. } => code_core::protocol::TokenUsage::default(),
        };
        // ensure background helpers stop before returning
        self.commit_anim_running.store(false, Ordering::Release);
        self.input_running.store(false, Ordering::Release);
        usage
    }

    pub(crate) fn session_id(&self) -> Option<uuid::Uuid> {
        match &self.app_state {
            AppState::Chat { widget } => widget.session_id(),
            AppState::Onboarding { .. } => None,
        }
    }

    /// Return a human-readable performance summary if timing was enabled.
    pub(crate) fn perf_summary(&self) -> Option<String> {
        if !self.timing_enabled {
            return None;
        }
        let mut out = String::new();
        if let AppState::Chat { widget } = &self.app_state {
            out.push_str(&widget.perf_summary());
            out.push_str("\n\n");
        }
        out.push_str(&self.timing.summarize());
        Some(out)
    }
}

