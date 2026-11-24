//! The ordered place-token comparison: only the millisecond orders two
//! values minted by a start. Two values from the same millisecond with
//! different nonces have no order between them and must stay `Unordered`, so
//! a live owner is never settled on a coin flip.
use layerfs_history::{compare_place, PlaceOrder};

fn token(milliseconds: u64, nonce: &str) -> String {
    format!("p1:worker:{milliseconds:016}:{nonce}")
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
    // A value written before the ordering is unordered against anything else.
    assert_eq!(compare_place("worker:old", &token(2, "00")), PlaceOrder::Unordered);
}
