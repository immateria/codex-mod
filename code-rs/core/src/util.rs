use std::time::Duration;

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Timelike, Utc};
use rand::Rng;
use reqwest;

// ── MIME type constants ──────────────────────────────────────────────
pub const MIME_OCTET_STREAM: &str = "application/octet-stream";
pub const MIME_IMAGE_PNG: &str = "image/png";

/// Maximum number of characters to keep when truncating a text snippet
/// for display or logging (e.g. browser inspect, auto-drive goal, scratchpad).
pub const MAX_SNIPPET_CHARS: usize = 800;

const INITIAL_DELAY_MS: u64 = 200;
const BACKOFF_FACTOR: f64 = 2.0;
/// Cap the exponent to avoid f64 overflow when `attempt` is very large.
const MAX_BACKOFF_EXPONENT: u32 = 31;

pub(crate) fn backoff(attempt: u64) -> Duration {
    let exp_power = (attempt.saturating_sub(1)).min(MAX_BACKOFF_EXPONENT as u64) as u32;
    let exp = BACKOFF_FACTOR.powi(exp_power as i32);
    let base_f = (INITIAL_DELAY_MS as f64 * exp).clamp(0.0, u64::MAX as f64);
    let jitter = rand::rng().random_range(0.9..1.1);
    let ms = (base_f * jitter).clamp(0.0, u64::MAX as f64) as u64;
    Duration::from_millis(ms)
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

pub fn strip_bash_lc_and_escape(command: &[String]) -> String {
    code_shell_command::strip_bash_lc_and_escape(command)
}

pub(crate) fn is_shell_like_executable(token: &str) -> bool {
    code_shell_command::is_shell_like_executable(token)
}

/// Serialize a [`reqwest::header::HeaderMap`] into a deterministic JSON object.
pub(crate) fn header_map_to_json(headers: &reqwest::header::HeaderMap) -> serde_json::Value {
    let mut ordered: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (name, value) in headers {
        ordered
            .entry(name.as_str().to_owned())
            .or_default()
            .push(value.to_str().unwrap_or_default().to_owned());
    }
    serde_json::to_value(ordered).unwrap_or(serde_json::Value::Null)
}

/// Canonicalize `path`, falling back to the original if resolution fails
/// (e.g. the path doesn't exist yet or permissions prevent stat).
pub(crate) fn canonicalize_or_original(path: &std::path::Path) -> std::path::PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Truncate `input` to the last valid UTF-8 char boundary at or before
/// `max_len` bytes, returning a borrowed slice.
pub(crate) fn truncate_on_char_boundary(input: &str, max_len: usize) -> &str {
    if input.len() <= max_len {
        return input;
    }
    let mut end = max_len;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    &input[..end]
}

/// Truncate `input` to at most `max_bytes` bytes on a UTF-8 boundary,
/// returning an owned `String`.
pub(crate) fn truncate_utf8_prefix_by_bytes(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_owned();
    }
    if max_bytes == 0 {
        return String::new();
    }
    truncate_on_char_boundary(input, max_bytes).to_owned()
}

/// Check whether a string value is "truthy" (case-insensitive).
///
/// Accepts: `"1"`, `"true"`, `"yes"`, `"on"`.
pub fn is_truthy(value: &str) -> bool {
    value.eq_ignore_ascii_case("1")
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
}

/// Check whether an environment variable is set to a truthy value.
pub(crate) fn is_env_truthy(var: &str) -> bool {
    std::env::var(var)
        .ok()
        .is_some_and(|v| is_truthy(&v))
}

// ── DateTime truncation helpers ──────────────────────────────────────

/// Truncate a UTC timestamp to the start of its hour.
pub fn truncate_to_hour(ts: DateTime<Utc>) -> DateTime<Utc> {
    let naive = ts.naive_utc();
    let trimmed = naive
        .with_minute(0)
        .and_then(|dt| dt.with_second(0))
        .and_then(|dt| dt.with_nanosecond(0))
        .unwrap_or(naive);
    Utc.from_utc_datetime(&trimmed)
}

/// Truncate a UTC timestamp to the start of its day (midnight).
pub(crate) fn truncate_to_day(ts: DateTime<Utc>) -> DateTime<Utc> {
    let date = ts.date_naive();
    let start = date.and_hms_opt(0, 0, 0).unwrap_or_else(|| ts.naive_utc());
    Utc.from_utc_datetime(&start)
}

/// Truncate a UTC timestamp to the start of its month.
pub(crate) fn truncate_to_month(ts: DateTime<Utc>) -> DateTime<Utc> {
    let date = ts.date_naive();
    let month_start = NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
        .and_then(|month| month.and_hms_opt(0, 0, 0))
        .unwrap_or_else(|| ts.naive_utc());
    Utc.from_utc_datetime(&month_start)
}
