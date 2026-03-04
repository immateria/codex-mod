pub mod context;
mod cmd_safe_commands;
mod fork_bomb;
pub mod is_dangerous_command;
pub mod is_safe_command;
pub mod windows_dangerous_commands;
pub mod windows_safe_commands;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use strum_macros::Display;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Display, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum CommandSafetyRuleset {
    Auto,
    Posix,
    Windows,
}
