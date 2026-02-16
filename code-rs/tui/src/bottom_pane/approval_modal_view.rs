use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
use crate::user_approval_widget::ApprovalRequest;
use crate::user_approval_widget::UserApprovalWidget;

use super::BottomPane;
use super::BottomPaneView;
use super::CancellationEvent;
use super::bottom_pane_view::ConditionalUpdate;
use crate::ui_interaction::redraw_if;
use std::collections::VecDeque;

/// Modal overlay asking the user to approve/deny a sequence of requests.
pub(crate) struct ApprovalModalView<'a> {
    current: UserApprovalWidget<'a>,
    queue: VecDeque<(ApprovalRequest, BackgroundOrderTicket)>,
    app_event_tx: AppEventSender,
}

impl ApprovalModalView<'_> {
    pub fn new(
        request: ApprovalRequest,
        ticket: BackgroundOrderTicket,
        app_event_tx: AppEventSender,
    ) -> Self {
        Self {
            current: super::build_user_approval_widget(request, ticket, app_event_tx.clone()),
            queue: VecDeque::new(),
            app_event_tx,
        }
    }

    pub fn enqueue_request(
        &mut self,
        req: ApprovalRequest,
        ticket: BackgroundOrderTicket,
    ) {
        self.queue.push_back((req, ticket));
    }

    /// Advance to next request if the current one is finished.
    fn maybe_advance(&mut self) {
        if self.current.is_complete()
            && let Some((req, ticket)) = self.queue.pop_front() {
                self.current =
                    super::build_user_approval_widget(req, ticket, self.app_event_tx.clone());
            }
    }
}

impl<'a> BottomPaneView<'a> for ApprovalModalView<'a> {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_direct(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_direct(key_event))
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'a>) -> CancellationEvent {
        self.current.on_ctrl_c();
        self.queue.clear();
        CancellationEvent::Handled
    }

    fn is_complete(&self) -> bool {
        self.current.is_complete() && self.queue.is_empty()
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.current.desired_height(width)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        (&self.current).render_ref(area, buf);
    }

    fn try_consume_approval_request(
        &mut self,
        req: ApprovalRequest,
        ticket: BackgroundOrderTicket,
    ) -> Option<(ApprovalRequest, BackgroundOrderTicket)> {
        self.enqueue_request(req, ticket);
        None
    }
}

impl ApprovalModalView<'_> {
    fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.current.handle_key_event(key_event);
        self.maybe_advance();
        matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
    }
}
