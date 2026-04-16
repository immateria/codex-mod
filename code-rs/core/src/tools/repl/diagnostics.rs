use super::types::KernelDebugSnapshot;
use std::collections::VecDeque;

/// Separator used between stderr tail lines when formatting.
pub(super) const STDERR_TAIL_SEPARATOR: &str = " | ";

/// Maximum recent-stderr lines kept for diagnostics on kernel failure.
pub(super) const STDERR_TAIL_LINE_LIMIT: usize = 20;

/// Per-line byte cap in the stderr tail ring buffer.
pub(super) const STDERR_TAIL_LINE_MAX_BYTES: usize = 512;

/// Total byte cap across all lines in the stderr tail.
pub(super) const STDERR_TAIL_MAX_BYTES: usize = 4_096;

/// Byte budget for stderr context sent to the model in error diagnostics.
const MODEL_DIAG_STDERR_MAX_BYTES: usize = 1_024;

/// Byte budget for the error string sent to the model in error diagnostics.
const MODEL_DIAG_ERROR_MAX_BYTES: usize = 256;

/// Max exec IDs to include in unexpected-close log messages.
pub(super) const EXEC_ID_LOG_LIMIT: usize = 8;

pub(super) fn format_exit_status(status: std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("code={code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("signal={signal}");
        }
    }
    "unknown".to_string()
}

pub(super) fn format_stderr_tail(lines: &VecDeque<String>) -> String {
    if lines.is_empty() {
        return "<empty>".to_string();
    }
    let mut out = lines[0].clone();
    for line in lines.iter().skip(1) {
        out.push_str(STDERR_TAIL_SEPARATOR);
        out.push_str(line);
    }
    out
}

pub(super) fn truncate_utf8_prefix_by_bytes(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }
    if max_bytes == 0 {
        return String::new();
    }
    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    input[..end].to_string()
}

pub(super) fn stderr_tail_formatted_bytes(lines: &VecDeque<String>) -> usize {
    if lines.is_empty() {
        return 0;
    }
    let payload_bytes: usize = lines.iter().map(String::len).sum();
    let separator_bytes = STDERR_TAIL_SEPARATOR.len() * (lines.len() - 1);
    payload_bytes + separator_bytes
}

pub(super) fn stderr_tail_bytes_with_candidate(lines: &VecDeque<String>, line: &str) -> usize {
    if lines.is_empty() {
        return line.len();
    }
    stderr_tail_formatted_bytes(lines) + STDERR_TAIL_SEPARATOR.len() + line.len()
}

pub(super) fn push_stderr_tail_line(lines: &mut VecDeque<String>, line: &str) -> String {
    let max_line_bytes = STDERR_TAIL_LINE_MAX_BYTES.min(STDERR_TAIL_MAX_BYTES);
    let bounded_line = truncate_utf8_prefix_by_bytes(line, max_line_bytes);
    if bounded_line.is_empty() {
        return bounded_line;
    }

    while !lines.is_empty()
        && (lines.len() >= STDERR_TAIL_LINE_LIMIT
            || stderr_tail_bytes_with_candidate(lines, &bounded_line) > STDERR_TAIL_MAX_BYTES)
    {
        lines.pop_front();
    }

    lines.push_back(bounded_line.clone());
    bounded_line
}

pub(super) fn is_kernel_status_exited(status: &str) -> bool {
    status.starts_with("exited(")
}

pub(super) fn should_include_diagnostics_for_write_error(
    err_message: &str,
    snapshot: &KernelDebugSnapshot,
) -> bool {
    is_kernel_status_exited(&snapshot.status)
        || err_message.to_ascii_lowercase().contains("broken pipe")
}

pub(super) fn format_model_kernel_failure_details(
    reason: &str,
    stream_error: Option<&str>,
    snapshot: &KernelDebugSnapshot,
) -> String {
    let payload = serde_json::json!({
        "reason": reason,
        "stream_error": stream_error
            .map(|err| truncate_utf8_prefix_by_bytes(err, MODEL_DIAG_ERROR_MAX_BYTES)),
        "kernel_pid": snapshot.pid,
        "kernel_status": snapshot.status,
        "kernel_stderr_tail": truncate_utf8_prefix_by_bytes(
            &snapshot.stderr_tail,
            MODEL_DIAG_STDERR_MAX_BYTES,
        ),
    });
    serde_json::to_string(&payload)
        .unwrap_or_else(|err| format!(r#"{{"reason":"serialization_error","error":"{err}"}}"#))
}

pub(super) fn with_model_failure_message(
    base_message: &str,
    reason: &str,
    stream_error: Option<&str>,
    snapshot: &KernelDebugSnapshot,
) -> String {
    format!(
        "{base_message}\n\nrepl diagnostics: {}",
        format_model_kernel_failure_details(reason, stream_error, snapshot)
    )
}
