//! Spec §10 wire codes for every `HistoryError`, moved out of
//! `history/lookup.rs`: product source holds no tests (the product-boundary
//! guard).

use layerfs_bridge::control::SdkValue;
use layerfs_history::error::{
    HolderKind, HolderState, Missing, MountHolders, MovedState, NameHolder, NameSource,
    NameTakenState, OpConflict, RemovedPath, RemovedState,
};
use layerfs_history::records::{GcIntentKind, GcScopeKind};
use layerfs_history::{CommitId, HistoryError, LayerId, UnknownOutcomeState};
use layerfs_store_service::history::lookup::history;

fn layer(tag: u8) -> LayerId {
    let mut bytes = [0x32u8; 33];
    bytes[32] = tag;
    LayerId::from_slice(&bytes).expect("a tagged Layer identity")
}

fn commit(tag: u8) -> CommitId {
    let mut bytes = [0x12u8; 33];
    bytes[32] = tag;
    CommitId::from_slice(&bytes).expect("a tagged Commit identity")
}

fn empty(detail: &SdkValue) -> bool {
    matches!(detail, SdkValue::Map(fields) if fields.is_empty())
}

/// Spec 10: every HistoryError has one explicit wire code, built before
/// the service's `_ => "Failed"` wildcard for the codes that once fell
/// through it (StaleStore, Removed, Unsupported).
#[test]
fn every_history_error_maps_to_its_spec_10_wire_code() {
    let cases = [
        (HistoryError::InvalidInput("bad"), "InvalidInput"),
        (HistoryError::Missing(Missing::Branch), "NoSuchBranch"),
        (HistoryError::Missing(Missing::Commit), "NoSuchCommit"),
        (HistoryError::Missing(Missing::LayerStack), "NotFound"),
        (HistoryError::Missing(Missing::Layer), "NotFound"),
        (HistoryError::Unsupported("no"), "Unsupported"),
        (HistoryError::Exists("there"), "Exists"),
        (HistoryError::Conflict("moved"), "Conflict"),
        (HistoryError::StaleStore, "StaleStore"),
        (HistoryError::Busy, "Busy"),
        (HistoryError::Capacity("full"), "Capacity"),
        (HistoryError::Integrity("broken"), "Failed"),
        (HistoryError::NotInHistory("other stack"), "NotInHistory"),
        (HistoryError::OwnershipUnavailable, "Failed"),
        (HistoryError::ContinuityUnavailable, "Failed"),
    ];
    for (error, code) in cases {
        let refused = history(error);
        assert_eq!(refused.code, code, "{refused:?}");
        assert!(empty(&refused.detail), "{} carries no detail", refused.code);
    }
}

/// The moving-head families map to the one HeadMoved code, whatever their
/// shape; their detail stays empty because the code names the outcome.
#[test]
fn the_moved_head_family_maps_to_head_moved() {
    let moved = HistoryError::HeadMoved(Box::new(MovedState {
        expected_head: Some(commit(0x12)),
        actual_head: None,
        expected_base: layer(0x32),
        actual_base: layer(0x33),
    }));
    let refused = history(moved);
    assert_eq!(refused.code, "HeadMoved");
    assert!(empty(&refused.detail));

    let stack = HistoryError::StackMoved {
        expected: layer(0x32),
        actual: layer(0x33),
    };
    assert_eq!(history(stack).code, "HeadMoved");

    let base = HistoryError::BaseMismatch {
        commit_base: layer(0x32),
        branch_base: layer(0x33),
    };
    assert_eq!(history(base).code, "Failed");
}

#[test]
fn name_taken_reports_the_holder_and_source() {
    let state = NameTakenState::new(
        NameHolder::new(HolderKind::Project, "shop", HolderState::Deleted),
        NameSource::Home,
    );
    let refused = history(HistoryError::NameTaken(Box::new(state)));
    assert_eq!(refused.code, "NameTaken");
    let holder = refused.detail.get("holder").expect("holder");
    assert_eq!(holder.get_text("kind"), Some("project"));
    assert_eq!(holder.get_text("name"), Some("shop"));
    assert_eq!(holder.get_text("state"), Some("deleted"));
    assert_eq!(refused.detail.get_text("source"), Some("home"));
}

#[test]
fn removed_reports_every_path_and_frontier() {
    let state = RemovedState::new(vec![RemovedPath::new("/b", 9), RemovedPath::new("/a", 4)]);
    let refused = history(HistoryError::Removed(Box::new(state)));
    assert_eq!(refused.code, "Removed");
    let SdkValue::List(paths) = refused.detail.get("paths").expect("paths") else {
        panic!("paths are a list: {:?}", refused.detail);
    };
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[1].get_text("path"), Some("/a"));
    assert_eq!(paths[1].get_int("frontier_serial"), Some(4));
}

#[test]
fn mounts_active_reports_every_holder() {
    let refused = history(HistoryError::MountsActive(Box::new(MountHolders::new(
        Vec::new(),
    ))));
    assert_eq!(refused.code, "MountsActive");
    let SdkValue::List(holders) = refused.detail.get("holders").expect("holders") else {
        panic!("holders are a list: {:?}", refused.detail);
    };
    assert!(holders.is_empty());
}

/// Busy and Unresolved share one entity/phase/op shape, and the resource
/// scope kind names what is held (store, project or branch), not the
/// operation kind. The PostgreSQL holder is null unless a lock named one.
#[test]
fn busy_and_unresolved_report_the_resource_scope_and_holder() {
    let busy = OpConflict::new(
        GcIntentKind::Compaction,
        GcScopeKind::Store,
        "store",
        Some("swap".to_owned()),
        "00ff".to_owned(),
        false,
    )
    .with_backend(
        Some(4242),
        Some("10.0.0.7".to_owned()),
        Some("2026-01-02 03:04:05+00".to_owned()),
    );
    let refused = history(HistoryError::BusyOp(Box::new(busy)));
    assert_eq!(refused.code, "Busy");
    let entity = refused.detail.get("entity").expect("entity");
    assert_eq!(entity.get_text("kind"), Some("store"));
    assert_eq!(entity.get_text("name"), Some("store"));
    assert_eq!(refused.detail.get_text("phase"), Some("swap"));
    assert_eq!(refused.detail.get_text("op"), Some("00ff"));
    assert!(!refused.detail.get_bool("uncertain"));
    assert_eq!(refused.detail.get_int("pid"), Some(4242));
    assert_eq!(refused.detail.get_text("client_addr"), Some("10.0.0.7"));
    assert_eq!(
        refused.detail.get_text("backend_start"),
        Some("2026-01-02 03:04:05+00")
    );

    // Unresolved reports where the dead owner's op sits; on SQLite no
    // backend is named, so the holder fields are null.
    let unresolved = OpConflict::new(
        GcIntentKind::Removal,
        GcScopeKind::Project,
        "shop",
        None,
        "00aa".to_owned(),
        true,
    );
    let refused = history(HistoryError::Unresolved(Box::new(unresolved)));
    assert_eq!(refused.code, "Unresolved");
    let entity = refused.detail.get("entity").expect("entity");
    assert_eq!(entity.get_text("kind"), Some("project"));
    assert_eq!(entity.get_text("name"), Some("shop"));
    assert!(refused.detail.get("phase").expect("phase") == &SdkValue::Null);
    assert!(refused.detail.get_bool("uncertain"));
    assert!(refused.detail.get("pid").expect("pid") == &SdkValue::Null);
    assert!(refused.detail.get("client_addr").expect("addr") == &SdkValue::Null);
    assert!(refused.detail.get("backend_start").expect("start") == &SdkValue::Null);
}

/// Spec §10 A6: an `UnknownOutcome` a call can name reports
/// `{entity: {kind, name}, phase, op}`. The bare persistence error has no
/// entity yet, so a service call attaches one with `with_unknown`.
#[test]
fn unknown_reports_the_entity_and_the_durable_operation() {
    let state = UnknownOutcomeState::new(
        GcScopeKind::Project,
        "shop",
        Some("init".to_owned()),
        Some("00ff".to_owned()),
    );
    let refused = history(HistoryError::UnknownOutcome.with_unknown(state));
    assert_eq!(refused.code, "Unknown");
    let entity = refused.detail.get("entity").expect("entity");
    assert_eq!(entity.get_text("kind"), Some("project"));
    assert_eq!(entity.get_text("name"), Some("shop"));
    assert_eq!(refused.detail.get_text("phase"), Some("init"));
    assert_eq!(refused.detail.get_text("op"), Some("00ff"));
}

/// Both `phase` and `op` are null when no durable operation is known; the
/// entity is still named. A definite failure keeps its own code and detail.
#[test]
fn unknown_without_a_durable_operation_nulls_phase_and_op() {
    let state = UnknownOutcomeState::new(GcScopeKind::Store, "store", None, None);
    let refused = history(HistoryError::UnknownOutcome.with_unknown(state));
    assert_eq!(refused.code, "Unknown");
    let entity = refused.detail.get("entity").expect("entity");
    assert_eq!(entity.get_text("kind"), Some("store"));
    assert_eq!(entity.get_text("name"), Some("store"));
    assert!(refused.detail.get("phase").expect("phase") == &SdkValue::Null);
    assert!(refused.detail.get("op").expect("op") == &SdkValue::Null);

    let busy = history(HistoryError::Busy.with_unknown(UnknownOutcomeState::new(
        GcScopeKind::Branch,
        "work",
        None,
        None,
    )));
    assert_eq!(busy.code, "Busy");
    assert!(empty(&busy.detail), "a definite refusal keeps its detail");
}

/// A published history refusal keeps its own wire code: `Busy` and
/// `UnknownOutcome` stay themselves, and a reader or storage failure is not
/// known to be a history refusal, so it is `Unknown` (#45 Round 3).
#[test]
fn a_published_refusal_keeps_its_history_code() {
    use layerfs_store_service::history::lookup::published;
    use layerfs_store_service::store::PortError;
    use std::sync::Arc;

    let busy = published(Arc::new(PortError::History(HistoryError::Busy)));
    assert_eq!(busy.code, "Busy", "{busy:?}");
    let unknown = published(Arc::new(PortError::History(HistoryError::UnknownOutcome)));
    assert_eq!(unknown.code, "Unknown", "{unknown:?}");
    let poisoned = published(Arc::new(PortError::Poisoned));
    assert_eq!(poisoned.code, "Unknown", "{poisoned:?}");
}

/// F43R4-1: a bare `UnknownOutcome` renders the empty-map "no detail" (spec
/// gap G1), the shape `named_unknown` must fill with the entity it knows.
#[test]
fn a_bare_unknown_outcome_renders_the_empty_map_detail() {
    let refused = history(HistoryError::UnknownOutcome);
    assert_eq!(refused.code, "Unknown");
    assert!(empty(&refused.detail), "{refused:?}");
}
