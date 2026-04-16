/// Generate the boilerplate `ChromeRenderable`, `ChromeMouseHandler`, and
/// standard `BottomPaneView` trait implementations for a settings page view.
///
/// # Height variants
///
/// - `height = { <expr> }` — inline expression (may reference `self`)
/// - `height_fn = <method>` — delegates to `self.<method>(width)`
///
/// # Complete variants
///
/// - `complete_field = <ident>` — generates `self.<ident>` (field access)
/// - `complete_fn = <ident>` — generates `self.<ident>()` (method call)
///
/// # Optional trailing features (order matters: paste → hover → as_any)
///
/// - `paste = <method>` — generates `handle_paste` delegating to `self.<method>(text)`
/// - `hover = <method>` — generates `update_hover` delegating to
///   `self.<method>(mouse_pos, area)`
/// - `as_any = yes` — generates `as_any` / `as_any_mut` returning `Some(self)`
macro_rules! impl_settings_pane {
    // ════════════════════════════════════════════════════════════════════
    // Entry points — 4 combos: (height | height_fn) × (complete_field | complete_fn)
    // Optional trailing: paste, hover, as_any (in that order).
    // ════════════════════════════════════════════════════════════════════

    // height + complete_field [+ options]
    ($view_ty:ty, $key_method:ident,
     height = { $($height:tt)* }, complete_field = $cf:ident
     $(, paste = $p:ident)? $(, hover = $hm:ident)? $(, as_any = $asany:ident)?
    ) => {
        impl_settings_pane!(@chrome $view_ty);
        impl_settings_pane!(@view $view_ty, $key_method,
            @h_expr { $($height)* }, @c_field $cf
            $(, @paste $p)? $(, @hover $hm)? $(, @as_any $asany)?);
    };

    // height + complete_fn [+ options]
    ($view_ty:ty, $key_method:ident,
     height = { $($height:tt)* }, complete_fn = $cm:ident
     $(, paste = $p:ident)? $(, hover = $hm:ident)? $(, as_any = $asany:ident)?
    ) => {
        impl_settings_pane!(@chrome $view_ty);
        impl_settings_pane!(@view $view_ty, $key_method,
            @h_expr { $($height)* }, @c_fn $cm
            $(, @paste $p)? $(, @hover $hm)? $(, @as_any $asany)?);
    };

    // height_fn + complete_field [+ options]
    ($view_ty:ty, $key_method:ident,
     height_fn = $hf:ident, complete_field = $cf:ident
     $(, paste = $p:ident)? $(, hover = $hm:ident)? $(, as_any = $asany:ident)?
    ) => {
        impl_settings_pane!(@chrome $view_ty);
        impl_settings_pane!(@view $view_ty, $key_method,
            @h_fn $hf, @c_field $cf
            $(, @paste $p)? $(, @hover $hm)? $(, @as_any $asany)?);
    };

    // height_fn + complete_fn [+ options]
    ($view_ty:ty, $key_method:ident,
     height_fn = $hf:ident, complete_fn = $cm:ident
     $(, paste = $p:ident)? $(, hover = $hm:ident)? $(, as_any = $asany:ident)?
    ) => {
        impl_settings_pane!(@chrome $view_ty);
        impl_settings_pane!(@view $view_ty, $key_method,
            @h_fn $hf, @c_fn $cm
            $(, @paste $p)? $(, @hover $hm)? $(, @as_any $asany)?);
    };

    // ════════════════════════════════════════════════════════════════════
    // ChromeRenderable + ChromeMouseHandler (shared, no `self` issues)
    // ════════════════════════════════════════════════════════════════════
    (@chrome $view_ty:ty) => {
        impl crate::bottom_pane::chrome_view::ChromeRenderable for $view_ty {
            fn render_in_framed_chrome(
                &self,
                area: ratatui::layout::Rect,
                buf: &mut ratatui::buffer::Buffer,
            ) {
                self.render_framed(area, buf);
            }

            fn render_in_content_only_chrome(
                &self,
                area: ratatui::layout::Rect,
                buf: &mut ratatui::buffer::Buffer,
            ) {
                self.render_content_only(area, buf);
            }
        }

        impl crate::bottom_pane::chrome_view::ChromeMouseHandler for $view_ty {
            fn handle_mouse_event_direct_in_framed_chrome(
                &mut self,
                mouse_event: crossterm::event::MouseEvent,
                area: ratatui::layout::Rect,
            ) -> bool {
                self.handle_mouse_event_direct_framed(mouse_event, area)
            }

            fn handle_mouse_event_direct_in_content_only_chrome(
                &mut self,
                mouse_event: crossterm::event::MouseEvent,
                area: ratatui::layout::Rect,
            ) -> bool {
                self.handle_mouse_event_direct_content_only(mouse_event, area)
            }
        }
    };

    // ════════════════════════════════════════════════════════════════════
    // BottomPaneView impls — 4 height×complete combos, each with optional
    // paste/hover/as_any via $()? repetitions.
    // ════════════════════════════════════════════════════════════════════

    // ── h_expr + c_field ───────────────────────────────────────────────
    (@view $view_ty:ty, $km:ident, @h_expr { $($h:tt)* }, @c_field $cf:ident
     $(, @paste $p:ident)? $(, @hover $hm:ident)? $(, @as_any $asany:ident)?
    ) => {
        impl<'a> crate::bottom_pane::BottomPaneView<'a> for $view_ty {
            impl_settings_pane!(@key_methods $km);
            impl_settings_pane!(@mouse_method);
            $(impl_settings_pane!(@paste_method $p);)?
            $(impl_settings_pane!(@hover_method $hm);)?
            $(impl_settings_pane!(@as_any_methods $asany);)?
            fn is_complete(&self) -> bool { self.$cf }
            fn desired_height(&self, _width: u16) -> u16 { $($h)* }
            impl_settings_pane!(@render_method);
        }
    };

    // ── h_expr + c_fn ──────────────────────────────────────────────────
    (@view $view_ty:ty, $km:ident, @h_expr { $($h:tt)* }, @c_fn $cm:ident
     $(, @paste $p:ident)? $(, @hover $hm:ident)? $(, @as_any $asany:ident)?
    ) => {
        impl<'a> crate::bottom_pane::BottomPaneView<'a> for $view_ty {
            impl_settings_pane!(@key_methods $km);
            impl_settings_pane!(@mouse_method);
            $(impl_settings_pane!(@paste_method $p);)?
            $(impl_settings_pane!(@hover_method $hm);)?
            $(impl_settings_pane!(@as_any_methods $asany);)?
            fn is_complete(&self) -> bool { self.$cm() }
            fn desired_height(&self, _width: u16) -> u16 { $($h)* }
            impl_settings_pane!(@render_method);
        }
    };

    // ── h_fn + c_field ─────────────────────────────────────────────────
    (@view $view_ty:ty, $km:ident, @h_fn $hf:ident, @c_field $cf:ident
     $(, @paste $p:ident)? $(, @hover $hm:ident)? $(, @as_any $asany:ident)?
    ) => {
        impl<'a> crate::bottom_pane::BottomPaneView<'a> for $view_ty {
            impl_settings_pane!(@key_methods $km);
            impl_settings_pane!(@mouse_method);
            $(impl_settings_pane!(@paste_method $p);)?
            $(impl_settings_pane!(@hover_method $hm);)?
            $(impl_settings_pane!(@as_any_methods $asany);)?
            fn is_complete(&self) -> bool { self.$cf }
            fn desired_height(&self, width: u16) -> u16 { self.$hf(width) }
            impl_settings_pane!(@render_method);
        }
    };

    // ── h_fn + c_fn ────────────────────────────────────────────────────
    (@view $view_ty:ty, $km:ident, @h_fn $hf:ident, @c_fn $cm:ident
     $(, @paste $p:ident)? $(, @hover $hm:ident)? $(, @as_any $asany:ident)?
    ) => {
        impl<'a> crate::bottom_pane::BottomPaneView<'a> for $view_ty {
            impl_settings_pane!(@key_methods $km);
            impl_settings_pane!(@mouse_method);
            $(impl_settings_pane!(@paste_method $p);)?
            $(impl_settings_pane!(@hover_method $hm);)?
            $(impl_settings_pane!(@as_any_methods $asany);)?
            fn is_complete(&self) -> bool { self.$cm() }
            fn desired_height(&self, width: u16) -> u16 { self.$hf(width) }
            impl_settings_pane!(@render_method);
        }
    };

    // ════════════════════════════════════════════════════════════════════
    // Shared method fragments (expanded inside impl blocks)
    // ════════════════════════════════════════════════════════════════════

    (@key_methods $km:ident) => {
        fn handle_key_event(
            &mut self,
            _pane: &mut crate::bottom_pane::BottomPane<'a>,
            key_event: crossterm::event::KeyEvent,
        ) {
            let _ = self.$km(key_event);
        }

        fn handle_key_event_with_result(
            &mut self,
            _pane: &mut crate::bottom_pane::BottomPane<'a>,
            key_event: crossterm::event::KeyEvent,
        ) -> crate::bottom_pane::ConditionalUpdate {
            crate::ui_interaction::redraw_if(self.$km(key_event))
        }
    };

    (@mouse_method) => {
        fn handle_mouse_event(
            &mut self,
            _pane: &mut crate::bottom_pane::BottomPane<'a>,
            mouse_event: crossterm::event::MouseEvent,
            area: ratatui::layout::Rect,
        ) -> crate::bottom_pane::ConditionalUpdate {
            crate::ui_interaction::redraw_if(
                crate::bottom_pane::chrome_view::FramedMut::new(self)
                    .handle_mouse_event_direct(mouse_event, area),
            )
        }
    };

    (@paste_method $p:ident) => {
        fn handle_paste(
            &mut self,
            text: String,
        ) -> crate::bottom_pane::ConditionalUpdate {
            crate::ui_interaction::redraw_if(self.$p(text))
        }
    };

    (@hover_method $hm:ident) => {
        fn update_hover(
            &mut self,
            mouse_pos: (u16, u16),
            area: ratatui::layout::Rect,
        ) -> bool {
            self.$hm(mouse_pos, area)
        }
    };

    (@as_any_methods $_marker:ident) => {
        fn as_any(&self) -> Option<&dyn std::any::Any> {
            Some(self)
        }

        fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
            Some(self)
        }
    };

    (@render_method) => {
        fn render(&self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
            crate::bottom_pane::chrome_view::Framed::new(self).render(area, buf);
        }
    };
}
