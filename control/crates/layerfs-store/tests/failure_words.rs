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
