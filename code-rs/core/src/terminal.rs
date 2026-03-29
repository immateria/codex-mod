pub fn user_agent() -> String {
    // `codex-terminal-detection` internally caches the detection result.
    code_terminal_detection::user_agent()
}
