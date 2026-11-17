//! The PostgreSQL SQLSTATE classifier (#42 F42-1). `Capacity` is only the
//! definite insufficient-resource code: class 53 also carries the transient
//! connection shortage, and class 54 carries statement-complexity refusals.
use layerfs_persistence::internals::{sqlstate, BackendError};

#[test]
fn the_definite_insufficient_resource_codes_are_capacity() {
    for code in ["53100", "53200", "53400", "54000"] {
        assert_eq!(sqlstate(code), BackendError::Capacity, "SQLSTATE {code}");
    }
}

#[test]
fn too_many_connections_is_busy_not_capacity() {
    assert_eq!(sqlstate("53300"), BackendError::Busy);
}

#[test]
fn statement_limit_refusals_are_not_capacity() {
    for code in ["54001", "54011", "54023"] {
        assert_ne!(sqlstate(code), BackendError::Capacity, "SQLSTATE {code}");
        assert_eq!(sqlstate(code), BackendError::Integrity, "SQLSTATE {code}");
    }
}

#[test]
fn the_neighbouring_classes_keep_their_verdicts() {
    assert_eq!(sqlstate("55P03"), BackendError::Busy);
    assert_eq!(sqlstate("57014"), BackendError::Cancelled);
    assert_eq!(sqlstate("23505"), BackendError::Integrity);
    assert_eq!(sqlstate("42501"), BackendError::Integrity);
    assert_eq!(sqlstate("08006"), BackendError::Unknown);
}
