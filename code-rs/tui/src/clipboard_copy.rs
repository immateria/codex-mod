use base64::Engine;
use std::io::Write;

/// Copy text to the system clipboard via OSC 52 escape sequence.
///
/// This works in terminals that support OSC 52 (iTerm2, kitty, `WezTerm`,
/// Alacritty, tmux with `set-clipboard on`, etc.). Terminals that do not
/// support it will silently ignore the escape.
pub(crate) fn copy_to_clipboard_osc52(text: &str) {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    // OSC 52 ; c ; <base64> ST   (ST = \x07 or \x1b\\)
    let payload = format!("\x1b]52;c;{encoded}\x07");
    let mut stdout = std::io::stdout();
    // INTENTIONAL: best-effort clipboard write; failure is non-fatal in a TUI.
    let _ = stdout.write_all(payload.as_bytes());
    let _ = stdout.flush();
}
