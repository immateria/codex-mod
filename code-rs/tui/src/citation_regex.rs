use regex_lite::Regex;

// This is defined in its own file so the compiled regex is initialized once
// and shared across render code.
lazy_static::lazy_static! {
    /// Regular expression that matches Codex-style source file citations such as:
    ///
    /// ```text
    /// 【F:src/main.rs†L10-L20】
    /// ```
    ///
    /// Capture groups:
    /// 1. file path (anything except the dagger `†` symbol)
    /// 2. start line number (digits)
    /// 3. optional end line (digits or `?`)
    pub(crate) static ref CITATION_REGEX: Regex = Regex::new(
        r"【F:([^†]+)†L(\d+)(?:-L(\d+|\?))?】"
    ).unwrap_or_else(|err| panic!("failed to compile citation regex: {err}"));
}
