#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
static TEST_FORCE_NO_PICKER: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
static TEST_FORCE_NO_CLIPBOARD: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
pub(crate) fn set_test_force_no_picker(enabled: bool) {
    TEST_FORCE_NO_PICKER.store(enabled, Ordering::SeqCst);
}

#[cfg(test)]
pub(crate) fn set_test_force_no_clipboard(enabled: bool) {
    TEST_FORCE_NO_CLIPBOARD.store(enabled, Ordering::SeqCst);
}

pub(crate) fn supports_native_picker() -> bool {
    if cfg!(target_os = "android") {
        return false;
    }
    #[cfg(test)]
    if TEST_FORCE_NO_PICKER.load(Ordering::SeqCst) {
        return false;
    }
    if env_truthy("CODEX_TUI_FORCE_NO_PICKER") {
        return false;
    }
    true
}

pub(crate) fn supports_reveal_in_file_manager() -> bool {
    if cfg!(target_os = "android") {
        return false;
    }
    #[cfg(test)]
    if TEST_FORCE_NO_PICKER.load(Ordering::SeqCst) {
        return false;
    }
    if env_truthy("CODEX_TUI_FORCE_NO_PICKER") {
        return false;
    }
    true
}

pub(crate) fn supports_clipboard_image_paste() -> bool {
    if cfg!(target_os = "android") {
        return false;
    }
    #[cfg(test)]
    if TEST_FORCE_NO_CLIPBOARD.load(Ordering::SeqCst) {
        return false;
    }
    if env_truthy("CODEX_TUI_FORCE_NO_CLIPBOARD") {
        return false;
    }
    true
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| !value.trim().is_empty() && value.trim() != "0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn supports_clipboard_image_paste_defaults_to_true_on_non_android() {
        let _lock = TEST_LOCK.lock().expect("lock");
        set_test_force_no_clipboard(false);

        if cfg!(target_os = "android") {
            assert!(!supports_clipboard_image_paste());
        } else {
            assert!(supports_clipboard_image_paste());
        }
    }

    #[test]
    fn supports_clipboard_image_paste_can_be_forced_off_in_tests() {
        let _lock = TEST_LOCK.lock().expect("lock");
        set_test_force_no_clipboard(true);
        assert!(!supports_clipboard_image_paste());
        set_test_force_no_clipboard(false);
    }
}
