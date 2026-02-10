use super::*;

const AGENT_SMOKE_TEST_PROMPT: &str = "Reply only with the string \"ok\". Do not include any other words.";
const AGENT_SMOKE_TEST_EXPECTED: &str = "ok";
const AGENT_SMOKE_TEST_TIMEOUT: TokioDuration = TokioDuration::from_secs(20);

fn should_validate_in_read_only(_cfg: &AgentConfig) -> bool { true }

async fn run_agent_smoke_test(cfg: AgentConfig) -> Result<String, String> {
    let model_name = cfg.name.clone();
    let read_only = should_validate_in_read_only(&cfg);
    let mut task = tokio::spawn(async move {
        execute_model_with_permissions(ExecuteModelRequest {
            agent_id: "agent-smoke-test",
            model: &model_name,
            prompt: AGENT_SMOKE_TEST_PROMPT,
            read_only,
            working_dir: None,
            config: Some(cfg),
            reasoning_effort: code_protocol::config_types::ReasoningEffort::High,
            review_output_json_path: None,
            source_kind: None,
            log_tag: None,
        })
        .await
    });
    let timer = tokio::time::sleep(AGENT_SMOKE_TEST_TIMEOUT);
    tokio::pin!(timer);
    tokio::select! {
        res = &mut task => {
            res.map_err(|e| format!("agent validation task failed: {e}"))?
        }
        _ = timer.as_mut() => {
            task.abort();
            let _ = task.await;
            Err(format!(
                "agent validation timed out after {}s",
                AGENT_SMOKE_TEST_TIMEOUT.as_secs()
            ))
        }
    }
}

fn summarize_agent_output(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return "<empty response>".to_string();
    }
    const MAX_LEN: usize = 240;
    if trimmed.len() <= MAX_LEN {
        trimmed.to_string()
    } else {
        let mut cutoff = MAX_LEN.min(trimmed.len());
        while cutoff > 0 && !trimmed.is_char_boundary(cutoff) {
            cutoff -= 1;
        }
        if cutoff == 0 {
            // Fallback: take first char to avoid empty slice
            let mut chars = trimmed.chars();
            if let Some(first) = chars.next() {
                format!("{first}…")
            } else {
                "…".to_string()
            }
        } else {
            format!("{}…", &trimmed[..cutoff])
        }
    }
}

pub async fn smoke_test_agent(cfg: AgentConfig) -> Result<(), String> {
    let output = run_agent_smoke_test(cfg).await?;
    let normalized = output.trim().to_ascii_lowercase();
    if normalized == AGENT_SMOKE_TEST_EXPECTED {
        Ok(())
    } else {
        Err(format!(
            "agent response missing \"ok\": {}",
            summarize_agent_output(&output)
        ))
    }
}

fn run_smoke_test_with_new_runtime(cfg: AgentConfig) -> Result<(), String> {
    TokioRuntimeBuilder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to build validation runtime: {e}"))?
        .block_on(smoke_test_agent(cfg))
}

pub fn smoke_test_agent_blocking(cfg: AgentConfig) -> Result<(), String> {
    if tokio::runtime::Handle::try_current().is_ok() {
        thread::Builder::new()
            .name("agent-smoke-test".into())
            .spawn(move || run_smoke_test_with_new_runtime(cfg))
            .map_err(|e| format!("failed to spawn agent validation thread: {e}"))?
            .join()
            .map_err(|_| "agent validation thread panicked".to_string())?
    } else {
        run_smoke_test_with_new_runtime(cfg)
    }
}
