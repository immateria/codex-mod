use tracing::debug;
use tracing::warn;

const HANDLER_ERROR_LIMIT: u32 = 3;

pub(in super::super) fn should_restart_handler(consecutive_errors: u32) -> bool {
    consecutive_errors >= HANDLER_ERROR_LIMIT
}

fn should_ignore_handler_error(message_lower: &str) -> bool {
    // Chromiumoxide uses oneshot channels internally for CDP request/response.
    // When we cancel or time out an in-flight request (dropping its future), the
    // CDP runtime may surface this as an error string containing "oneshot".
    // These should not be treated as connection failures.
    if message_lower.contains("oneshot") {
        return true;
    }

    // These can happen for individual targets/tabs while the overall CDP
    // connection is still healthy (e.g. tabs closing, navigations, reloads).
    const TRANSIENT_SUBSTRINGS: &[&str] = &[
        "no such session",
        "session closed",
        "invalid session",
        "target closed",
        "target crashed",
        "context destroyed",
        "execution context was destroyed",
        "cannot find context",
    ];

    TRANSIENT_SUBSTRINGS
        .iter()
        .any(|needle| message_lower.contains(needle))
}

pub(in super::super) fn should_stop_handler<E: std::fmt::Display>(
    label: &'static str,
    result: std::result::Result<(), E>,
    consecutive_errors: &mut u32,
) -> bool {
    match result {
        Ok(()) => {
            *consecutive_errors = 0;
            false
        }
        Err(err) => {
            let message = err.to_string();
            let message_lower = message.to_ascii_lowercase();
            if should_ignore_handler_error(&message_lower) {
                *consecutive_errors = 0;
                debug!("{label} event handler error ignored: {message}");
                return false;
            }
            *consecutive_errors = consecutive_errors.saturating_add(1);
            let count = *consecutive_errors;
            if count <= HANDLER_ERROR_LIMIT {
                debug!("{label} event handler error: {err} (count: {count})");
            }
            if should_restart_handler(count) {
                warn!("{label} event handler errors exceeded limit; restarting browser connection");
                return true;
            }
            false
        }
    }
}
