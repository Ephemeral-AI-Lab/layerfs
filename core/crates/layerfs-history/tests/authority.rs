//! The ordered place-token comparison: only the millisecond orders two
//! values one home minted, and another home's values are unordered. Two
//! values from the same millisecond with different nonces have no order
//! between them and must stay `Unordered`, so a live owner is never settled
//! on a coin flip.
use layerfs_history::{
    compare_place, ordered_token_parts, Liveness, PlaceAuthority, PlaceOrder, PlaceRecord,
};

fn token(milliseconds: u64, nonce: &str) -> String {
    homed("aa11", "worker", milliseconds, nonce)
}

fn homed(home: &str, kind: &str, milliseconds: u64, nonce: &str) -> String {
    format!("p1:{home}.{kind}:{milliseconds:016}:{nonce}")
}

#[test]
fn tokens_in_one_millisecond_with_different_nonces_are_unordered() {
    assert_eq!(
        compare_place(&token(1, "aa"), &token(1, "bb")),
        PlaceOrder::Unordered
    );
    assert_eq!(
        compare_place(&token(1, "bb"), &token(1, "aa")),
        PlaceOrder::Unordered
    );
    assert_eq!(
        compare_place(&token(1, "aa"), &token(1, "aa")),
        PlaceOrder::Equal
    );
}

#[test]
fn only_a_different_millisecond_orders_a_token() {
    assert_eq!(
        compare_place(&token(1, "ff"), &token(2, "00")),
        PlaceOrder::RecordedOlder
    );
    assert_eq!(
        compare_place(&token(2, "00"), &token(1, "ff")),
        PlaceOrder::RecordedNewer
    );
    // A value written before the ordering cannot be related to anything
    // else: no evidence either way.
    assert_eq!(
        compare_place("worker:old", &token(2, "00")),
        PlaceOrder::Foreign
    );
}

#[test]
fn tokens_of_different_homes_are_foreign_and_never_dead() {
    // Two homes each name a `box` machine: home B's newer start must not
    // prove home A's older placement gone.
    let a = homed("aa11", "worker", 1, "aa");
    let b = homed("bb22", "worker", 2, "bb");
    assert_eq!(compare_place(&a, &b), PlaceOrder::Foreign);
    assert_eq!(compare_place(&b, &a), PlaceOrder::Foreign);
    let mount = PlaceAuthority {
        machine_incarnation: Some(a.clone()),
        sandbox_generation: Some(homed("aa11", "sandbox", 1, "aa")),
    };
    let other_home = PlaceRecord {
        machine_incarnation: Some(b),
        sandbox_generation: Some(homed("bb22", "sandbox", 2, "bb")),
        sandbox_retired: Some(homed("bb22", "sandbox", 2, "bb")),
    };
    // Another home's values are no evidence: the holder stays live, as with
    // no place evidence at all.
    assert_eq!(mount.liveness(Some(&other_home)), Liveness::Live);
    // The same home's newer start still proves the older placement dead.
    let same_home = PlaceRecord {
        machine_incarnation: Some(homed("aa11", "worker", 2, "bb")),
        ..PlaceRecord::default()
    };
    assert_eq!(mount.liveness(Some(&same_home)), Liveness::Dead);
}

#[test]
fn a_token_without_a_home_id_is_not_ordered() {
    assert_eq!(ordered_token_parts("p1:worker:0000000000000001:aa"), None);
    assert_eq!(ordered_token_parts("p1:.worker:0000000000000001:aa"), None);
    assert_eq!(
        ordered_token_parts(&homed("aa11", "worker", 1, "aa")),
        Some(("aa11.worker", 1, "aa"))
    );
    assert_eq!(
        compare_place(
            "p1:worker:0000000000000001:aa",
            "p1:worker:0000000000000002:aa"
        ),
        PlaceOrder::Foreign
    );
}

#[test]
fn a_tombstone_newer_than_the_live_generation_is_uncertain() {
    let generation = homed("aa11", "sandbox", 5, "aa");
    let mount = PlaceAuthority {
        machine_incarnation: None,
        sandbox_generation: Some(generation.clone()),
    };
    let place = |retired: String| PlaceRecord {
        machine_incarnation: None,
        sandbox_generation: Some(generation.clone()),
        sandbox_retired: Some(retired),
    };
    // Only the tombstone of this very generation retires the live record.
    assert_eq!(
        mount.liveness(Some(&place(generation.clone()))),
        Liveness::Dead
    );
    // A newer tombstone beside an equal record (a clock step): uncertain.
    assert_eq!(
        mount.liveness(Some(&place(homed("aa11", "sandbox", 9, "bb")))),
        Liveness::Uncertain
    );
    // An older tombstone names an earlier Sandbox of the same name.
    assert_eq!(
        mount.liveness(Some(&place(homed("aa11", "sandbox", 1, "bb")))),
        Liveness::Live
    );
}
