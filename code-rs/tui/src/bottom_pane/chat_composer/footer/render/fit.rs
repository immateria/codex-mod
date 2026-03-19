use ratatui::style::Style;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

use super::sections::BuiltFooterSections;
use super::{
    span_width,
    spans_start_with_bullet,
    SectionPriority,
    FOOTER_TRAILING_PAD,
};

pub(super) fn fit_sections_to_width(
    total_width: usize,
    label_style: Style,
    built: &BuiltFooterSections,
) -> (Vec<Span<'static>>, Vec<Span<'static>>, usize, usize) {
    let left_sections = &built.left_sections;
    let right_sections = &built.right_sections;

    let mut include_tokens = built.include_tokens;
    let mut token_use_compact = false;
    let token_spans_full = &built.token_spans_full;
    let token_spans_compact = &built.token_spans_compact;

    let separator = Span::from("  •  ").style(label_style);
    let separator_len = UnicodeWidthStr::width(separator.content.as_ref());

    let base_left_pad = Span::from("  ").style(label_style);
    let base_left_pad_len = UnicodeWidthStr::width(base_left_pad.content.as_ref());

    let leading_bullet_pad = Span::from(" ").style(label_style);
    let leading_bullet_pad_len = UnicodeWidthStr::width(leading_bullet_pad.content.as_ref());

    let mut include_auto_review_status = left_sections.iter().any(|section| {
        section.priority == SectionPriority::AutoReview && section.enabled
    });
    let mut include_auto_review_agent_hint = right_sections.iter().any(|section| {
        section.priority == SectionPriority::AutoReview && section.enabled
    });
    let mut include_access_mode = true;
    let mut include_footer_hint = true;
    let mut include_ctrl_c = built.ctrl_c_present;
    let mut include_editor = built.editor_present;
    let mut include_right_other = true;

    let build_left = |
        include_auto_review_status: bool,
        include_access_mode: bool,
        include_footer_hint: bool,
        include_ctrl_c: bool,
    | -> (Vec<Span<'static>>, usize) {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut len = 0usize;
        let mut last_section_was_separator = false;

        let is_separator = |section: &[Span<'static>]| {
            section.len() == 1 && section[0].content.trim() == "•"
        };

        for section in left_sections {
            let include = match section.priority {
                SectionPriority::AutoReview => include_auto_review_status && section.enabled,
                SectionPriority::CtrlCQuit => include_ctrl_c && section.enabled,
                SectionPriority::AccessMode => include_access_mode && section.enabled,
                SectionPriority::FooterHint => include_footer_hint && section.enabled,
                _ => section.enabled,
            };
            if !include {
                continue;
            }

            let section_is_separator = is_separator(&section.spans);
            if !spans.is_empty()
                && !last_section_was_separator
                && !section_is_separator
            {
                let pad_text = if spans_start_with_bullet(&section.spans) {
                    "  "
                } else {
                    "   "
                };
                let pad = Span::from(pad_text).style(label_style);
                len += UnicodeWidthStr::width(pad.content.as_ref());
                spans.push(pad);
            }

            spans.extend(section.spans.clone());
            len += span_width(&section.spans);
            last_section_was_separator = section_is_separator;
        }

        (spans, len)
    };

    let build_right = |
        include_tokens: bool,
        use_compact_tokens: bool,
        include_auto_review_agent_hint: bool,
        include_editor: bool,
        include_right_other: bool,
    | -> (Vec<Span<'static>>, usize) {
        let mut assembled: Vec<Span<'static>> = Vec::new();
        let mut len = 0usize;

        let token_spans = if use_compact_tokens {
            token_spans_compact
        } else {
            token_spans_full
        };
        if include_tokens && !token_spans.is_empty() {
            assembled.extend(token_spans.clone());
            len += span_width(token_spans);
        }

        for section in right_sections {
            let include = match section.priority {
                SectionPriority::AutoReview => include_auto_review_agent_hint && section.enabled,
                SectionPriority::Editor => include_editor && section.enabled,
                SectionPriority::RightOther => include_right_other && section.enabled,
                _ => section.enabled,
            };
            if !include || section.spans.is_empty() {
                continue;
            }

            if !assembled.is_empty() {
                assembled.push(separator.clone());
                len += separator_len;
            }
            assembled.extend(section.spans.clone());
            len += span_width(&section.spans);
        }

        (assembled, len)
    };

    let mut removal_stage = 0usize;
    let mut left_len;
    let mut right_len;

    let (final_left, final_right) = loop {
        let (left_spans_eval, l_len) = build_left(
            include_auto_review_status,
            include_access_mode,
            include_footer_hint,
            include_ctrl_c,
        );
        let (right_spans_eval, r_len) = build_right(
            include_tokens,
            token_use_compact,
            include_auto_review_agent_hint,
            include_editor,
            include_right_other,
        );

        let add_base_pad = include_auto_review_status && !left_spans_eval.is_empty();
        let add_leading_bullet_pad = built.auto_drive_active
            && !include_auto_review_status
            && spans_start_with_bullet(&left_spans_eval);
        left_len = l_len
            + if add_base_pad { base_left_pad_len } else { 0 }
            + if add_leading_bullet_pad {
                leading_bullet_pad_len
            } else {
                0
            };
        right_len = r_len;
        let total_len = left_len + right_len + FOOTER_TRAILING_PAD;

        if total_len <= total_width {
            let mut with_pad = left_spans_eval;
            if add_base_pad {
                with_pad.insert(0, base_left_pad.clone());
            }
            if add_leading_bullet_pad {
                with_pad.insert(0, leading_bullet_pad.clone());
            }
            break (with_pad, right_spans_eval);
        }

        if include_tokens && !token_use_compact && !token_spans_compact.is_empty() {
            token_use_compact = true;
            continue;
        }

        match removal_stage {
            0 => {
                include_right_other = false;
            }
            1 => {
                include_access_mode = false;
            }
            2 => {
                include_footer_hint = false;
            }
            3 => {
                include_auto_review_agent_hint = false;
            }
            4 => {
                include_editor = false;
            }
            5 => {
                include_auto_review_status = false;
                include_auto_review_agent_hint = false;
            }
            6 => {
                include_ctrl_c = false;
            }
            7 => {
                include_tokens = false;
            }
            _ => {
                let mut with_pad = left_spans_eval;
                if add_base_pad {
                    with_pad.insert(0, base_left_pad.clone());
                }
                if add_leading_bullet_pad {
                    with_pad.insert(0, leading_bullet_pad.clone());
                }
                break (with_pad, right_spans_eval);
            }
        }

        removal_stage += 1;
    };

    (final_left, final_right, left_len, right_len)
}
