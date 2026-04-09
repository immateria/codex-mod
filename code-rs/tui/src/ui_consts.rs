//! Shared UI constants for layout and alignment within the TUI.

// ---------------------------------------------------------------------------
// Separator strings — used to join metadata spans and summary parts.
// ---------------------------------------------------------------------------

/// Middle-dot separator used between metadata items (model, timestamp, tokens).
pub(crate) const SEP_DOT: &str = " · ";

/// Em-dash separator with surrounding spaces — used between title and first
/// hint group in overlay title bars (diff viewer, theme picker, guide).
pub(crate) const SEP_EM: &str = " ——— ";

/// Em-dash separator with trailing space only — used between successive hint
/// groups in overlay title bars (e.g. "explain ——— undo ——— close").
pub(crate) const SEP_EM_CONT: &str = "——— ";

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
