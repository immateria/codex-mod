/// This is designed to be called pre-main() (using `#[ctor::ctor]`) to perform
/// various process hardening steps, such as
/// - disabling core dumps
/// - disabling ptrace attach on Linux and macOS.
/// - removing dangerous environment variables such as LD_PRELOAD and DYLD_*
pub fn pre_main_hardening() {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pre_main_hardening_linux();

    #[cfg(target_os = "macos")]
    pre_main_hardening_macos();

    #[cfg(windows)]
    pre_main_hardening_windows();
}

fn is_termux_runtime() -> bool {
    std::env::var("TERMUX_VERSION")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

fn hardening_error_message(operation: &str, err: &std::io::Error) -> String {
    let mut message = format!("ERROR: {operation} failed: {err}");
    if is_termux_runtime() {
        message.push_str(
            "\nTermux note: secure mode may be incompatible with your Android environment. Unset `CODEX_SECURE_MODE` to run without hardening.",
        );
    }
    message
}

#[cfg(any(target_os = "linux", target_os = "android"))]
const PRCTL_FAILED_EXIT_CODE: i32 = 5;

#[cfg(target_os = "macos")]
const PTRACE_DENY_ATTACH_FAILED_EXIT_CODE: i32 = 6;

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
const SET_RLIMIT_CORE_FAILED_EXIT_CODE: i32 = 7;

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) fn pre_main_hardening_linux() {
    // Disable ptrace attach / mark process non-dumpable.
    let ret_code = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
    if ret_code != 0 {
        let err = std::io::Error::last_os_error();
        eprintln!("{}", hardening_error_message("prctl(PR_SET_DUMPABLE, 0)", &err));
        std::process::exit(PRCTL_FAILED_EXIT_CODE);
    }

    // For "defense in depth," set the core file size limit to 0.
    set_core_file_size_limit_to_zero();

    // Official Codex releases are MUSL-linked, which means that variables such
    // as LD_PRELOAD are ignored anyway, but just to be sure, clear them here.
    let ld_keys: Vec<String> = std::env::vars()
        .filter_map(|(key, _)| {
            if key.starts_with("LD_") {
                Some(key)
            } else {
                None
            }
        })
        .collect();

    for key in ld_keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn pre_main_hardening_macos() {
    // Prevent debuggers from attaching to this process.
    let ret_code = unsafe { libc::ptrace(libc::PT_DENY_ATTACH, 0, std::ptr::null_mut(), 0) };
    if ret_code == -1 {
        eprintln!(
            "ERROR: ptrace(PT_DENY_ATTACH) failed: {}",
            std::io::Error::last_os_error()
        );
        std::process::exit(PTRACE_DENY_ATTACH_FAILED_EXIT_CODE);
    }

    // Set the core file size limit to 0 to prevent core dumps.
    set_core_file_size_limit_to_zero();

    // Remove all DYLD_ environment variables, which can be used to subvert
    // library loading.
    let dyld_keys: Vec<String> = std::env::vars()
        .filter_map(|(key, _)| {
            if key.starts_with("DYLD_") {
                Some(key)
            } else {
                None
            }
        })
        .collect();

    for key in dyld_keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

#[cfg(unix)]
fn set_core_file_size_limit_to_zero() {
    let rlim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };

    let ret_code = unsafe { libc::setrlimit(libc::RLIMIT_CORE, &rlim) };
    if ret_code != 0 {
        let err = std::io::Error::last_os_error();
        eprintln!("{}", hardening_error_message("setrlimit(RLIMIT_CORE)", &err));
        std::process::exit(SET_RLIMIT_CORE_FAILED_EXIT_CODE);
    }
}

#[cfg(windows)]
pub(crate) fn pre_main_hardening_windows() {
    // TODO(mbolin): Perform the appropriate configuration for Windows.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn hardening_error_message_includes_termux_hint_when_env_var_is_set() {
        let _lock = ENV_LOCK.lock().expect("lock");
        let previous = std::env::var("TERMUX_VERSION").ok();
        unsafe { std::env::set_var("TERMUX_VERSION", "0.118.0") };

        let err = std::io::Error::new(std::io::ErrorKind::Other, "boom");
        let message = hardening_error_message("prctl(PR_SET_DUMPABLE, 0)", &err);
        assert!(
            message.contains("Termux note: secure mode may be incompatible"),
            "expected Termux hint in message: {message}"
        );

        match previous {
            Some(value) => unsafe { std::env::set_var("TERMUX_VERSION", value) },
            None => unsafe { std::env::remove_var("TERMUX_VERSION") },
        }
    }

    #[test]
    fn hardening_error_message_omits_termux_hint_when_env_var_is_unset() {
        let _lock = ENV_LOCK.lock().expect("lock");
        let previous = std::env::var("TERMUX_VERSION").ok();
        unsafe { std::env::remove_var("TERMUX_VERSION") };

        let err = std::io::Error::new(std::io::ErrorKind::Other, "boom");
        let message = hardening_error_message("setrlimit(RLIMIT_CORE)", &err);
        assert!(
            !message.contains("Termux note:"),
            "did not expect Termux hint in message: {message}"
        );

        match previous {
            Some(value) => unsafe { std::env::set_var("TERMUX_VERSION", value) },
            None => unsafe { std::env::remove_var("TERMUX_VERSION") },
        }
    }
}
