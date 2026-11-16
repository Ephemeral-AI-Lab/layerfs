//! §8 step 10 unit checks for the PostgreSQL rebuild, moved out of
//! `backend/postgresql/compact/`: product source holds no tests (the
//! product-boundary guard).
#![cfg(feature = "postgresql")]

use layerfs_persistence::internals::{
    decode_kept_waste, effective_rank, encode_kept_waste, report, standby_satisfied, sync_name,
    sync_rank, Replica,
};
use layerfs_persistence::{CompactKeptWaste, CompactPath, CompactResidue};

fn replica(name: &str, sync_state: &str) -> Replica {
    Replica {
        name: name.to_owned(),
        streaming: true,
        sync_state: sync_state.to_owned(),
    }
}

#[test]
fn the_standby_predicate_waits_only_for_a_named_standby() {
    // Below remote_write, or with no names, no standby is required.
    assert!(standby_satisfied(0, "", &[]));
    assert!(standby_satisfied(1, "", &[]));
    assert!(standby_satisfied(2, "", &[]));
    assert!(standby_satisfied(4, "  ", &[]));
    // At or above remote_write with names, a streaming sync/quorum standby
    // must exist.
    assert!(!standby_satisfied(2, "FIRST 1 (s1)", &[]));
    assert!(!standby_satisfied(4, "ANY 1 (s1)", &[]));
    assert!(standby_satisfied(
        2,
        "FIRST 1 (s1)",
        &[replica("s1", "sync")]
    ));
    assert!(standby_satisfied(3, "ANY 2 (s1,s2)", &[replica("s1", "quorum")]) == false);
    assert!(standby_satisfied(
        3,
        "ANY 2 (s1,s2)",
        &[replica("s1", "quorum"), replica("s2", "quorum")]
    ));
}

#[test]
fn synchronous_commit_strength_is_the_spec_order() {
    assert_eq!(sync_rank("off"), 0);
    assert_eq!(sync_rank("local"), 1);
    assert_eq!(sync_rank("remote_write"), 2);
    assert_eq!(sync_rank("on"), 3);
    assert_eq!(sync_rank("remote_apply"), 4);
    assert_eq!(sync_name(4), "remote_apply");
    assert_eq!(sync_name(0), "off");
}

#[test]
fn the_effective_sync_floor_is_local() {
    // Both `off` still commits at `local`: the spec's floor (§8 step 11).
    assert_eq!(effective_rank("off", "off"), 1);
    assert_eq!(effective_rank("local", "off"), 1);
    assert_eq!(effective_rank("off", "remote_apply"), 4);
    assert_eq!(effective_rank("on", "off"), 3);
}

/// The `gc_op.kept_waste` bytea round-trips the whole §11 part, references
/// and capped flag included; a corrupt blob is refused, never zeroed.
#[test]
fn kept_waste_round_trips_through_the_op_blob() {
    let waste = CompactKeptWaste {
        dead_in_retained_packs_bytes: 5,
        delta_bases_bytes: 7,
        delta_base_references: vec![[9; 32], [10; 32]],
        delta_base_references_capped: true,
        inode_blocks_bytes: 11,
    };
    let bytes = encode_kept_waste(&waste);
    assert_eq!(decode_kept_waste(&bytes).unwrap(), waste);
    let mut broken = bytes;
    broken[0] = 2;
    assert!(decode_kept_waste(&broken).is_err());
}

/// A resumed `G + 1` run with dead bytes and no relation delta reports
/// `no-shrink` rather than the misleading `no-waste`.
#[test]
fn a_resumed_rebuild_with_no_relation_delta_explains_no_shrink() {
    let receipt = report(
        CompactPath::Rebuild,
        1,
        2,
        60,
        60,
        46,
        0,
        0,
        CompactKeptWaste::default(),
        57_344,
        CompactResidue::default(),
        1_000,
        1,
    );
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "physicalShrink" && zero.reason == "no-shrink"));
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "backendReusable" && zero.reason == "postgresql-in-place"));
}
