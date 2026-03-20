use code_core::parse_command::ParsedCommand;

use crate::history::compat::ExecAction;

pub(crate) fn action_enum_from_parsed(
    parsed: &[code_core::parse_command::ParsedCommand],
) -> ExecAction {
    use code_core::parse_command::ParsedCommand;
    for p in parsed {
        match p {
            ParsedCommand::Read { .. } => return ExecAction::Read,
            ParsedCommand::Search { .. } => return ExecAction::Search,
            ParsedCommand::ListFiles { .. } => return ExecAction::List,
            _ => {}
        }
    }
    ExecAction::Run
}

pub(crate) fn first_context_path(parsed_commands: &[ParsedCommand]) -> Option<String> {
    for parsed in parsed_commands.iter() {
        match parsed {
            ParsedCommand::ListFiles { path, .. } | ParsedCommand::Search { path, .. } => {
                if let Some(p) = path {
                    return Some(p.clone());
                }
            }
            _ => {}
        }
    }
    None
}

