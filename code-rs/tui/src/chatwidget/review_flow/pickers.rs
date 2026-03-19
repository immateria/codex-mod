use super::super::*;
use code_protocol::protocol::ReviewTarget;

impl ChatWidget<'_> {
    pub(crate) fn show_review_commit_loading(&mut self) {
        let loading_item = SelectionItem {
            name: "Loading recent commits…".to_string(),
            description: None,
            is_current: true,
            actions: Vec::new(),
        };
        let view = ListSelectionView::new(
            " Select a commit ".to_string(),
            Some("Fetching recent commits from git".to_string()),
            Some("Esc cancel".to_string()),
            vec![loading_item],
            self.app_event_tx.clone(),
            6,
        );
        self.bottom_pane.show_list_selection(view);
    }

    pub(crate) fn present_review_commit_picker(&mut self, commits: Vec<CommitLogEntry>) {
        if commits.is_empty() {
            self.bottom_pane
                .flash_footer_notice("No recent commits found for review".to_string());
            self.request_redraw();
            return;
        }

        let auto_resolve = self.config.tui.review_auto_resolve;
        let mut items: Vec<SelectionItem> = Vec::with_capacity(commits.len());
        for entry in commits {
            let subject = entry.subject.trim().to_string();
            let sha = entry.sha.trim().to_string();
            if sha.is_empty() {
                continue;
            }
            let short_sha: String = sha.chars().take(7).collect();
            let title = if subject.is_empty() {
                short_sha.clone()
            } else {
                format!("{short_sha} — {subject}")
            };
            let prompt = if subject.is_empty() {
                format!(
                    "Review the code changes introduced by commit {sha}. Provide prioritized, actionable findings."
                )
            } else {
                format!(
                    "Review the code changes introduced by commit {sha} (\"{subject}\"). Provide prioritized, actionable findings."
                )
            };
            let hint = format!("commit {short_sha}");
            let preparation = format!("Preparing code review for commit {short_sha}");
            let prompt_closure = prompt.clone();
            let hint_closure = hint.clone();
            let prep_closure = preparation.clone();
            let target_closure = ReviewTarget::Commit {
                sha: sha.clone(),
                title: (!subject.is_empty()).then_some(subject.clone()),
            };
            let auto_flag = auto_resolve;
            items.push(SelectionItem {
                name: title,
                description: None,
                is_current: false,
                actions: vec![Box::new(move |tx: &crate::app_event_sender::AppEventSender| {
                    tx.send(crate::app_event::AppEvent::RunReviewWithScope {
                        target: target_closure.clone(),
                        prompt: prompt_closure.clone(),
                        hint: Some(hint_closure.clone()),
                        preparation_label: Some(prep_closure.clone()),
                        auto_resolve: auto_flag,
                    });
                })],
            });
        }

        if items.is_empty() {
            self.bottom_pane
                .flash_footer_notice("No recent commits found for review".to_string());
            self.request_redraw();
            return;
        }

        let view = ListSelectionView::new(
            " Select a commit ".to_string(),
            Some("Choose a commit to review".to_string()),
            Some("Enter select · Esc cancel".to_string()),
            items,
            self.app_event_tx.clone(),
            10,
        );

        self.bottom_pane.show_list_selection(view);
    }

    pub(crate) fn show_review_branch_loading(&mut self) {
        let loading_item = SelectionItem {
            name: "Loading local branches…".to_string(),
            description: None,
            is_current: true,
            actions: Vec::new(),
        };
        let view = ListSelectionView::new(
            " Select a base branch ".to_string(),
            Some("Fetching local branches".to_string()),
            Some("Esc cancel".to_string()),
            vec![loading_item],
            self.app_event_tx.clone(),
            6,
        );
        self.bottom_pane.show_list_selection(view);
    }

    pub(crate) fn present_review_branch_picker(
        &mut self,
        current_branch: Option<String>,
        branches: Vec<String>,
    ) {
        let current_trimmed = current_branch.as_ref().map(|s| s.trim().to_string());
        let mut items: Vec<SelectionItem> = Vec::new();
        let auto_resolve = self.config.tui.review_auto_resolve;
        for branch in branches {
            let branch_trimmed = branch.trim();
            if branch_trimmed.is_empty() {
                continue;
            }
            if current_trimmed
                .as_ref()
                .is_some_and(|current| current == branch_trimmed)
            {
                continue;
            }

            let title = if let Some(current) = current_trimmed.as_ref() {
                format!("{current} → {branch_trimmed}")
            } else {
                format!("Compare against {branch_trimmed}")
            };

            let prompt = if let Some(current) = current_trimmed.as_ref() {
                format!(
                    "Review the code changes between the current branch '{current}' and '{branch_trimmed}'. Identify the intent of the changes in '{current}' and ensure no obvious gaps remain. Find all geniune bugs or regressions which need to be addressed before merging. Return ALL issues which need to be addressed, not just the first one you find."
                )
            } else {
                format!(
                    "Review the code changes that would merge into '{branch_trimmed}'. Identify bugs, regressions, risky patterns, and missing tests before merge."
                )
            };
            let hint = format!("against {branch_trimmed}");
            let preparation = format!("Preparing code review against {branch_trimmed}");
            let prompt_closure = prompt.clone();
            let hint_closure = hint.clone();
            let prep_closure = preparation.clone();
            let target_closure =
                ReviewTarget::BaseBranch { branch: branch_trimmed.to_string() };
            let auto_flag = auto_resolve;
            items.push(SelectionItem {
                name: title,
                description: None,
                is_current: false,
                actions: vec![Box::new(move |tx: &crate::app_event_sender::AppEventSender| {
                    tx.send(crate::app_event::AppEvent::RunReviewWithScope {
                        target: target_closure.clone(),
                        prompt: prompt_closure.clone(),
                        hint: Some(hint_closure.clone()),
                        preparation_label: Some(prep_closure.clone()),
                        auto_resolve: auto_flag,
                    });
                })],
            });
        }

        if items.is_empty() {
            self.bottom_pane
                .flash_footer_notice("No alternative branches found for review".to_string());
            self.request_redraw();
            return;
        }

        let subtitle = current_trimmed
            .as_ref()
            .map(|current| format!("Current branch: {current}"));

        let view = ListSelectionView::new(
            " Select a base branch ".to_string(),
            subtitle,
            Some("Enter select · Esc cancel".to_string()),
            items,
            self.app_event_tx.clone(),
            10,
        );

        self.bottom_pane.show_list_selection(view);
    }
}
