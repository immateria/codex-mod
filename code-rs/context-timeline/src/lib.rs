//! Generic baseline + delta + snapshot timeline storage.

mod timeline;

pub use timeline::{ContextTimeline, DeltaEntry, Fingerprint, SnapshotEntry, TimelineError};

