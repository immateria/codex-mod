    use super::*;
    use super::{
        CAPTURE_AUTO_TURN_COMMIT_STUB,
        GIT_DIFF_NAME_ONLY_BETWEEN_STUB,
    };
    use crate::app_event::AppEvent;
    use crate::bottom_pane::panes::auto_coordinator::AutoCoordinatorViewModel;
    use crate::bottom_pane::settings_pages::mcp::McpServerRow;
    use crate::chatwidget::message::UserMessage;
    use crate::chatwidget::smoke_helpers::{enter_test_runtime_guard, ChatWidgetHarness};
    use crate::history_cell::{self, ExploreAggregationCell, HistoryCellType};
    use code_auto_drive_core::{
    AutoContinueMode,
    AutoRunPhase,
    AutoRunSummary,
    TurnComplexity,
    TurnMode,
    AUTO_RESOLVE_MAX_REVIEW_ATTEMPTS,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use code_core::config_types::AutoResolveAttemptLimit;
    use code_core::history::state::{
    AssistantStreamDelta,
    AssistantStreamState,
    HistoryId,
    HistoryRecord,
    HistorySnapshot,
    HistoryState,
    InlineSpan,
    MessageLine,
    MessageLineKind,
    OrderKeySnapshot,
    PlainMessageKind,
    PlainMessageRole,
    PlainMessageState,
    TextEmphasis,
    TextTone,
    };
    use code_core::parse_command::ParsedCommand;
    use code_core::protocol::OrderMeta;
    use code_core::config_types::{McpServerConfig, McpServerTransportConfig};
    use code_core::protocol::{
    AskForApproval,
    AgentMessageDeltaEvent,
    AgentMessageEvent,
    AgentStatusUpdateEvent,
    ErrorEvent,
    Event,
    EventMsg,
    ExecCommandBeginEvent,
    McpAuthStatus,
    McpServerFailure,
    McpServerFailurePhase,
    TaskCompleteEvent,
    };
    use code_core::protocol::AgentInfo as CoreAgentInfo;
    use code_protocol::protocol::{ReviewTarget, TurnAbortedEvent, TurnAbortReason};
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::process::Command;
    use tempfile::tempdir;
    use std::sync::Arc;
    use std::path::PathBuf;
    
    include!("harness.rs");
    include!("review.rs");
    include!("ordering.rs");
    include!("hooks.rs");
    include!("autodrive.rs");
    include!("streaming.rs");
