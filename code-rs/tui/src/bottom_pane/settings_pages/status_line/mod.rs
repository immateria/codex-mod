use crate::app_event_sender::AppEventSender;
use code_core::config_types::StatusLineLane;
use strum_macros::{Display, EnumIter, EnumString};

mod input;
mod item;
mod model;
mod mouse;
mod pane_impl;
mod render;

#[derive(EnumIter, EnumString, Display, Debug, Clone, Copy, Eq, PartialEq)]
#[strum(serialize_all = "kebab_case")]
pub(crate) enum StatusLineItem {
    ModelName,
    ModelWithReasoning,
    ServiceTier,
    Shell,
    ShellStyle,
    CurrentDir,
    ProjectRoot,
    GitBranch,
    NetworkMediation,
    Approval,
    Sandbox,
    ContextRemaining,
    ContextUsed,
    FiveHourLimit,
    WeeklyLimit,
    CodexVersion,
    ContextWindowSize,
    UsedTokens,
    TotalInputTokens,
    TotalOutputTokens,
    SessionId,
    JsRepl,
    ActiveProfile,
}

#[derive(Clone, Copy)]
struct StatusLineChoice {
    item: StatusLineItem,
    enabled: bool,
}

pub(crate) struct StatusLineSetupView {
    app_event_tx: AppEventSender,
    top_choices: Vec<StatusLineChoice>,
    bottom_choices: Vec<StatusLineChoice>,
    top_selected_index: usize,
    bottom_selected_index: usize,
    active_lane: StatusLineLane,
    primary_lane: StatusLineLane,
    complete: bool,
}
