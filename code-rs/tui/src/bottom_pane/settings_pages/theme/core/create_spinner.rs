use super::super::*;

use code_login::AuthMode;

impl ThemeSelectionView {
    /// Spawn a background task that creates a custom spinner using the LLM with a JSON schema
    pub(in crate::bottom_pane::settings_pages::theme) fn kickoff_spinner_creation(
        &self,
        user_prompt: String,
        progress_tx: std::sync::mpsc::Sender<ProgressMsg>,
    ) {
        let tx = self.app_event_tx.clone();
        let before_ticket = self.before_ticket.clone();
        let fallback_tx = self.app_event_tx.clone();
        let fallback_ticket = self.before_ticket.clone();
        let completion_tx = progress_tx.clone();
        if thread_spawner::spawn_lightweight("spinner-create", move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tx.send_background_before_next_output_with_ticket(
                        &before_ticket,
                        format!("Failed to start runtime: {e}"),
                    );
                    return;
                }
            };
            rt.block_on(async move {
                // Load current config (CLI-style) and construct a one-off ModelClient
                let cfg = match code_core::config::Config::load_with_cli_overrides(vec![], code_core::config::ConfigOverrides::default()) {
                    Ok(c) => c,
                    Err(e) => {
                        tx.send_background_before_next_output_with_ticket(
                            &before_ticket,
                            format!("Config error: {e}"),
                        );
                        return;
                    }
                };
                // Use the same auth preference as the active Codex session.
                // When logged in with ChatGPT, prefer ChatGPT auth; otherwise fall back to API key.
                let preferred_auth = if cfg.using_chatgpt_auth {
                    AuthMode::ChatGPT
                } else {
                    AuthMode::ApiKey
                };
                let auth_mgr = code_core::AuthManager::shared_with_mode_and_originator(
                    cfg.code_home.clone(),
                    preferred_auth,
                    cfg.responses_originator_header.clone(),
                    cfg.cli_auth_credentials_store_mode,
                );
                let debug_logger = match code_core::debug_logger::DebugLogger::new(true) {
                    Ok(logger) => logger,
                    Err(err) => {
                        tracing::warn!("spinner debug logger init failed: {err}");
                        match code_core::debug_logger::DebugLogger::new(false) {
                            Ok(logger) => logger,
                            Err(disabled_err) => {
                                tx.send_background_before_next_output_with_ticket(
                                    &before_ticket,
                                    format!(
                                        "Failed to initialize debug logger: {err}; fallback failed: {disabled_err}"
                                    ),
                                );
                                return;
                            }
                        }
                    }
                };
                let client = code_core::ModelClient::new(code_core::ModelClientInit {
                    config: std::sync::Arc::new(cfg.clone()),
                    auth_manager: Some(auth_mgr),
                    otel_event_manager: None,
                    provider: cfg.model_provider.clone(),
                    effort: code_core::config_types::ReasoningEffort::Low,
                    summary: cfg.model_reasoning_summary,
                    verbosity: cfg.model_text_verbosity,
                    session_id: uuid::Uuid::new_v4(),
                    // Enable debug logs for targeted triage of stream issues
                    debug_logger: std::sync::Arc::new(std::sync::Mutex::new(debug_logger)),
                });

                // Build developer guidance and input
                let developer = "You are performing a custom task to create a terminal spinner.\n\nRequirements:\n- Output JSON ONLY, no prose.\n- `interval` is the delay in milliseconds between frames; MUST be between 50 and 300 inclusive.\n- `frames` is an array of strings; each element is a frame displayed sequentially at the given interval.\n- The spinner SHOULD have between 2 and 60 frames.\n- Each frame SHOULD be between 1 and 30 characters wide. ALL frames MUST be the SAME width (same number of characters). If you propose frames with varying widths, PAD THEM ON THE LEFT with spaces so they are uniform.\n- You MAY use both ASCII and Unicode characters (e.g., box drawing, braille, arrows). Use EMOJIS ONLY if the user explicitly requests emojis in their prompt.\n- Be creative! You have the full range of Unicode to play with!\n".to_string();
                let input: Vec<code_protocol::models::ResponseItem> = vec![
                    code_protocol::models::ResponseItem::Message {
                        id: None,
                        role: "developer".to_string(),
                        content: vec![code_protocol::models::ContentItem::InputText {
                            text: developer,
                        }],
                        end_turn: None,
                        phase: None,
                    },
                    code_protocol::models::ResponseItem::Message {
                        id: None,
                        role: "user".to_string(),
                        content: vec![code_protocol::models::ContentItem::InputText {
                            text: user_prompt,
                        }],
                        end_turn: None,
                        phase: None,
                    },
                ];

                // JSON schema for structured output
                let schema = serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "minLength": 1, "maxLength": 40, "description": "Display name for the spinner (1 - 3 words, shown in the UI)."},
                        "interval": {"type": "integer", "minimum": 50, "maximum": 300, "description": "Delay between frames in milliseconds (50 - 300)."},
                        "frames": {
                            "type": "array",
                            "items": {"type": "string", "minLength": 1, "maxLength": 30},
                            "minItems": 2,
                            "maxItems": 60,
                            "description": "2 - 60 frames, 1 - 30 characters each (every frame should be the same length of characters)."
                        }
                    },
                    "required": ["name", "interval", "frames"],
                    "additionalProperties": false
                });
                let format = code_core::TextFormat { r#type: "json_schema".to_string(), name: Some("custom_spinner".to_string()), strict: Some(true), schema: Some(schema) };

                let mut prompt = code_core::Prompt::default();
                prompt.input = input;
                prompt.store = true;
                prompt.text_format = Some(format);
                prompt.set_log_tag("ui/theme_spinner");

                // Stream and collect final JSON
                use futures::StreamExt;
                let _ = progress_tx.send(ProgressMsg::ThinkingDelta("(connecting to model)".to_string()));
                let mut stream = match client.stream(&prompt).await {
                    Ok(s) => s,
                    Err(e) => {
                        tx.send_background_before_next_output_with_ticket(
                            &before_ticket,
                            format!("Request error: {e}"),
                        );
                        tracing::info!("spinner request error: {e}");
                        return;
                    }
                };
                let mut out = String::new();
                let mut think_sum = String::new();
                // Capture the last transport/stream error so we can surface it to the UI
                let mut last_err: Option<String> = None;
                while let Some(ev) = stream.next().await {
                    match ev {
                        Ok(code_core::ResponseEvent::Created { .. }) => { tracing::info!("LLM: created"); let _ = progress_tx.send(ProgressMsg::SetStatus("(starting generation)".to_string())); }
                        Ok(code_core::ResponseEvent::ReasoningSummaryDelta { delta, .. }) => { tracing::info!(target: "spinner", "LLM[thinking]: {}", delta); let _ = progress_tx.send(ProgressMsg::ThinkingDelta(delta.clone())); think_sum.push_str(&delta); }
                        Ok(code_core::ResponseEvent::ReasoningContentDelta { delta, .. }) => { tracing::info!(target: "spinner", "LLM[reasoning]: {}", delta); }
                        Ok(code_core::ResponseEvent::OutputTextDelta { delta, .. }) => { tracing::info!(target: "spinner", "LLM[delta]: {}", delta); let _ = progress_tx.send(ProgressMsg::OutputDelta(delta.clone())); out.push_str(&delta); }
                        Ok(code_core::ResponseEvent::OutputItemDone { item, .. }) => {
                            if let code_protocol::models::ResponseItem::Message { content, .. } = item {
                                for c in content { if let code_protocol::models::ContentItem::OutputText { text } = c { out.push_str(&text); } }
                            }
                            tracing::info!(target: "spinner", "LLM[item_done]");
                        }
                        Ok(code_core::ResponseEvent::Completed { .. }) => { tracing::info!("LLM: completed"); break; }
                        Err(e) => {
                            let msg = e.to_string();
                            tracing::info!("LLM stream error: {msg}");
                            last_err = Some(msg);
                            break; // Stop consuming after a terminal transport error
                        }
                        _ => {}
                    }
                }

                let _ = progress_tx.send(ProgressMsg::RawOutput(out.clone()));

                // If we received no content at all, surface the transport error explicitly
                if out.trim().is_empty() {
                    let err = last_err
                        .map(|e| format!(
                            "model stream error: {} | raw_out_len={} think_len={}",
                            e,
                            out.len(),
                            think_sum.len()
                        ))
                        .unwrap_or_else(|| format!(
                            "model stream returned no content | raw_out_len={} think_len={}",
                            out.len(),
                            think_sum.len()
                        ));
                    let _ = progress_tx.send(ProgressMsg::CompletedErr { error: err, _raw_snippet: String::new() });
                    return;
                }

                // Parse JSON; on failure, attempt to salvage a top-level object and log raw output
                let v: serde_json::Value = match serde_json::from_str(&out) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::info!(target: "spinner", "Strict JSON parse failed: {}", e);
                        tracing::info!(target: "spinner", "Raw output: {}", out);
                        fn extract_first_json_object(s: &str) -> Option<String> {
                            let mut depth = 0usize;
                            let mut in_str = false;
                            let mut esc = false;
                            let mut start: Option<usize> = None;
                            for (i, ch) in s.char_indices() {
                                if in_str {
                                    if esc { esc = false; }
                                    else if ch == '\\' { esc = true; }
                                    else if ch == '"' { in_str = false; }
                                    continue;
                                }
                                match ch {
                                    '"' => in_str = true,
                                    '{' => { if depth == 0 { start = Some(i); } depth += 1; },
                                    '}' => { if depth > 0 { depth -= 1; if depth == 0 { let end = i + ch.len_utf8(); return start.map(|st| s[st..end].to_string()); } } },
                                    _ => {}
                                }
                            }
                            None
                        }
                        if let Some(obj) = extract_first_json_object(&out) {
                            match serde_json::from_str::<serde_json::Value>(&obj) {
                                Ok(v) => v,
                                Err(e2) => {
                                    let _ = progress_tx.send(ProgressMsg::CompletedErr {
                                        error: e2.to_string(),
                                        _raw_snippet: out.chars().take(200).collect::<String>(),
                                    });
                                    return;
                                }
                            }
                        } else {
                            // Prefer a clearer message if we saw a transport error
                            let msg = last_err
                                .map(|le| format!("model stream error: {le}"))
                                .unwrap_or_else(|| e.to_string());
                            let _ = progress_tx.send(ProgressMsg::CompletedErr {
                                error: msg,
                                _raw_snippet: out.chars().take(200).collect::<String>(),
                            });
                            return;
                        }
                    }
                };
                let interval = v
                    .get("interval")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(120)
                    .clamp(50, 300);
                let display_name = v
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Custom")
                    .to_string();
                let mut frames: Vec<String> = v
                    .get("frames")
                    .and_then(serde_json::Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();

                // Enforce frame width limit (truncate to first 20 terminal columns).
                const MAX_COLS: usize = 20;
                frames = frames
                    .into_iter()
                    .map(|frame| {
                        let (prefix, _suffix, _width) =
                            crate::live_wrap::take_prefix_by_width(&frame, MAX_COLS);
                        prefix
                    })
                    .filter(|f| !f.is_empty())
                    .collect();

                // Enforce count 2–50
                if frames.len() > 50 { frames.truncate(50); }
                if frames.len() < 2 { let _ = progress_tx.send(ProgressMsg::CompletedErr { error: "too few frames after validation".to_string(), _raw_snippet: out.chars().take(200).collect::<String>() }); return; }

                // Normalize: left-pad frames to equal display width so previews align.
                let max_len = frames
                    .iter()
                    .map(|frame| unicode_width::UnicodeWidthStr::width(frame.as_str()))
                    .max()
                    .unwrap_or(0);
                let norm_frames: Vec<String> = frames
                    .into_iter()
                    .map(|frame| {
                        let cur = unicode_width::UnicodeWidthStr::width(frame.as_str());
                        if cur >= max_len {
                            frame
                        } else {
                            let pad = " ".repeat(max_len.saturating_sub(cur));
                            format!("{pad}{frame}")
                        }
                    })
                    .collect();

                // Persist + activate
                let _ = progress_tx.send(ProgressMsg::CompletedOk { name: display_name, interval, frames: norm_frames });
            });
        })
        .is_none()
        {
            let _ = completion_tx.send(ProgressMsg::CompletedErr {
                error: "background worker unavailable".to_string(),
                _raw_snippet: String::new(),
            });
            fallback_tx.send_background_before_next_output_with_ticket(
                &fallback_ticket,
                "Failed to generate spinner preview: background worker unavailable".to_string(),
            );
        }
    }
}
