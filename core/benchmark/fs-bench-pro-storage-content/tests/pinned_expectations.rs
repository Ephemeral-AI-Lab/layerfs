//! The pinned-constant oracle: O1 and O3, and the ways each can silently fail.
//!
//! `gates_and_oracles.md` section 4 defines O1 as an expected root *"from a frozen
//! constant"* and O3 as counts *"pinned as constants"*. These tests hold the table
//! to what the specification asks of it, and they hold the gate to failing when it
//! should rather than when it should not.

use fs_bench_storage_content::gates::Status;
use fs_bench_storage_content::registry::{self, Admission};
use fs_bench_storage_content::workload::expected::{identity_digest, Expected};
use layerfs_content::ObjectId;

/// The table parses, and it is not empty.
#[test]
fn the_pinned_table_parses_and_carries_constants() {
    let expected = Expected::load().expect("tests/golden/expected.tsv must parse");
    assert!(
        !expected.is_empty(),
        "a table with no constants pins nothing and every gate would be vacuous"
    );
}

/// Every admission case carries at least one pinned O3 constant.
///
/// This is the coverage rule the registry self-check asserts: the pinned set cannot
/// shrink without the self-check saying so. The sealed-oracle parity set records why
/// — an earlier revision under-counted `edit_reference`, and the oracle could have
/// silently shrunk with it.
#[test]
fn every_admission_case_pins_at_least_one_counter() {
    let expected = Expected::load().expect("the table must parse");
    let uncovered = expected.coverage(registry::cases());
    assert!(
        uncovered.is_empty(),
        "admission cases with no pinned O3 constant: {uncovered:?}"
    );
}

/// A pinned counter that moved is a `FAIL`, with both numbers.
#[test]
fn a_moved_counter_fails_and_names_both_numbers() {
    let expected = Expected::load().expect("the table must parse");
    let case = registry::cases()
        .iter()
        .find(|case| case.admission == Admission::Admission)
        .expect("the registry has admission cases");
    let pinned = expected.counters_of(case.id);
    assert!(!pinned.is_empty());
    let moved: Vec<(String, i128)> = pinned
        .iter()
        .map(|(key, value)| (key.clone(), value + 1))
        .collect();
    let gates = expected.counter_gates(case.id, &moved);
    assert_eq!(gates[0].status, Status::Fail, "a moved counter must fail");
    assert!(
        gates[0].measured.contains(&pinned[0].0),
        "the failure names the counter that moved: {}",
        gates[0].measured
    );
}

/// An invocation that published none of its row's pinned counters is
/// `INCOMPLETE`, never a pass: a gate that compares nothing is not a gate.
#[test]
fn an_invocation_that_gates_nothing_is_incomplete() {
    let expected = Expected::load().expect("the table must parse");
    let case = registry::cases()
        .iter()
        .find(|case| case.admission == Admission::Admission)
        .expect("the registry has admission cases");
    let gates = expected.counter_gates(case.id, &[]);
    assert_eq!(
        gates[0].status,
        Status::Incomplete,
        "an invocation that published no pinned counter must be INCOMPLETE: {:?}",
        gates[0]
    );
}

/// A pinned counter the *other* invocation publishes is not a defect here.
///
/// `c2.delta.cdc-locality` writes `verify.members` from its deferred verification
/// invocation. The performance invocation must not be failed for a counter that
/// belongs to the other phase; `runner.py verify` takes that coverage back from the
/// row's whole trace.
#[test]
fn a_counter_published_by_the_other_invocation_is_not_a_defect_here() {
    let expected = Expected::load().expect("the table must parse");
    let case = registry::cases()
        .iter()
        .find(|case| case.id == "dedup-cdc-overwrite-1")
        .expect("the delta family is registered");
    let published: Vec<(String, i128)> = expected
        .counters_of(case.id)
        .into_iter()
        .filter(|(key, _)| key != "verify.members")
        .collect();
    let gates = expected.counter_gates(case.id, &published);
    assert_eq!(
        gates[0].status,
        Status::Pass,
        "a counter that belongs to the deferred phase must not fail this one: {:?}",
        gates[0]
    );
}

/// A case with no pinned constant at all is `INCOMPLETE`, not a silent skip.
#[test]
fn a_case_with_no_pinned_constant_is_incomplete() {
    let expected = Expected::load().expect("the table must parse");
    let gates = expected.counter_gates("no-such-case", &[]);
    assert_eq!(gates[0].status, Status::Incomplete);
}

/// An identity digest is a function of the identities and their order.
#[test]
fn an_identity_digest_commits_to_order() {
    let first = ObjectId::for_bytes(b"first");
    let second = ObjectId::for_bytes(b"second");
    let forwards = identity_digest(&[first, second]);
    let backwards = identity_digest(&[second, first]);
    assert_ne!(forwards, backwards, "the digest must commit to the order");
    assert_eq!(forwards.len(), 64);
    assert_eq!(
        forwards,
        identity_digest(&[first, second]),
        "the digest must be deterministic"
    );
}

/// A digest gate against a pinned constant is two-sided.
#[test]
fn a_pinned_digest_gate_is_two_sided() {
    let expected = Expected::load().expect("the table must parse");
    let (case_id, name) = registry::cases()
        .iter()
        .flat_map(|case| {
            [
                "file_root",
                "filesystem_root",
                "members",
                "additions",
                "leaves",
                "supplied",
            ]
            .into_iter()
            .map(move |name| (case.id, name))
        })
        .find(|(case_id, name)| expected.digest(case_id, name).is_some())
        .expect("at least one identity is pinned");
    let pinned = expected.digest(case_id, name).expect("pinned").to_string();
    assert_eq!(
        expected
            .digest_gate(case_id, name, "g1.o1-pinned-identity", &pinned)
            .status,
        Status::Pass
    );
    assert_eq!(
        expected
            .digest_gate(case_id, name, "g1.o1-pinned-identity", &"0".repeat(64))
            .status,
        Status::Fail,
        "a digest that is not the pinned one must fail"
    );
    assert_eq!(
        expected
            .digest_gate(
                case_id,
                "not_a_pinned_name",
                "g1.o1-pinned-identity",
                &pinned
            )
            .status,
        Status::Incomplete,
        "an unpinned name must be INCOMPLETE rather than quietly skipped"
    );
}
