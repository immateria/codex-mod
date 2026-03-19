use super::super::*;
use code_protocol::protocol::ReviewTarget;

impl ChatWidget<'_> {
    pub(in crate::chatwidget) fn auto_resolve_enabled(&self) -> bool {
        self.auto_resolve_state.is_some()
    }

    pub(in crate::chatwidget) fn configured_auto_resolve_re_reviews(&self) -> u32 {
        self.config
            .auto_drive
            .auto_resolve_review_attempts
            .get()
    }

    pub(in crate::chatwidget) fn auto_resolve_clear(&mut self) {
        self.auto_resolve_state = None;
        self.maybe_resume_auto_after_review();
    }

    pub(in crate::chatwidget) fn auto_resolve_notice<S: Into<String>>(&mut self, message: S) {
        self.push_background_tail(message.into());
        self.request_redraw();
    }

    pub(in crate::chatwidget) fn auto_resolve_commit_sha(&self) -> Option<String> {
        self.auto_resolve_state
            .as_ref()
            .and_then(|state| match &state.target {
                ReviewTarget::Commit { sha, .. } => Some(sha.clone()),
                _ => None,
            })
    }

    pub(in crate::chatwidget) fn auto_resolve_should_block_auto_resume(&self) -> bool {
        match self.auto_resolve_state.as_ref().map(|state| &state.phase) {
            Some(AutoResolvePhase::PendingFix { .. })
            | Some(AutoResolvePhase::AwaitingFix { .. })
            | Some(AutoResolvePhase::AwaitingJudge { .. }) => true,
            Some(AutoResolvePhase::WaitingForReview) => self.is_review_flow_active(),
            None => false,
        }
    }

    pub(in crate::chatwidget) fn maybe_resume_auto_after_review(&mut self) {
        if !self.auto_state.is_active() || !self.auto_state.awaiting_review() {
            return;
        }
        if self.is_review_flow_active() || self.auto_resolve_should_block_auto_resume() {
            return;
        }
        self.auto_state.on_complete_review();
        if !self.auto_state.should_bypass_coordinator_next_submit() {
            self.auto_send_conversation();
        }
        self.request_redraw();
    }

    pub(in crate::chatwidget) fn auto_resolve_format_findings(review: &ReviewOutputEvent) -> String {
        let mut sections: Vec<String> = Vec::new();
        if !review.findings.is_empty() {
            sections.push(format_review_findings_block(&review.findings, None));
        }
        let explanation = review.overall_explanation.trim();
        if !explanation.is_empty() {
            sections.push(explanation.to_string());
        }
        sections
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub(in crate::chatwidget) fn auto_resolve_handle_review_enter(&mut self) {
        if let Some(state) = self.auto_resolve_state.as_mut() {
            state.phase = AutoResolvePhase::WaitingForReview;
            state.last_review = None;
            state.last_fix_message = None;
        }
    }

    pub(in crate::chatwidget) fn auto_resolve_handle_review_exit(&mut self, review_output: Option<ReviewOutputEvent>) {
        if self.auto_resolve_state.is_none() {
            return;
        }

        let notice: Option<String>;
        let mut should_clear = false;
        {
            let Some(state) = self.auto_resolve_state.as_mut() else {
                return;
            };
            match review_output {
                Some(ref output) => {
                    state.attempt = state.attempt.saturating_add(1);
                    state.last_review = Some(output.clone());
                    state.last_fix_message = None;

                    if output.findings.is_empty() {
                        notice = Some("Auto-resolve: review reported no actionable findings. Exiting.".to_string());
                        should_clear = true;
                    } else if state.max_attempts > 0 && state.attempt > state.max_attempts {
                        let limit = state.max_attempts;
                        notice = Some(match limit {
                            0 => "Auto-resolve: attempt limit is set to 0, so automation stopped after the initial review.".to_string(),
                            1 => "Auto-resolve: reached the review attempt limit (1 allowed review). Handing control back to you.".to_string(),
                            _ => format!(
                                "Auto-resolve: reached the review attempt limit ({limit} allowed reviews). Handing control back to you."
                            ),
                        });
                        should_clear = true;
                    } else {
                        state.phase = AutoResolvePhase::PendingFix {
                            review: output.clone(),
                        };
                        notice = Some("Auto-resolve: review found issues. Preparing follow-up fix request.".to_string());
                    }
                }
                None => {
                    notice = Some(
                        "Auto-resolve: review ended without findings. Please inspect manually.".to_string(),
                    );
                    should_clear = true;
                }
            }
        }

        if should_clear {
            self.auto_resolve_clear();
        }
        if let Some(message) = notice {
            self.auto_resolve_notice(message);
        }
    }

    pub(in crate::chatwidget) fn auto_resolve_on_task_complete(&mut self, last_agent_message: Option<String>) {
        let Some(state_snapshot) = self.auto_resolve_state.clone() else {
            return;
        };

        match state_snapshot.phase {
            AutoResolvePhase::PendingFix { review } => {
                if let Some(state) = self.auto_resolve_state.as_mut() {
                    state.phase = AutoResolvePhase::AwaitingFix {
                        review: review.clone(),
                    };
                }
                self.dispatch_auto_fix(&review);
            }
            AutoResolvePhase::AwaitingFix { review } => {
                if let Some(state) = self.auto_resolve_state.as_mut() {
                    state.last_fix_message = last_agent_message.clone();
                    state.phase = AutoResolvePhase::AwaitingJudge {
                        review: review.clone(),
                    };
                }
                self.dispatch_auto_judge(&review, last_agent_message);
            }
            AutoResolvePhase::AwaitingJudge { review } => {
                let message = last_agent_message.unwrap_or_default();
                self.auto_resolve_process_judge(review, message);
            }
            AutoResolvePhase::WaitingForReview => {}
        }
    }

    pub(in crate::chatwidget) fn dispatch_auto_fix(&mut self, review: &ReviewOutputEvent) {
        let summary = Self::auto_resolve_format_findings(review);
        let mut preface = String::from(
            "You are continuing an automated /review resolution loop. Review the listed findings and determine whether they represent real issues introduced by our changes. If they are, apply the necessary fixes and resolve any similar issues you can identify before responding."
        );
        if !summary.is_empty() {
            preface.push_str("\n\nFindings:\n");
            preface.push_str(&summary);
        }
        if let Some(commit) = self.auto_resolve_commit_sha() {
                let short_sha: String = commit.chars().take(7).collect();
                preface.push_str("\n\nCommit under review: ");
                preface.push_str(&commit);
                preface.push_str(" (short SHA ");
                preface.push_str(&short_sha);
                preface.push_str(
                    "). If you make changes to address these findings, amend this commit before responding so the review target reflects your fixes.",
                );
            }

        // Pass the full structured output so the resolving agent sees file paths and line ranges.
        if let Ok(raw_json) = serde_json::to_string_pretty(review) {
            preface.push_str("\n\nFull review JSON (includes file paths and line ranges):\n");
            preface.push_str(&raw_json);
        }

        if let Some(context) = self.turn_context_block() {
            preface.push_str("\n\n");
            preface.push_str(&context);
        }

        self.auto_resolve_notice("Auto-resolve: asking the agent to verify and address the review findings.");
        self.submit_hidden_text_message_with_preface(
            "Is this a real issue introduced by our changes? If so, please fix and resolve all similar issues.".to_string(),
            preface,
        );
    }

    pub(in crate::chatwidget) fn dispatch_auto_judge(&mut self, review: &ReviewOutputEvent, fix_message: Option<String>) {
        let summary = Self::auto_resolve_format_findings(review);
        let mut preface = String::from(
            "You are evaluating whether the latest fixes resolved the findings from `/review`. Respond with a strict JSON object containing `status` and optional `rationale`. Valid `status` values: `review_again`, `no_issue`, `continue_fix`. Do not include any additional text before or after the JSON."
        );
        if !summary.is_empty() {
            preface.push_str("\n\nOriginal findings:\n");
            preface.push_str(&summary);
        }
        if let Some(fix) = fix_message.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            preface.push_str("\n\nLatest agent response:\n");
            preface.push_str(fix);
        }
        preface.push_str("\n\nReturn JSON: {\"status\": \"...\", \"rationale\": \"optional explanation\"}.");
        if let Some(commit) = self.auto_resolve_commit_sha() {
                let short_sha: String = commit.chars().take(7).collect();
                preface.push_str("\n\nCommit under review: ");
                preface.push_str(&commit);
                preface.push_str(" (short SHA ");
                preface.push_str(&short_sha);
                preface.push_str(
                    "). Confirm that any fixes have been committed (amend the commit if necessary) before returning `no_issue`.",
                );
            }

        if let Some(context) = self.turn_context_block() {
            preface.push_str("\n\n");
            preface.push_str(&context);
        }

        self.auto_resolve_notice("Auto-resolve: requesting status JSON from the agent.");
        self.submit_hidden_text_message_with_preface("Auto-resolve status check".to_string(), preface);
    }

    pub(in crate::chatwidget) fn dispatch_auto_continue(&mut self, review: &ReviewOutputEvent) {
        let summary = Self::auto_resolve_format_findings(review);
        let mut preface = String::from(
            "The previous status check indicated more work is required on the review findings. Continue addressing the remaining issues before responding."
        );
        if !summary.is_empty() {
            preface.push_str("\n\nOutstanding findings:\n");
            preface.push_str(&summary);
        }
        if let Some(context) = self.turn_context_block() {
            preface.push_str("\n\n");
            preface.push_str(&context);
        }
        self.auto_resolve_notice("Auto-resolve: asking the agent to continue working on the findings.");
        self.submit_hidden_text_message_with_preface("Please continue".to_string(), preface);
    }

    pub(in crate::chatwidget) fn restart_auto_resolve_review(&mut self) {
        let Some(state_snapshot) = self.auto_resolve_state.clone() else {
            return;
        };
        let next_attempt = state_snapshot.attempt.saturating_add(1);
        let re_reviews_allowed = state_snapshot.max_attempts;
        let total_allowed = re_reviews_allowed.saturating_add(1);
        let attempt_label = if re_reviews_allowed == 0 {
            "attempt limit reached".to_string()
        } else {
            format!("attempt {next_attempt} of {total_allowed}")
        };
        let prep_label = format!("Preparing follow-up code review ({attempt_label})");
        let mut base_prompt = state_snapshot.prompt.trim_end().to_string();
        if let Some(idx) = base_prompt.find(AUTO_RESOLVE_REVIEW_FOLLOWUP) {
            base_prompt = base_prompt[..idx].trim_end().to_string();
        }

        let mut next_hint = state_snapshot.hint.clone();
        let mut next_target = state_snapshot.target.clone();

        if matches!(next_target, ReviewTarget::Commit { .. })
            && let Some(new_commit) = self.current_head_commit_sha()
        {
            let short_sha: String = new_commit.chars().take(7).collect();
            let subject = self.commit_subject_for(&new_commit);
            base_prompt = match subject.as_deref() {
                Some(subject) => format!(
                    "Review the code changes introduced by commit {new_commit} (\"{subject}\"). Provide prioritized, actionable findings."
                ),
                None => format!(
                    "Review the code changes introduced by commit {new_commit}. Provide prioritized, actionable findings."
                ),
            };
            next_hint = format!("commit {short_sha}");
            next_target = ReviewTarget::Commit {
                sha: new_commit,
                title: subject,
            };
        }

        let mut continued_prompt = base_prompt.clone();
        if let Some(last_review) = state_snapshot.last_review.as_ref() {
            let recap = Self::auto_resolve_format_findings(last_review);
            if !recap.is_empty() {
                continued_prompt.push_str("\n\nPreviously reported findings to re-validate:\n");
                continued_prompt.push_str(&recap);
            }
        }
        if let ReviewTarget::Commit { sha, .. } = &state_snapshot.target
            && let Some(true) = self.worktree_has_uncommitted_changes()
        {
            continued_prompt.push_str(
                "\n\nNote: there are uncommitted changes in the working tree since commit ",
            );
            continued_prompt.push_str(sha);
            continued_prompt.push_str(
                ". Ensure the review covers the updated workspace rather than only the original commit snapshot.",
            );
        }
        continued_prompt.push_str("\n\n");
        continued_prompt.push_str(AUTO_RESOLVE_REVIEW_FOLLOWUP);
        let hint = (!next_hint.trim().is_empty()).then(|| next_hint.clone());
        self.begin_review(next_target.clone(), continued_prompt, hint, Some(prep_label));
        if let Some(state) = self.auto_resolve_state.as_mut() {
            state.phase = AutoResolvePhase::WaitingForReview;
            state.target = next_target;
            state.prompt = base_prompt;
            state.hint = next_hint;
            state.last_review = None;
            state.last_fix_message = None;
        }
    }

    pub(in crate::chatwidget) fn auto_resolve_process_judge(&mut self, review: ReviewOutputEvent, message: String) {
        let trimmed = message.trim();
        let Some(decision) = Self::auto_resolve_parse_decision(trimmed) else {
            self.auto_resolve_notice("Auto-resolve: expected JSON status but received something else. Stopping automation.");
            self.auto_resolve_clear();
            return;
        };

        let status = decision.status.to_ascii_lowercase();
        let rationale = decision.rationale.unwrap_or_default();

        match status.as_str() {
            "no_issue" => {
                let rationale_text = rationale.trim();
                let attempt_limit_reached = self
                    .auto_resolve_state
                    .as_ref()
                    .is_some_and(|state| {
                        let allowed = state.max_attempts.saturating_add(1);
                        state.attempt >= allowed
                    });

                if attempt_limit_reached {
                    let limit = self
                        .auto_resolve_state
                        .as_ref()
                        .map(|state| state.max_attempts)
                        .unwrap_or(0);
                    let message = if rationale_text.is_empty() {
                        match limit {
                            0 => "Auto-resolve: agent reported no remaining issues but automation is disabled (limit 0). Please inspect manually.".to_string(),
                            1 => "Auto-resolve: agent reported no remaining issues but hit the single allowed review. Please inspect manually.".to_string(),
                            _ => format!(
                                "Auto-resolve: agent reported no remaining issues but hit the review attempt limit ({limit}). Please inspect manually."
                            ),
                        }
                    } else {
                        match limit {
                            0 => format!(
                                "Auto-resolve: no remaining issues. {rationale_text} Automation is disabled (limit 0); handing control back to you."
                            ),
                            1 => format!(
                                "Auto-resolve: no remaining issues. {rationale_text} The single allowed review is complete; handing control back to you."
                            ),
                            _ => format!(
                                "Auto-resolve: no remaining issues. {rationale_text} Review attempt limit ({limit}) reached; handing control back to you."
                            ),
                        }
                    };
                    self.auto_resolve_notice(message);
                    self.auto_resolve_clear();
                } else {
                    if rationale_text.is_empty() {
                        self.auto_resolve_notice(
                            "Auto-resolve: agent reported no remaining issues. Running follow-up /review to confirm.".to_string(),
                        );
                    } else {
                        self.auto_resolve_notice(format!(
                            "Auto-resolve: no remaining issues. {rationale_text} Running follow-up /review to confirm."
                        ));
                    }
                    if let Some(state) = self.auto_resolve_state.as_mut() {
                        state.phase = AutoResolvePhase::WaitingForReview;
                    }
                    self.restart_auto_resolve_review();
                }
            }
            "continue_fix" => {
                if let Some(state) = self.auto_resolve_state.as_mut() {
                    state.phase = AutoResolvePhase::AwaitingFix {
                        review: review.clone(),
                    };
                }
                self.dispatch_auto_continue(&review);
            }
            "review_again" => {
                let stop = self
                    .auto_resolve_state
                    .as_ref()
                    .is_some_and(|state| {
                        let allowed = state.max_attempts.saturating_add(1);
                        state.attempt >= allowed
                    });
                if stop {
                    let limit = self
                        .auto_resolve_state
                        .as_ref()
                        .map(|state| state.max_attempts)
                        .unwrap_or(0);
                    let message = if limit == 0 {
                        "Auto-resolve: review-again requested but automation is disabled (limit 0). Stopping.".to_string()
                    } else if limit == 1 {
                        "Auto-resolve: review-again requested but the attempt limit has been reached (1 allowed review). Stopping.".to_string()
                    } else {
                        format!(
                            "Auto-resolve: review-again requested but the attempt limit has been reached ({limit} allowed reviews). Stopping."
                        )
                    };
                    self.auto_resolve_notice(message);
                    self.auto_resolve_clear();
                } else {
                    if rationale.trim().is_empty() {
                        self.auto_resolve_notice("Auto-resolve: running another /review pass.".to_string());
                    } else {
                        let rationale_text = rationale.trim();
                        self.auto_resolve_notice(format!(
                            "Auto-resolve: running another /review pass. {rationale_text}"
                        ));
                    }
                    self.restart_auto_resolve_review();
                }
            }
            other => {
                self.auto_resolve_notice(format!(
                    "Auto-resolve: unexpected status '{other}'. Stopping automation."
                ));
                self.auto_resolve_clear();
            }
        }
    }

    pub(in crate::chatwidget) fn auto_resolve_parse_decision(raw: &str) -> Option<AutoResolveDecision> {
        if let Ok(decision) = serde_json::from_str::<AutoResolveDecision>(raw) {
            return Some(decision);
        }

        if let Some(start) = raw.find("{" )
            && let Some(end) = raw.rfind("}") {
                let slice = &raw[start..=end];
                if let Ok(decision) = serde_json::from_str::<AutoResolveDecision>(slice) {
                    return Some(decision);
                }
            }

        // try to strip ```json fences
        if let Some(json_start) = raw.find("```")
            && let Some(rest) = raw[json_start + 3..].split_once("```") {
                let candidate = rest.0.trim_start_matches("json").trim();
                if let Ok(decision) = serde_json::from_str::<AutoResolveDecision>(candidate) {
                    return Some(decision);
                }
            }

        None
    }

}
