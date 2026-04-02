//! Settings pages/panes that *consume* the shared `settings_ui` primitives.
//!
//! This module is intentionally separate from `settings_ui`: `settings_ui` is
//! the shared component/toolkit layer, while `settings_pages` contains the
//! concrete pages that render settings and handle input.

pub(crate) mod accounts;
pub(crate) mod apps;
pub(crate) mod agents;
pub(crate) mod auto_drive;
pub(crate) mod experimental_features;
pub(crate) mod exec_limits;
pub(crate) mod interface;
pub(crate) mod js_repl;
pub(crate) mod mcp;
pub(crate) mod memories;
pub(crate) mod model;
#[cfg(feature = "managed-network-proxy")]
pub(crate) mod network;
pub(crate) mod notifications;
pub(crate) mod overview;
pub(crate) mod planning;
pub(crate) mod plugins;
pub(crate) mod prompts;
pub(crate) mod review;
pub(crate) mod secrets;
pub(crate) mod shell;
pub(crate) mod shell_escalation;
pub(crate) mod shell_profiles;
pub(crate) mod skills;
pub(crate) mod status_line;
pub(crate) mod theme;
pub(crate) mod updates;
pub(crate) mod validation;
pub(crate) mod verbosity;
