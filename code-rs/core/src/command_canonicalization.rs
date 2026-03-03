pub use code_shell_command::command_canonicalization::{
    CanonicalApprovalCommandKind, canonical_approval_command_kind, canonicalize_command_for_approval,
};

pub(crate) fn normalize_command_for_persistence(command: &[String]) -> Vec<String> {
    code_shell_command::command_canonicalization::normalize_command_for_persistence(command)
}

