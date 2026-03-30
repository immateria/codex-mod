use once_cell::sync::Lazy;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};
use std::fs;
use std::process::{Command, Output};
use std::str::FromStr;
use base64::prelude::{Engine as _, BASE64_STANDARD};

use ratatui::style::{Modifier, Style};
use crate::header_wave::HeaderWaveEffect;
use crate::auto_drive_strings;
use crate::auto_drive_style::AutoDriveVariant;
use crate::spinner;
use crate::thread_spawner;

use code_common::elapsed::format_duration;
use code_common::model_presets::builtin_model_presets;
use code_common::model_presets::clamp_reasoning_effort_for_model;
use code_common::model_presets::ModelPreset;
use code_common::shell_presets::merge_shell_presets;
use code_common::shell_presets::ShellPreset;
use code_core::agent_defaults::{agent_model_spec, enabled_agent_model_specs};
use code_core::smoke_test_agent_blocking;
use code_core::config::Config;
use code_core::config::persist_shell;
use code_core::git_info::CommitLogEntry;
use code_core::config_types::AgentConfig;
use code_core::config_types::AutoDriveContinueMode;
use code_core::config_types::Notifications;
use code_core::config_types::ReasoningEffort;
use code_core::config_types::ShellConfig;
use code_core::config_types::ShellPresetConfig;
use code_core::config_types::ShellScriptStyle;
use code_core::config_types::TextVerbosity;
use code_core::plan_tool::{PlanItemArg, StepStatus, UpdatePlanArgs};
use code_core::model_family::derive_default_model_family;
use code_core::model_family::find_family_for_model;
use code_core::account_usage::{
    self,
    RateLimitWarningScope,
    StoredRateLimitSnapshot,
    StoredUsageSummary,
    TokenTotals,
};
use code_core::auth_accounts::{self, StoredAccount};
use code_login::AuthManager;
use code_login::AuthMode;
use code_protocol::dynamic_tools::DynamicToolResponse;
use code_protocol::num_format::format_with_separators;
use code_core::split_command_and_args;
use code_utils_sleep_inhibitor::SleepInhibitor;
use code_utils_stream_parser as stream_parser;
use serde_json::Value as JsonValue;


mod diff_handlers;
mod agent_summary;
mod agent_editor_flow;
mod esc;
mod modals;
mod agent;
mod agent_install;
mod internals;
mod code_event_pipeline;
mod cloud_workflow;
mod context_flow;
mod diff_ui;
mod exec_tools;
mod gh_actions;
mod history_links;
mod history_pipeline;
mod history_render;
mod history_virtualization_impl;
mod help_handlers;
mod secrets_help;
mod settings_handlers;
mod settings_overlay;
mod settings_routing;
mod plugins_shared_state;
mod apps_shared_state;
mod apps_picker;
mod limits_overlay;
mod interrupts;
mod input_pipeline;
mod layout_scroll;
mod message;
mod notifications;
mod ordering;
mod system_ordering;
mod background_review;
mod overlay_rendering;
mod perf;
mod rate_limit_refresh;
mod repo_workflow;
mod review_flow;
mod session_flow;
mod shell_config_flow;
mod session_tuning_flow;
mod status_line_flow;
mod streaming;
mod terminal_handlers;
mod terminal;
mod terminal_flow;
mod terminal_surface_image;
mod terminal_surface_header;
mod terminal_surface_render;
mod tools;
mod browser_sessions;
#[cfg(not(target_os = "android"))]
mod chrome_connection;

pub(crate) use plugins_shared_state::{
    PluginDetailKey,
    PluginsActionInProgress,
    PluginsDetailState,
    PluginsListState,
    PluginsSharedState,
};
pub(crate) use apps_shared_state::{
    AppsAccountSnapshot,
    AppsAccountStatusState,
    AppsActionInProgress,
    AppsSharedState,
    ConnectedAppSummary,
};
#[cfg(target_os = "android")]
mod chrome_connection_android;
mod agent_runs;
mod web_search_sessions;
mod auto_drive_cards;
mod auto_drive_flow;
pub(crate) mod tool_cards;
mod running_tools;
#[cfg(test)]
mod tests;
#[cfg(any(test, feature = "test-helpers"))]
pub mod smoke_helpers;
#[cfg(feature = "test-helpers")]
pub(crate) use self::history_render::{
    history_layout_cache_stats_for_test,
    reset_history_layout_cache_stats_for_test,
};

#[cfg(test)]
pub(crate) use self::esc::EscIntent;
use self::agent_summary::agent_summary_counts;
use self::esc::AutoGoalEscState;
use self::agent_install::{
    AgentInstallSessionArgs,
    GuidedTerminalControl,
    UpgradeTerminalSessionArgs,
    start_agent_install_session,
    start_direct_terminal_session,
    start_prompt_terminal_session,
    start_upgrade_terminal_session,
    wrap_command,
};
use self::internals::preamble::*;
use self::internals::state::*;
use code_auto_drive_core::{
    start_auto_coordinator,
    AutoCoordinatorCommand,
    AutoCoordinatorEvent,
    AutoCoordinatorEventSender,
    AutoCoordinatorHandle,
    AutoCoordinatorStatus,
    AutoDriveHistory,
    AutoDriveController,
    AutoRunSummary,
    AutoRunPhase,
    AutoControllerEffect,
    AutoTurnAgentsAction,
    AutoTurnAgentsTiming,
    AutoTurnCliAction,
    AutoTurnReviewState,
    AutoResolveState,
    AutoResolvePhase,
    AUTO_RESOLVE_REVIEW_FOLLOWUP,
    CoordinatorContext,
    CoordinatorRouterResponse,
    route_user_message,
    TurnConfig,
    TurnDescriptor,
};
use self::limits_overlay::{LimitsOverlayContent, LimitsTab};
use crate::insert_history::word_wrap_lines;
use self::rate_limit_refresh::{
    start_rate_limit_refresh,
    start_rate_limit_refresh_for_account,
};
use self::history_render::{
    CachedLayout, HistoryRenderState, RenderRequest, RenderRequestKind, RenderSettings, VisibleCell,
};
use code_core::parse_command::ParsedCommand;
use code_core::{AutoDriveMode, AutoDrivePidFile};
use code_core::TextFormat;
use code_core::protocol::AgentMessageDeltaEvent;
use code_core::protocol::ApprovedCommandMatchKind;
use code_core::protocol::AskForApproval;
use code_core::protocol::SandboxPolicy;
use code_core::protocol::AgentSourceKind;
use code_core::protocol::AgentMessageEvent;
use code_core::protocol::AgentReasoningDeltaEvent;
use code_core::protocol::AgentReasoningEvent;
use code_core::protocol::AgentReasoningRawContentDeltaEvent;
use code_core::protocol::AgentReasoningRawContentEvent;
use code_core::protocol::AgentReasoningSectionBreakEvent;
use code_core::protocol::ApplyPatchApprovalRequestEvent;
use code_core::protocol::BackgroundEventEvent;
use code_core::protocol::BrowserSnapshotEvent;
use code_core::protocol::CollaborationModeKind;
use code_core::protocol::CustomToolCallBeginEvent;
use code_core::protocol::CustomToolCallEndEvent;
use code_core::protocol::CustomToolCallUpdateEvent;
use code_core::protocol::ErrorEvent;
use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::protocol::ExecApprovalRequestEvent;
use code_core::protocol::ExecCommandBeginEvent;
use code_core::protocol::ExecCommandEndEvent;
use code_core::protocol::ExecOutputStream;
use code_core::protocol::EnvironmentContextDeltaEvent;
use code_core::protocol::EnvironmentContextFullEvent;
use code_core::protocol::InputItem;
use code_core::protocol::McpAuthStatus;
use code_core::protocol::McpServerFailure;
use code_core::protocol::SessionConfiguredEvent;
// MCP tool call handlers moved into chatwidget::tools
use code_core::protocol::Op;
use code_core::protocol::ReviewOutputEvent;
use code_core::protocol::ReviewRequest;
use code_core::protocol::PatchApplyBeginEvent;
use code_core::protocol::PatchApplyEndEvent;
use code_core::protocol::TaskCompleteEvent;
use code_core::protocol::TokenUsage;
use code_core::protocol::TurnDiffEvent;
use code_core::protocol::ViewImageToolCallEvent;
use code_core::review_coord::{bump_snapshot_epoch_for, try_acquire_lock, ReviewGuard};
use code_core::codex::compact::COMPACTION_CHECKPOINT_MESSAGE;
use crate::bottom_pane::{AgentHintLabel, AutoReviewFooterStatus, AutoReviewPhase, SettingsSection};
use crate::bottom_pane::panes::auto_coordinator::{
    AutoActiveViewModel,
    AutoCoordinatorButton,
    AutoCoordinatorViewModel,
    CountdownState,
};
use crate::prompt_args;
use crate::bottom_pane::settings_pages::accounts::{
    LoginAccountsState,
    LoginAccountsView,
    LoginAddAccountState,
    LoginAddAccountView,
};
use crate::bottom_pane::settings_pages::agents::{AgentEditorInit, AgentEditorView, SubagentEditorView};
use crate::bottom_pane::settings_pages::auto_drive::{AutoDriveSettingsInit, AutoDriveSettingsView};
use crate::bottom_pane::settings_pages::mcp::{McpServerRow, McpServerRows, McpSettingsView};
use crate::bottom_pane::settings_pages::model::ModelSelectionView;
use crate::bottom_pane::settings_pages::notifications::{NotificationsMode, NotificationsSettingsView};
use crate::bottom_pane::settings_pages::planning::PlanningSettingsView;
use crate::bottom_pane::settings_pages::prompts::PromptsSettingsView;
use crate::bottom_pane::settings_pages::review::{ReviewSettingsInit, ReviewSettingsView};
use crate::bottom_pane::settings_pages::skills::SkillsSettingsView;
use crate::bottom_pane::settings_pages::status_line::{StatusLineItem, StatusLineSetupView};
use crate::bottom_pane::settings_pages::theme::ThemeSelectionView;
use crate::bottom_pane::settings_pages::updates::{UpdateSettingsInit, UpdateSettingsView, UpdateSharedState};
use crate::bottom_pane::settings_pages::validation::ValidationSettingsView;
use crate::exec_command::strip_bash_lc_and_escape;
#[cfg(feature = "code-fork")]
use crate::tui_event_extensions::handle_browser_screenshot;
use crate::chatwidget::message::UserMessage;
use crate::history::compat::{
    ContextBrowserSnapshotRecord,
    ContextDeltaField,
    ContextDeltaRecord,
    ContextRecord,
};

impl ChatWidget<'_> {
    fn is_auto_review_agent(agent: &code_core::protocol::AgentInfo) -> bool {
        if matches!(agent.source_kind, Some(AgentSourceKind::AutoReview)) {
            return true;
        }
        if let Some(batch) = agent.batch_id.as_deref()
            && batch.eq_ignore_ascii_case("auto-review") {
                return true;
            }
        false
    }
    fn format_code_bridge_call(&self, args: &JsonValue) -> Option<String> {
        let action = args.get("action")?.as_str()?.to_lowercase();
        let mut out = String::from("Code Bridge\n");
        match action.as_str() {
            "subscribe" => {
                out.push_str("└ Subscribe");
                if let Some(level) = args.get("level").and_then(|v| v.as_str()) {
                    out.push_str(&format!("  level={level}"));
                }
                Some(out)
            }
            "screenshot" => {
                out.push_str("└ Screenshot");
                Some(out)
            }
            "javascript" => {
                out.push_str("└ JavaScript\n");
                if let Some(code) = args.get("code").and_then(|v| v.as_str()) {
                    out.push_str("   ```javascript\n");
                    out.push_str(code);
                    out.push_str("\n   ```");
                }
                Some(out)
            }
            _ => None,
        }
    }

    fn format_kill_call(&self, args: &JsonValue) -> Option<String> {
        if let Some(call_id) = args.get("call_id").and_then(|v| v.as_str()) {
            let mut out = String::from("Kill\n");
            out.push_str(&format!("└ call_id: {call_id}"));
            return Some(out);
        }
        None
    }

    fn format_tool_call_preview(&self, name: &str, arguments: &str) -> Option<String> {
        let parsed: JsonValue = serde_json::from_str(arguments).ok()?;
        match name {
            "code_bridge" => self.format_code_bridge_call(&parsed),
            "kill" => self.format_kill_call(&parsed),
            _ => None,
        }
    }

    fn browser_overlay_progress_line(
        &self,
        width: u16,
        current: Duration,
        total: Duration,
    ) -> Line<'static> {
        let width = width.max(20) as usize;
        let prefix = "▶ ";
        let suffix = format!(
            " {} / {}",
            self.format_overlay_mm_ss(current),
            self.format_overlay_mm_ss(total)
        );
        let slider_width = width
            .saturating_sub(prefix.len())
            .saturating_sub(suffix.chars().count())
            .max(5);

        let progress_ratio = if total.as_millis() == 0 {
            0.0
        } else {
            (current.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
        };
        let progress_cells = (progress_ratio * slider_width as f64).round() as usize;

        let mut slider = String::with_capacity(slider_width);
        let pointer_idx = if slider_width <= 1 {
            0
        } else {
            progress_cells.clamp(0, slider_width.saturating_sub(1))
        };
        for i in 0..slider_width {
            if i == pointer_idx {
                slider.push('◉');
            } else {
                slider.push('─');
            }
        }

        let spans: Vec<Span> = vec![
            Span::styled(prefix.to_string(), Style::default().fg(crate::colors::text())),
            Span::styled(
                slider,
                Style::default()
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(suffix, Style::default().fg(crate::colors::text())),
        ];

        Line::from(spans)
    }

    fn format_overlay_mm_ss(&self, duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        format!("{minutes:02}:{seconds:02}")
    }

    fn normalize_action_time_label(&self, label: &str) -> String {
        if let Some((minutes, seconds)) = label.split_once('m') {
            let minutes = minutes.trim().parse::<u64>().unwrap_or(0);
            let seconds = seconds
                .trim()
                .trim_start_matches(char::is_whitespace)
                .trim_end_matches('s')
                .parse::<u64>()
                .unwrap_or(0);
            return format!("{minutes:02}:{seconds:02}");
        }
        if let Some(stripped) = label.strip_suffix('s')
            && let Ok(seconds) = stripped.trim().parse::<u64>() {
                return format!("00:{:02}", seconds.min(59));
            }
        label.to_string()
    }
}
use code_git_tooling::{
    create_ghost_commit,
    restore_ghost_commit,
    CreateGhostCommitOptions,
    GhostCommit,
    GitToolingError,
};
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use image::imageops::FilterType;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;
use ratatui_image::picker::Picker;
use std::cell::{Cell, RefCell};
use std::sync::mpsc;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task;
use uuid::Uuid;

include!("shared_defs/mod.rs");
include!("impl_chunks/core_git_startup_autodrive.rs");
include!("impl_chunks/perf_spinner_interrupts_redraw.rs");
include!("impl_chunks/perf_demo_and_status_outputs.rs");
include!("impl_chunks/rate_limits_and_updates_settings.rs");
include!("impl_chunks/memories_report_and_login_flow.rs");
include!("impl_chunks/settings_overlays_builders.rs");
include!("impl_chunks/agents_overlay_and_terminal_mode.rs");
include!("impl_chunks/popups_config_theme_access.rs");
include!("impl_chunks/cancel_ctrlc_autodrive_celebration_and_task_state.rs");
include!("impl_chunks/history_insert_and_answer_streaming.rs");
include!("impl_chunks/reasoning_terminal_browser.rs");
include!("impl_chunks/validation_and_mcp_commands.rs");
include!("impl_chunks/submit_messages_and_usage.rs");

async fn run_background_review(
    config: Config,
    app_event_tx: AppEventSender,
    base_snapshot: Option<GhostCommit>,
    turn_context: Option<String>,
    prefer_fallback: bool,
) {
    background_review::run_background_review_inner(
        config,
        app_event_tx,
        base_snapshot,
        turn_context,
        prefer_fallback,
    )
    .await;
}

fn insert_background_lock(agent_id: &str, guard: code_core::review_coord::ReviewGuard) {
    background_review::insert_background_lock_inner(agent_id, guard);
}

fn release_background_lock(agent_id: &Option<String>) {
    background_review::release_background_lock_inner(agent_id);
}

#[cfg(test)]
type AutoReviewStub = Option<Box<dyn FnMut() + Send>>;

#[cfg(test)]
type AutoReviewStubSlot = std::sync::Mutex<AutoReviewStub>;

#[cfg(test)]
static AUTO_REVIEW_STUB: once_cell::sync::Lazy<AutoReviewStubSlot> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

#[cfg(test)]
struct AutoReviewStubGuard;

#[cfg(test)]
impl AutoReviewStubGuard {
    fn install<F: FnMut() + Send + 'static>(f: F) -> Self {
        let mut guard = AUTO_REVIEW_STUB.lock().unwrap();
        *guard = Some(Box::new(f));
        AutoReviewStubGuard
    }
}

#[cfg(test)]
impl Drop for AutoReviewStubGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = AUTO_REVIEW_STUB.lock() {
            *guard = None;
        }
    }
}
