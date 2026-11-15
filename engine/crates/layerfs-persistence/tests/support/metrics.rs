#![allow(dead_code)]
//! Shared metric assertions for the compaction captures.
//!
//! `compact_metrics` (SQLite §7) and `postgresql_compact` (PostgreSQL §8)
//! both read a `CompactMetricsCapture`, so the closed label dimensions and
//! values are checked once here for both backends.

use layerfs_persistence::compact_metrics::CompactMetrics;

/// The value of one sample, or `None` when this run did not emit it.
pub fn metric(metrics: &CompactMetrics, name: &str, labels: &[(&str, &str)]) -> Option<u64> {
    metrics
        .samples()
        .into_iter()
        .find(|sample| {
            sample.name == name
                && labels
                    .iter()
                    .all(|(key, value)| sample.label(key) == Some(*value))
        })
        .map(|sample| sample.value)
}

/// The value of one sample the run must emit.
pub fn value(metrics: &CompactMetrics, name: &str, labels: &[(&str, &str)]) -> u64 {
    metric(metrics, name, labels).unwrap_or_else(|| {
        panic!(
            "no sample {name} {labels:?}; emitted: {:?}",
            metrics.samples()
        )
    })
}

/// Bounded cardinality: every label key is a closed dimension and every value
/// is one the enums name, never a Store name, path, operation id or artifact.
pub fn assert_bounded(metrics: &CompactMetrics) {
    let keys = [
        "outcome",
        "code",
        "kind",
        "reason",
        "state",
        "objective",
        "result",
        "phase",
        "statistic",
        "which",
        "part",
    ];
    let values = [
        "rebuild",
        "none",
        "failed",
        "uncertain",
        "busy",
        "unresolved",
        "mark",
        "build",
        "verify",
        "install",
        "pg_mark",
        "pg_build",
        "pg_swap",
        "pg_cleanup",
        "run",
        "leftover",
        "count",
        "sum",
        "max",
        "before",
        "after",
        "bound",
        "refined",
        "dead_in_retained_packs",
        "delta_bases",
        "inode_blocks",
        "residue_free",
        "generation",
        "pass",
        "fail",
    ];
    for sample in metrics.samples() {
        for (key, label) in &sample.labels {
            assert!(keys.contains(key), "non-closed label key {key}");
            assert!(
                values.contains(label),
                "non-closed label value {label} for {key}"
            );
        }
    }
}
