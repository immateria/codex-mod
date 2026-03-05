// This module is for turning user-/model-controlled identifiers (e.g. `sub_id`, `call_id`) into
// a single filesystem-safe path component. It is intentionally *not* a general-purpose path
// sanitizer: it does not make arbitrary relative/absolute paths safe to read/write.
//
// Usage today is primarily: build subdirectories for spooled stdout/stderr and for storing
// oversized tool outputs under a base directory we already control.
//
// If you need to validate that a user-provided path stays within a base directory, use a
// dedicated "path within base" check (see `core/src/apply_patch.rs` for that pattern).
const MAX_COMPONENT_LEN: usize = 64;
const HASH_HEX_LEN: usize = 16;
const HASH_SEPARATOR_LEN: usize = 1; // '-'
const MAX_SLUG_LEN: usize = MAX_COMPONENT_LEN - HASH_SEPARATOR_LEN - HASH_HEX_LEN;

/// Returns a safe, single path component derived from `value`.
///
/// - Output is always `<= 64` bytes (ASCII-only) and contains no separators or `..`-style
///   traversal components.
/// - If `value` is already safe, it is returned as-is (preserves readability).
/// - Otherwise we return a slug (best-effort human readable) with a stable hash suffix for
///   uniqueness and to prevent collisions between distinct unsafe inputs.
pub(crate) fn safe_path_component(value: &str, fallback: &str) -> String {
    if is_safe_single_component(value) {
        return value.to_string();
    }

    let mut slug = build_slug(value, MAX_SLUG_LEN);
    if slug.is_empty() || slug == "." || slug == ".." {
        slug = build_slug(fallback, MAX_SLUG_LEN);
        if slug.is_empty() || slug == "." || slug == ".." {
            slug = "id".to_string();
        }
    }

    let hash = fnv1a_64(value.as_bytes());
    format!("{slug}-{hash:016x}")
}

fn build_slug(value: &str, max_len: usize) -> String {
    let mut slug = String::with_capacity(max_len);
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | '.') {
            slug.push(ch);
        } else {
            slug.push('_');
        }
        if slug.len() >= max_len {
            break;
        }
    }

    slug.trim_matches(|ch| matches!(ch, '_' | '-' | '.'))
        .to_string()
}

fn is_safe_single_component(value: &str) -> bool {
    if value.is_empty() || value == "." || value == ".." {
        return false;
    }

    if value.len() > MAX_COMPONENT_LEN {
        return false;
    }

    // Windows has additional rules (drive prefixes, device names, and trailing dots/spaces)
    // that make path-component validation trickier. We apply a conservative subset of those
    // rules unconditionally so `Path::join` cannot reinterpret this component.
    if value != value.trim() {
        return false;
    }
    if value.ends_with('.') {
        return false;
    }

    if is_windows_device_name(value) {
        return false;
    }

    !value
        .chars()
        .any(|ch| matches!(ch, '/' | '\\' | '\0' | ':') || ch.is_ascii_control())
}

fn is_windows_device_name(value: &str) -> bool {
    // Windows reserves a small set of device names, including when used with an extension.
    // We reject them here so the fast-path never returns a component that can't be created
    // or could be handled specially by the OS.
    // See: https://learn.microsoft.com/windows/win32/fileio/naming-a-file
    let base = value.split('.').next().unwrap_or(value);
    let base = base.trim();
    let upper = base.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "CONIN$"
            | "CONOUT$"
            | "CLOCK$"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::safe_path_component;
    use super::MAX_COMPONENT_LEN;

    fn assert_safe_component(value: &str) {
        assert!(!value.is_empty());
        assert_ne!(value, ".");
        assert_ne!(value, "..");
        assert!(!value.contains('/'));
        assert!(!value.contains('\\'));
        assert!(!value.contains('\0'));
        assert!(!value.contains(':'));
        assert!(value.len() <= MAX_COMPONENT_LEN, "len={} value={value}", value.len());
    }

    #[test]
    fn safe_path_component_rejects_parent_dir_and_separators() {
        for value in ["..", "../", "..\\", "a/b", "a\\b"] {
            let out = safe_path_component(value, "fallback");
            assert_safe_component(&out);
            assert_ne!(out, value);
        }
    }

    #[test]
    fn safe_path_component_rejects_windows_prefixy_names() {
        for value in ["C:", "con", "NUL.txt", "COM1", "foo ", " foo", "foo."] {
            let out = safe_path_component(value, "fallback");
            assert_safe_component(&out);
            assert_ne!(out, value);
        }
    }

    #[test]
    fn safe_path_component_preserves_simple_ids() {
        for value in ["call_abc123", "sub-1", "Agent.01"] {
            let out = safe_path_component(value, "fallback");
            assert_safe_component(&out);
            assert_eq!(out, value);
        }
    }
}
