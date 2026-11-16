//! A definite persistence refusal for lack of capacity is recognised
//! (#379, #42 §10).
use layerfs_storage::{port::PersistenceError, StorageError};

fn refused(status: &str) -> StorageError {
    PersistenceError::Refused {
        status: status.to_owned(),
    }
    .into()
}

#[test]
fn only_a_definite_capacity_refusal_counts() {
    assert!(refused("Capacity").is_capacity_refusal());
    assert!(!refused("ReadOnly").is_capacity_refusal());
    assert!(!StorageError::from(PersistenceError::Uncertain).is_capacity_refusal());
    assert!(!StorageError::from(PersistenceError::Busy).is_capacity_refusal());
    assert!(!StorageError::from(PersistenceError::Malformed).is_capacity_refusal());
    // A declared bound refused the offered value before any write, so it is a
    // capacity refusal too (§10), even though it is not a full disk.
    assert!(StorageError::CapacityExceeded {
        what: "pack.assembled_length",
        limit: 1,
        actual: 2,
    }
    .is_capacity_refusal());
}

#[test]
fn a_failed_cleanup_keeps_the_refusal_only_when_its_outcome_is_known() {
    let definite = StorageError::CleanupFailed {
        original: Box::new(refused("Capacity")),
        cleanup: Box::new(refused("Capacity")),
    };
    assert!(definite.is_capacity_refusal());
    let uncertain = StorageError::CleanupFailed {
        original: Box::new(refused("Capacity")),
        cleanup: Box::new(PersistenceError::Uncertain.into()),
    };
    assert!(!uncertain.is_capacity_refusal());
}
