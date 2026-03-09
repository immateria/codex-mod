use super::*;
use unicode_width::UnicodeWidthStr;

const FOOTER_TRAILING_PAD: usize = 1;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SectionPriority {
    CtrlCQuit = 2,
    AutoReview = 3,
    Editor = 4,
    FooterHint = 5,
    AccessMode = 6,
    RightOther = 7,
}

#[derive(Clone, Debug)]
struct FooterSection {
    priority: SectionPriority,
    spans: Vec<Span<'static>>,
    enabled: bool,
}

fn span_width(spans: &[Span<'static>]) -> usize {
    spans.iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

fn spans_start_with_bullet(spans: &[Span<'static>]) -> bool {
    spans.iter()
        .find_map(|span| {
            let trimmed = span.content.trim_start();
            (!trimmed.is_empty()).then(|| trimmed.starts_with('•'))
        })
        .unwrap_or(false)
}

impl ChatComposer {
    pub(crate) fn footer_height(&self) -> u16 {
        if self.render_mode == ComposerRenderMode::FooterOnly {
            return match &self.active_popup {
                ActivePopup::Command(popup) => popup.calculate_required_height(),
                ActivePopup::File(popup) => popup.calculate_required_height(),
                ActivePopup::None => {
                    if self.standard_terminal_hint.is_some() {
                        1
                    } else {
                        0
                    }
                }
            };
        }

        match (&self.active_popup, self.embedded_mode) {
            (ActivePopup::Command(popup), _) => popup.calculate_required_height(),
            (ActivePopup::File(popup), _) => popup.calculate_required_height(),
            (ActivePopup::None, true) => 0,
            (ActivePopup::None, false) => 1,
        }
    }

    pub(crate) fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        match &self.active_popup {
            ActivePopup::Command(popup) => {
                popup.render_ref(area, buf);
            }
            ActivePopup::File(popup) => {
                popup.render_ref(area, buf);
            }
            ActivePopup::None => {
                if self.embedded_mode {
                    return;
                }

                let now = Instant::now();

                let key_hint_style = Style::default().fg(crate::colors::function());
                let label_style = Style::default().fg(crate::colors::text_dim());

                if let Some(hints) = &self.footer_hint_override {
                    let mut left_spans: Vec<Span<'static>> = vec![Span::from("  ")];
                    for (idx, (key, label)) in hints.iter().enumerate() {
                        if idx > 0 {
                            left_spans.push(Span::from("   ").style(label_style));
                        }
                        if !key.is_empty() {
                            left_spans.push(Span::from(key.clone()).style(key_hint_style));
                        }
                        if !label.is_empty() {
                            let prefix = if key.is_empty() {
                                String::new()
                            } else {
                                String::from(" ")
                            };
                            left_spans.push(
                                Span::from(format!("{prefix}{label}")).style(label_style),
                            );
                        }
                    }

                    let token_spans: Vec<Span<'static>> = self.token_usage_spans(label_style);
                    let left_len = span_width(&left_spans);
                    let right_len = span_width(&token_spans);
                    let total_width = area.width as usize;
                    let spacer = if total_width > left_len + right_len + FOOTER_TRAILING_PAD {
                        " ".repeat(total_width - left_len - right_len - FOOTER_TRAILING_PAD)
                    } else {
                        String::from(" ")
                    };

                    let mut line_spans = left_spans;
                    line_spans.push(Span::from(spacer));
                    line_spans.extend(token_spans);
                    line_spans.push(Span::from(" "));

                    Line::from(line_spans)
                        .style(
                            Style::default()
                                .fg(crate::colors::text_dim())
                                .add_modifier(Modifier::DIM),
                        )
                        .render_ref(area, buf);
                    return;
                }

                let mut left_sections: Vec<FooterSection> = Vec::new();
                let mut right_sections: Vec<FooterSection> = Vec::new();

                let mut auto_review_status_spans: Vec<Span<'static>> = Vec::new();
                let mut auto_review_agent_hint: Vec<Span<'static>> = Vec::new();
                if let Some(status) = self.auto_review_status {
                    let (status_spans, agent_hint_spans) =
                        Self::auto_review_footer_sections(status, self.agent_hint_label);
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

                let show_access_label = if let Some(until) = self.access_mode_label_expiry {
                    now <= until
                } else {
                    true
                };
                let mut left_misc_before_ctrlc: Vec<Span<'static>> = Vec::new();
                if show_access_label && !self.auto_drive_active
                    && let Some(label) = &self.access_mode_label
                {
                    left_misc_before_ctrlc.push(Span::from(label.clone()).style(label_style));
                    let show_suffix = if let Some(until) = self.access_mode_hint_expiry {
                        now <= until
                    } else {
                        self.access_mode_label_expiry.is_some()
                    };
                    if show_suffix {
                        left_misc_before_ctrlc.push(Span::from("  (").style(label_style));
                        left_misc_before_ctrlc.push(Span::from("Shift+Tab").style(key_hint_style));
                        left_misc_before_ctrlc.push(Span::from(" change)").style(label_style));
                    }
                }

                let mut ctrl_c_spans: Vec<Span<'static>> = Vec::new();
                if self.ctrl_c_quit_hint {
                    if !left_misc_before_ctrlc.is_empty() {
                        ctrl_c_spans.push(Span::from("   "));
                    }
                    ctrl_c_spans.push(Span::from("Ctrl+C").style(key_hint_style));
                    ctrl_c_spans.push(Span::from(" again to quit").style(label_style));
                }
                let ctrl_c_present = !ctrl_c_spans.is_empty();

                let mut left_misc_after_ctrlc: Vec<Span<'static>> = Vec::new();
                if let Some(hint) = &self.standard_terminal_hint {
                    if self.auto_drive_active {
                        let (left_hint, right_hint) = match hint.split_once('\t') {
                            Some((left, right)) => {
                                (left.trim().to_string(), Some(right.trim().to_string()))
                            }
                            None => (hint.trim().to_string(), None),
                        };

                        let auto_label_style = Style::default().fg(crate::colors::text_dim());
                        let auto_key_style = Style::default().fg(crate::colors::info());

                        if !left_hint.is_empty() {
                            if !left_misc_after_ctrlc.is_empty() {
                                left_misc_after_ctrlc
                                    .push(Span::from("   ").style(auto_label_style));
                            }
                            let spans = Self::build_auto_drive_hint_spans(
                                &left_hint,
                                auto_key_style,
                                auto_label_style,
                            );
                            left_misc_after_ctrlc.extend(spans);
                        }

                        if let Some(right_hint) = right_hint
                            && !right_hint.is_empty()
                        {
                            let spans = Self::build_auto_drive_hint_spans(
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

                if let Some((msg, until)) = &self.footer_notice
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

                let token_spans_full: Vec<Span<'static>> = self.token_usage_spans(label_style);
                let token_spans_compact: Vec<Span<'static>> =
                    self.token_usage_spans_compact(label_style);
                let mut include_tokens =
                    !token_spans_full.is_empty() || !token_spans_compact.is_empty();
                let mut token_use_compact = false;

                let editor_spans: Vec<Span<'static>> =
                    if !self.auto_drive_active && !self.ctrl_c_quit_hint {
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

                if !self.using_chatgpt_auth {
                    right_sections.push(FooterSection {
                        priority: SectionPriority::RightOther,
                        spans: vec![Span::from("API key").style(label_style)],
                        enabled: true,
                    });
                }

                let total_width = area.width as usize;
                let separator = Span::from("  •  ").style(label_style);
                let separator_len = UnicodeWidthStr::width(separator.content.as_ref());

                let base_left_pad = Span::from("  ").style(label_style);
                let base_left_pad_len = UnicodeWidthStr::width(base_left_pad.content.as_ref());

                let leading_bullet_pad = Span::from(" ").style(label_style);
                let leading_bullet_pad_len =
                    UnicodeWidthStr::width(leading_bullet_pad.content.as_ref());

                let mut include_auto_review_status = left_sections.iter().any(|section| {
                    section.priority == SectionPriority::AutoReview && section.enabled
                });
                let mut include_auto_review_agent_hint = right_sections.iter().any(|section| {
                    section.priority == SectionPriority::AutoReview && section.enabled
                });
                let mut include_access_mode = true;
                let mut include_footer_hint = true;
                let mut include_ctrl_c = ctrl_c_present;
                let mut include_editor = editor_present;
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

                    for section in &left_sections {
                        let include = match section.priority {
                            SectionPriority::AutoReview => {
                                include_auto_review_status && section.enabled
                            }
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
                        &token_spans_compact
                    } else {
                        &token_spans_full
                    };
                    if include_tokens && !token_spans.is_empty() {
                        assembled.extend(token_spans.clone());
                        len += span_width(token_spans);
                    }

                    for section in &right_sections {
                        let include = match section.priority {
                            SectionPriority::AutoReview => {
                                include_auto_review_agent_hint && section.enabled
                            }
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
                    let add_leading_bullet_pad = self.auto_drive_active
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

                let mut left_spans = final_left;
                let right_spans = final_right;
                if left_len + right_len + FOOTER_TRAILING_PAD > total_width {
                    let mut remaining =
                        total_width.saturating_sub(right_len + FOOTER_TRAILING_PAD);
                    if remaining == 0 {
                        left_spans.clear();
                    } else {
                        let mut truncated: Vec<Span> = Vec::new();
                        for span in &left_spans {
                            if remaining == 0 {
                                break;
                            }
                            let span_len = UnicodeWidthStr::width(span.content.as_ref());
                            if span_len <= remaining {
                                truncated.push(span.clone());
                                remaining -= span_len;
                                continue;
                            }

                            if span.content.trim().is_empty() {
                                truncated.push(Span::from(" ".repeat(remaining)).style(span.style));
                                remaining = 0;
                            } else if remaining <= 1 {
                                truncated.push(Span::from("…").style(span.style));
                                remaining = 0;
                            } else {
                                let collected =
                                    crate::text_formatting::truncate_to_display_width_with_suffix(
                                        span.content.as_ref(),
                                        remaining,
                                        "…",
                                    );
                                truncated.push(Span::from(collected).style(span.style));
                                remaining = 0;
                            }
                        }
                        if truncated.is_empty() {
                            truncated.push(Span::from("  "));
                        }
                        left_spans = truncated;
                    }
                    left_len = span_width(&left_spans);
                }

                let spacer = if total_width > left_len + right_len + FOOTER_TRAILING_PAD {
                    " ".repeat(total_width - left_len - right_len - FOOTER_TRAILING_PAD)
                } else {
                    String::from(" ")
                };

                let mut line_spans = left_spans;
                line_spans.push(Span::from(spacer));
                line_spans.extend(right_spans);
                line_spans.push(Span::from(" "));

                Line::from(line_spans)
                    .style(
                        Style::default()
                            .fg(crate::colors::text_dim())
                            .add_modifier(Modifier::DIM),
                    )
                    .render_ref(area, buf);
            }
        }
    }
}
