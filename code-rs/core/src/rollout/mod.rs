//! Rollout module: persistence and discovery of session rollout files.

use code_protocol::protocol::SessionSource;

pub const SESSIONS_SUBDIR: &str = "sessions";
pub const ARCHIVED_SESSIONS_SUBDIR: &str = "archived_sessions";
pub const INTERACTIVE_SESSION_SOURCES: &[SessionSource] =
    &[SessionSource::Cli, SessionSource::VSCode];

pub mod catalog;
pub mod fork;
pub mod list;
pub(crate) mod policy;
pub mod recorder;

pub use code_protocol::protocol::SessionMeta;
pub use recorder::RolloutRecorder;

#[cfg(test)]
pub mod tests;
