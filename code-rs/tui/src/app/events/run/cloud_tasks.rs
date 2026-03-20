            match event {
                AppEvent::FetchCloudTasks { environment } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_tasks_loading();
                    }
                    let tx = self.app_event_tx.clone();
                    let env_clone = environment.clone();
                    tokio::spawn(async move {
                        match cloud_tasks_service::fetch_tasks(environment).await {
                            Ok(tasks) => tx.send(AppEvent::PresentCloudTasks {
                                environment: env_clone,
                                tasks,
                            }),
                            Err(err) => tx.send(AppEvent::CloudTasksError {
                                message: err.to_string(),
                            }),
                        }
                    });
                }
                AppEvent::PresentCloudTasks { environment, tasks } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.present_cloud_tasks(environment, tasks);
                    }
                }
                AppEvent::CloudTasksError { message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_tasks_error(message);
                    }
                }
                AppEvent::FetchCloudEnvironments => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_environment_loading();
                    }
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        match cloud_tasks_service::fetch_environments().await {
                            Ok(envs) => tx.send(AppEvent::PresentCloudEnvironments { environments: envs }),
                            Err(err) => tx.send(AppEvent::CloudTasksError { message: err.to_string() }),
                        }
                    });
                }
                AppEvent::PresentCloudEnvironments { environments } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.present_cloud_environment_picker(environments);
                    }
                }
                AppEvent::SetCloudEnvironment { environment } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_cloud_environment(environment);
                    }
                }
                AppEvent::ShowCloudTaskActions { task_id } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_task_actions(task_id);
                    }
                }
                AppEvent::FetchCloudTaskDiff { task_id } => {
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let task = TaskId(task_id.clone());
                        match cloud_tasks_service::fetch_task_diff(task.clone()).await {
                            Ok(Some(diff)) => {
                                tx.send(AppEvent::DiffResult(diff));
                            }
                            Ok(None) => tx.send(AppEvent::CloudTasksError {
                                message: format!("Task {} has no diff available", task.0),
                            }),
                            Err(err) => tx.send(AppEvent::CloudTasksError { message: err.to_string() }),
                        }
                    });
                }
                AppEvent::FetchCloudTaskMessages { task_id } => {
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let task = TaskId(task_id.clone());
                        match cloud_tasks_service::fetch_task_messages(task).await {
                            Ok(messages) if !messages.is_empty() => {
                                let joined = messages.join("\n\n");
                                tx.send(AppEvent::InsertBackgroundEvent {
                                    message: format!("Cloud task output for {task_id}:\n{joined}"),
                                    placement: crate::app_event::BackgroundPlacement::Tail,
                                    order: None,
                                });
                            }
                            Ok(_) => tx.send(AppEvent::CloudTasksError {
                                message: format!("Task {task_id} has no assistant messages"),
                            }),
                            Err(err) => tx.send(AppEvent::CloudTasksError { message: err.to_string() }),
                        }
                    });
                }
                AppEvent::ApplyCloudTask { task_id, preflight } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_task_apply_status(&task_id, preflight);
                    }
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let task = TaskId(task_id.clone());
                        let result = cloud_tasks_service::apply_task(task, preflight).await;
                        tx.send(AppEvent::CloudTaskApplyFinished {
                            task_id,
                            outcome: result.map_err(|err| CloudTaskError::Msg(err.to_string())),
                            preflight,
                        });
                    });
                }
                AppEvent::CloudTaskApplyFinished { task_id, outcome, preflight } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_cloud_task_apply_finished(task_id, outcome, preflight);
                    }
                }
                AppEvent::OpenCloudTaskCreate => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_task_create_prompt();
                    }
                }
                AppEvent::SubmitCloudTaskCreate { env_id, prompt, best_of_n } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_task_create_progress();
                    }
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let result = cloud_tasks_service::create_task(env_id.clone(), prompt.clone(), best_of_n).await;
                        tx.send(AppEvent::CloudTaskCreated {
                            env_id,
                            result: result.map_err(|err| CloudTaskError::Msg(err.to_string())),
                        });
                    });
                }
                AppEvent::CloudTaskCreated { env_id, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_cloud_task_created(env_id.clone(), result);
                    }
                }
                AppEvent::StartReviewCommitPicker => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_review_commit_loading();
                    }
                    let cwd = self.config.cwd.clone();
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let commits = code_core::git_info::recent_commits(&cwd, 60).await;
                        tx.send(AppEvent::PresentReviewCommitPicker { commits });
                    });
                }
                AppEvent::PresentReviewCommitPicker { commits } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.present_review_commit_picker(commits);
                    }
                }
                AppEvent::StartReviewBranchPicker => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_review_branch_loading();
                    }
                    let cwd = self.config.cwd.clone();
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let (branches, current_branch) = tokio::join!(
                            code_core::git_info::local_git_branches(&cwd),
                            code_core::git_info::current_branch_name(&cwd),
                        );
                        tx.send(AppEvent::PresentReviewBranchPicker {
                            current_branch,
                            branches,
                        });
                    });
                }
                AppEvent::PresentReviewBranchPicker {
                    current_branch,
                    branches,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.present_review_branch_picker(current_branch, branches);
                    }
                }
                AppEvent::DiffResult(text) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.add_diff_output(text);
                    }
                }
                event => {
                    include!("theme_spinner_and_login.rs");
                }
            }
