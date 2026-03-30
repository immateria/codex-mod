#[cfg(not(any(test, feature = "test-helpers")))]
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static FORCE_MINIMAL_HEADER_OVERRIDE_SET: AtomicBool = AtomicBool::new(false);
static FORCE_MINIMAL_HEADER_OVERRIDE_VALUE: AtomicBool = AtomicBool::new(false);
#[cfg(not(any(test, feature = "test-helpers")))]
static FORCE_MINIMAL_HEADER_ENV: OnceLock<bool> = OnceLock::new();

pub(crate) fn force_minimal_header() -> bool {
    if FORCE_MINIMAL_HEADER_OVERRIDE_SET.load(Ordering::Relaxed) {
        return FORCE_MINIMAL_HEADER_OVERRIDE_VALUE.load(Ordering::Relaxed);
    }

    #[cfg(any(test, feature = "test-helpers"))]
    {
        true
    }

    #[cfg(not(any(test, feature = "test-helpers")))]
    {
        *FORCE_MINIMAL_HEADER_ENV.get_or_init(|| {
            std::env::var_os("CODEX_TUI_FORCE_MINIMAL_HEADER").is_some()
        })
    }
}

#[cfg(test)]
pub(crate) struct ForceMinimalHeaderOverrideGuard {
    prev_set: bool,
    prev_value: bool,
}

#[cfg(test)]
impl ForceMinimalHeaderOverrideGuard {
    pub(crate) fn set(value: bool) -> Self {
        let prev_set = FORCE_MINIMAL_HEADER_OVERRIDE_SET.load(Ordering::Relaxed);
        let prev_value = FORCE_MINIMAL_HEADER_OVERRIDE_VALUE.load(Ordering::Relaxed);

        FORCE_MINIMAL_HEADER_OVERRIDE_VALUE.store(value, Ordering::Relaxed);
        FORCE_MINIMAL_HEADER_OVERRIDE_SET.store(true, Ordering::Relaxed);

        Self {
            prev_set,
            prev_value,
        }
    }
}

#[cfg(test)]
impl Drop for ForceMinimalHeaderOverrideGuard {
    fn drop(&mut self) {
        FORCE_MINIMAL_HEADER_OVERRIDE_VALUE.store(self.prev_value, Ordering::Relaxed);
        FORCE_MINIMAL_HEADER_OVERRIDE_SET.store(self.prev_set, Ordering::Relaxed);
    }
}
