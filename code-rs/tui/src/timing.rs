use std::time::Duration;

/// Standard animation frame interval (120ms ≈ 8 fps) used across the TUI for
/// streaming text reveals, celebration effects, header wave, and spinner ticks.
pub(crate) const ANIMATION_FRAME_INTERVAL: Duration = Duration::from_millis(120);

/// Default redraw debounce — the minimum interval between consecutive terminal
/// redraws to avoid overwhelming slow terminals.
pub(crate) const REDRAW_DEBOUNCE: Duration = Duration::from_millis(33);
