//! #647: a definite pre-effect History refusal that reached the Commit through
//! a Workspace service port keeps its own class. The Store's file-digest write
//! meeting SQLite's Busy is `HistoryError::Busy` inside
//! `WorkspaceError::Service` (what `store/digests.rs` returns when
//! `record_file_digests` is refused); read as a `Workspace` service failure it
//! was answered as an unknown outcome and held every owner.
use layerfs_content::ContentError;
use layerfs_history::HistoryError;
use layerfs_store_service::store::{service_cause, CommitError};

/// The label the producer's Content result carries; only the provider branch
/// of the classification reads it.
fn label() -> ContentError {
    ContentError::OutputRejected
}

#[test]
fn a_refused_digest_write_classifies_as_history_busy() {
    let service: Box<dyn std::error::Error + Send + Sync> = Box::new(HistoryError::Busy);
    let classified = service_cause(service, label()).expect("the typed History cause is kept");
    match classified {
        CommitError::History(HistoryError::Busy) => {}
        other => panic!("{other:?}"),
    }
    assert!(
        !classified.uncertain(),
        "a refusal made before any effect is not an unknown outcome"
    );
}

#[test]
fn a_history_cause_that_is_unknown_stays_unknown() {
    let service: Box<dyn std::error::Error + Send + Sync> = Box::new(HistoryError::UnknownOutcome);
    let classified = service_cause(service, label()).expect("the typed History cause is kept");
    assert!(matches!(
        classified,
        CommitError::History(HistoryError::UnknownOutcome)
    ));
    assert!(classified.uncertain());
}

#[test]
fn another_service_cause_is_not_taken_for_a_history_failure() {
    let service: Box<dyn std::error::Error + Send + Sync> =
        Box::new(std::io::Error::other("not a Store port"));
    assert!(service_cause(service, label()).is_err());
}
