use std::cell::RefCell;

/// Implements `as_any` and `as_any_mut` for a `HistoryCell` impl block.
/// Place inside `impl HistoryCell for T { ... }` to generate the standard
/// two-method pair that enables downcasting.
macro_rules! impl_as_any {
    () => {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    };
}

/// Declares a module-level layout build counter for test observability.
///
/// Generates a `thread_local!` counter plus reset/get/bump functions.
///
/// Usage:
/// ```ignore
/// layout_build_counter!(EXEC_LAYOUT_BUILDS,
///     reset_exec_layout_builds_for_test,
///     exec_layout_builds_for_test,
///     bump_exec_layout_builds);
/// ```
#[cfg(feature = "test-helpers")]
macro_rules! layout_build_counter {
    ($static_name:ident, $reset_fn:ident, $get_fn:ident, $bump_fn:ident) => {
        thread_local! {
            static $static_name: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
        }

        pub(crate) fn $reset_fn() {
            $static_name.with(|c| c.set(0));
        }

        pub(crate) fn $get_fn() -> u64 {
            $static_name.with(std::cell::Cell::get)
        }

        fn $bump_fn() {
            $static_name.with(|c| c.set(c.get().saturating_add(1)));
        }
    };
}

pub(crate) trait LayoutCacheKey: Copy + Eq + Default {
    fn width(&self) -> u16;
}

impl LayoutCacheKey for u16 {
    fn width(&self) -> u16 {
        *self
    }
}

impl LayoutCacheKey for (u16, bool) {
    fn width(&self) -> u16 {
        self.0
    }
}

/// Generic width-keyed layout cache.
///
/// Stores a computed layout `L` alongside the terminal width it was computed
/// for. When the width changes the layout is recomputed via a caller-supplied
/// closure. This eliminates the identical `invalidate` / `ensure_layout` /
/// `layout_for_width` boilerplate that was duplicated across ExecCell,
/// JsReplCell, MergedExecCell, and WebFetchToolCell.
pub(crate) struct LayoutCache<L: Default, K: LayoutCacheKey = u16> {
    inner: RefCell<CacheEntry<L, K>>,
}

struct CacheEntry<L, K> {
    key: K,
    layout: L,
}

impl<L: Default, K: LayoutCacheKey> LayoutCache<L, K> {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(CacheEntry {
                key: K::default(),
                layout: L::default(),
            }),
        }
    }

    /// Mark the cached layout as stale so the next `get_or_compute` call
    /// will rebuild it regardless of width.
    pub fn invalidate(&self) {
        *self.inner.borrow_mut() = CacheEntry {
            key: K::default(),
            layout: L::default(),
        };
    }

    /// Return a reference to the cached layout for `width`, recomputing it
    /// via `compute` when stale or when the width has changed.
    ///
    /// When `width` is zero the layout is reset to its `Default` value.
    pub fn get_or_compute_key(
        &self,
        key: K,
        compute: impl FnOnce(K) -> L,
    ) -> std::cell::Ref<'_, L> {
        if key.width() == 0 {
            self.invalidate();
        } else {
            let needs_rebuild = self.inner.borrow().key != key;
            if needs_rebuild {
                let layout = compute(key);
                *self.inner.borrow_mut() = CacheEntry { key, layout };
            }
        }
        std::cell::Ref::map(self.inner.borrow(), |entry| &entry.layout)
    }
}

impl<L: Default> LayoutCache<L, u16> {
    /// Convenience wrapper for width-only caches.
    pub fn get_or_compute(
        &self,
        width: u16,
        compute: impl FnOnce(u16) -> L,
    ) -> std::cell::Ref<'_, L> {
        self.get_or_compute_key(width, compute)
    }
}
