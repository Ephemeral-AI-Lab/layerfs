//! T10.3: the bounded metrics a real `store.compact` path records.
//!
//! The three observations are a successful rebuild, the `none` path and the
//! leftover a stopped run leaves for the next call. The capture is
//! thread-local, so the other tests in this binary cannot pollute it.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::HistoryError;
use layerfs_persistence::compact_metrics::CompactMetricsCapture;
use layerfs_persistence::{
    CompactFailpoint, CompactPath, CompactRequest, PersistenceConfig, SqliteHandles,
    SqlitePersistenceProfile,
};
use layerfs_storage::{Storage, StoragePolicy};
use std::path::{Path, PathBuf};
use support::metrics::{assert_bounded, metric, value};
use support::Temp;

const BINDING: &[u8] = b"sqlite-compact-metrics-test-binding";

fn create_store() -> (Temp, SqliteHandles, PathBuf) {
    let temp = Temp::new("sqlite-compact-metrics");
    let path = temp.join("store.sqlite");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(path.clone())
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &layerfs_history::HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .unwrap();
    (temp, handles, path)
}

fn reopen(path: &Path) -> SqliteHandles {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        BINDING,
        [71; 32],
    )
    .unwrap()
}

fn request(op: u8) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    }
}

fn request_at(op: u8, failpoint: CompactFailpoint) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        failpoint: Some(failpoint),
        ..CompactRequest::default()
    }
}

fn compact_path(store: &Path) -> PathBuf {
    let mut name = store.as_os_str().to_os_string();
    name.push(".compact");
    PathBuf::from(name)
}

fn whole_file(handles: &SqliteHandles, bytes: &[u8]) -> ObjectId {
    let object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).unwrap(),
    )
    .unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    object.id()
}

#[test]
fn a_successful_rebuild_emits_phases_outcome_and_report_gauges() {
    let (_temp, handles, path) = create_store();
    let _dead = whole_file(&handles, b"a pack no retained root reaches");
    let capture = CompactMetricsCapture::start();
    let receipt = handles.compact(&request(40)).unwrap();
    let metrics = capture.finish();

    assert_eq!(receipt.path, CompactPath::Rebuild, "the dead pack rebuilds");
    assert_bounded(&metrics);
    assert_eq!(value(&metrics, "compact.runs", &[]), 1);
    assert_eq!(
        value(&metrics, "compact.operations", &[("outcome", "rebuild")]),
        1
    );
    for phase in ["mark", "build", "verify", "install"] {
        assert_eq!(
            value(
                &metrics,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "count")]
            ),
            1,
            "{phase} was timed once"
        );
        assert!(
            value(
                &metrics,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "sum")]
            ) > 0,
            "{phase} took time"
        );
        assert!(
            value(
                &metrics,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "max")]
            ) > 0,
            "{phase} has a duration"
        );
    }
    assert_eq!(
        value(&metrics, "compact.duration_ns", &[("statistic", "count")]),
        1
    );
    assert!(
        value(&metrics, "compact.duration_ns", &[("statistic", "sum")]) > 0,
        "the run took time"
    );
    assert_eq!(
        value(&metrics, "compact.dead_bytes", &[("kind", "bound")]),
        receipt.dead_bytes_bound
    );
    assert_eq!(
        value(&metrics, "compact.dead_bytes", &[("kind", "refined")]),
        receipt.dead_bytes_refined
    );
    let kept = receipt
        .kept_waste
        .dead_in_retained_packs_bytes
        .saturating_add(receipt.kept_waste.delta_bases_bytes)
        .saturating_add(receipt.kept_waste.inode_blocks_bytes);
    assert_eq!(value(&metrics, "compact.kept_waste_bytes", &[]), kept);
    assert_eq!(
        value(&metrics, "compact.residue_bytes", &[]),
        receipt.residue.bytes,
        "a successful settle leaves no residue"
    );
    assert_eq!(
        value(&metrics, "compact.generation", &[("which", "before")]),
        receipt.generation_before
    );
    assert_eq!(
        value(&metrics, "compact.generation", &[("which", "after")]),
        receipt.generation_after
    );
    assert_eq!(
        value(
            &metrics,
            "compact.slo",
            &[("objective", "residue_free"), ("result", "pass")]
        ),
        1
    );
    assert_eq!(
        value(
            &metrics,
            "compact.slo",
            &[("objective", "generation"), ("result", "pass")]
        ),
        1
    );
    assert_eq!(value(&metrics, "compact.settles", &[("kind", "run")]), 1);
    assert_eq!(
        metric(&metrics, "compact.retries", &[("reason", "leftover")]),
        None
    );

    let reopened = reopen(&path);
    clean(&reopened);
}

#[test]
fn the_none_path_emits_only_the_mark_phase_and_keeps_the_generation() {
    let (_temp, handles, path) = create_store();
    let capture = CompactMetricsCapture::start();
    let receipt = handles.compact(&request(41)).unwrap();
    let metrics = capture.finish();

    assert_eq!(receipt.path, CompactPath::None);
    assert_bounded(&metrics);
    assert_eq!(value(&metrics, "compact.runs", &[]), 1);
    assert_eq!(
        value(&metrics, "compact.operations", &[("outcome", "none")]),
        1
    );
    assert_eq!(
        value(
            &metrics,
            "compact.phase.duration_ns",
            &[("phase", "mark"), ("statistic", "count")]
        ),
        1
    );
    for phase in ["build", "verify", "install"] {
        assert_eq!(
            metric(
                &metrics,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "count")]
            ),
            None,
            "the none path never enters {phase}"
        );
    }
    assert_eq!(
        value(&metrics, "compact.dead_bytes", &[("kind", "bound")]),
        receipt.dead_bytes_bound
    );
    assert_eq!(
        value(&metrics, "compact.dead_bytes", &[("kind", "refined")]),
        receipt.dead_bytes_refined
    );
    assert_eq!(
        value(&metrics, "compact.residue_bytes", &[]),
        0,
        "none leaves no artifact"
    );
    assert_eq!(
        value(&metrics, "compact.generation", &[("which", "before")]),
        receipt.generation_before
    );
    assert_eq!(
        value(&metrics, "compact.generation", &[("which", "after")]),
        receipt.generation_after,
        "none keeps the generation"
    );
    assert_eq!(
        value(
            &metrics,
            "compact.slo",
            &[("objective", "generation"), ("result", "pass")]
        ),
        1
    );

    let reopened = reopen(&path);
    clean(&reopened);
}

#[test]
fn a_stopped_run_then_the_leftover_settle_emits_failure_retry_and_settle_metrics() {
    let (_temp, handles, path) = create_store();
    let _dead = whole_file(&handles, b"a pack no retained root reaches");

    let capture = CompactMetricsCapture::start();
    let error = handles
        .compact(&request_at(42, CompactFailpoint::AfterBuild))
        .unwrap_err();
    let stopped = capture.finish();
    assert!(matches!(error, HistoryError::UnknownOutcome), "{error:?}");
    assert_bounded(&stopped);
    assert_eq!(value(&stopped, "compact.runs", &[]), 1);
    assert_eq!(
        value(&stopped, "compact.operations", &[("outcome", "uncertain")]),
        1,
        "a stop point leaves an unknown outcome"
    );
    for phase in ["mark", "build"] {
        assert_eq!(
            value(
                &stopped,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "count")]
            ),
            1,
            "the stopped run reached {phase}"
        );
    }
    assert_eq!(
        metric(&stopped, "compact.generation", &[("which", "after")]),
        None,
        "an unknown outcome publishes no report gauge"
    );
    assert_eq!(
        metric(&stopped, "compact.settles", &[("kind", "leftover")]),
        None,
        "the stopped run settles no leftover"
    );

    let capture = CompactMetricsCapture::start();
    let reopened = reopen(&path);
    let receipt = reopened.compact(&request(43)).unwrap();
    let settled = capture.finish();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_bounded(&settled);
    assert_eq!(
        value(&settled, "compact.operations", &[("outcome", "rebuild")]),
        1
    );
    assert_eq!(
        value(&settled, "compact.retries", &[("reason", "leftover")]),
        1
    );
    assert_eq!(
        value(&settled, "compact.settles", &[("kind", "leftover")]),
        1
    );
    assert_eq!(value(&settled, "compact.settles", &[("kind", "run")]), 1);
    assert_eq!(value(&settled, "compact.residue_bytes", &[]), 0);

    let final_handles = reopen(&path);
    clean(&final_handles);
}

#[test]
fn a_definite_failure_emits_a_failed_outcome_without_report_gauges() {
    let (_temp, handles, path) = create_store();
    // A stray artifact no record names makes settle_leftover refuse, before
    // the mark: a definite failure, not an unknown outcome.
    std::fs::write(compact_path(&path), b"stray").unwrap();
    let capture = CompactMetricsCapture::start();
    let error = handles.compact(&request(44)).unwrap_err();
    let metrics = capture.finish();
    assert!(matches!(error, HistoryError::Exists(_)), "{error:?}");
    assert_bounded(&metrics);
    assert_eq!(value(&metrics, "compact.runs", &[]), 1);
    assert_eq!(
        value(&metrics, "compact.operations", &[("outcome", "failed")]),
        1
    );
    assert_eq!(
        value(&metrics, "compact.duration_ns", &[("statistic", "count")]),
        1
    );
    assert!(value(&metrics, "compact.duration_ns", &[("statistic", "sum")]) > 0);
    assert_eq!(
        metric(&metrics, "compact.dead_bytes", &[("kind", "bound")]),
        None
    );
    assert_eq!(
        metric(
            &metrics,
            "compact.slo",
            &[("objective", "residue_free"), ("result", "pass")]
        ),
        None
    );
}

fn clean(handles: &SqliteHandles) {
    let mut out = Vec::new();
    handles
        .diagnose(
            layerfs_persistence::diagnostic::Options::default(),
            &mut |observation| out.push(observation),
        )
        .unwrap();
    assert!(
        !out.iter().any(|observation| matches!(
            observation.outcome,
            layerfs_persistence::diagnostic::Outcome::Fail
                | layerfs_persistence::diagnostic::Outcome::Unknown
        )),
        "{out:?}"
    );
}
