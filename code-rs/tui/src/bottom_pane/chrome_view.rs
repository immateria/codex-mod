use crossterm::event::MouseEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// Shared wrapper types for views that can render in two chrome modes:
/// - `Framed`: the view draws its own outer frame/chrome (bottom pane).
/// - `ContentOnly`: the view renders into a content rect that already has outer chrome (overlay).
///
/// The intent is to make "pick chrome once" a one-liner at call sites:
/// `view.framed().render(...)` vs `view.content_only().render(...)`.
pub(crate) trait ChromeRenderable {
    fn render_in_framed_chrome(&self, area: Rect, buf: &mut Buffer);
    fn render_in_content_only_chrome(&self, area: Rect, buf: &mut Buffer);
}

pub(crate) trait ChromeMouseHandler {
    fn handle_mouse_event_direct_in_framed_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool;

    fn handle_mouse_event_direct_in_content_only_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool;
}

pub(crate) struct Framed<'v, V> {
    view: &'v V,
}

/// Content-only (shell-less) chrome wrapper.
pub(crate) struct ContentOnly<'v, V> {
    view: &'v V,
}

pub(crate) struct FramedMut<'v, V> {
    view: &'v mut V,
}

/// Mutable content-only (shell-less) chrome wrapper.
pub(crate) struct ContentOnlyMut<'v, V> {
    view: &'v mut V,
}

impl<'v, V> Framed<'v, V> {
    pub(crate) fn new(view: &'v V) -> Self {
        Self { view }
    }
}

impl<'v, V> ContentOnly<'v, V> {
    pub(crate) fn new(view: &'v V) -> Self {
        Self { view }
    }
}

impl<'v, V> FramedMut<'v, V> {
    pub(crate) fn new(view: &'v mut V) -> Self {
        Self { view }
    }
}

impl<'v, V> ContentOnlyMut<'v, V> {
    pub(crate) fn new(view: &'v mut V) -> Self {
        Self { view }
    }
}

impl<'v, V: ChromeRenderable> Framed<'v, V> {
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render_in_framed_chrome(area, buf);
    }
}

impl<'v, V: ChromeRenderable> ContentOnly<'v, V> {
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render_in_content_only_chrome(area, buf);
    }
}

impl<'v, V: ChromeMouseHandler> FramedMut<'v, V> {
    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.view
            .handle_mouse_event_direct_in_framed_chrome(mouse_event, area)
    }
}

impl<'v, V: ChromeMouseHandler> ContentOnlyMut<'v, V> {
    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.view
            .handle_mouse_event_direct_in_content_only_chrome(mouse_event, area)
    }
}

/// Generate chrome-mode convenience methods for a settings view type.
///
/// - `impl_chrome_view!(ViewType)` generates `content_only()` and `content_only_mut()`.
/// - `impl_chrome_view!(ViewType, framed)` also adds `framed()` and `framed_mut()`.
///
/// This eliminates per-view type aliases and the identical 2–4 method blocks that
/// were previously copy-pasted across every settings page module.
macro_rules! impl_chrome_view {
    ($view:ty) => {
        impl $view {
            pub(crate) fn content_only(
                &self,
            ) -> $crate::bottom_pane::chrome_view::ContentOnly<'_, $view> {
                $crate::bottom_pane::chrome_view::ContentOnly::new(self)
            }

            pub(crate) fn content_only_mut(
                &mut self,
            ) -> $crate::bottom_pane::chrome_view::ContentOnlyMut<'_, $view> {
                $crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
            }
        }
    };
    ($view:ty, framed) => {
        impl $view {
            pub(crate) fn framed(
                &self,
            ) -> $crate::bottom_pane::chrome_view::Framed<'_, $view> {
                $crate::bottom_pane::chrome_view::Framed::new(self)
            }

            pub(crate) fn content_only(
                &self,
            ) -> $crate::bottom_pane::chrome_view::ContentOnly<'_, $view> {
                $crate::bottom_pane::chrome_view::ContentOnly::new(self)
            }

            #[allow(dead_code)] // used in tests via framed_mut()
            pub(crate) fn framed_mut(
                &mut self,
            ) -> $crate::bottom_pane::chrome_view::FramedMut<'_, $view> {
                $crate::bottom_pane::chrome_view::FramedMut::new(self)
            }

            pub(crate) fn content_only_mut(
                &mut self,
            ) -> $crate::bottom_pane::chrome_view::ContentOnlyMut<'_, $view> {
                $crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
            }
        }
    };
}

pub(crate) use impl_chrome_view;
