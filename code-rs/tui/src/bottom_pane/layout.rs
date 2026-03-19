use ratatui::layout::Rect;

use super::panes::auto_coordinator::AutoCoordinatorView;
use super::{ActiveViewKind, BottomPane, BottomPaneView};

impl<'a> BottomPane<'a> {
    pub fn desired_height(&self, width: u16) -> u16 {
        let (view_height, pad_lines) = if let Some(view) = self.active_view.as_ref() {
            let is_auto = matches!(self.active_view_kind, ActiveViewKind::AutoCoordinator);
            let top_spacer = if is_auto {
                0
            } else if self.top_spacer_enabled {
                1
            } else {
                0
            };
            let composer_height = if is_auto {
                let composer_visible = view
                    .as_ref()
                    .as_any()
                    .and_then(|any| any.downcast_ref::<AutoCoordinatorView>())
                    .map(AutoCoordinatorView::composer_visible)
                    .unwrap_or(true);
                if composer_visible {
                    self.composer.desired_height(width)
                } else {
                    self.composer.footer_height()
                }
            } else {
                0
            };
            let pad = BottomPane::BOTTOM_PAD_LINES;
            let base_height = view
                .desired_height(width)
                .saturating_add(top_spacer)
                .saturating_add(composer_height);

            (base_height, pad)
        } else {
            // Optionally add 1 for the empty line above the composer
            let spacer: u16 = if self.top_spacer_enabled { 1 } else { 0 };
            (
                spacer.saturating_add(self.composer.desired_height(width)),
                Self::BOTTOM_PAD_LINES,
            )
        };

        view_height.saturating_add(pad_lines)
    }

    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        // Hide the cursor whenever any overlay view is active.
        if self.active_view.is_some() {
            None
        } else {
            let composer_rect = compute_composer_rect(area, self.top_spacer_enabled);
            self.composer.cursor_pos(composer_rect)
        }
    }

    pub(super) fn compute_active_view_rect(
        &self,
        area: Rect,
        view: &dyn BottomPaneView<'a>,
    ) -> Option<Rect> {
        let horizontal_padding = BottomPane::HORIZONTAL_PADDING;
        if matches!(self.active_view_kind, ActiveViewKind::AutoCoordinator) {
            let content_width = area.width.saturating_sub(horizontal_padding * 2);
            let composer_visible = view
                .as_any()
                .and_then(|any| any.downcast_ref::<AutoCoordinatorView>())
                .map(AutoCoordinatorView::composer_visible)
                .unwrap_or(true);
            let composer_height = if composer_visible {
                self.composer.desired_height(area.width)
            } else {
                self.composer.footer_height()
            };
            let max_view_height = area
                .height
                .saturating_sub(composer_height)
                .saturating_sub(BottomPane::BOTTOM_PAD_LINES);
            let view_height = view.desired_height(content_width).min(max_view_height);
            if view_height == 0 {
                return None;
            }
            return Some(Rect {
                x: area.x.saturating_add(horizontal_padding),
                y: area.y,
                width: content_width,
                height: view_height,
            });
        }

        let mut avail = area.height;
        if self.top_spacer_enabled && avail > 0 {
            avail = avail.saturating_sub(1);
        }
        let pad = BottomPane::BOTTOM_PAD_LINES.min(avail.saturating_sub(1));
        let view_height = avail.saturating_sub(pad);
        if view_height == 0 {
            return None;
        }
        let y_base = if self.top_spacer_enabled {
            area.y.saturating_add(1)
        } else {
            area.y
        };
        Some(Rect {
            x: area.x.saturating_add(horizontal_padding),
            y: y_base,
            width: area.width.saturating_sub(horizontal_padding * 2),
            height: view_height,
        })
    }
}

pub(super) fn compute_composer_rect(area: Rect, top_spacer_enabled: bool) -> Rect {
    let horizontal_padding = BottomPane::HORIZONTAL_PADDING;
    let mut y_offset = 0u16;
    if top_spacer_enabled {
        y_offset = y_offset.saturating_add(1);
    }
    let available = area.height.saturating_sub(y_offset);
    let height =
        available.saturating_sub(BottomPane::BOTTOM_PAD_LINES.min(available.saturating_sub(1)));
    Rect {
        x: area.x.saturating_add(horizontal_padding),
        y: area.y.saturating_add(y_offset),
        width: area.width.saturating_sub(horizontal_padding * 2),
        height,
    }
}
