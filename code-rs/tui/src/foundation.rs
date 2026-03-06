/// Shared wrappers around upstream helpers so fork-specific code can rely on a
/// stable import surface while upstream modules continue to evolve.
pub(crate) mod wrapping {
    pub(crate) use crate::insert_history::word_wrap_lines;
}

pub(crate) mod palette {
    pub(crate) use crate::colors::*;
}
