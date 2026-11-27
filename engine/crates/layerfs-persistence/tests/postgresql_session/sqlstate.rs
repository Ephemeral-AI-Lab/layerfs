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

/// A statement the server refused with a reply is a definite failure, not an
/// unknown outcome: `P0001` comes from the `pack_immutable` trigger's RAISE
/// EXCEPTION, class P0 from any PL/pgSQL error and class 21 from a
/// cardinality violation (finding #3). Server I/O and internal errors stay
/// unknown.
#[test]
fn refused_statements_are_definite_and_server_failures_stay_unknown() {
    for code in ["P0001", "P0002", "P0003", "P0004", "21000"] {
        assert_eq!(sqlstate(code), BackendError::Integrity, "SQLSTATE {code}");
    }
    for code in ["58030", "XX000", "XX001"] {
        assert_eq!(sqlstate(code), BackendError::Unknown, "SQLSTATE {code}");
    }
}
