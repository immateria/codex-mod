//! Bottom pane: shows the ChatComposer or a BottomPaneView, if one is active.

use crate::app_event_sender::AppEventSender;
use crate::auto_drive_style::AutoDriveVariant;
use crate::chatwidget::BackgroundOrderTicket;
use crate::user_approval_widget::{ApprovalRequest, UserApprovalWidget};
pub(crate) use bottom_pane_view::BottomPaneView;
pub(crate) use bottom_pane_view::ConditionalUpdate;
pub(crate) use chrome::{ChromeMode, LastRenderContext};
use code_protocol::custom_prompts::CustomPrompt;
use code_protocol::skills::Skill;
use panes::auto_coordinator::AutoCoordinatorViewModel;

mod chrome;
mod chrome_view;
mod bottom_pane_view;
mod chat_composer;
mod input;
mod layout;
mod popup_consts;
mod render;
mod state;
#[cfg(test)]
mod tests;
mod views;
pub(crate) mod panes;
pub(crate) mod settings_pages;
mod settings_overlay;
pub(crate) mod settings_ui;
pub(crate) use settings_overlay::SettingsSection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancellationEvent {
    Ignored,
    Handled,
}

pub(crate) use chat_composer::{AgentHintLabel, AutoReviewFooterStatus, AutoReviewPhase, ChatComposer};
pub(crate) use chat_composer::InputResult;
#[cfg(feature = "code-fork")]
use panes::approval_modal::ApprovalUi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveViewKind {
    None,
    AutoCoordinator,
    ModelSelection,
    RequestUserInput,
    ShellSelection,
    Other,
}

/// Pane displayed in the lower half of the chat UI.
pub(crate) struct BottomPane<'a> {
    /// Composer is retained even when a BottomPaneView is displayed so the
    /// input state is retained when the view is closed.
    composer: ChatComposer,

    /// If present, this is displayed instead of the `composer`.
    active_view: Option<Box<dyn BottomPaneView<'a> + 'a>>,
    active_view_kind: ActiveViewKind,

    app_event_tx: AppEventSender,
    has_input_focus: bool,
    is_task_running: bool,
    ctrl_c_quit_hint: bool,
    compact_compose: bool,
    pending_auto_coordinator: Option<AutoCoordinatorViewModel>,

    /// Whether to reserve an empty spacer line above the input composer.
    /// Defaults to true for visual breathing room, but can be disabled when
    /// the chat history is scrolled up to allow history to reclaim that row.
    top_spacer_enabled: bool,

    /// When true, always reserve the spacer row so higher-level UI (e.g. a
    /// bottom status line) can reliably render into it.
    force_top_spacer: bool,

    using_chatgpt_auth: bool,

    auto_drive_variant: AutoDriveVariant,
    auto_drive_active: bool,

    custom_prompts: Vec<CustomPrompt>,
    skills: Vec<Skill>,

}

pub(crate) struct BottomPaneParams {
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) has_input_focus: bool,
    pub(crate) using_chatgpt_auth: bool,
    pub(crate) auto_drive_variant: AutoDriveVariant,
}

impl<'a> BottomPane<'a> {
    // Reduce bottom padding so footer sits one line lower
    const BOTTOM_PAD_LINES: u16 = 1;
    const HORIZONTAL_PADDING: u16 = 1;
}

#[cfg(feature = "code-fork")]
fn build_user_approval_widget<'a>(
    request: ApprovalRequest,
    ticket: BackgroundOrderTicket,
    app_event_tx: AppEventSender,
) -> UserApprovalWidget<'a> {
    <UserApprovalWidget<'a> as ApprovalUi>::build(request, ticket, app_event_tx)
}

#[cfg(not(feature = "code-fork"))]
fn build_user_approval_widget<'a>(
    request: ApprovalRequest,
    ticket: BackgroundOrderTicket,
    app_event_tx: AppEventSender,
) -> UserApprovalWidget<'a> {
    UserApprovalWidget::new(request, ticket, app_event_tx)
}
