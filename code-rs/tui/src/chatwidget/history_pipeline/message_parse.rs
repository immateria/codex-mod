use super::*;

impl ChatWidget<'_> {
    pub(in super::super) fn persist_user_image_if_needed(
        &self,
        path: &std::path::Path,
    ) -> Option<PathBuf> {
        if !path.exists() || !path.is_file() {
            return None;
        }

        let temp_dir = std::env::temp_dir();
        let path_lossy = path.to_string_lossy();
        let looks_ephemeral = path.starts_with(&temp_dir)
            || path_lossy.contains("/TemporaryItems/")
            || path_lossy.contains("\\TemporaryItems\\");
        if !looks_ephemeral {
            return None;
        }

        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("png")
            .to_string();

        let mut dir = self
            .config
            .code_home
            .join("working")
            .join("_pasted_images");
        if let Some(session_id) = self.session_id {
            dir = dir.join(session_id.to_string());
        }

        if let Err(err) = std::fs::create_dir_all(&dir) {
            tracing::warn!(
                "failed to create pasted image dir {}: {}",
                dir.display(),
                err
            );
            return None;
        }

        let file_name = format!("dropped-{}.{}", Uuid::new_v4(), ext);
        let dest = dir.join(file_name);

        match std::fs::copy(path, &dest) {
            Ok(_) => Some(dest),
            Err(err) => {
                tracing::warn!(
                    "failed to persist dropped image {} -> {}: {}",
                    path.display(),
                    dest.display(),
                    err
                );
                None
            }
        }
    }

    pub(in super::super) fn parse_message_with_images(&mut self, text: String) -> UserMessage {
        let Some(mut ordered_items) = self.collect_placeholder_image_items(&text) else {
            return UserMessage {
                display_text: text.clone(),
                ordered_items: vec![InputItem::Text { text }],
                suppress_persistence: false,
            };
        };

        self.append_direct_image_paths_to_items(&text, &mut ordered_items);
        let display_text = Self::normalize_display_text_for_history(&text);

        UserMessage {
            display_text,
            ordered_items,
            suppress_persistence: false,
        }
    }

    fn collect_placeholder_image_items(&mut self, text: &str) -> Option<Vec<InputItem>> {
        let placeholder_regex = regex_lite::Regex::new(r"\[image: [^\]]+\]").ok()?;
        let mut ordered_items: Vec<InputItem> = Vec::new();
        let mut cursor = 0usize;

        for mat in placeholder_regex.find_iter(text) {
            if mat.start() > cursor {
                let chunk = &text[cursor..mat.start()];
                if !chunk.trim().is_empty() {
                    ordered_items.push(InputItem::Text {
                        text: chunk.to_string(),
                    });
                }
            }

            let placeholder = mat.as_str();
            if let Some(path) = self.pending_images.remove(placeholder) {
                if path.exists() && path.is_file() {
                    // Emit marker + image so the model keeps user-authored placement.
                    ordered_items.push(InputItem::Text {
                        text: placeholder.to_string(),
                    });
                    ordered_items.push(InputItem::LocalImage { path });
                } else {
                    tracing::warn!(
                        "pending image placeholder {} resolved to missing path {}",
                        placeholder,
                        path.display()
                    );
                    self.push_background_tail(format!(
                        "Dropped image file went missing; not attaching ({})",
                        path.display()
                    ));
                    ordered_items.push(InputItem::Text {
                        text: placeholder.to_string(),
                    });
                }
            } else {
                // Unknown placeholder: preserve verbatim.
                ordered_items.push(InputItem::Text {
                    text: placeholder.to_string(),
                });
            }
            cursor = mat.end();
        }

        if cursor < text.len() {
            let chunk = &text[cursor..];
            if !chunk.trim().is_empty() {
                ordered_items.push(InputItem::Text {
                    text: chunk.to_string(),
                });
            }
        }

        Some(ordered_items)
    }

    fn append_direct_image_paths_to_items(&self, text: &str, ordered_items: &mut Vec<InputItem>) {
        use std::path::Path;

        const IMAGE_EXTENSIONS: &[&str] = &[
            ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".svg", ".ico", ".tiff", ".tif",
        ];

        // Preserve existing behavior: direct typed paths are appended to avoid
        // re-ordering user text while still attaching image content.
        for word in text.split_whitespace() {
            if word.starts_with("[image:") {
                continue;
            }

            let lower = word.to_lowercase();
            let is_image_path = IMAGE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext));
            if !is_image_path {
                continue;
            }

            let path = Path::new(word);
            if !path.exists() {
                continue;
            }

            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
            let persisted_path = self
                .persist_user_image_if_needed(path)
                .unwrap_or_else(|| path.to_path_buf());
            ordered_items.push(InputItem::Text {
                text: format!("[image: {filename}]"),
            });
            ordered_items.push(InputItem::LocalImage {
                path: persisted_path,
            });
        }
    }

    fn normalize_display_text_for_history(text: &str) -> String {
        let normalized = text.replace("\r\n", "\n");
        let lines: Vec<String> = normalized
            .lines()
            .map(|line| line.trim_end().to_string())
            .collect();

        let start = lines
            .iter()
            .position(|line| !line.trim().is_empty())
            .unwrap_or(lines.len());
        let end = lines
            .iter()
            .rposition(|line| !line.trim().is_empty())
            .map(|idx| idx + 1)
            .unwrap_or(start);

        lines[start..end].join("\n")
    }
}
