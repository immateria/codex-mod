use chrono::Local;
use chrono::Timelike;

pub(crate) fn current_label() -> &'static str {
    label_for_hour(current_hour())
}

fn current_hour() -> u32 {
    if let Ok(fake) = std::env::var("CODEX_TUI_FAKE_HOUR")
        && let Ok(parsed) = fake.parse::<u32>()
    {
        return parsed.min(23);
    }

    Local::now().hour()
}

pub(crate) fn label_for_hour(hour: u32) -> &'static str {
    match hour {
        5..=9 => "this morning",
        10..=13 => "today",
        14..=16 => "this afternoon",
        17..=20 => "this evening",
        _ => "tonight",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_match_expected_ranges() {
        assert_eq!(label_for_hour(5), "this morning");
        assert_eq!(label_for_hour(10), "today");
        assert_eq!(label_for_hour(14), "this afternoon");
        assert_eq!(label_for_hour(17), "this evening");
        assert_eq!(label_for_hour(23), "tonight");
    }
}