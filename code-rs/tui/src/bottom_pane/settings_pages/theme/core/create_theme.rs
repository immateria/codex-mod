use super::super::*;

use code_login::AuthMode;

impl ThemeSelectionView {
    /// Spawn a background task that creates a custom theme using the LLM.
    pub(in crate::bottom_pane::settings_pages::theme) fn kickoff_theme_creation(
        &self,
        user_prompt: String,
        progress_tx: std::sync::mpsc::Sender<ProgressMsg>,
    ) {
        let tx = self.app_event_tx.clone();
        // Capture a compact example of the current theme as guidance
        fn color_to_hex(c: ratatui::style::Color) -> Option<String> {
            match c {
                ratatui::style::Color::Rgb(r, g, b) => {
                    Some(format!("#{r:02X}{g:02X}{b:02X}"))
                }
                _ => None,
            }
        }
        let cur = crate::theme::current_theme();
        let mut example = serde_json::json!({"name": "Current", "colors": {}});
        if let Some(v) = color_to_hex(cur.primary) {
            example["colors"]["primary"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.secondary) {
            example["colors"]["secondary"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.background) {
            example["colors"]["background"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.foreground) {
            example["colors"]["foreground"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.border) {
            example["colors"]["border"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.border_focused) {
            example["colors"]["border_focused"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.selection) {
            example["colors"]["selection"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.cursor) {
            example["colors"]["cursor"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.success) {
            example["colors"]["success"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.warning) {
            example["colors"]["warning"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.error) {
            example["colors"]["error"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.info) {
            example["colors"]["info"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.text) {
            example["colors"]["text"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.text_dim) {
            example["colors"]["text_dim"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.text_bright) {
            example["colors"]["text_bright"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.keyword) {
            example["colors"]["keyword"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.string) {
            example["colors"]["string"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.comment) {
            example["colors"]["comment"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.function) {
            example["colors"]["function"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.spinner) {
            example["colors"]["spinner"] = serde_json::Value::String(v);
        }
        if let Some(v) = color_to_hex(cur.progress) {
            example["colors"]["progress"] = serde_json::Value::String(v);
        }

        let before_ticket = self.before_ticket.clone();
        let fallback_tx = self.app_event_tx.clone();
        let fallback_ticket = self.before_ticket.clone();
        let completion_tx = progress_tx.clone();
        if thread_spawner::spawn_lightweight("theme-create", move || {
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
                let auth_mgr = code_core::AuthManager::shared_with_mode_and_originator(
                    cfg.code_home.clone(),
                    AuthMode::ApiKey,
                    cfg.responses_originator_header.clone(),
                    cfg.cli_auth_credentials_store_mode,
                );
                let debug_logger = match code_core::debug_logger::DebugLogger::new(false) {
                    Ok(logger) => logger,
                    Err(err) => {
                        tracing::warn!("theme debug logger init failed: {err}");
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
                    effort: cfg.model_reasoning_effort,
                    summary: cfg.model_reasoning_summary,
                    verbosity: cfg.model_text_verbosity,
                    session_id: uuid::Uuid::new_v4(),
                    debug_logger: std::sync::Arc::new(std::sync::Mutex::new(debug_logger)),
                });

                // Prompt with example and detailed field usage to help the model choose appropriate colors
                let developer = format!(
                    "You are designing a TUI color theme for a terminal UI.\n\nOutput: Strict JSON only. Include fields: `name` (string), `is_dark` (boolean), and `colors` (object of hex strings #RRGGBB).\n\nImportant rules:\n- Include EVERY `colors` key below. If you are not changing a value, copy it from the Current example.\n- Ensure strong contrast and readability for text vs background and for dim/bright variants.\n- Favor accessible color contrast (WCAG-ish) where possible.\n\nColor semantics (how the UI uses them):\n- background: main screen background.\n- foreground: primary foreground accents for widgets.\n- text: normal body text; must be readable on background.\n- text_dim: secondary/description text; slightly lower contrast than text.\n- text_bright: headings/emphasis; higher contrast than text.\n- primary: primary action/highlight color for selected items/buttons.\n- secondary: secondary accents (less prominent than primary).\n- border: container borders/dividers; should be visible but subtle against background.\n- border_focused: border when focused/active; slightly stronger than border.\n- selection: background for selected list rows; must contrast with text.\n- cursor: text caret color in input fields; must contrast with background.\n- success/warning/error/info: status badges and notices.\n- keyword/string/comment/function: syntax highlight accents in code blocks.\n- spinner: glyph color for loading animations; should be visible on background.\n- progress: progress-bar foreground color.\n\nCurrent theme example (copy unchanged values from here):\n{example}",
                );
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

                let schema = serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "minLength": 1, "maxLength": 40},
                        "is_dark": {"type": "boolean"},
                        "colors": {
                            "type": "object",
                            "properties": {
                                "primary": {"type": "string"},
                                "secondary": {"type": "string"},
                                "background": {"type": "string"},
                                "foreground": {"type": "string"},
                                "border": {"type": "string"},
                                "border_focused": {"type": "string"},
                                "selection": {"type": "string"},
                                "cursor": {"type": "string"},
                                "success": {"type": "string"},
                                "warning": {"type": "string"},
                                "error": {"type": "string"},
                                "info": {"type": "string"},
                                "text": {"type": "string"},
                                "text_dim": {"type": "string"},
                                "text_bright": {"type": "string"},
                                "keyword": {"type": "string"},
                                "string": {"type": "string"},
                                "comment": {"type": "string"},
                                "function": {"type": "string"},
                                "spinner": {"type": "string"},
                                "progress": {"type": "string"}
                            },
                            "required": [
                                "primary", "secondary", "background", "foreground", "border",
                                "border_focused", "selection", "cursor", "success", "warning",
                                "error", "info", "text", "text_dim", "text_bright", "keyword",
                                "string", "comment", "function", "spinner", "progress"
                            ],
                            "additionalProperties": false
                        }
                    },
                    "required": ["name", "is_dark", "colors"],
                    "additionalProperties": false
                });
                let format = code_core::TextFormat { r#type: "json_schema".to_string(), name: Some("custom_theme".to_string()), strict: Some(true), schema: Some(schema) };

                let mut prompt = code_core::Prompt::default();
                prompt.input = input;
                prompt.store = true;
                prompt.text_format = Some(format);
                prompt.set_log_tag("ui/theme_builder");

                use futures::StreamExt;
                let _ = progress_tx.send(ProgressMsg::ThinkingDelta("(connecting to model)".to_string()));
                let mut stream = match client.stream(&prompt).await {
                    Ok(s) => s,
                    Err(e) => {
                        tx.send_background_before_next_output_with_ticket(
                            &before_ticket,
                            format!("Request error: {e}"),
                        );
                        return;
                    }
                };
                let mut out = String::new();
                // Capture the last transport/stream error so we can surface it to the UI
                let mut last_err: Option<String> = None;
                while let Some(ev) = stream.next().await {
                    match ev {
                        Ok(code_core::ResponseEvent::Created { .. }) => {
                            let _ = progress_tx.send(ProgressMsg::SetStatus("(starting generation)".to_string()));
                        }
                        Ok(code_core::ResponseEvent::ReasoningSummaryDelta { delta, .. }) => {
                            let _ = progress_tx.send(ProgressMsg::ThinkingDelta(delta));
                        }
                        Ok(code_core::ResponseEvent::ReasoningContentDelta { delta, .. }) => {
                            let _ = progress_tx.send(ProgressMsg::ThinkingDelta(delta));
                        }
                        Ok(code_core::ResponseEvent::OutputTextDelta { delta, .. }) => {
                            let _ = progress_tx.send(ProgressMsg::OutputDelta(delta.clone()));
                            out.push_str(&delta);
                        }
                        Ok(code_core::ResponseEvent::OutputItemDone {
                            item: code_protocol::models::ResponseItem::Message { content, .. },
                            ..
                        }) => {
                            for c in content {
                                if let code_protocol::models::ContentItem::OutputText {
                                    text,
                                } = c
                                {
                                    out.push_str(&text);
                                }
                            }
                        }
                        Ok(code_core::ResponseEvent::Completed { .. }) => break,
                        Err(e) => {
                            let msg = e.to_string();
                            let _ = progress_tx.send(ProgressMsg::ThinkingDelta(format!(
                                "(stream error: {msg})"
                            )));
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
                        .map(|e| format!("model stream error: {e}"))
                        .unwrap_or_else(|| "model stream returned no content".to_string());
                    let _ = progress_tx.send(ProgressMsg::CompletedErr {
                        error: err,
                        _raw_snippet: String::new(),
                    });
                    return;
                }
                // Try strict parse first; if that fails, salvage the first JSON object in the text.
                let v: serde_json::Value = match serde_json::from_str(&out) {
                    Ok(v) => v,
                    Err(e) => {
                        // Attempt to extract the first top-level JSON object from the stream text
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
                                        _raw_snippet: out.chars().take(200).collect(),
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
                                _raw_snippet: out.chars().take(200).collect(),
                            });
                            return;
                        }
                    }
                };
                let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("Custom").trim().to_string();
                let is_dark = v.get("is_dark").and_then(serde_json::Value::as_bool);
                let mut colors = code_core::config_types::ThemeColors::default();
                if let Some(map) = v.get("colors").and_then(|x| x.as_object()) {
                    let get = |k: &str| {
                        map.get(k)
                            .and_then(serde_json::Value::as_str)
                            .map(str::trim)
                            .map(str::to_string)
                    };
                    colors.primary = get("primary");
                    colors.secondary = get("secondary");
                    colors.background = get("background");
                    colors.foreground = get("foreground");
                    colors.border = get("border");
                    colors.border_focused = get("border_focused");
                    colors.selection = get("selection");
                    colors.cursor = get("cursor");
                    colors.success = get("success");
                    colors.warning = get("warning");
                    colors.error = get("error");
                    colors.info = get("info");
                    colors.text = get("text");
                    colors.text_dim = get("text_dim");
                    colors.text_bright = get("text_bright");
                    colors.keyword = get("keyword");
                    colors.string = get("string");
                    colors.comment = get("comment");
                    colors.function = get("function");
                    colors.spinner = get("spinner");
                    colors.progress = get("progress");
                }
                let _ = progress_tx.send(ProgressMsg::CompletedThemeOk(Box::new(
                    ThemeGenerationResult {
                        name,
                        colors,
                        is_dark,
                    },
                )));
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
                "Failed to generate theme: background worker unavailable".to_string(),
            );
        }
    }
}
