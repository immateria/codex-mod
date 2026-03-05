//! Auto Drive diagnostics interposer.
//!
//! Today this crate only exposes the structured completion-check schema and
//! parsed reply type used by the TUI's Auto Drive flows. A higher-level
//! diagnostics runtime can be layered on top later without changing the wire
//! shape consumed by the model.

/// Schema for the forced JSON diagnostics reply.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct CompletionCheck {
    pub complete: bool,
    pub explanation: String,
}

/// Diagnostics namespace for the shared completion-check schema.
pub struct AutoDriveDiagnostics;

impl AutoDriveDiagnostics {
    pub fn completion_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["complete", "explanation"],
            "properties": {
                "complete": { "type": "boolean" },
                "explanation": { "type": "string" }
            },
            "additionalProperties": false
        })
    }
}
