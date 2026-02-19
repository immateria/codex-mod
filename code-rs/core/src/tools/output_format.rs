use crate::exec::EXEC_CAPTURE_MAX_BYTES;
use crate::exec::ExecToolCallOutput;

pub(crate) fn format_exec_output_str(exec_output: &ExecToolCallOutput) -> String {
    let ExecToolCallOutput {
        aggregated_output,
        duration,
        timed_out,
        ..
    } = exec_output;

    // Always use the aggregated (stdout + stderr interleaved) stream so the
    // model sees the full build log regardless of which stream a tool used.
    let mut formatted_output = aggregated_output.text.clone();
    if let Some(truncated_before_bytes) = aggregated_output.truncated_before_bytes {
        let note = format!(
            "… clipped {} from the start of command output (showing last {}).\n\n",
            crate::util::format_bytes(truncated_before_bytes),
            crate::util::format_bytes(EXEC_CAPTURE_MAX_BYTES),
        );
        formatted_output = format!("{note}{formatted_output}");
    }

    if *timed_out {
        let timeout_ms = duration.as_millis();
        formatted_output =
            format!("command timed out after {timeout_ms} milliseconds\n{formatted_output}");
    }
    if let Some(truncated_after_lines) = aggregated_output.truncated_after_lines {
        formatted_output.push_str(&format!(
            "\n\n[Output truncated after {truncated_after_lines} lines: too many lines or bytes.]",
        ));
    }

    formatted_output
}

pub(crate) fn format_exec_output_payload(exec_output: &ExecToolCallOutput, output: &str) -> String {
    let duration_seconds = ((exec_output.duration.as_secs_f32()) * 10.0).round() / 10.0;
    serde_json::json!({
        "output": output,
        "metadata": {
            "exit_code": exec_output.exit_code,
            "duration_seconds": duration_seconds,
        },
    })
    .to_string()
}
