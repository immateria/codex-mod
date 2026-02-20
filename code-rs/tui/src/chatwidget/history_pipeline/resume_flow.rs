use super::*;
use crate::app_event::SessionPickerAction;

impl ChatWidget<'_> {
    pub(crate) fn show_resume_picker(&mut self) {
        self.show_session_picker(SessionPickerAction::Resume);
    }

    pub(crate) fn show_fork_picker(&mut self) {
        self.show_session_picker(SessionPickerAction::Fork);
    }

    fn show_session_picker(&mut self, action: SessionPickerAction) {
        if self.resume_picker_loading {
            self.bottom_pane
                .flash_footer_notice("Still loading past sessions…".to_string());
            return;
        }
        self.resume_picker_loading = true;
        self.bottom_pane.flash_footer_notice_for(
            "Loading past sessions…".to_string(),
            std::time::Duration::from_secs(30),
        );
        self.request_redraw();

        let cwd = self.config.cwd.clone();
        let code_home = self.config.code_home.clone();
        let exclude_path = self.config.experimental_resume.clone();
        let tx = self.app_event_tx.clone();

        tokio::spawn(async move {
            let fetch_cwd = cwd.clone();
            let fetch_code_home = code_home.clone();
            let fetch_exclude = exclude_path.clone();
            let result = tokio::task::spawn_blocking(move || {
                crate::resume::discovery::list_sessions_for_cwd(
                    &fetch_cwd,
                    &fetch_code_home,
                    fetch_exclude.as_deref(),
                )
            })
            .await;

            match result {
                Ok(candidates) => {
                    tx.send(AppEvent::SessionPickerLoaded {
                        action,
                        cwd,
                        candidates,
                    });
                }
                Err(err) => {
                    tx.send(AppEvent::SessionPickerLoadFailed {
                        action,
                        message: format!("Failed to load past sessions: {err}"),
                    });
                }
            }
        });
    }

    pub(in super::super) fn resume_rows_from_candidates(
        candidates: Vec<crate::resume::discovery::ResumeCandidate>,
    ) -> Vec<crate::bottom_pane::resume_selection_view::ResumeRow> {
        fn human_ago(ts: &str) -> String {
            use chrono::{DateTime, Local};
            if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
                let local_dt = dt.with_timezone(&Local);
                let now = Local::now();
                let delta = now.signed_duration_since(local_dt);
                let secs = delta.num_seconds().max(0);
                let mins = secs / 60;
                let hours = mins / 60;
                let days = hours / 24;
                if days >= 7 {
                    return local_dt.format("%Y-%m-%d %H:%M").to_string();
                }
                if days >= 1 {
                    return format!("{days}d ago");
                }
                if hours >= 1 {
                    return format!("{hours}h ago");
                }
                if mins >= 1 {
                    return format!("{mins}m ago");
                }
                return "just now".to_string();
            }
            ts.to_string()
        }

        candidates
            .into_iter()
            .map(|c| {
                let modified = human_ago(&c.modified_ts.unwrap_or_default());
                let created = human_ago(&c.created_ts.unwrap_or_default());
                let user_message_count = c.user_message_count;
                let user_msgs = format!("{user_message_count}");
                let branch = c.branch.unwrap_or_else(|| "-".to_string());
                let nickname = c
                    .nickname
                    .and_then(|name| {
                        let trimmed = name.trim();
                        (!trimmed.is_empty()).then(|| trimmed.to_string())
                    });
                let snippet = c.snippet.or(c.subtitle);
                let mut summary = match (nickname, snippet) {
                    (Some(name), Some(snippet)) => format!("{name} - {snippet}"),
                    (Some(name), None) => name,
                    (None, Some(snippet)) => snippet,
                    (None, None) => String::new(),
                };
                const SNIPPET_MAX: usize = 64;
                if summary.chars().count() > SNIPPET_MAX {
                    summary = summary.chars().take(SNIPPET_MAX).collect::<String>() + "…";
                }
                crate::bottom_pane::resume_selection_view::ResumeRow {
                    modified,
                    created,
                    user_msgs,
                    branch,
                    last_user_message: summary,
                    path: c.path,
                }
            })
            .collect()
    }

    pub(crate) fn present_session_picker(
        &mut self,
        action: SessionPickerAction,
        cwd: std::path::PathBuf,
        candidates: Vec<crate::resume::discovery::ResumeCandidate>,
    ) {
        self.resume_picker_loading = false;
        if candidates.is_empty() {
            self.bottom_pane
                .flash_footer_notice("No past sessions found for this folder".to_string());
            self.request_redraw();
            return;
        }
        let rows = Self::resume_rows_from_candidates(candidates);
        let count = rows.len();
        let title = match action {
            SessionPickerAction::Resume => format!("Resume Session — {}", cwd.display()),
            SessionPickerAction::Fork => format!("Fork Session — {}", cwd.display()),
        };
        self.bottom_pane
            .show_resume_selection(title, Some(String::new()), rows, action);
        self.bottom_pane
            .flash_footer_notice(format!("Loaded {count} past sessions."));
        self.request_redraw();
    }

    pub(crate) fn handle_session_picker_load_failed(&mut self, message: String) {
        self.resume_picker_loading = false;
        self.bottom_pane.flash_footer_notice(message);
        self.request_redraw();
    }
}
