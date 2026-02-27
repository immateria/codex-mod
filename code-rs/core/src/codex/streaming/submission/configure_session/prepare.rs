use super::*;

impl Runner<'_> {
    pub(super) async fn prepare(
        &mut self,
        req: ConfigureSessionRequest,
    ) -> Result<Prepared, ConfigureSessionControl> {
        let ConfigureSessionRequest {
            submission_id,
            provider,
            model,
            model_explicit,
            model_reasoning_effort,
            preferred_model_reasoning_effort,
            model_reasoning_summary,
            model_text_verbosity,
            provided_user_instructions,
            provided_base_instructions,
            approval_policy,
            sandbox_policy,
            disable_response_storage,
            notify,
            cwd,
            resume_path,
            demo_developer_message,
            dynamic_tools,
            shell_override,
            shell_style_profiles,
            network,
            collaboration_mode,
        } = req;

        debug!(
            "Configuring session: model={model}; provider={provider:?}; resume={resume_path:?}"
        );

        if !cwd.is_absolute() {
            let message = format!("cwd is not absolute: {cwd:?}");
            self.send_error_event(&submission_id, message).await;
            return Err(ConfigureSessionControl::Exit);
        }

        let current_config = Arc::clone(&self.config);
        let mut updated_config = (*current_config).clone();

        let model_changed = !updated_config.model.eq_ignore_ascii_case(&model);
        let effort_changed = updated_config.model_reasoning_effort != model_reasoning_effort;
        let preferred_effort_changed = preferred_model_reasoning_effort
            .as_ref()
            .map(|preferred| {
                updated_config.preferred_model_reasoning_effort != Some(*preferred)
            })
            .unwrap_or(false);

        let old_model_family = updated_config.model_family.clone();
        let old_tool_output_max_bytes = updated_config.tool_output_max_bytes;
        let old_default_tool_output_max_bytes = old_model_family.tool_output_max_bytes();

        updated_config.model = model.clone();
        updated_config.model_explicit = model_explicit;
        updated_config.model_provider = provider.clone();
        updated_config.model_reasoning_effort = model_reasoning_effort;
        if let Some(preferred) = preferred_model_reasoning_effort {
            updated_config.preferred_model_reasoning_effort = Some(preferred);
        }
        updated_config.model_reasoning_summary = model_reasoning_summary;
        updated_config.model_text_verbosity = model_text_verbosity;
        updated_config.user_instructions = provided_user_instructions;
        let base_instructions = provided_base_instructions.or_else(|| {
            crate::model_family::base_instructions_override_for_personality(
                &model,
                updated_config.model_personality,
            )
        });
        updated_config.base_instructions = base_instructions.clone();
        updated_config.approval_policy = approval_policy;
        updated_config.sandbox_policy = sandbox_policy.clone();
        updated_config.disable_response_storage = disable_response_storage;
        updated_config.notify = notify.clone();
        updated_config.cwd = cwd.clone();
        updated_config.dynamic_tools = dynamic_tools.clone();
        updated_config.network = network.clone();
        updated_config.shell_style_profiles = shell_style_profiles;

        updated_config.network_proxy = match updated_config.network.as_ref().filter(|net| net.enabled)
        {
            Some(net) => {
                match crate::config::network_proxy_spec::NetworkProxySpec::from_config(
                    net.to_network_proxy_config(),
                ) {
                    Ok(spec) => Some(spec),
                    Err(err) => {
                        let message = format!("invalid managed network config: {err}");
                        self.send_error_event(&submission_id, message).await;
                        return Err(ConfigureSessionControl::Exit);
                    }
                }
            }
            None => None,
        };

        updated_config.model_family = find_family_for_model(&updated_config.model)
            .unwrap_or_else(|| derive_default_model_family(&updated_config.model));

        let new_default_tool_output_max_bytes = updated_config.model_family.tool_output_max_bytes();

        let old_context_window = old_model_family.context_window;
        let new_context_window = updated_config.model_family.context_window;
        let old_max_tokens = old_model_family.max_output_tokens;
        let new_max_tokens = updated_config.model_family.max_output_tokens;
        let old_auto_compact = old_model_family.auto_compact_token_limit();
        let new_auto_compact = updated_config.model_family.auto_compact_token_limit();

        maybe_update_from_model_info(
            &mut updated_config.model_context_window,
            old_context_window,
            new_context_window,
        );
        maybe_update_from_model_info(
            &mut updated_config.model_max_output_tokens,
            old_max_tokens,
            new_max_tokens,
        );
        maybe_update_from_model_info(
            &mut updated_config.model_auto_compact_token_limit,
            old_auto_compact,
            new_auto_compact,
        );

        if old_tool_output_max_bytes == old_default_tool_output_max_bytes {
            updated_config.tool_output_max_bytes = new_default_tool_output_max_bytes;
        }

        let resolved_shell = shell::default_user_shell_with_override(
            shell_override.as_ref().or(updated_config.shell.as_ref()),
        )
        .await;
        let active_shell_style = resolved_shell.script_style();
        let active_shell_style_label = active_shell_style.map(|style| style.to_string());
        let mut shell_style_profile_messages: Vec<String> = Vec::new();
        let mut shell_style_skill_filter: Option<HashSet<String>> = None;
        let mut shell_style_disabled_skills: HashSet<String> = HashSet::new();
        let mut shell_style_skill_roots: Vec<PathBuf> = Vec::new();
        let mut shell_style_mcp_include: HashSet<String> = HashSet::new();
        let mut shell_style_mcp_exclude: HashSet<String> = HashSet::new();
        let mut effective_mcp_servers = updated_config.mcp_servers.clone();

        if let Some(style) = active_shell_style
            && let Some(profile) = updated_config.shell_style_profiles.get(&style).cloned()
        {
            shell_style_mcp_include = profile
                .mcp_servers
                .include
                .iter()
                .map(|name| name.trim().to_ascii_lowercase())
                .filter(|name| !name.is_empty())
                .collect();
            if !shell_style_mcp_include.is_empty() {
                effective_mcp_servers.retain(|name, _| {
                    shell_style_mcp_include.contains(&name.to_ascii_lowercase())
                });
            }

            shell_style_mcp_exclude = profile
                .mcp_servers
                .exclude
                .iter()
                .map(|name| name.trim().to_ascii_lowercase())
                .filter(|name| !name.is_empty())
                .collect();
            if !shell_style_mcp_exclude.is_empty() {
                effective_mcp_servers.retain(|name, _| {
                    !shell_style_mcp_exclude.contains(&name.to_ascii_lowercase())
                });
            }

            for message in profile.prepend_developer_messages {
                let trimmed = message.trim();
                if !trimmed.is_empty() {
                    shell_style_profile_messages.push(trimmed.to_string());
                }
            }

            for reference in profile.references {
                let full_path = if reference.is_relative() {
                    updated_config.cwd.join(&reference)
                } else {
                    reference.clone()
                };
                match std::fs::read_to_string(&full_path) {
                    Ok(contents) => {
                        let trimmed = contents.trim();
                        if !trimmed.is_empty() {
                            shell_style_profile_messages.push(format!(
                                "Shell style reference `{style}` from `{}`:\n\n{trimmed}",
                                full_path.display(),
                            ));
                        }
                    }
                    Err(err) => {
                        warn!(
                            "failed to read shell style reference {}: {err}",
                            full_path.display()
                        );
                    }
                }
            }

            let requested_skills: HashSet<String> = profile
                .skills
                .iter()
                .map(|name| name.trim().to_ascii_lowercase())
                .filter(|name| !name.is_empty())
                .collect();
            if !requested_skills.is_empty() {
                shell_style_skill_filter = Some(requested_skills);
            }

            shell_style_disabled_skills.extend(
                profile
                    .disabled_skills
                    .iter()
                    .map(|name| name.trim().to_ascii_lowercase())
                    .filter(|name| !name.is_empty()),
            );

            shell_style_skill_roots.extend(
                profile
                    .skill_roots
                    .into_iter()
                    .filter(|path| !path.as_os_str().is_empty()),
            );
        }

        let command_safety_profile = crate::safety::resolve_command_safety_profile(
            &resolved_shell,
            shell_override.as_ref().or(updated_config.shell.as_ref()),
            &updated_config.shell_style_profiles,
        );

        let mut skills_outcome = if updated_config.skills_enabled {
            Some(if shell_style_skill_roots.is_empty() {
                load_skills(&updated_config)
            } else {
                crate::skills::loader::load_skills_with_additional_roots(
                    &updated_config,
                    shell_style_skill_roots.iter().cloned(),
                )
            })
        } else {
            None
        };
        if let Some(outcome) = &mut skills_outcome {
            for err in &outcome.errors {
                warn!("invalid skill {}: {}", err.path.display(), err.message);
            }

            let available_skill_names: HashSet<String> = outcome
                .skills
                .iter()
                .map(|skill| skill.name.trim().to_ascii_lowercase())
                .collect();

            if let Some(skill_filter) = shell_style_skill_filter.as_ref() {
                let mut matched_skills: HashSet<String> = HashSet::new();
                outcome.skills.retain(|skill| {
                    let normalized = skill.name.trim().to_ascii_lowercase();
                    let keep = skill_filter.contains(&normalized);
                    if keep {
                        matched_skills.insert(normalized);
                    }
                    keep
                });

                if let Some(style_label) = active_shell_style_label.as_deref() {
                    for requested in skill_filter {
                        if !matched_skills.contains(requested) {
                            warn!(
                                "shell style profile `{style_label}` requested unknown skill `{requested}`"
                            );
                        }
                    }
                }
            }

            if !shell_style_disabled_skills.is_empty() {
                outcome.skills.retain(|skill| {
                    let normalized = skill.name.trim().to_ascii_lowercase();
                    !shell_style_disabled_skills.contains(&normalized)
                });

                if let Some(style_label) = active_shell_style_label.as_deref() {
                    for requested in &shell_style_disabled_skills {
                        if !available_skill_names.contains(requested) {
                            warn!(
                                "shell style profile `{style_label}` requested unknown disabled skill `{requested}`"
                            );
                        }
                    }
                }
            }
        }

        let session_skills = skills_outcome
            .as_ref()
            .map(|outcome| super::skills::strip_skill_contents(outcome.skills.as_slice()))
            .unwrap_or_default();

        let computed_user_instructions = get_user_instructions(
            &updated_config,
            skills_outcome.as_ref().map(|outcome| outcome.skills.as_slice()),
        )
        .await;
        updated_config.user_instructions = computed_user_instructions.clone();

        let effective_user_instructions = computed_user_instructions.clone();

        // Optionally resume an existing rollout.
        let mut restored_items: Option<Vec<RolloutItem>> = None;
        let mut restored_history_snapshot: Option<crate::history::HistorySnapshot> = None;
        let mut resume_notice: Option<String> = None;
        let mut rollout_recorder: Option<RolloutRecorder> = None;
        if let Some(path) = resume_path.as_ref() {
            match RolloutRecorder::resume(&updated_config, path).await {
                Ok((rec, saved)) => {
                    self.session_id = saved.session_id;
                    if !saved.items.is_empty() {
                        restored_items = Some(saved.items);
                    }
                    if let Some(snapshot) = saved.history_snapshot {
                        restored_history_snapshot = Some(snapshot);
                    }
                    rollout_recorder = Some(rec);
                }
                Err(e) => {
                    warn!("failed to resume rollout from {path:?}: {e}");
                    resume_notice = Some(format!(
                        "WARN: Failed to load previous session from {}: {e}. Starting a new conversation instead.",
                        path.display()
                    ));
                    updated_config.experimental_resume = None;
                }
            }
        }

        let new_config = Arc::new(updated_config);

        if new_config.model_explicit
            && (model_changed || effort_changed || preferred_effort_changed)
            && let Err(err) = persist_model_selection(
                &new_config.code_home,
                new_config.active_profile.as_deref(),
                &new_config.model,
                Some(new_config.model_reasoning_effort),
                new_config.preferred_model_reasoning_effort,
            )
            .await
        {
            warn!("failed to persist model selection: {err:#}");
        }

        self.config = Arc::clone(&new_config);
        self.file_watcher.register_config(self.config.as_ref());

        let rollout_recorder = match rollout_recorder {
            Some(rec) => Some(rec),
            None => {
                match RolloutRecorder::new(
                    &self.config,
                    crate::rollout::recorder::RolloutRecorderParams::new(
                        code_protocol::mcp_protocol::ConversationId::from(self.session_id),
                        effective_user_instructions.clone(),
                        SessionSource::Cli,
                    ),
                )
                .await
                {
                    Ok(r) => Some(r),
                    Err(e) => {
                        warn!("failed to initialise rollout recorder: {e}");
                        None
                    }
                }
            }
        };

        Ok(Prepared {
            submission_id,
            provider,
            model,
            model_reasoning_effort,
            model_reasoning_summary,
            model_text_verbosity,
            approval_policy,
            sandbox_policy,
            disable_response_storage,
            notify,
            cwd,
            collaboration_mode,
            demo_developer_message,
            dynamic_tools,
            shell_override_present: shell_override.is_some(),
            base_instructions,
            effective_user_instructions,
            resolved_shell,
            command_safety_profile,
            active_shell_style,
            active_shell_style_label,
            shell_style_profile_messages,
            shell_style_mcp_include,
            shell_style_mcp_exclude,
            effective_mcp_servers,
            session_skills,
            restored_items,
            restored_history_snapshot,
            resume_notice,
            rollout_recorder,
        })
    }
}

