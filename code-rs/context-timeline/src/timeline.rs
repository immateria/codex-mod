//! Core timeline data structure and operations.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub trait Fingerprint {
    fn fingerprint(&self) -> String;
}

#[derive(Debug, Error)]
pub enum TimelineError {
    #[error("Baseline already set")]
    BaselineAlreadySet,

    #[error("Baseline not set")]
    BaselineNotSet,

    #[error("Delta sequence out of order: expected {expected}, got {actual}")]
    DeltaSequenceOutOfOrder { expected: u64, actual: u64 },

    #[error("Delta not found for sequence: {0}")]
    DeltaNotFound(u64),

    #[error("Snapshot not found for fingerprint: {0}")]
    SnapshotNotFound(String),
}

/// Entry representing a delta in the timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeltaEntry<D> {
    pub sequence: u64,
    pub delta: D,
    /// Timestamp when this delta was recorded (ISO 8601).
    pub recorded_at: String,
}

/// Entry representing a snapshot in the timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotEntry<S> {
    pub fingerprint: String,
    pub snapshot: S,
    /// Timestamp when this snapshot was recorded (ISO 8601).
    pub recorded_at: String,
}

/// Core timeline structure managing baseline, deltas, and snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTimeline<S, D> {
    /// Baseline snapshot (immutable once set).
    baseline: Option<S>,
    /// Deltas indexed by sequence number.
    deltas: BTreeMap<u64, DeltaEntry<D>>,
    /// Snapshots indexed by fingerprint for deduplication.
    snapshots: HashMap<String, SnapshotEntry<S>>,
    /// Next expected sequence number for delta append.
    next_sequence: u64,
}

impl<S, D> Default for ContextTimeline<S, D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S, D> ContextTimeline<S, D> {
    /// Creates a new empty timeline.
    pub fn new() -> Self {
        Self {
            baseline: None,
            deltas: BTreeMap::new(),
            snapshots: HashMap::new(),
            next_sequence: 1,
        }
    }

    /// Sets the baseline snapshot. Can only be called once.
    ///
    /// # Errors
    ///
    /// Returns `TimelineError::BaselineAlreadySet` if baseline is already set.
    pub fn add_baseline_once(&mut self, snapshot: S) -> Result<(), TimelineError> {
        if self.baseline.is_some() {
            return Err(TimelineError::BaselineAlreadySet);
        }
        self.baseline = Some(snapshot);
        Ok(())
    }

    /// Applies a delta with sequence validation.
    ///
    /// # Errors
    ///
    /// - Returns `TimelineError::BaselineNotSet` if baseline hasn't been set.
    /// - Returns `TimelineError::DeltaSequenceOutOfOrder` if sequence doesn't match expected.
    pub fn apply_delta(&mut self, sequence: u64, delta: D) -> Result<(), TimelineError> {
        if self.baseline.is_none() {
            return Err(TimelineError::BaselineNotSet);
        }

        if sequence != self.next_sequence {
            return Err(TimelineError::DeltaSequenceOutOfOrder {
                expected: self.next_sequence,
                actual: sequence,
            });
        }

        let entry = DeltaEntry {
            sequence,
            delta,
            recorded_at: current_timestamp(),
        };

        self.deltas.insert(sequence, entry);
        self.next_sequence = self.next_sequence.saturating_add(1);

        Ok(())
    }

    /// Records a snapshot with hash-based deduplication.
    ///
    /// If a snapshot with the same fingerprint already exists, returns `Ok(false)`.
    /// Otherwise, records the snapshot and returns `Ok(true)`.
    pub fn record_snapshot(&mut self, snapshot: S) -> Result<bool, TimelineError>
    where
        S: Fingerprint,
    {
        let fingerprint = snapshot.fingerprint();

        if self.snapshots.contains_key(&fingerprint) {
            return Ok(false);
        }

        let entry = SnapshotEntry {
            fingerprint: fingerprint.clone(),
            snapshot,
            recorded_at: current_timestamp(),
        };

        self.snapshots.insert(fingerprint, entry);
        Ok(true)
    }

    // Lookup helpers

    /// Returns a reference to the baseline snapshot if set.
    pub fn baseline(&self) -> Option<&S> {
        self.baseline.as_ref()
    }

    /// Returns a reference to the delta entry for the given sequence.
    pub fn get_delta(&self, sequence: u64) -> Option<&DeltaEntry<D>> {
        self.deltas.get(&sequence)
    }

    /// Returns a reference to the snapshot entry for the given fingerprint.
    pub fn get_snapshot(&self, fingerprint: &str) -> Option<&SnapshotEntry<S>> {
        self.snapshots.get(fingerprint)
    }

    /// Returns all delta sequences in order.
    pub fn delta_sequences(&self) -> Vec<u64> {
        self.deltas.keys().copied().collect()
    }

    /// Returns all snapshot fingerprints.
    pub fn snapshot_fingerprints(&self) -> Vec<String> {
        self.snapshots.keys().cloned().collect()
    }

    /// Returns the number of deltas stored.
    pub fn delta_count(&self) -> usize {
        self.deltas.len()
    }

    /// Returns the number of snapshots stored.
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Returns the next expected sequence number.
    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Returns `true` when no baseline or deltas are stored.
    pub fn is_empty(&self) -> bool {
        self.baseline.is_none() && self.deltas.is_empty()
    }

    /// Returns the last `max_deltas` deltas in chronological order (oldest to newest).
    pub fn recent_deltas(&self, max_deltas: usize) -> Vec<&DeltaEntry<D>> {
        if max_deltas == 0 {
            return Vec::new();
        }

        let mut recent: Vec<&DeltaEntry<D>> = self.deltas.values().rev().take(max_deltas).collect();
        recent.reverse();
        recent
    }

    /// Estimates the total memory usage in bytes.
    ///
    /// This is a rough estimate based on JSON serialization size. It is not an
    /// allocator-accurate measurement.
    pub fn estimated_bytes(&self) -> usize
    where
        S: Serialize,
        D: Serialize,
    {
        let baseline_bytes = self
            .baseline
            .as_ref()
            .map_or(0, estimate_serialized_bytes);

        let deltas_bytes: usize = self.deltas.values().map(estimate_delta_bytes).sum();

        let snapshots_bytes: usize = self
            .snapshots
            .values()
            .map(|entry| estimate_serialized_bytes(&entry.snapshot))
            .sum();

        baseline_bytes + deltas_bytes + snapshots_bytes
    }
}

fn estimate_serialized_bytes<T: Serialize>(value: &T) -> usize {
    serde_json::to_string(value).map_or(0, |s| s.len())
}

fn estimate_delta_bytes<D: Serialize>(entry: &DeltaEntry<D>) -> usize {
    estimate_serialized_bytes(&entry.delta) + entry.recorded_at.len() + 8
}

/// Returns current timestamp in ISO 8601 format.
fn current_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct TestSnapshot {
        cwd: String,
        branch: Option<String>,
    }

    impl Fingerprint for TestSnapshot {
        fn fingerprint(&self) -> String {
            // Stable + cheap enough for unit tests.
            format!("cwd={};branch={}", self.cwd, self.branch.as_deref().unwrap_or(""))
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct TestDelta {
        base: String,
        cwd: String,
    }

    fn snapshot(cwd: &str, branch: Option<&str>) -> TestSnapshot {
        TestSnapshot {
            cwd: cwd.to_string(),
            branch: branch.map(std::string::ToString::to_string),
        }
    }

    fn delta(base: &str, cwd: &str) -> TestDelta {
        TestDelta {
            base: base.to_string(),
            cwd: cwd.to_string(),
        }
    }

    fn unwrap_ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("expected Ok(..), got Err({err:?})"),
        }
    }

    #[test]
    fn add_baseline_once_success() {
        let mut timeline: ContextTimeline<TestSnapshot, TestDelta> = ContextTimeline::new();
        let baseline = snapshot("/repo", Some("main"));

        assert!(timeline.add_baseline_once(baseline.clone()).is_ok());
        assert_eq!(timeline.baseline(), Some(&baseline));
    }

    #[test]
    fn add_baseline_once_fails_when_already_set() {
        let mut timeline: ContextTimeline<TestSnapshot, TestDelta> = ContextTimeline::new();
        let snapshot1 = snapshot("/repo", Some("main"));
        let snapshot2 = snapshot("/other", Some("feature"));

        unwrap_ok(timeline.add_baseline_once(snapshot1));
        let result = timeline.add_baseline_once(snapshot2);

        assert!(matches!(result, Err(TimelineError::BaselineAlreadySet)));
    }

    #[test]
    fn apply_delta_requires_baseline() {
        let mut timeline: ContextTimeline<TestSnapshot, TestDelta> = ContextTimeline::new();
        let d = delta("fingerprint", "/new-repo");

        let result = timeline.apply_delta(1, d);
        assert!(matches!(result, Err(TimelineError::BaselineNotSet)));
    }

    #[test]
    fn apply_delta_validates_sequence() {
        let mut timeline: ContextTimeline<TestSnapshot, TestDelta> = ContextTimeline::new();
        unwrap_ok(timeline.add_baseline_once(snapshot("/repo", Some("main"))));

        let d = delta("fingerprint", "/new-repo");
        let result = timeline.apply_delta(5, d);
        assert!(matches!(
            result,
            Err(TimelineError::DeltaSequenceOutOfOrder {
                expected: 1,
                actual: 5
            })
        ));
    }

    #[test]
    fn apply_delta_increments_sequence() {
        let mut timeline: ContextTimeline<TestSnapshot, TestDelta> = ContextTimeline::new();
        unwrap_ok(timeline.add_baseline_once(snapshot("/repo", Some("main"))));

        assert_eq!(timeline.next_sequence(), 1);
        unwrap_ok(timeline.apply_delta(1, delta("fp1", "/repo-1")));
        assert_eq!(timeline.next_sequence(), 2);
        unwrap_ok(timeline.apply_delta(2, delta("fp2", "/repo-2")));
        assert_eq!(timeline.next_sequence(), 3);
    }

    #[test]
    fn get_delta_and_sequences() {
        let mut timeline: ContextTimeline<TestSnapshot, TestDelta> = ContextTimeline::new();
        unwrap_ok(timeline.add_baseline_once(snapshot("/repo", Some("main"))));

        let d = delta("fp1", "/repo-1");
        unwrap_ok(timeline.apply_delta(1, d.clone()));

        let retrieved = timeline.get_delta(1);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.map(|d| d.sequence), Some(1));
        assert_eq!(retrieved.map(|d| &d.delta), Some(&d));
        assert!(timeline.get_delta(99).is_none());

        assert_eq!(timeline.delta_sequences(), vec![1]);
    }

    #[test]
    fn record_snapshot_deduplicates_by_fingerprint() {
        let mut timeline: ContextTimeline<TestSnapshot, TestDelta> = ContextTimeline::new();
        let s = snapshot("/repo", Some("main"));

        let first = unwrap_ok(timeline.record_snapshot(s.clone()));
        let second = unwrap_ok(timeline.record_snapshot(s));

        assert!(first);
        assert!(!second);
        assert_eq!(timeline.snapshot_count(), 1);
    }

    #[test]
    fn recent_deltas_returns_chronological_subset() {
        let mut timeline: ContextTimeline<TestSnapshot, TestDelta> = ContextTimeline::new();
        unwrap_ok(timeline.add_baseline_once(snapshot("/repo", Some("main"))));

        for seq in 1..=5 {
            let d = delta(&format!("fp{seq}"), &format!("/repo-{seq}"));
            unwrap_ok(timeline.apply_delta(seq, d));
        }

        let recent = timeline.recent_deltas(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].sequence, 4);
        assert_eq!(recent[1].sequence, 5);
    }

    #[test]
    fn estimated_bytes_grows_with_contents() {
        let mut timeline: ContextTimeline<TestSnapshot, TestDelta> = ContextTimeline::new();
        assert_eq!(timeline.estimated_bytes(), 0);

        unwrap_ok(timeline.add_baseline_once(snapshot("/repo", Some("main"))));
        let baseline_size = timeline.estimated_bytes();
        assert!(baseline_size > 0);

        unwrap_ok(timeline.apply_delta(1, delta("fp", "/new-repo")));
        let with_delta_size = timeline.estimated_bytes();
        assert!(with_delta_size > baseline_size);

        unwrap_ok(timeline.record_snapshot(snapshot("/snap", Some("feature"))));
        let with_snapshot_size = timeline.estimated_bytes();
        assert!(with_snapshot_size > with_delta_size);
    }

    #[test]
    fn timeline_serialization_roundtrip() {
        let mut timeline: ContextTimeline<TestSnapshot, TestDelta> = ContextTimeline::new();
        unwrap_ok(timeline.add_baseline_once(snapshot("/repo", Some("main"))));
        unwrap_ok(timeline.apply_delta(1, delta("fp", "/new")));

        let json = match serde_json::to_string(&timeline) {
            Ok(value) => value,
            Err(err) => panic!("serialize failed: {err:?}"),
        };

        let deserialized: ContextTimeline<TestSnapshot, TestDelta> = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(err) => panic!("deserialize failed: {err:?}"),
        };

        assert!(deserialized.baseline().is_some());
        assert_eq!(deserialized.delta_count(), 1);
        assert_eq!(deserialized.next_sequence(), 2);
    }
}

