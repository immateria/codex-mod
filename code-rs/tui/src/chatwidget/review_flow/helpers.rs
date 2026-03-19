use super::super::*;

impl ChatWidget<'_> {
    pub(in crate::chatwidget) fn worktree_has_uncommitted_changes(&self) -> Option<bool> {
        let output = Command::new("git")
            .current_dir(&self.config.cwd)
            .args(["status", "--short"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        Some(!stdout.trim().is_empty())
    }

    pub(in crate::chatwidget) fn current_head_commit_sha(&self) -> Option<String> {
        let output = Command::new("git")
            .current_dir(&self.config.cwd)
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            None
        } else {
            Some(stdout)
        }
    }

    pub(in crate::chatwidget) fn commit_subject_for(&self, commit: &str) -> Option<String> {
        let output = Command::new("git")
            .current_dir(&self.config.cwd)
            .args(["show", "-s", "--format=%s", commit])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            None
        } else {
            Some(stdout)
        }
    }

    pub(in crate::chatwidget) fn strip_context_sections(text: &str) -> String {
        // Remove any <context>...</context> blocks. If a closing tag is missing,
        // drop everything from the opening tag to the end of the string so we
        // never leak a stray <context> marker back into the next prompt.
        const START: &str = "<context"; // allow attributes or whitespace before '>'
        const END: &str = "</context>";

        let lower = text.to_ascii_lowercase();
        let mut cleaned = String::with_capacity(text.len());
        let mut cursor: usize = 0;

        while let Some(start_rel) = lower[cursor..].find(START) {
            let start = cursor + start_rel;
            cleaned.push_str(&text[cursor..start]);

            // Advance past the opening tag terminator '>' if present; otherwise
            // treat the rest of the string as part of the unclosed context block.
            let after_start = match text[start..].find('>') {
                Some(off) => start + off + 1,
                None => return cleaned, // Unclosed start tag: drop the remainder
            };

            // Look for the matching closing tag. If not found, drop the tail.
            if let Some(end_rel) = lower[after_start..].find(END) {
                let end = after_start + end_rel + END.len();
                cursor = end;
            } else {
                return cleaned;
            }
        }

        // Append any trailing text after the last removed block.
        cleaned.push_str(&text[cursor..]);

        // Clean up any stray closing tags that had no opener.
        if cleaned.contains(END) {
            cleaned = cleaned.replace(END, "");
        }

        cleaned
    }

    pub(in crate::chatwidget) fn turn_context_block(&self) -> Option<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut any = false;
        lines.push("<context>".to_string());
        lines.push("Below are the most recent messages related to this code change.".to_string());
        if let Some(user) = self
            .last_user_message
            .as_ref()
            .map(|msg| Self::strip_context_sections(msg))
            .map(|msg| msg.trim().to_string())
            .filter(|msg| !msg.is_empty())
        {
            any = true;
            lines.push(format!("<user>{user}</user>"));
        }
        if let Some(dev) = self
            .last_developer_message
            .as_ref()
            .map(|msg| Self::strip_context_sections(msg))
            .map(|msg| msg.trim().to_string())
            .filter(|msg| !msg.is_empty())
        {
            any = true;
            lines.push(format!("<developer>{dev}</developer>"));
        }
        if let Some(assistant) = self
            .last_assistant_message
            .as_ref()
            .map(|msg| Self::strip_context_sections(msg))
            .map(|msg| msg.trim().to_string())
            .filter(|msg| !msg.is_empty())
        {
            any = true;
            lines.push(format!("<assistant>{assistant}</assistant>"));
        }
        lines.push("</context>".to_string());

        if any {
            Some(lines.join("\n"))
        } else {
            None
        }
    }

}
