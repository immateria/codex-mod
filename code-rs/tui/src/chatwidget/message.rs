//! Message composition helpers and types for the chat widget.

use code_core::protocol::InputItem;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub(crate) struct UserMessage {
    /// What to show in the chat history (keeps placeholders like "[image: name.png]")
    pub(crate) display_text: String,
    /// Items to send to the core/model in the correct order, with inline
    /// markers preceding images so the LLM knows placement.
    pub(crate) ordered_items: Vec<InputItem>,
    /// Skip adding this message to the persisted history when true.
    pub(crate) suppress_persistence: bool,
}

impl From<String> for UserMessage {
    fn from(text: String) -> Self {
        let has_content = !text.trim().is_empty();
        let ordered = if has_content {
            vec![InputItem::Text { text: text.clone() }]
        } else {
            Vec::new()
        };
        Self {
            display_text: text,
            ordered_items: ordered,
            suppress_persistence: false,
        }
    }
}

pub(crate) fn create_initial_user_message(text: String, image_paths: Vec<PathBuf>) -> Option<UserMessage> {
    if text.is_empty() && image_paths.is_empty() {
        None
    } else {
        let has_text = !text.trim().is_empty();
        let mut ordered: Vec<InputItem> = Vec::with_capacity(
            usize::from(has_text) + image_paths.len() * 2,
        );
        if has_text {
            ordered.push(InputItem::Text { text: text.clone() });
        }
        for path in image_paths {
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
            ordered.push(InputItem::Text { text: format!("[image: {filename}]") });
            ordered.push(InputItem::LocalImage { path });
        }
        Some(UserMessage {
            display_text: text,
            ordered_items: ordered,
            suppress_persistence: false,
        })
    }
}
