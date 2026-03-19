use super::buttons;
use super::header;
use super::prompt;
use super::VariantContext;
use super::super::{AutoActiveViewModel, AutoCoordinatorView};

pub(super) fn estimated_height_active_inner(
    view: &AutoCoordinatorView,
    width: u16,
    model: &AutoActiveViewModel,
    composer_height: u16,
) -> u16 {
    let ctx = buttons::build_context(model);
    estimated_height_active_with_context_inner(view, width, &ctx, model, composer_height)
}

pub(super) fn estimated_height_active_with_context_inner(
    view: &AutoCoordinatorView,
    width: u16,
    ctx: &VariantContext,
    model: &AutoActiveViewModel,
    composer_height: u16,
) -> u16 {
    let mut total = 1usize // blank spacer row
        .saturating_add(AutoCoordinatorView::HEADER_HEIGHT as usize);

    if !model.awaiting_submission {
        return total.min(u16::MAX as usize) as u16;
    }

    let inner_width = inner_width(width);
    let prompt_lines = prompt::cli_prompt_lines(model);

    if !model.editing_prompt {
        let block_lines = prompt::pending_prompt_content_lines(view, model, inner_width as usize).len();
        if block_lines > 0 {
            total = total.saturating_add(block_lines.saturating_add(2));
        }
    }

    let mut button_height = 0usize;
    if ctx.button.is_some() {
        button_height = 3;
    }

    if model.editing_prompt {
        let display_message = header::resolve_display_message(view, model);
        total = total.saturating_add(prompt::status_message_wrap_count(
            view,
            inner_width,
            &display_message,
        ));

        if let Some(ref lines) = prompt_lines {
            let prompt_height = AutoCoordinatorView::lines_height(lines, inner_width) as usize;
            total = total.saturating_add(prompt_height);
            if prompt_height > 0 && ctx.button.is_some() {
                total = total.saturating_add(1); // spacer before button
            }
        }

        if ctx.manual_hint.is_some() {
            let hint_height = ctx
                .manual_hint
                .as_ref()
                .map(|text| AutoCoordinatorView::wrap_count(text, inner_width))
                .unwrap_or(0)
                .max(1);
            total = total.saturating_add(hint_height);
        }

        let ctrl_hint = ctx.ctrl_hint.trim();
        if ctx.button.is_none() && !ctrl_hint.is_empty() {
            let ctrl_height = AutoCoordinatorView::wrap_count(ctrl_hint, inner_width).max(1);
            total = total.saturating_add(1); // spacer before ctrl hint
            total = total.saturating_add(ctrl_height);
        }
    }

    if button_height > 0 {
        total = total.saturating_add(button_height);
    }

    let composer_block = usize::from(composer_height);
    if composer_block > 0 {
        total = total.saturating_add(composer_block);
    }

    total.min(u16::MAX as usize) as u16
}

fn inner_width(width: u16) -> u16 {
    width.max(1)
}

