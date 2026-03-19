use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, WidgetRef, Wrap};

use crate::app_event::AppEvent;
use crate::bottom_pane::ChatComposer;
use crate::colors;

use super::buttons;
use super::clear;
use super::estimation;
use super::header;
use super::prompt;
use super::super::{AutoActiveViewModel, AutoCoordinatorView};
use super::HeaderRenderParams;

pub(super) fn render_active_inner(
    view: &AutoCoordinatorView,
    area: Rect,
    buf: &mut Buffer,
    model: &AutoActiveViewModel,
    composer: Option<&ChatComposer>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let frame_style = view.style.frame.clone();
    if model.started_at.is_some()
        && (model.waiting_for_response
            || model.awaiting_submission
            || model.waiting_for_review
            || model.cli_running)
    {
        view.app_event_tx
            .send(AppEvent::ScheduleFrameIn(Duration::from_secs(1)));
    }
    let display_message = header::resolve_display_message(view, model);
    let intro = header::intro_state(frame_style.title_text, model);
    if let Some(delay) = intro.schedule_next_in {
        view.app_event_tx.send(AppEvent::ScheduleFrameIn(delay));
    }

    let ctx = buttons::build_context(model);

    let composer_visible = model.show_composer && composer.is_some();
    let mut view_origin = area.y;
    let mut view_height = area.height;

    if !composer_visible {
        let expected = estimation::estimated_height_active_with_context_inner(view, area.width, &ctx, model, 0)
            .min(view_height);
        let offset = view_height.saturating_sub(expected.max(1));
        view_origin = view_origin.saturating_add(offset);
        view_height = view_height.saturating_sub(offset);
    }

    if view_height == 0 {
        return;
    }

    // Draw spacer row to match composer spacing.
    let spacer_row = Rect {
        x: area.x,
        y: view_origin,
        width: area.width,
        height: 1,
    };
    clear::clear_row(spacer_row, buf);

    if view_height <= 1 {
        return;
    }

    let header_height = AutoCoordinatorView::HEADER_HEIGHT.min(view_height.saturating_sub(1));
    let header_area = Rect {
        x: area.x,
        y: view_origin + 1,
        width: area.width,
        height: header_height,
    };
    let header_label = intro.header_text.as_ref();
    header::render_header(
        view,
        buf,
        HeaderRenderParams {
            area: header_area,
            model,
            frame_style: &frame_style,
            display_message: &display_message,
            header_label,
            full_title: frame_style.title_text,
            intro: &intro,
        },
    );

    if view_height <= 1 + AutoCoordinatorView::HEADER_HEIGHT {
        return;
    }

    let mut inner = Rect {
        x: area.x,
        y: view_origin + 1 + AutoCoordinatorView::HEADER_HEIGHT,
        width: area.width,
        height: view_height
            .saturating_sub(1)
            .saturating_sub(AutoCoordinatorView::HEADER_HEIGHT),
    };
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    if !model.awaiting_submission {
        return;
    }

    if !intro.body_visible {
        clear::clear_rect(inner, buf);
        return;
    }

    let mut prompt_lines = prompt::cli_prompt_lines(model);
    let has_pending_prompt = prompt::has_pending_prompt_content(model);
    let mut top_lines: Vec<Line<'static>> = Vec::new();
    let mut after_lines: Vec<Line<'static>> = Vec::new();
    let mut button_block = buttons::button_block_lines(view, &ctx);

    if !model.editing_prompt {
        if has_pending_prompt {
            let base_y = inner.y;
            let base_height = inner.height;
            let used = prompt::render_pending_prompt_block(view, inner, buf, model);
            if used >= base_height {
                return;
            }
            if used > 0 {
                let new_y = base_y + used;
                let remaining_height = base_height.saturating_sub(new_y.saturating_sub(base_y));
                inner = Rect {
                    x: inner.x,
                    y: new_y,
                    width: inner.width,
                    height: remaining_height,
                };
            } else {
                clear::clear_rect(inner, buf);
            }
        } else {
            clear::clear_rect(inner, buf);
        }
    } else if let Some(mut lines) = prompt_lines.take() {
        top_lines.append(&mut lines);
    }

    if model.editing_prompt {
        if let Some(hint_line) = buttons::manual_hint_line(&ctx) {
            if !after_lines.is_empty() {
                after_lines.push(Line::default());
            }
            after_lines.push(hint_line);
        }

        if let Some(ctrl_hint_line) = buttons::ctrl_hint_line(&ctx) {
            if !after_lines.is_empty() {
                after_lines.push(Line::default());
            }
            after_lines.push(ctrl_hint_line);
        }

        if let Some(progress_text) = header::compose_status_line(model) {
            let line = Line::from(Span::styled(
                progress_text,
                Style::default().fg(colors::text()),
            ));
            if top_lines.is_empty() {
                top_lines.push(line);
            } else {
                top_lines.insert(0, line);
            }
        }

        if let Some(line) = prompt::status_message_line(view, &display_message) {
            top_lines.push(Line::default());
            top_lines.push(line);
        }

        if let Some(ref mut block) = button_block {
            if !top_lines.is_empty() {
                top_lines.push(Line::default());
            }
            top_lines.append(block);
        }
    } else if let Some(ref mut block) = button_block {
        if !after_lines.is_empty() {
            after_lines.push(Line::default());
        }
        after_lines.append(block);
    }

    let mut top_height = AutoCoordinatorView::lines_height(&top_lines, inner.width);
    let mut after_height = AutoCoordinatorView::lines_height(&after_lines, inner.width);

    // Composer desired height is computed with a slightly wider width to account for
    // border and selection marker spacing, so the "on paper" height estimation matches render-time wrapping exactly.
    let mut composer_block: u16 = if model.show_composer {
        if let Some(composer) = composer {
            let measurement_width = inner.width.saturating_add(2);
            let mut desired_block = composer.desired_height(measurement_width);
            if desired_block < AutoCoordinatorView::MIN_COMPOSER_VIEWPORT {
                desired_block = AutoCoordinatorView::MIN_COMPOSER_VIEWPORT;
            }
            desired_block
        } else {
            0
        }
    } else {
        0
    };

    let total_needed = top_height as usize + after_height as usize + composer_block as usize;

    if total_needed > inner.height as usize {
        let mut deficit = total_needed - inner.height as usize;

        let reduce_after = usize::from(after_height).min(deficit);
        after_height = after_height.saturating_sub(reduce_after as u16);
        deficit -= reduce_after;

        let reduce_top = usize::from(top_height).min(deficit);
        top_height = top_height.saturating_sub(reduce_top as u16);
        deficit -= reduce_top;

        if deficit > 0 && model.show_composer {
            let reducible = composer_block.saturating_sub(AutoCoordinatorView::MIN_COMPOSER_VIEWPORT);
            let reduce_composer = usize::from(reducible).min(deficit);
            composer_block = composer_block.saturating_sub(reduce_composer as u16);
        }
    }

    let composer_height = if model.show_composer && composer.is_some() {
        let max_space_for_composer =
            inner.height.saturating_sub(top_height).saturating_sub(after_height);

        if max_space_for_composer == 0 {
            1
        } else {
            composer_block.min(max_space_for_composer).max(1)
        }
    } else {
        0
    };

    if composer_height == 0 {
        let used_height = top_height.saturating_add(after_height);
        if used_height > 0 && used_height < inner.height {
            let offset = inner.height - used_height;
            inner.y = inner.y.saturating_add(offset);
            inner.height = used_height;
        }
    }

    let mut cursor_y = inner.y;
    if top_height > 0 {
        let max_height = inner.y + inner.height - cursor_y;
        let rect_height = top_height.min(max_height);
        if rect_height > 0 {
            let top_rect = Rect {
                x: inner.x,
                y: cursor_y,
                width: inner.width,
                height: rect_height,
            };
            Paragraph::new(top_lines.clone())
                .wrap(Wrap { trim: true })
                .render(top_rect, buf);
            cursor_y = cursor_y.saturating_add(rect_height);
        }
    }

    if composer_height > 0 && cursor_y < inner.y + inner.height && let Some(composer) = composer {
        let max_height = inner.y + inner.height - cursor_y;
        let rect_height = composer_height.min(max_height);
        if rect_height > 0 {
            let composer_rect = Rect {
                x: inner.x,
                y: cursor_y,
                width: inner.width,
                height: rect_height,
            };
            composer.render_ref(composer_rect, buf);
            cursor_y = cursor_y.saturating_add(rect_height);
        }
    }

    if after_height > 0 && cursor_y < inner.y + inner.height {
        let max_height = inner
            .y
            .saturating_add(inner.height)
            .saturating_sub(cursor_y);
        let rect_height = after_height.min(max_height);
        if rect_height > 0 {
            let after_rect = Rect {
                x: inner.x,
                y: cursor_y,
                width: inner.width,
                height: rect_height,
            };
            Paragraph::new(after_lines.clone())
                .wrap(Wrap { trim: true })
                .render(after_rect, buf);
        }
    }
}

