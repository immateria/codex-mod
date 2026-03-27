pub mod config;

pub mod otel_event_manager;
#[cfg(feature = "otel")]
pub mod otel_provider;

#[cfg(not(feature = "otel"))]
mod imp {
    use reqwest::header::HeaderMap;
    use tracing::Span;

    pub struct OtelProvider;

    impl OtelProvider {
        pub fn from(_settings: &crate::config::OtelSettings) -> Option<Self> {
            None
        }

        pub fn headers(_span: &Span) -> HeaderMap {
            HeaderMap::new()
        }
    }
}

#[cfg(not(feature = "otel"))]
pub use imp::OtelProvider;

/// Lightweight metrics sink used by `code-features` and other subsystems.
///
/// The full fork uses structured tracing events via `OtelEventManager`; this
/// type keeps upstream-compatible call sites compiling without forcing an
/// OpenTelemetry dependency everywhere.
#[derive(Debug, Clone, Default)]
pub struct SessionTelemetry;

impl SessionTelemetry {
    pub fn counter(&self, _name: &str, _inc: u64, _tags: &[(&str, &str)]) {}
}
