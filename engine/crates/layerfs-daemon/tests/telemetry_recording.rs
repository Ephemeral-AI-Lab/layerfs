//! Traced recording is scoped to its guard: it ends on every exit, and a
//! later untraced operation on the same thread records nothing.
use layerfs_daemon::telemetry::{phase, recording, timed, Recording};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn root(fail: bool) -> Result<u32, &'static str> {
    timed("root", |scope| {
        phase(scope, "inner", || if fail { Err("refused") } else { Ok(7) })
    })
}

#[test]
fn untraced_roots_record_nothing_and_keep_results() {
    assert!(!recording());
    assert_eq!(root(false), Ok(7));
    assert_eq!(root(true), Err("refused"));
    // A recording started afterwards holds none of those roots.
    assert!(Recording::start().finish().is_empty());
}

#[test]
fn traced_roots_record_phases_and_nested_roots_with_unchanged_results() {
    let recording_guard = Recording::start();
    assert!(recording());
    let value = timed("commit", |scope| {
        phase(scope, "construct", || {
            timed("captured.namespace", |_| Ok::<_, ()>(3))
        })
    });
    assert_eq!(value, Ok(3));
    assert_eq!(root(true), Err("refused"));
    let reports = recording_guard.finish();
    assert!(!recording());
    assert_eq!(
        reports.len(),
        2,
        "the nested root is attached, not a sibling"
    );
    let commit = reports[0].root().unwrap();
    assert_eq!(commit.name(), "commit");
    let construct = &commit.children()[0];
    assert_eq!(construct.name(), "construct");
    assert_eq!(construct.children()[0].name(), "captured.namespace");
    let refused = reports[1].root().unwrap();
    assert_eq!(refused.children()[0].name(), "inner");
    assert_eq!(
        refused.outcome(),
        layerfs_telemetry::timer::NodeOutcome::Error
    );
}

#[test]
fn recording_ends_on_ok_error_and_unwinding_exits() {
    for fail in [false, true] {
        {
            let _guard = Recording::start();
            let _ = root(fail);
        }
        assert!(!recording());
        assert_eq!(root(false), Ok(7));
        assert!(Recording::start().finish().is_empty());
    }
    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _guard = Recording::start();
        let _ = timed("root", |_| -> Result<(), ()> {
            panic!("operation panicked")
        });
    }));
    assert!(unwound.is_err());
    assert!(!recording(), "unwinding ended recording");
    assert_eq!(root(false), Ok(7));
    assert!(
        Recording::start().finish().is_empty(),
        "nothing from the unwound or later untraced roots leaks"
    );
}
