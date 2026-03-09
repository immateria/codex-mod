use super::*;

fn format_with_thousands(n: u64) -> String {
    let s = n.to_string();
    let len = s.len();
    let mut out = String::with_capacity(len + len / 3);
    for (index, byte) in s.bytes().enumerate() {
        if index > 0 && (len - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(byte as char);
    }
    out
}

fn steady_context_window_footer_label(
    context_window: u64,
    context_mode: Option<ContextMode>,
) -> Option<&'static str> {
    match (context_window, context_mode) {
        (EXTENDED_CONTEXT_WINDOW_1M, Some(ContextMode::Auto)) => Some("1M Auto"),
        (EXTENDED_CONTEXT_WINDOW_1M, _) => Some("1M Context"),
        _ => None,
    }
}

// Phase-specific labels intentionally take precedence over the steady-state
// context-mode label while an auto-context operation is active.
fn context_window_footer_label(
    context_window: u64,
    context_mode: Option<ContextMode>,
    auto_context_phase: Option<AutoContextPhase>,
) -> Option<&'static str> {
    match auto_context_phase {
        Some(AutoContextPhase::Checking) if context_window == EXTENDED_CONTEXT_WINDOW_1M => {
            Some("Checking context...")
        }
        Some(AutoContextPhase::Compacting) if context_window == EXTENDED_CONTEXT_WINDOW_1M => {
            Some("Compacting...")
        }
        _ => steady_context_window_footer_label(context_window, context_mode),
    }
}

fn percent_remaining(context_window: u64, tokens_used: u64) -> u8 {
    if context_window == 0 {
        return 0;
    }
    let remaining = context_window.saturating_sub(tokens_used);
    ((remaining.saturating_mul(100) + (context_window / 2)) / context_window).min(100) as u8
}

fn context_window_detail_spans(
    context_window: u64,
    tokens_used: u64,
    context_mode: Option<ContextMode>,
    auto_context_phase: Option<AutoContextPhase>,
    label_style: Style,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let pct_remaining = percent_remaining(context_window, tokens_used);
    spans.push(Span::from(" (").style(label_style));
    spans.push(
        Span::from(pct_remaining.to_string()).style(label_style.add_modifier(Modifier::BOLD)),
    );
    spans.push(Span::from("% left").style(label_style));
    if let Some(context_label) =
        context_window_footer_label(context_window, context_mode, auto_context_phase)
    {
        spans.push(Span::from(" • ").style(label_style));
        spans.push(Span::from(context_label).style(label_style.add_modifier(Modifier::BOLD)));
    }
    spans.push(Span::from(")").style(label_style));
    spans
}

fn build_token_usage_spans(
    token_usage_info: &TokenUsageInfo,
    label_style: Style,
) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let turn_usage = &token_usage_info.last_token_usage;
    let tokens_used = turn_usage.tokens_in_context_window();
    let detail_spans = if let Some(context_window) = token_usage_info.model_context_window
        && context_window > 0
    {
        context_window_detail_spans(
            context_window,
            tokens_used,
            token_usage_info.context_mode,
            token_usage_info.auto_context_phase,
            label_style,
        )
    } else {
        Vec::new()
    };

    let mut full_spans: Vec<Span<'static>> = Vec::new();
    let used_str = format_with_thousands(tokens_used);
    full_spans.push(Span::from(used_str).style(label_style.add_modifier(Modifier::BOLD)));
    full_spans.push(Span::from(" tokens").style(label_style));
    full_spans.extend(detail_spans.clone());

    (full_spans, detail_spans)
}

impl ChatComposer {
    pub(crate) fn token_usage_spans(&self, label_style: Style) -> Vec<Span<'static>> {
        self.token_usage_info
            .as_ref()
            .map(|token_usage_info| build_token_usage_spans(token_usage_info, label_style).0)
            .unwrap_or_default()
    }

    pub(crate) fn token_usage_spans_compact(&self, label_style: Style) -> Vec<Span<'static>> {
        self.token_usage_info
            .as_ref()
            .map(|token_usage_info| build_token_usage_spans(token_usage_info, label_style).1)
            .unwrap_or_default()
    }
}
