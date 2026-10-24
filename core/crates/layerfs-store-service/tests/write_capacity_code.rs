//! A Store port's definite refusal for lack of space reaches an SDK file
//! write as `Capacity`, not `Unknown` (#379). `Unknown` would also keep the
//! call's private Workspace.
use layerfs_history::HistoryError;
use layerfs_storage::{port::PersistenceError, StorageError};
use layerfs_store_service::{store::PortError, write::mutation::failure};
use layerfs_workspace::WorkspaceError;
use std::sync::Arc;

fn service(error: PortError) -> WorkspaceError {
    WorkspaceError::Service(Box::new(Arc::new(error)))
}

#[test]
fn a_history_capacity_refusal_is_capacity() {
    let refusal = failure(service(PortError::History(HistoryError::Capacity(
        "persistence",
    ))));
    assert_eq!(refusal.code, "Capacity");
}

#[test]
fn a_storage_capacity_refusal_is_capacity() {
    let storage = StorageError::from(PersistenceError::Refused {
        status: "Capacity".to_owned(),
    });
    assert_eq!(
        failure(service(PortError::Storage(storage))).code,
        "Capacity"
    );
}

#[test]
fn other_port_failures_stay_unknown() {
    let unknown = failure(service(PortError::History(HistoryError::UnknownOutcome)));
    assert_eq!(unknown.code, "Unknown");
    assert_eq!(failure(service(PortError::Poisoned)).code, "Unknown");
    let uncertain = StorageError::from(PersistenceError::Uncertain);
    assert_eq!(
        failure(service(PortError::Storage(uncertain))).code,
        "Unknown"
    );
}
