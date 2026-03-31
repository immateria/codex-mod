use super::*;

impl ChatComposer {
    pub fn update_status_message(&mut self, message: String) {
        self.show_auto_drive_goal_title =
            message.to_ascii_lowercase().contains("auto drive goal");
        self.status_message = Self::map_status_message(&message);
    }

    pub fn status_message(&self) -> Option<&str> {
        let trimmed = self.status_message.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    // Map technical status messages to user-friendly ones
    pub(crate) fn map_status_message(technical_message: &str) -> String {
        if technical_message.trim().is_empty() {
            return String::new();
        }

        let lower = technical_message.to_ascii_lowercase();

        // Auto Review: preserve the phase text so the footer shows
        // "Auto Review: Reviewing/Resolving" instead of a generic label.
        if lower.contains("auto review") {
            let cleaned = technical_message.trim();
            if cleaned.is_empty() {
                "Auto Review".to_string()
            } else {
                cleaned.to_string()
            }
        } else if lower.contains("auto drive goal") {
            "Auto Drive Goal".to_string()
        } else if lower.contains("auto drive") {
            "Auto Drive".to_string()
        }
        // Thinking/reasoning patterns
        else if lower.contains("reasoning")
            || lower.contains("thinking")
            || lower.contains("planning")
            || lower.contains("waiting for model")
        {
            "Thinking".to_string()
        }
        // Tool/command execution patterns
        else if lower.contains("tool")
            || lower.contains("command")
            || lower.contains("running")
            || lower.contains("bash")
            || lower.contains("shell")
        {
            "Using tools".to_string()
        }
        // Browser activity
        else if lower.contains("browser")
            || lower.contains("chrome")
            || lower.contains("cdp")
            || lower.contains("navigate to")
            || lower.contains("open url")
            || lower.contains("load url")
            || lower.contains("screenshot")
        {
            "Browsing".to_string()
        }
        // Multi-agent orchestration
        else if lower.contains("agent")
            || lower.contains("orchestrating")
            || lower.contains("coordinating")
        {
            "Agents".to_string()
        }
        // Response generation patterns
        else if lower.contains("generating")
            || lower.contains("responding")
            || lower.contains("streaming")
            || lower.contains("writing response")
            || lower.contains("assistant")
            || lower.contains("chat completions")
            || lower.contains("completion")
        {
            "Responding".to_string()
        }
        // Initial connection / handshake
        else if lower.contains("connecting") {
            "Connecting".to_string()
        }
        // Transient network/stream retry patterns → keep spinner visible with a
        // clear reconnecting message so the user knows we are still working.
        else if lower.contains("retrying")
            || lower.contains("reconnecting")
            || lower.contains("disconnected")
            || lower.contains("stream error")
            || lower.contains("stream closed")
            || lower.contains("timeout")
            || lower.contains("transport")
            || lower.contains("network")
            || lower.contains("connection")
        {
            "Reconnecting".to_string()
        }
        // File/code editing patterns
        else if lower.contains("editing")
            || lower.contains("writing")
            || lower.contains("modifying")
            || lower.contains("creating file")
            || lower.contains("updating")
            || lower.contains("patch")
        {
            "Coding".to_string()
        }
        // Catch some common technical terms
        else if lower.contains("processing") || lower.contains("analyzing") {
            "Thinking".to_string()
        } else if lower == "search" || lower.contains("searching") {
            "Searching".to_string()
        } else if lower.contains("reading") {
            "Reading".to_string()
        } else {
            // Default fallback - use "working" for unknown status
            "Working".to_string()
        }
    }
}
