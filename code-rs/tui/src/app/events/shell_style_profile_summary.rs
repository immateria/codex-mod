pub(super) async fn generate_shell_style_profile_summary(
    config: std::sync::Arc<code_core::config::Config>,
    auth_manager: std::sync::Arc<code_core::AuthManager>,
    style: code_core::config_types::ShellScriptStyle,
    profile: code_core::config_types::ShellStyleProfileConfig,
) -> anyhow::Result<String> {
    use code_core::ResponseEvent;
    use futures::StreamExt;
    use std::sync::Mutex;

    let debug_logger = std::sync::Arc::new(Mutex::new(code_core::debug_logger::DebugLogger::new(
        false,
    )?));
    let session_id = uuid::Uuid::new_v4();

    let client = code_core::ModelClient::new(code_core::ModelClientInit {
        config: config.clone(),
        auth_manager: Some(auth_manager),
        otel_event_manager: None,
        provider: config.model_provider.clone(),
        effort: code_core::config_types::ReasoningEffort::Minimal,
        summary: code_core::config_types::ReasoningSummary::None,
        verbosity: code_core::config_types::TextVerbosity::Low,
        session_id,
        debug_logger,
    });

    let mut prompt = code_core::Prompt::default();
    prompt.include_additional_instructions = false;
    prompt.base_instructions_override = Some(
        "You write concise, user-facing summaries for configuration profiles. Output 1-2 sentences. No markdown."
            .to_string(),
    );

    let profile_json = serde_json::to_string_pretty(&profile).unwrap_or_else(|_| format!("{profile:?}"));
    let input_text = format!(
        "Write a short summary for this shell style profile.\nStyle: {style}\n\nProfile JSON:\n{profile_json}\n\nReturn only the summary text."
    );
    prompt.input.push(code_core::ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![code_core::ContentItem::InputText { text: input_text }],
        end_turn: None,
        phase: None,
    });
    prompt.set_log_tag("shell-style-profile-summary");

    let mut stream = client.stream(&prompt).await?;
    let mut output = String::new();
    while let Some(event) = stream.next().await {
        match event? {
            ResponseEvent::OutputTextDelta { delta, .. } => output.push_str(&delta),
            ResponseEvent::Completed { .. } => break,
            _ => {}
        }
    }

    let summary = output.trim().replace(['\r', '\n'], " ");
    if summary.is_empty() {
        anyhow::bail!("empty summary generated");
    }

    Ok(summary)
}
