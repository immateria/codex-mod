use std::time::Duration;

use rand::Rng;
use reqwest;

const INITIAL_DELAY_MS: u64 = 200;
const BACKOFF_FACTOR: f64 = 2.0;

pub(crate) fn backoff(attempt: u64) -> Duration {
    let exp = BACKOFF_FACTOR.powi(attempt.saturating_sub(1) as i32);
    let base = (INITIAL_DELAY_MS as f64 * exp) as u64;
    let jitter = rand::rng().random_range(0.9..1.1);
    Duration::from_millis((base as f64 * jitter) as u64)
}

/// Format byte counts with binary units (`KiB`, `MiB`, `GiB`).
pub fn format_bytes(bytes: usize) -> String {
    const KIB: usize = 1024;
    const MIB: usize = KIB * 1024;
    const GIB: usize = MIB * 1024;

    if bytes >= GIB {
        let gib = bytes as f64 / GIB as f64;
        format!("{gib:.1} GiB")
    } else if bytes >= MIB {
        let mib = bytes as f64 / MIB as f64;
        format!("{mib:.1} MiB")
    } else if bytes >= KIB {
        let kib = bytes as f64 / KIB as f64;
        format!("{kib:.1} KiB")
    } else {
        format!("{bytes} B")
    }
}

/// Blocks until the given endpoint responds, pausing between attempts with
/// exponential backoff (capped). Used to pause retries while the user is
/// offline so we resume immediately once connectivity returns.
pub(crate) async fn wait_for_connectivity(probe_url: &str) {
    // Cap individual waits to avoid very long sleeps while still backing off.
    const MAX_DELAY: Duration = Duration::from_secs(30);
    let client = reqwest::Client::new();
    let mut attempt: u64 = 1;
    loop {
        // Treat any HTTP response as proof that DNS + TLS + routing are back.
        // Servers like api.openai.com respond 4xx/421 to bare HEADs, so do
        // not gate on status here.
        if client.head(probe_url).send().await.is_ok() {
            return;
        }

        let delay = backoff(attempt).min(MAX_DELAY);
        attempt = attempt.saturating_add(1);
        tokio::time::sleep(delay).await;
    }
}

pub fn escape_command(command: &[String]) -> String {
    code_shell_command::escape_command(command)
}

pub fn strip_bash_lc_and_escape(command: &[String]) -> String {
    code_shell_command::strip_bash_lc_and_escape(command)
}

pub(crate) fn is_shell_like_executable(token: &str) -> bool {
    code_shell_command::is_shell_like_executable(token)
}
