impl ChatWidget<'_> {
    pub(crate) fn handle_perf_command(&mut self, args: String) {
        let arg = args.trim().to_lowercase();
        match arg.as_str() {
            "on" => {
                self.perf_state.enabled = true;
                self.perf_state.pending_scroll_rows.set(0);
                self.add_perf_output("performance tracing: on".to_string());
            }
            "off" => {
                self.perf_state.enabled = false;
                self.perf_state.pending_scroll_rows.set(0);
                self.add_perf_output("performance tracing: off".to_string());
            }
            "reset" => {
                self.perf_state.stats.borrow_mut().reset();
                self.perf_state.pending_scroll_rows.set(0);
                self.add_perf_output("performance stats reset".to_string());
            }
            "show" | "" => {
                let summary = self.perf_state.stats.borrow().summary();
                self.add_perf_output(summary);
            }
            _ => {
                self.add_perf_output("usage: /perf on | off | show | reset".to_string());
            }
        }
        self.request_redraw();
    }

    pub(crate) fn handle_demo_command(&mut self, command_args: String) {
        let trimmed_args = command_args.trim();
        if !trimmed_args.is_empty() {
            if self.handle_demo_auto_drive_card_background_palette(trimmed_args) {
                self.request_redraw();
                return;
            }

            self.history_push_plain_state(history_cell::new_warning_event(format!(
                "demo: unknown args '{trimmed_args}' (try: /demo auto drive card)",
            )));
            self.request_redraw();
            return;
        }

        use ratatui::style::Modifier as RtModifier;
        use ratatui::style::Style as RtStyle;
        use ratatui::text::Span;

        self.push_background_tail("demo: populating history with sample cells…");
        enum DemoPatch {
            Add {
                path: &'static str,
                content: &'static str,
            },
            Update {
                path: &'static str,
                unified_diff: &'static str,
                original: &'static str,
                new_content: &'static str,
            },
        }

        let scenarios = [
            (
                "build automation",
                "How do I wire up CI, linting, and release automation for this repo?",
                vec![
                    ("Context", "scan workspace layout and toolchain."),
                    ("Next", "surface build + validation commands."),
                    ("Goal", "summarize a reproducible workflow."),
                ],
                vec![
                    "streaming preview: inspecting package manifests…",
                    "streaming preview: drafting deployment summary…",
                    "streaming preview: cross-checking lint targets…",
                ],
                "**Here's a demo walkthrough:**\n\n1. Run `./build-fast.sh perf` to compile quickly.\n2. Cache artifacts in `code-rs/target/perf`.\n3. Finish by sharing `./build-fast.sh run` output.\n\n```bash\n./build-fast.sh perf run\n```",
                vec![
                    (vec!["git", "status"], "On branch main\nnothing to commit, working tree clean\n"),
                    (vec!["rg", "--files"], ""),
                ],
                Some(DemoPatch::Add {
                    path: "src/demo.rs",
                    content: "fn main() {\n    println!(\"demo\");\n}\n",
                }),
                UpdatePlanArgs {
                    name: Some("Demo Scroll Plan".to_string()),
                    explanation: None,
                    plan: vec![
                        PlanItemArg {
                            step: "Create reproducible builds".to_string(),
                            status: StepStatus::InProgress,
                        },
                        PlanItemArg {
                            step: "Verify validations".to_string(),
                            status: StepStatus::Pending,
                        },
                        PlanItemArg {
                            step: "Document follow-up tasks".to_string(),
                            status: StepStatus::Completed,
                        },
                    ],
                },
                ("browser_open", "https://example.com", "navigated to example.com"),
                ReasoningEffort::High,
                "demo: lint warnings will appear here",
                "demo: this slot shows error output",
                Some("diff --git a/src/lib.rs b/src/lib.rs\n@@ -1,3 +1,5 @@\n-pub fn hello() {}\n+pub fn hello() {\n+    println!(\"hello, demo!\");\n+}\n"),
            ),
            (
                "release rehearsal",
                "What checklist should I follow before tagging a release?",
                vec![
                    ("Inventory", "collect outstanding changes and docs."),
                    ("Verify", "run smoke tests and package audits."),
                    ("Announce", "draft release notes and rollout plan."),
                ],
                vec![
                    "streaming preview: aggregating changelog entries…",
                    "streaming preview: validating release artifacts…",
                    "streaming preview: preparing announcement copy…",
                ],
                "**Release rehearsal:**\n\n1. Run `./scripts/create_github_release.sh --dry-run`.\n2. Capture artifact hashes in the notes.\n3. Schedule follow-up validation in automation.\n\n```bash\n./scripts/create_github_release.sh 1.2.3 --dry-run\n```",
                vec![
                    (vec!["git", "--no-pager", "diff", "--stat"], " src/lib.rs | 10 ++++++----\n 1 file changed, 6 insertions(+), 4 deletions(-)\n"),
                    (vec!["ls", "-1"], "Cargo.lock\nREADME.md\nsrc\ntarget\n"),
                ],
                Some(DemoPatch::Update {
                    path: "src/release.rs",
                    unified_diff: "--- a/src/release.rs\n+++ b/src/release.rs\n@@ -1 +1,3 @@\n-pub fn release() {}\n+pub fn release() {\n+    println!(\"drafting release\");\n+}\n",
                    original: "pub fn release() {}\n",
                    new_content: "pub fn release() {\n    println!(\"drafting release\");\n}\n",
                }),
                UpdatePlanArgs {
                    name: Some("Release Gate Plan".to_string()),
                    explanation: None,
                    plan: vec![
                        PlanItemArg {
                            step: "Finalize changelog".to_string(),
                            status: StepStatus::Completed,
                        },
                        PlanItemArg {
                            step: "Run smoke tests".to_string(),
                            status: StepStatus::InProgress,
                        },
                        PlanItemArg {
                            step: "Tag release".to_string(),
                            status: StepStatus::Pending,
                        },
                        PlanItemArg {
                            step: "Notify stakeholders".to_string(),
                            status: StepStatus::Pending,
                        },
                    ],
                },
                ("browser_open", "https://example.com/releases", "reviewed release dashboard"),
                ReasoningEffort::Medium,
                "demo: release checklist warning",
                "demo: release checklist error",
                Some("diff --git a/CHANGELOG.md b/CHANGELOG.md\n@@ -1,3 +1,6 @@\n+## 1.2.3\n+- polish release flow\n+- document automation hooks\n"),
            ),
        ];

        for (idx, scenario) in scenarios.iter().enumerate() {
            let (
                label,
                prompt,
                reasoning_steps,
                stream_lines,
                assistant_body,
                execs,
                patch_change,
                plan,
                tool_call,
                effort,
                warning_text,
                error_text,
                diff_snippet,
            ) = scenario;

            self.push_background_tail(format!(
                "demo: scenario {} — {}",
                idx + 1,
                label
            ));

            self.history_push_plain_state(history_cell::new_user_prompt((*prompt).to_string()));

            let mut reasoning_lines: Vec<Line<'static>> = reasoning_steps
                .iter()
                .map(|(title, body)| {
                    Line::from(vec![
                        Span::styled(
                            format!("{title}:"),
                            RtStyle::default().add_modifier(RtModifier::BOLD),
                        ),
                        Span::raw(format!(" {body}")),
                    ])
                })
                .collect();
            reasoning_lines.push(
                Line::from(format!("Scenario summary: {label}"))
                    .style(RtStyle::default().fg(crate::colors::text_dim())),
            );
            let reasoning_cell = history_cell::CollapsibleReasoningCell::new_with_id(
                reasoning_lines,
                Some(format!("demo-reasoning-{idx}")),
            );
            reasoning_cell.set_collapsed(false);
            reasoning_cell.set_in_progress(false);
            self.history_push(reasoning_cell);

            let preview_lines: Vec<ratatui::text::Line<'static>> = stream_lines
                .iter()
                .map(|line| Line::from((*line).to_string()))
                .collect();
            let state = self.synthesize_stream_state_from_lines(None, &preview_lines, false);
            let streaming_preview = history_cell::new_streaming_content(state, &self.config);
            self.history_push(streaming_preview);

            let assistant_state = AssistantMessageState {
                id: HistoryId::ZERO,
                stream_id: None,
                markdown: (*assistant_body).to_string(),
                citations: Vec::new(),
                metadata: None,
                token_usage: None,
                mid_turn: false,
                created_at: SystemTime::now(),
            };
            let assistant_cell =
                history_cell::AssistantMarkdownCell::from_state(assistant_state, &self.config);
            self.history_push(assistant_cell);

            for (command_tokens, stdout) in execs {
                let cmd_vec: Vec<String> = command_tokens.iter().map(std::string::ToString::to_string).collect();
                let parsed = code_core::parse_command::parse_command(&cmd_vec);
                self.history_push(history_cell::new_active_exec_command(
                    cmd_vec.clone(),
                    parsed.clone(),
                ));
                if !stdout.is_empty() {
                    let output = history_cell::CommandOutput {
                        exit_code: 0,
                        stdout: stdout.to_string(),
                        stderr: String::new(),
                    };
                    self.history_push(history_cell::new_completed_exec_command(
                        cmd_vec,
                        parsed,
                        output,
                    ));
                }
            }

            if let Some(diff) = diff_snippet {
                self.history_push_diff(None, diff.to_string());
            }

            if let Some(patch) = patch_change {
                let mut patch_changes = HashMap::new();
                let message = match patch {
                    DemoPatch::Add { path, content } => {
                        patch_changes.insert(
                            PathBuf::from(path),
                            code_core::protocol::FileChange::Add {
                                content: (*content).to_string(),
                            },
                        );
                        format!("patch: simulated failure while applying {path}")
                    }
                    DemoPatch::Update {
                        path,
                        unified_diff,
                        original,
                        new_content,
                    } => {
                        patch_changes.insert(
                            PathBuf::from(path),
                            code_core::protocol::FileChange::Update {
                                unified_diff: (*unified_diff).to_string(),
                                move_path: None,
                                original_content: (*original).to_string(),
                                new_content: (*new_content).to_string(),
                            },
                        );
                        format!("patch: simulated failure while applying {path}")
                    }
                };
                self.history_push(history_cell::new_patch_event(
                    history_cell::PatchEventType::ApprovalRequest,
                    patch_changes,
                ));
                self.history_push_plain_state(history_cell::new_patch_apply_failure(message));
            }

            self.history_push(history_cell::new_plan_update(plan.clone()));

            let (tool_name, url, result) = tool_call;
            self.history_push(history_cell::new_completed_custom_tool_call(
                (*tool_name).to_string(),
                Some((*url).to_string()),
                Duration::from_millis(420 + (idx as u64 * 150)),
                true,
                (*result).to_string(),
            ));

            self.history_push_plain_state(history_cell::new_warning_event((*warning_text).to_string()));
            self.history_push_plain_state(history_cell::new_error_event((*error_text).to_string()));

            self.history_push_plain_state(history_cell::new_model_output("gpt-5.1-codex", *effort));
            self.history_push_plain_state(history_cell::new_reasoning_output(*effort));

            self.history_push_plain_state(history_cell::new_status_output(
                &self.config,
                &self.total_token_usage,
                &self.last_token_usage,
                None,
                None,
            ));

            self.history_push_plain_state(history_cell::new_prompts_output());
        }

        let final_preview_lines = vec![
            Line::from("streaming preview: final tokens rendered."),
            Line::from("streaming preview: viewport ready for scroll testing."),
        ];
        let final_state =
            self.synthesize_stream_state_from_lines(None, &final_preview_lines, false);
        let final_stream = history_cell::new_streaming_content(final_state, &self.config);
        self.history_push(final_stream);

        self.push_background_tail("demo: rendering sample tool cards for theme review…");

        let mut agent_card = history_cell::AgentRunCell::new("Demo Agent Batch".to_string());
        agent_card.set_batch_label(Some("Demo Agents".to_string()));
        agent_card.set_task(Some("Draft a release checklist".to_string()));
        agent_card.set_context(Some("Context: codex workspace demo run".to_string()));
        agent_card.set_plan(vec![
            "Collect recent commits".to_string(),
            "Summarize blockers".to_string(),
            "Draft announcement".to_string(),
        ]);
        let completed_preview = history_cell::AgentStatusPreview {
            id: "demo-completed".to_string(),
            name: "Docs Scout".to_string(),
            status: "Completed".to_string(),
            model: Some("gpt-5.1-large".to_string()),
            details: vec![history_cell::AgentDetail::Result(
                "Summarized API changes".to_string(),
            )],
            status_kind: history_cell::AgentStatusKind::Completed,
            step_progress: Some(history_cell::StepProgress {
                completed: 3,
                total: 3,
            }),
            elapsed: Some(Duration::from_secs(32)),
            last_update: Some("Wrapped up summary".to_string()),
            ..history_cell::AgentStatusPreview::default()
        };
        let running_preview = history_cell::AgentStatusPreview {
            id: "demo-running".to_string(),
            name: "Lint Fixer".to_string(),
            status: "Running".to_string(),
            model: Some("code-gpt-5.2".to_string()),
            details: vec![history_cell::AgentDetail::Progress(
                "Refining suggested fixes".to_string(),
            )],
            status_kind: history_cell::AgentStatusKind::Running,
            step_progress: Some(history_cell::StepProgress {
                completed: 1,
                total: 3,
            }),
            elapsed: Some(Duration::from_secs(18)),
            last_update: Some("Step 2 of 3".to_string()),
            ..history_cell::AgentStatusPreview::default()
        };
        agent_card.set_agent_overview(vec![completed_preview, running_preview]);
        agent_card.set_latest_result(vec!["Generated release briefing".to_string()]);
        agent_card.record_action("Collecting changelog entries");
        agent_card.record_action("Writing release notes");
        agent_card.set_duration(Some(Duration::from_secs(96)));
        agent_card.set_write_mode(Some(true));
        agent_card.set_status_label("Completed");
        agent_card.mark_completed();
        self.history_push(agent_card);

        let mut agent_read_card = history_cell::AgentRunCell::new("Demo Read Batch".to_string());
        agent_read_card.set_batch_label(Some("Read Agents".to_string()));
        agent_read_card.set_task(Some("Survey docs for regression notes".to_string()));
        agent_read_card.set_context(Some("Scope: analyze docs, no writes".to_string()));
        agent_read_card.set_plan(vec![
            "Gather doc highlights".to_string(),
            "Verify changelog snippets".to_string(),
        ]);
        let pending_preview = history_cell::AgentStatusPreview {
            id: "demo-read-pending".to_string(),
            name: "Doc Harvester".to_string(),
            status: "Pending".to_string(),
            model: Some("gpt-4.5".to_string()),
            details: vec![history_cell::AgentDetail::Info(
                "Waiting for search index".to_string(),
            )],
            status_kind: history_cell::AgentStatusKind::Pending,
            ..history_cell::AgentStatusPreview::default()
        };
        let running_read = history_cell::AgentStatusPreview {
            id: "demo-read-running".to_string(),
            name: "Spec Parser".to_string(),
            status: "Running".to_string(),
            model: Some("code-gpt-3.5".to_string()),
            details: vec![history_cell::AgentDetail::Progress(
                "Scanning RFC summaries".to_string(),
            )],
            status_kind: history_cell::AgentStatusKind::Running,
            step_progress: Some(history_cell::StepProgress {
                completed: 2,
                total: 5,
            }),
            elapsed: Some(Duration::from_secs(22)),
            ..history_cell::AgentStatusPreview::default()
        };
        agent_read_card.set_agent_overview(vec![pending_preview, running_read]);
        agent_read_card.record_action("Fetching documentation excerpts");
        agent_read_card.set_duration(Some(Duration::from_secs(54)));
        agent_read_card.set_write_mode(Some(false));
        agent_read_card.set_status_label("Running");
        self.history_push(agent_read_card);

        let mut browser_card = history_cell::BrowserSessionCell::new();
        browser_card.set_url("https://example.dev/releases");
        browser_card.set_headless(Some(false));
        browser_card.record_action(
            Duration::from_millis(0),
            Duration::from_millis(420),
            "open".to_string(),
            Some("https://example.dev/releases".to_string()),
            None,
            Some("status=200".to_string()),
        );
        browser_card.record_action(
            Duration::from_millis(620),
            Duration::from_millis(380),
            "scroll".to_string(),
            Some("main timeline".to_string()),
            Some("dy=512".to_string()),
            None,
        );
        browser_card.record_action(
            Duration::from_millis(1280),
            Duration::from_millis(520),
            "click".to_string(),
            Some(".release-card".to_string()),
            Some("index=2".to_string()),
            Some("status=OK".to_string()),
        );
        browser_card.add_console_message("Loaded demo assets".to_string());
        browser_card.add_console_message("Fetched changelog via XHR".to_string());
        browser_card.set_status_code(Some("200 OK".to_string()));
        self.history_push(browser_card);

        let mut search_card = history_cell::WebSearchSessionCell::new();
        search_card.set_query(Some("rust async cancellation strategy".to_string()));
        search_card.ensure_started_message();
        search_card.record_info(Duration::from_millis(120), "Searching documentation index");
        search_card.record_success(Duration::from_millis(620), "Found tokio.rs guides");
        search_card.record_success(Duration::from_millis(1040), "Linked blog: cancellation patterns");
        search_card.set_status(history_cell::WebSearchStatus::Completed);
        search_card.set_duration(Some(Duration::from_millis(1400)));
        self.history_push(search_card);

        let mut auto_drive_card =
            history_cell::AutoDriveCardCell::new(Some("Stabilize nightly CI pipeline".to_string()));
        auto_drive_card.push_action(
            "Queued smoke tests across agents",
            history_cell::AutoDriveActionKind::Info,
        );
        auto_drive_card.push_action(
            "Warning: macOS shard flaked",
            history_cell::AutoDriveActionKind::Warning,
        );
        auto_drive_card.push_action(
            "Action required: retry or pause run",
            history_cell::AutoDriveActionKind::Error,
        );
        auto_drive_card.set_status(history_cell::AutoDriveStatus::Paused);
        self.history_push(auto_drive_card);

        let goal = "Stabilize nightly CI pipeline".to_string();
        self.auto_state.last_run_summary = Some(AutoRunSummary {
            duration: Duration::from_secs(95),
            turns_completed: 4,
            message: Some("Auto Drive completed demo run.".to_string()),
            goal: Some(goal),
        });
        let celebration_message = "Diagnostics report: all demo checks passed.".to_string();
        self.auto_state.last_completion_explanation = Some(celebration_message.clone());
        self.schedule_auto_drive_card_celebration(Duration::from_secs(2), Some(celebration_message));

        self.request_redraw();
    }

    fn handle_demo_auto_drive_card_background_palette(&mut self, args: &str) -> bool {
        if !Self::demo_command_is_auto_drive_card_backgrounds(args) {
            return false;
        }

        let (r, g, b) = crate::colors::color_to_rgb(crate::colors::background());
        let luminance = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0;
        let theme_label = if luminance < 0.5 { "dark" } else { "light" };

        self.history_push_plain_state(history_cell::plain_message_state_from_lines(
            vec![
                ratatui::text::Line::from("Auto Drive card — ANSI-16 background palette"),
                ratatui::text::Line::from(format!(
                    "Theme context: {theme_label} (based on current /theme background)",
                )),
                ratatui::text::Line::from(
                    "Tip: switch /theme (dark/light) and rerun to compare.".to_string(),
                ),
            ],
            HistoryCellType::Notice,
        ));

        use ratatui::style::Color;
        const PALETTE: &[(Color, &str)] = &[
            (Color::Black, "Black"),
            (Color::Red, "Red"),
            (Color::Green, "Green"),
            (Color::Yellow, "Yellow"),
            (Color::Blue, "Blue"),
            (Color::Magenta, "Magenta"),
            (Color::Cyan, "Cyan"),
            (Color::Gray, "Gray"),
            (Color::DarkGray, "DarkGray"),
            (Color::LightRed, "LightRed"),
            (Color::LightGreen, "LightGreen"),
            (Color::LightYellow, "LightYellow"),
            (Color::LightBlue, "LightBlue"),
            (Color::LightMagenta, "LightMagenta"),
            (Color::LightCyan, "LightCyan"),
            (Color::White, "White"),
        ];

        for (idx, (bg, name)) in PALETTE.iter().enumerate() {
            let ordinal = idx + 1;
            let goal = format!("ANSI-16 bg {ordinal:02}: {name}");
            let mut auto_drive_card = history_cell::AutoDriveCardCell::new(Some(goal));
            auto_drive_card.disable_reveal();
            auto_drive_card.set_background_override(Some(*bg));
            auto_drive_card.push_action(
                "Queued smoke tests across agents",
                AutoDriveActionKind::Info,
            );
            auto_drive_card.push_action(
                "Warning: macOS shard flaked",
                AutoDriveActionKind::Warning,
            );
            auto_drive_card.push_action(
                "Action required: retry or pause run",
                AutoDriveActionKind::Error,
            );
            auto_drive_card.set_status(AutoDriveStatus::Paused);
            self.history_push(auto_drive_card);
        }

        true
    }

    fn demo_command_is_auto_drive_card_backgrounds(args: &str) -> bool {
        let normalized = args.trim().to_ascii_lowercase();
        let simplified = normalized.replace(['-', '_'], " ");
        let tokens: std::collections::HashSet<&str> = simplified.split_whitespace().collect();
        if tokens.is_empty() {
            return false;
        }

        let wants_auto_drive = (tokens.contains("auto") && tokens.contains("drive"))
            || tokens.contains("autodrive")
            || tokens.contains("auto-drive");
        let wants_card = tokens.contains("card") || tokens.contains("cards");
        let wants_background = tokens.contains("bg")
            || tokens.contains("background")
            || tokens.contains("backgrounds")
            || tokens.contains("color")
            || tokens.contains("colors")
            || tokens.contains("colour")
            || tokens.contains("colours");

        wants_auto_drive && (wants_card || wants_background)
    }

    fn add_perf_output(&mut self, text: String) {
        let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
        lines.push(ratatui::text::Line::from("performance".dim()));
        for l in text.lines() {
            lines.push(ratatui::text::Line::from(l.to_string()))
        }
        let state = history_cell::plain_message_state_from_lines(
            lines,
            crate::history_cell::HistoryCellType::Notice,
        );
        self.history_push_plain_state(state);
    }

    pub(crate) fn add_diff_output(&mut self, diff_output: String) {
        self.history_push_diff(None, diff_output);
    }

    pub(crate) fn add_status_output(&mut self) {
        self.history_push_plain_state(history_cell::new_status_output(
            &self.config,
            &self.total_token_usage,
            &self.last_token_usage,
            None,
            None,
        ));
    }

}
