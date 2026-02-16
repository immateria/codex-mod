//! Exec and tool call lifecycle helpers for `ChatWidget`.

use super::{
    running_tools,
    web_search_sessions,
    ChatWidget,
    ExecCallId,
    RunningCommand,
    RunningToolEntry,
    ToolCallId,
};
use crate::app_event::AppEvent;
use crate::height_manager::HeightEvent;
use crate::history::state::{
    ExecAction,
    ExecRecord,
    ExecStatus,
    ExecStreamChunk,
    ExecWaitNote,
    ExploreRecord,
    HistoryDomainEvent,
    HistoryDomainRecord,
    HistoryId,
    HistoryMutation,
    HistoryRecord,
    InlineSpan,
    MessageLine,
    MessageLineKind,
    PlainMessageKind,
    PlainMessageRole,
    PlainMessageState,
    TextEmphasis,
    TextTone,
};
use crate::history_cell::CommandOutput;
use crate::history_cell::{self, HistoryCell};
use code_core::parse_command::ParsedCommand;
use code_core::protocol::{ExecCommandBeginEvent, ExecCommandEndEvent, OrderMeta};
use std::path::PathBuf;
use std::time::SystemTime;

mod finalization;
mod helpers;
mod lifecycle;

pub(super) use finalization::finalize_all_running_as_interrupted;
pub(super) use finalization::finalize_all_running_due_to_answer;
pub(super) use finalization::finalize_wait_missing_exec;
pub(super) use finalization::try_merge_completed_exec_at;
pub(super) use lifecycle::handle_exec_begin_now;
pub(super) use lifecycle::handle_exec_end_now;
