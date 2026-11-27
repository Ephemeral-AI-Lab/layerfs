//! Ordered place values: each names the home that minted it, and a value is
//! minted past the one it replaces even when the wall clock stepped back.
use layerfs_home::reply::values::ordered_token;

fn milliseconds(value: &str) -> u64 {
    value.split(':').nth(2).unwrap().parse().unwrap()
}

#[test]
fn a_value_names_the_home_that_minted_it() {
    let value = ordered_token("0a1b", "worker", &[]);
    assert!(value.starts_with("p1:0a1b.worker:"), "{value}");
    // a value no home minted carries no home id
    let bare = ordered_token("", "worker", &[]);
    assert!(bare.starts_with("p1:worker:"), "{bare}");
}

#[test]
fn a_value_is_minted_past_its_floor_after_a_backwards_clock() {
    // The previous value was minted far ahead of this clock (the clock has
    // since stepped back): the new one still follows it.
    let ahead = u64::MAX / 4;
    let previous = format!("p1:0a1b.worker:{ahead:016}:aa");
    let next = ordered_token("0a1b", "worker", &[&previous, "", "worker:old"]);
    assert_eq!(milliseconds(&next), ahead + 1, "{next}");
    // a floor behind the clock changes nothing
    let behind = ordered_token("0a1b", "worker", &["p1:0a1b.worker:0000000000000001:aa"]);
    assert!(milliseconds(&behind) > 1, "{behind}");
}
