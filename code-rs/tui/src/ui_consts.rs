//! Shared UI constants for layout and alignment within the TUI.

// ---------------------------------------------------------------------------
// Separator strings — used to join metadata spans and summary parts.
// ---------------------------------------------------------------------------

/// Middle-dot separator used between metadata items (model, timestamp, tokens).
pub(crate) const SEP_DOT: &str = " · ";

// ---------------------------------------------------------------------------
// Standard layout margins — shared across settings panels, overlays, etc.
// ---------------------------------------------------------------------------

use ratatui::layout::Margin;

/// Standard horizontal padding (1 col left + right, 0 rows top + bottom).
/// Used by settings panels, overlay content areas, and padded render rects.
pub(crate) const HORIZONTAL_PAD: Margin = Margin::new(1, 0);

/// Uniform padding (1 col horizontal + 1 row vertical).
/// Used by overlay chrome, widget frames, and settings section boxes.
pub(crate) const UNIFORM_PAD: Margin = Margin::new(1, 1);

/// Indented horizontal padding (2 cols left + right).
/// Used for nested elements like command popups and indented form fields.
pub(crate) const NESTED_HPAD: Margin = Margin::new(2, 0);
