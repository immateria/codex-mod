/// Shared picker ordering for well-known models.
///
/// Keep this intentionally small and explicit. The UI can later layer user
/// favorites or pinned models above this baseline without changing view code.
const MODEL_PICKER_RANKS: &[(&str, u16)] = &[
    ("gpt-5.5", 0),
    ("gpt-5.4", 1),
    ("gpt-5.3-codex", 2),
    ("gpt-5.3-codex-spark", 3),
    ("gpt-5.2-codex", 4),
    ("gpt-5.2", 5),
    ("gpt-5.1-codex-max", 6),
    ("gpt-5.1-codex", 7),
    ("gpt-5.1-codex-mini", 8),
    ("gpt-5.1", 9),
];

pub fn picker_rank_for_model(model: &str) -> u16 {
    MODEL_PICKER_RANKS
        .iter()
        .find_map(|(name, rank)| name.eq_ignore_ascii_case(model).then_some(*rank))
        .unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::picker_rank_for_model;

    #[test]
    fn gpt_5_5_ranks_above_gpt_5_4() {
        assert!(picker_rank_for_model("gpt-5.5") < picker_rank_for_model("gpt-5.4"));
    }
}
