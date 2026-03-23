#[cfg(feature = "test-helpers")]
thread_local! {
    static HISTORY_LAYOUT_CACHE_HITS: Cell<u64> = const { Cell::new(0) };
    static HISTORY_LAYOUT_CACHE_MISSES: Cell<u64> = const { Cell::new(0) };
}

#[cfg(feature = "test-helpers")]
pub(crate) fn reset_history_layout_cache_stats_for_test() {
    HISTORY_LAYOUT_CACHE_HITS.with(|c| c.set(0));
    HISTORY_LAYOUT_CACHE_MISSES.with(|c| c.set(0));
}

#[cfg(feature = "test-helpers")]
pub(crate) fn history_layout_cache_stats_for_test() -> (u64, u64) {
    let hits = HISTORY_LAYOUT_CACHE_HITS.with(Cell::get);
    let misses = HISTORY_LAYOUT_CACHE_MISSES.with(Cell::get);
    (hits, misses)
}
