use std::time::Instant;

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use super::{ChatComposer, FooterSection, SectionPriority};

pub(super) struct BuiltFooterSections {
    pub(super) left_sections: Vec<FooterSection>,
    pub(super) right_sections: Vec<FooterSection>,
    pub(super) token_spans_full: Vec<Span<'static>>,
    pub(super) token_spans_compact: Vec<Span<'static>>,
    pub(super) include_tokens: bool,
    pub(super) ctrl_c_present: bool,
    pub(super) editor_present: bool,
    pub(super) auto_drive_active: bool,
}

pub(super) fn build_sections(
    view: &ChatComposer,
    now: Instant,
    key_hint_style: Style,
    label_style: Style,
) -> BuiltFooterSections {
    let mut left_sections: Vec<FooterSection> = Vec::new();
    let mut right_sections: Vec<FooterSection> = Vec::new();

    let mut auto_review_status_spans: Vec<Span<'static>> = Vec::new();
    let mut auto_review_agent_hint: Vec<Span<'static>> = Vec::new();
    if let Some(status) = view.auto_review_status {
        let (status_spans, agent_hint_spans) =
            ChatComposer::auto_review_footer_sections(status, view.agent_hint_label);
        auto_review_status_spans = status_spans;
        auto_review_agent_hint = agent_hint_spans;
    }

    if !auto_review_status_spans.is_empty() {
        left_sections.push(FooterSection {
            priority: SectionPriority::AutoReview,
            spans: auto_review_status_spans,
            enabled: true,
        });
    }

    if !auto_review_agent_hint.is_empty() {
        right_sections.push(FooterSection {
            priority: SectionPriority::AutoReview,
            spans: auto_review_agent_hint,
            enabled: true,
        });
    }

    let show_access_label = if let Some(until) = view.access_mode_label_expiry {
        now <= until
    } else {
        true
    };
    let mut left_misc_before_ctrlc: Vec<Span<'static>> = Vec::new();
    if show_access_label && !view.auto_drive_active
        && let Some(label) = &view.access_mode_label
    {
        left_misc_before_ctrlc.push(Span::from(label.clone()).style(label_style));
        let show_suffix = if let Some(until) = view.access_mode_hint_expiry {
            now <= until
        } else {
            view.access_mode_label_expiry.is_some()
        };
        if show_suffix {
            left_misc_before_ctrlc.push(Span::from("  (").style(label_style));
            left_misc_before_ctrlc.push(Span::from("Shift+Tab").style(key_hint_style));
            left_misc_before_ctrlc.push(Span::from(" change)").style(label_style));
        }
    }

    let mut ctrl_c_spans: Vec<Span<'static>> = Vec::new();
    if view.ctrl_c_quit_hint {
        if !left_misc_before_ctrlc.is_empty() {
            ctrl_c_spans.push(Span::from("   "));
        }
        ctrl_c_spans.push(Span::from("Ctrl+C").style(key_hint_style));
        ctrl_c_spans.push(Span::from(" again to quit").style(label_style));
    }
    let ctrl_c_present = !ctrl_c_spans.is_empty();

    let mut left_misc_after_ctrlc: Vec<Span<'static>> = Vec::new();
    if let Some(hint) = &view.standard_terminal_hint {
        if view.auto_drive_active {
            let (left_hint, right_hint) = match hint.split_once('\t') {
                Some((left, right)) => (left.trim().to_string(), Some(right.trim().to_string())),
                None => (hint.trim().to_string(), None),
            };

            let auto_label_style = Style::default().fg(crate::colors::text_dim());
            let auto_key_style = Style::default().fg(crate::colors::info());

            if !left_hint.is_empty() {
                if !left_misc_after_ctrlc.is_empty() {
                    left_misc_after_ctrlc
                        .push(Span::from("   ").style(auto_label_style));
                }
                let spans = ChatComposer::build_auto_drive_hint_spans(
                    &left_hint,
                    auto_key_style,
                    auto_label_style,
                );
                left_misc_after_ctrlc.extend(spans);
            }

            if let Some(right_hint) = right_hint
                && !right_hint.is_empty()
            {
                let spans = ChatComposer::build_auto_drive_hint_spans(
                    &right_hint,
                    auto_key_style,
                    auto_label_style,
                );
                if !spans.is_empty() {
                    right_sections.push(FooterSection {
                        priority: SectionPriority::RightOther,
                        spans,
                        enabled: true,
                    });
                }
            }
        } else {
            if !left_misc_after_ctrlc.is_empty() {
                left_misc_after_ctrlc.push(Span::from("   "));
            }
            left_misc_after_ctrlc.push(
                Span::from(hint.clone()).style(
                    Style::default()
                        .fg(crate::colors::warning())
                        .add_modifier(Modifier::BOLD),
                ),
            );
        }
    }

    if let Some((msg, until)) = &view.footer_notice
        && now <= *until
    {
        if !left_misc_after_ctrlc.is_empty() {
            left_misc_after_ctrlc.push(Span::from("   "));
        }
        left_misc_after_ctrlc.push(
            Span::from(msg.clone()).style(Style::default().add_modifier(Modifier::DIM)),
        );
    }

    if !left_misc_before_ctrlc.is_empty() {
        left_sections.push(FooterSection {
            priority: SectionPriority::AccessMode,
            spans: left_misc_before_ctrlc,
            enabled: true,
        });
    }
    if ctrl_c_present {
        left_sections.push(FooterSection {
            priority: SectionPriority::CtrlCQuit,
            spans: ctrl_c_spans,
            enabled: true,
        });
    }
    if !left_misc_after_ctrlc.is_empty() {
        left_sections.push(FooterSection {
            priority: SectionPriority::FooterHint,
            spans: left_misc_after_ctrlc,
            enabled: true,
        });
    }

    let token_spans_full: Vec<Span<'static>> = view.token_usage_spans(label_style);
    let token_spans_compact: Vec<Span<'static>> = view.token_usage_spans_compact(label_style);
    let include_tokens = !token_spans_full.is_empty() || !token_spans_compact.is_empty();

    let editor_spans: Vec<Span<'static>> = if !view.auto_drive_active && !view.ctrl_c_quit_hint {
        vec![
            Span::from("Ctrl+G").style(key_hint_style),
            Span::from(" editor").style(label_style),
        ]
    } else {
        Vec::new()
    };
    let editor_present = !editor_spans.is_empty();
    if editor_present {
        right_sections.push(FooterSection {
            priority: SectionPriority::Editor,
            spans: editor_spans,
            enabled: true,
        });
    }

    if !view.using_chatgpt_auth {
        right_sections.push(FooterSection {
            priority: SectionPriority::RightOther,
            spans: vec![Span::from("API key").style(label_style)],
            enabled: true,
        });
    }

    BuiltFooterSections {
        left_sections,
        right_sections,
        token_spans_full,
        token_spans_compact,
        include_tokens,
        ctrl_c_present,
        editor_present,
        auto_drive_active: view.auto_drive_active,
    }
}

