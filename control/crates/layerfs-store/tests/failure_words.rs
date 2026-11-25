//! Store failures in words (LF2 review FAIL 2): a `PersistenceError`'s own
//! `Display` prints Rust `Debug` text ("pack persistence: Refused { status:
//! .. }"); `words::persistence` says each outcome in words, and the Init,
//! import and acquisition words use it.
use layerfs_history::HistoryError;
use layerfs_project::store::InitError;
use layerfs_project::ProjectError;
use layerfs_storage::port::acquisition::AcquisitionError;
use layerfs_storage::port::PersistenceError;
use layerfs_store::words;

fn no_debug(text: &str) {
    for debug in [
        "{",
        "}",
        "Some(",
        "Refused",
        "Busy",
        "Uncertain",
        "pack persistence",
    ] {
        assert!(!text.contains(debug), "Debug text {debug:?} in {text:?}");
    }
}

#[test]
fn every_persistence_outcome_reads_as_words() {
    let all = [
        PersistenceError::Busy,
        PersistenceError::BackendUnavailable,
        PersistenceError::Missing,
        PersistenceError::Refused {
            status: "schema exists".to_owned(),
        },
        PersistenceError::Malformed,
        PersistenceError::Uncertain,
    ];
    for error in &all {
        no_debug(&words::persistence(error));
    }
    let refused = PersistenceError::Refused {
        status: "schema exists".to_owned(),
    };
    assert_eq!(
        words::persistence(&refused),
        "the Store refused it: schema exists"
    );
    assert!(words::persistence(&PersistenceError::Uncertain).contains("not known"));
}

#[test]
fn init_import_and_acquisition_failures_use_the_same_words() {
    let busy = words::init_error(&InitError::Persistence(PersistenceError::Busy));
    assert_eq!(busy, words::persistence(&PersistenceError::Busy));
    let acquisition = AcquisitionError::Persistence(PersistenceError::Malformed);
    assert_eq!(
        words::project_error(&ProjectError::Acquisition(acquisition)),
        "the Store answered something malformed"
    );
}

#[test]
fn a_history_failure_keeps_the_history_crates_words() {
    // a moved head is said in words (its engine Display prints the moved
    // state as Debug); every other history failure as the history crate says
    assert_eq!(
        words::init_error(&InitError::History(HistoryError::Busy)),
        "busy"
    );
}

/// An older-format Store's refusal (#776): `OldStore` carries the version
/// refusal as its message; a daemon's `Store open: ...` text wrapping it
/// is read back to it, as `sandbox remove` reads a refusal; any other
/// refusal carries none. Its fix removes a sqlite Store and disconnects a
/// PostgreSQL one, which keeps its data.
#[test]
fn an_old_format_store_is_named_by_its_version_refusal() {
    use layerfs_home::outcome::refused;
    use layerfs_store::state::{outdated_fix, outdated_status, OLD_STORE};
    let version = "SQLite Store schema version 2 (before each Commit recorded its attempt, \
#776): not supported by this release and not migrated; create a new Store";
    let raised = refused(OLD_STORE, version);
    assert_eq!(outdated_status(&raised).as_deref(), Some(version));
    let wrapped = format!(
        "Store open: physical Store I/O: pack persistence: Refused {{ status: {version:?} }}"
    );
    let daemon = refused("Failed", wrapped);
    assert_eq!(outdated_status(&daemon).as_deref(), Some(version));
    let other = refused(
        "Failed",
        "Store open: pack persistence: Refused { status: \"Closed\" }",
    );
    assert_eq!(outdated_status(&other), None);
    assert_eq!(outdated_fix("shop", false), "layerfs store remove shop");
    assert_eq!(
        outdated_fix("my pg", true),
        "layerfs store disconnect 'my pg'"
    );
}
