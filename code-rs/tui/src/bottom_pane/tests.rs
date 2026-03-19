use super::*;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

struct RecordingView {
    last_mouse_area: Rc<RefCell<Option<Rect>>>,
    ignored_on_ctrl_c: bool,
}

impl RecordingView {
    fn new(last_mouse_area: Rc<RefCell<Option<Rect>>>) -> Self {
        Self {
            last_mouse_area,
            ignored_on_ctrl_c: false,
        }
    }

    fn with_ignored_ctrl_c() -> Self {
        Self {
            last_mouse_area: Rc::new(RefCell::new(None)),
            ignored_on_ctrl_c: true,
        }
    }
}

impl<'a> BottomPaneView<'a> for RecordingView {
    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        _mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        *self.last_mouse_area.borrow_mut() = Some(area);
        ConditionalUpdate::NeedsRedraw
    }

    fn handle_key_event_with_result(
        &mut self,
        pane: &mut BottomPane<'a>,
        _key_event: KeyEvent,
    ) -> ConditionalUpdate {
        pane.clear_active_view();
        ConditionalUpdate::NeedsRedraw
    }

    fn is_complete(&self) -> bool {
        false
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'a>) -> CancellationEvent {
        if self.ignored_on_ctrl_c {
            CancellationEvent::Ignored
        } else {
            CancellationEvent::Handled
        }
    }

    fn desired_height(&self, _width: u16) -> u16 {
        4
    }

    fn render(&self, _area: Rect, _buf: &mut Buffer) {}

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

fn make_bottom_pane() -> BottomPane<'static> {
    let (tx, _rx) = mpsc::channel();
    BottomPane::new(BottomPaneParams {
        app_event_tx: AppEventSender::new(tx),
        has_input_focus: true,
        using_chatgpt_auth: false,
        auto_drive_variant: AutoDriveVariant::default(),
    })
}

#[test]
fn mouse_events_use_rendered_view_rect() {
    let last_mouse_area = Rc::new(RefCell::new(None));
    let mut pane = make_bottom_pane();
    pane.set_other_view(RecordingView::new(last_mouse_area.clone()), false);

    let area = Rect::new(0, 0, 40, 10);
    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: 3,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };

    let _ = pane.handle_mouse_event(mouse, area);

    assert_eq!(*last_mouse_area.borrow(), Some(Rect::new(1, 1, 38, 8)));
}

#[test]
fn callback_clear_does_not_reinsert_taken_view() {
    let mut pane = make_bottom_pane();
    pane.set_other_view(
        RecordingView {
            last_mouse_area: Rc::new(RefCell::new(None)),
            ignored_on_ctrl_c: false,
        },
        false,
    );

    let _ = pane.handle_key_event(KeyEvent::from(KeyCode::Enter));

    assert!(!pane.has_active_view());
    assert_eq!(pane.active_view_kind, ActiveViewKind::None);
}

#[test]
fn ignored_ctrl_c_does_not_show_quit_hint() {
    let mut pane = make_bottom_pane();
    pane.set_other_view(RecordingView::with_ignored_ctrl_c(), false);

    let event = pane.on_ctrl_c();

    assert_eq!(event, CancellationEvent::Ignored);
    assert!(pane.has_active_view());
    assert!(!pane.ctrl_c_quit_hint_visible());
}
