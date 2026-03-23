impl ChatWidget<'_> {
    pub(in super::super) fn usage_cost_usd_from_totals(totals: &TokenTotals) -> f64 {
        let non_cached_input = totals
            .input_tokens
            .saturating_sub(totals.cached_input_tokens);
        let input_cost = (non_cached_input as f64 / TOKENS_PER_MILLION)
            * INPUT_COST_PER_MILLION_USD;
        let cached_cost = (totals.cached_input_tokens as f64 / TOKENS_PER_MILLION)
            * CACHED_INPUT_COST_PER_MILLION_USD;
        let output_cost = (totals.output_tokens as f64 / TOKENS_PER_MILLION)
            * OUTPUT_COST_PER_MILLION_USD;
        input_cost + cached_cost + output_cost
    }

    pub(in super::super) fn format_usd(amount: f64) -> String {
        let cents = (amount * 100.0).round().max(0.0);
        let cents_u128 = cents as u128;
        let dollars_u128 = cents_u128 / 100;
        let cents_part = (cents_u128 % 100) as u8;
        let dollars = dollars_u128.min(i64::MAX as u128) as i64;
        if cents_part == 0 {
            format!("${} USD", format_with_separators(dollars))
        } else {
            format!(
                "${}.{:02} USD",
                format_with_separators(dollars),
                cents_part
            )
        }
    }

    pub(in super::super) fn accumulate_token_totals(target: &mut TokenTotals, delta: &TokenTotals) {
        target.input_tokens = target
            .input_tokens
            .saturating_add(delta.input_tokens);
        target.cached_input_tokens = target
            .cached_input_tokens
            .saturating_add(delta.cached_input_tokens);
        target.output_tokens = target
            .output_tokens
            .saturating_add(delta.output_tokens);
        target.reasoning_output_tokens = target
            .reasoning_output_tokens
            .saturating_add(delta.reasoning_output_tokens);
        target.total_tokens = target
            .total_tokens
            .saturating_add(delta.total_tokens);
    }
}
