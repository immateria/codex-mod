use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
use crate::user_approval_widget::{ApprovalRequest, UserApprovalWidget};

/// Trait bridging approval UI construction so the fork can swap implementations
/// without touching call sites.
pub(crate) trait ApprovalUi<'a> {
    fn build(
        request: ApprovalRequest,
        ticket: BackgroundOrderTicket,
        app_event_tx: AppEventSender,
    ) -> Self;
}

impl<'a> ApprovalUi<'a> for UserApprovalWidget<'a> {
    fn build(
        request: ApprovalRequest,
        ticket: BackgroundOrderTicket,
        app_event_tx: AppEventSender,
    ) -> Self {
        UserApprovalWidget::new(request, ticket, app_event_tx)
    }
}
