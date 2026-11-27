//! An object demand's Store failure keeps its kind in the Content error it
//! becomes (#508). Every failure other than a missing object used to read
//! "Store object demand", which hid concurrent demand and refused reads.
use layerfs_content::{ContentError, ObjectId};
use layerfs_storage::StorageError;
use layerfs_store_service::store::{PortError, ReadAdmissionError};
use std::collections::BTreeSet;

fn what(error: PortError) -> &'static str {
    match error.object_demand() {
        ContentError::ProviderFailure { what } => what,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_missing_object_stays_missing() {
    let id = ObjectId::from_bytes(&[3; 32]).unwrap();
    assert_eq!(
        PortError::Storage(StorageError::ObjectMissing(id)).object_demand(),
        ContentError::MissingObject
    );
}

#[test]
fn each_failure_kind_is_named() {
    let kinds = [
        PortError::ConcurrentDemand,
        PortError::Poisoned,
        PortError::ReadAdmission(ReadAdmissionError::Capacity),
        PortError::ReadAdmission(ReadAdmissionError::Stopped),
        PortError::ReadAdmission(ReadAdmissionError::NoReaders),
        PortError::ReadAdmission(ReadAdmissionError::Poisoned),
        PortError::Storage(StorageError::Integrity("pack frame")),
        PortError::Storage(StorageError::Io(std::io::Error::other("disk"))),
    ];
    let count = kinds.len();
    let named: BTreeSet<&str> = kinds.into_iter().map(what).collect();
    assert_eq!(named.len(), count, "{named:?}");
    assert!(named.iter().all(|w| w.starts_with("Store object demand: ")));
    assert_eq!(
        what(PortError::ConcurrentDemand),
        "Store object demand: concurrent demand in one operation"
    );
}
