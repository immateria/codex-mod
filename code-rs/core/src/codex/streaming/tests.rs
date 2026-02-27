use super::*;

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    use super::super::super::session::prune_history_items;
    use code_protocol::protocol::{
        BROWSER_SNAPSHOT_CLOSE_TAG,
        BROWSER_SNAPSHOT_OPEN_TAG,
        ENVIRONMENT_CONTEXT_CLOSE_TAG,
        ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG,
        ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG,
        ENVIRONMENT_CONTEXT_OPEN_TAG,
    };

    fn make_text_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    fn make_screenshot_message(tag: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputImage {
                image_url: tag.to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    #[test]
    fn prune_history_retains_recent_env_items() {
        let baseline1 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_OPEN_TAG}\n{{}}\n{ENVIRONMENT_CONTEXT_CLOSE_TAG}"
        ));
        let delta1 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG}\n{{\"cwd\":\"/repo\"}}\n{ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG}"
        ));
        let snapshot1 = make_text_message(&format!(
            "{BROWSER_SNAPSHOT_OPEN_TAG}\n{{\"url\":\"https://first\"}}\n{BROWSER_SNAPSHOT_CLOSE_TAG}"
        ));
        let screenshot1 = make_screenshot_message("data:image/png;base64,AAA");
        let user_msg = make_text_message("Regular user message");
        let baseline2 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_OPEN_TAG}\n{{\"cwd\":\"/repo2\"}}\n{ENVIRONMENT_CONTEXT_CLOSE_TAG}"
        ));
        let delta2 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG}\n{{\"cwd\":\"/repo2\"}}\n{ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG}"
        ));
        let snapshot2 = make_text_message(&format!(
            "{BROWSER_SNAPSHOT_OPEN_TAG}\n{{\"url\":\"https://second\"}}\n{BROWSER_SNAPSHOT_CLOSE_TAG}"
        ));
        let screenshot2 = make_screenshot_message("data:image/png;base64,BBB");
        let delta3 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG}\n{{\"cwd\":\"/repo3\"}}\n{ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG}"
        ));
        let snapshot3 = make_text_message(&format!(
            "{BROWSER_SNAPSHOT_OPEN_TAG}\n{{\"url\":\"https://third\"}}\n{BROWSER_SNAPSHOT_CLOSE_TAG}"
        ));
        let delta4 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG}\n{{\"cwd\":\"/repo4\"}}\n{ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG}"
        ));
        let screenshot3 = make_screenshot_message("data:image/png;base64,CCC");

        let history = vec![
            user_msg,
            baseline1,
            delta1.clone(),
            snapshot1.clone(),
            screenshot1,
            baseline2.clone(),
            delta2.clone(),
            snapshot2.clone(),
            screenshot2,
            delta3.clone(),
            snapshot3.clone(),
            delta4.clone(),
            screenshot3,
        ];

        let (pruned, stats) = prune_history_items(&history);

        // Baseline 1 should be removed; only the latest baseline retained
        assert!(pruned.contains(&baseline2));
        assert!(!pruned.contains(&history[1]));

        // Only the last three deltas should remain
        assert!(pruned.contains(&delta2));
        assert!(pruned.contains(&delta3));
        assert!(pruned.contains(&delta4));
        assert!(!pruned.contains(&delta1));

        // Only the last two browser snapshots should remain
        assert!(pruned.contains(&snapshot2));
        assert!(pruned.contains(&snapshot3));
        assert!(!pruned.contains(&snapshot1));

        // Stats reflect removals and kept counts
        assert_eq!(stats.removed_env_baselines, 1);
        assert_eq!(stats.removed_env_deltas, 1);
        assert_eq!(stats.removed_browser_snapshots, 1);
        assert_eq!(stats.kept_env_deltas, 3);
        assert_eq!(stats.kept_browser_snapshots, 2);
        assert_eq!(stats.kept_recent_screenshots, 1);
    }

    #[test]
    fn prune_history_no_env_items_is_identity() {
        let user = make_text_message("hi");
        let assistant = ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "response".to_string(),
            }],
            end_turn: None,
            phase: None,
        };
        let history = vec![user, assistant];

        let (pruned, stats) = prune_history_items(&history);
        assert_eq!(pruned, history);
        assert!(!stats.any_removed());
    }
}
