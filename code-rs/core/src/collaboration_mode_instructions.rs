use crate::protocol::CollaborationModeKind;

const COLLABORATION_MODE_PLAN: &str = include_str!("../templates/collaboration_mode/plan.md");
const COLLABORATION_MODE_DEFAULT: &str =
    include_str!("../templates/collaboration_mode/default.md");
const KNOWN_MODE_NAMES_PLACEHOLDER: &str = "{{KNOWN_MODE_NAMES}}";
const REQUEST_USER_INPUT_AVAILABILITY_PLACEHOLDER: &str = "{{REQUEST_USER_INPUT_AVAILABILITY}}";

const KNOWN_MODES: &[CollaborationModeKind] =
    &[CollaborationModeKind::Default, CollaborationModeKind::Plan];

pub(crate) fn render_collaboration_mode_instructions(mode: CollaborationModeKind) -> String {
    match mode {
        CollaborationModeKind::Plan => COLLABORATION_MODE_PLAN.to_string(),
        CollaborationModeKind::Default => default_mode_instructions(),
    }
}

fn default_mode_instructions() -> String {
    let known_mode_names = format_mode_names(KNOWN_MODES);
    let request_user_input_availability =
        request_user_input_availability_message(CollaborationModeKind::Default);

    COLLABORATION_MODE_DEFAULT
        .replace(KNOWN_MODE_NAMES_PLACEHOLDER, &known_mode_names)
        .replace(
            REQUEST_USER_INPUT_AVAILABILITY_PLACEHOLDER,
            &request_user_input_availability,
        )
}

fn request_user_input_availability_message(mode: CollaborationModeKind) -> String {
    let mode_name = display_name(mode);
    if allows_request_user_input(mode) {
        format!("The `request_user_input` tool is available in {mode_name} mode.")
    } else {
        format!(
            "The `request_user_input` tool is unavailable in {mode_name} mode. If you call it while in {mode_name} mode, it will return an error."
        )
    }
}

fn display_name(mode: CollaborationModeKind) -> &'static str {
    match mode {
        CollaborationModeKind::Default => "Default",
        CollaborationModeKind::Plan => "Plan",
    }
}

fn allows_request_user_input(_mode: CollaborationModeKind) -> bool {
    // In this fork, request_user_input is currently available in all modes.
    true
}

fn format_mode_names(modes: &[CollaborationModeKind]) -> String {
    let mode_names: Vec<&str> = modes.iter().map(|mode| display_name(*mode)).collect();
    match mode_names.as_slice() {
        [] => "none".to_string(),
        [mode_name] => (*mode_name).to_string(),
        [first, second] => format!("{first} and {second}"),
        [..] => mode_names.join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mode_replaces_placeholders() {
        let rendered = render_collaboration_mode_instructions(CollaborationModeKind::Default);
        assert!(!rendered.contains(KNOWN_MODE_NAMES_PLACEHOLDER));
        assert!(!rendered.contains(REQUEST_USER_INPUT_AVAILABILITY_PLACEHOLDER));
        assert!(rendered.contains("Known mode names are Default and Plan."));
        assert!(rendered.contains("The `request_user_input` tool is available in Default mode."));
    }

    #[test]
    fn renders_plan_mode_instructions() {
        let rendered = render_collaboration_mode_instructions(CollaborationModeKind::Plan);
        assert!(rendered.contains("# Plan Mode (Conversational)"));
    }

    #[test]
    fn mode_names_use_human_labels() {
        assert_eq!(format_mode_names(KNOWN_MODES), "Default and Plan");
    }
}
