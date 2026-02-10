use super::*;

const STREAM_PROGRESS_INTERVAL: StdDuration = StdDuration::from_secs(2);
const STREAM_PROGRESS_BYTES: usize = 2 * 1024;

pub(super) async fn stream_child_output(
    agent_id: &str,
    mut child: tokio::process::Child,
) -> Result<(std::process::ExitStatus, String, String), String> {
    let agent_id_owned = agent_id.to_string();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = stop_flag.clone();
    let heartbeat = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(TokioDuration::from_secs(30));
        loop {
            ticker.tick().await;
            if stop_clone.load(Ordering::Relaxed) {
                break;
            }
            AgentManager::touch_agent(&agent_id_owned).await;
        }
    });

    let stdout_task = child.stdout.take().map(|stdout| {
        let agent = agent_id.to_string();
        tokio::spawn(async move { stream_reader_to_progress(agent, "stdout", stdout).await })
    });

    let stderr_task = child.stderr.take().map(|stderr| {
        let agent = agent_id.to_string();
        tokio::spawn(async move { stream_reader_to_progress(agent, "stderr", stderr).await })
    });

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for agent process: {e}"))?;

    let stdout_buf = match stdout_task {
        Some(handle) => handle
            .await
            .map_err(|e| format!("Failed to read agent stdout: {e}"))?,
        None => String::new(),
    };

    let stderr_buf = match stderr_task {
        Some(handle) => handle
            .await
            .map_err(|e| format!("Failed to read agent stderr: {e}"))?,
        None => String::new(),
    };

    stop_flag.store(true, Ordering::Relaxed);
    heartbeat.abort();

    Ok((status, stdout_buf, stderr_buf))
}

async fn stream_reader_to_progress<R>(agent_id: String, label: &str, reader: R) -> String
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    let mut full = String::new();
    let mut chunk = String::new();
    let mut last_flush = Instant::now();

    while let Ok(Some(line)) = lines.next_line().await {
        let clean = line.trim_end_matches('\r');
        full.push_str(clean);
        full.push('\n');
        chunk.push_str(clean);
        chunk.push('\n');

        if chunk.len() >= STREAM_PROGRESS_BYTES || last_flush.elapsed() >= STREAM_PROGRESS_INTERVAL {
            flush_progress(&agent_id, label, &mut chunk).await;
            last_flush = Instant::now();
        }
    }

    if !chunk.is_empty() {
        flush_progress(&agent_id, label, &mut chunk).await;
    }

    full
}

async fn flush_progress(agent_id: &str, label: &str, chunk: &mut String) {
    let message = format!("[{label}] {}", chunk.trim_end());
    let mut mgr = AGENT_MANAGER.write().await;
    mgr.add_progress(agent_id, message).await;
    chunk.clear();
}
