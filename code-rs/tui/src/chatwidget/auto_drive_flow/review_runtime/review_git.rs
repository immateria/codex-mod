use super::super::*;

impl ChatWidget<'_> {
    #[cfg(test)]
    pub(super) fn auto_prepare_commit_scope(&mut self) -> AutoReviewOutcome {
        let Some(state) = self.auto_turn_review_state.take() else {
            return AutoReviewOutcome::Workspace;
        };

        let Some(base_commit) = state.base_commit else {
            return AutoReviewOutcome::Workspace;
        };

        let final_commit = match self.capture_auto_turn_commit("auto turn change snapshot", Some(&base_commit)) {
            Ok(commit) => commit,
            Err(err) => {
                tracing::warn!("failed to capture auto turn change snapshot: {err}");
                return AutoReviewOutcome::Workspace;
            }
        };

        let diff_paths = match self.git_diff_name_only_between(base_commit.id(), final_commit.id()) {
            Ok(paths) => paths,
            Err(err) => {
                tracing::warn!("failed to diff auto turn snapshots: {err}");
                return AutoReviewOutcome::Workspace;
            }
        };

        if diff_paths.is_empty() {
            self.push_background_tail("Auto review skipped: no file changes detected this turn.".to_string());
            return AutoReviewOutcome::Skip;
        }

        AutoReviewOutcome::Commit(AutoReviewCommitScope {
            commit: final_commit.id().to_string(),
            file_count: diff_paths.len(),
        })
    }

    pub(crate) fn prepare_auto_turn_review_state(&mut self) {
        if !self.auto_state.is_active() || !self.auto_state.review_enabled {
            self.auto_turn_review_state = None;
            return;
        }

        let read_only = self
            .pending_auto_turn_config
            .as_ref()
            .map(|cfg| cfg.read_only)
            .unwrap_or(false);

        if read_only {
            self.auto_turn_review_state = None;
            return;
        }

        let existing_base = self
            .auto_turn_review_state
            .as_ref()
            .and_then(|state| state.base_commit.as_ref());

        if existing_base.is_some() {
            return;
        }

        #[cfg(test)]
        {
            if CAPTURE_AUTO_TURN_COMMIT_STUB.lock().unwrap().is_some() {
                return;
            }
        }

        match self.capture_auto_turn_commit("auto turn base snapshot", None) {
            Ok(commit) => {
                self.auto_turn_review_state = Some(AutoTurnReviewState {
                    base_commit: Some(commit),
                });
            }
            Err(err) => {
                tracing::warn!("failed to capture auto turn base snapshot: {err}");
                self.auto_turn_review_state = None;
            }
        }
    }

    pub(crate) fn capture_auto_turn_commit(
        &self,
        message: &'static str,
        parent: Option<&GhostCommit>,
    ) -> Result<GhostCommit, GitToolingError> {
        #[cfg(test)]
        if let Some(stub) = CAPTURE_AUTO_TURN_COMMIT_STUB.lock().unwrap().as_ref() {
            let parent_id = parent.map(|commit| commit.id().to_string());
            return stub(message, parent_id);
        }
        let mut options = CreateGhostCommitOptions::new(self.config.cwd.as_path()).message(message);
        if let Some(parent_commit) = parent {
            options = options.parent(parent_commit.id());
        }
        let hook_repo_follow = self.config.cwd.clone();
        let hook = move || bump_snapshot_epoch_for(&hook_repo_follow);
        let result = create_ghost_commit(&options.post_commit_hook(&hook));
        if result.is_ok() {
            bump_snapshot_epoch_for(&self.config.cwd);
        }
        result
    }

    pub(crate) fn capture_auto_review_baseline_for_path(
        repo_path: PathBuf,
    ) -> Result<GhostCommit, GitToolingError> {
        #[cfg(test)]
        if let Some(stub) = CAPTURE_AUTO_TURN_COMMIT_STUB.lock().unwrap().as_ref() {
            return stub("auto review baseline snapshot", None);
        }
        let hook_repo = repo_path.clone();
        let options =
            CreateGhostCommitOptions::new(repo_path.as_path()).message("auto review baseline snapshot");
        let hook = move || bump_snapshot_epoch_for(&hook_repo);
        let result = create_ghost_commit(&options.post_commit_hook(&hook));
        if result.is_ok() {
            bump_snapshot_epoch_for(&repo_path);
        }
        result
    }

    pub(crate) fn spawn_auto_review_baseline_capture(&mut self) {
        let turn_sequence = self.turn_sequence;
        let repo_path = self.config.cwd.clone();
        let app_event_tx = self.app_event_tx.clone();

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let result = tokio::task::spawn_blocking(move || {
                    ChatWidget::capture_auto_review_baseline_for_path(repo_path)
                })
                .await
                .unwrap_or_else(|err| {
                    Err(GitToolingError::Io(io::Error::other(
                        format!("auto review baseline task failed: {err}"),
                    )))
                });
                app_event_tx.send(AppEvent::AutoReviewBaselineCaptured {
                    turn_sequence,
                    result,
                });
            });
        } else {
            std::thread::spawn(move || {
                let result = ChatWidget::capture_auto_review_baseline_for_path(repo_path);
                app_event_tx.send(AppEvent::AutoReviewBaselineCaptured {
                    turn_sequence,
                    result,
                });
            });
        }
    }

    pub(crate) fn handle_auto_review_baseline_captured(
        &mut self,
        turn_sequence: u64,
        result: Result<GhostCommit, GitToolingError>,
    ) {
        if turn_sequence != self.turn_sequence {
            tracing::debug!(
                "ignored auto review baseline for stale turn_sequence={turn_sequence}"
            );
            return;
        }
        if self.auto_review_baseline.is_some() {
            tracing::debug!("auto review baseline already set; skipping update");
            return;
        }
        match result {
            Ok(commit) => {
                self.auto_review_baseline = Some(commit);
            }
            Err(err) => {
                tracing::warn!("failed to capture auto review baseline: {err}");
            }
        }
    }

    #[cfg(test)]
    fn git_diff_name_only_between(
        &self,
        base_commit: &str,
        head_commit: &str,
    ) -> Result<Vec<String>, String> {
        #[cfg(test)]
        if let Some(stub) = GIT_DIFF_NAME_ONLY_BETWEEN_STUB.lock().unwrap().as_ref() {
            return stub(base_commit.to_string(), head_commit.to_string());
        }
        self.run_git_command(
            ["diff", "--name-only", base_commit, head_commit],
            |stdout| {
                let changes = stdout
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect();
                Ok(changes)
            },
        )
    }

}
