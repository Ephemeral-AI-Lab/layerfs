//! The in-process call refusal `store.compact` reports (#51 Round 3, F51R3-1).
//!
//! A call still running on the Store in this process keeps the open cache's
//! `InUse`; the compaction maps it into the GC vocabulary as `Busy` naming
//! the Store with a null phase and op. Product source holds no tests (the
//! product-boundary guard), so `compact::gc_refusal` is `#[doc(hidden)] pub`
//! for this target alone.
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::outcome::refused;
use layerfs_store::compact::gc_refusal;

fn in_use() -> SdkRefusal {
    refused("InUse", "a call is running on the Store in this process")
}

fn field<'a>(detail: &'a SdkValue, name: &str) -> &'a SdkValue {
    let SdkValue::Map(fields) = detail else {
        panic!("detail is not a map: {detail:?}");
    };
    fields
        .iter()
        .find(|(field, _)| field == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("detail has no `{name}`: {detail:?}"))
}

/// F51R3-1: a call running on the Store in this process is `Busy`, never
/// `InUse`, and names the Store with no phase or op.
#[test]
fn a_running_call_is_busy_for_the_store_not_in_use() {
    let busy = gc_refusal("shop", in_use());
    assert_eq!(busy.code, "Busy", "{busy:?}");
    assert!(
        busy.message.contains("Store shop is busy"),
        "{}",
        busy.message
    );
    assert!(!busy.message.contains("InUse"), "{}", busy.message);
    assert_eq!(
        field(&busy.detail, "entity"),
        &SdkValue::map([
            ("kind", SdkValue::text("store")),
            ("name", SdkValue::text("shop")),
        ])
    );
    assert_eq!(field(&busy.detail, "phase"), &SdkValue::Null);
    assert_eq!(field(&busy.detail, "op"), &SdkValue::Null);
}

/// The mapping touches only the open cache's `InUse`; every other refusal is
/// what it was, so `Busy`, `Failed` and the rest keep their detail.
#[test]
fn exactly_the_open_cache_refusal_is_mapped() {
    for (code, message) in [
        ("Failed", "the open cache failed"),
        ("Busy", "the Store is held by another writer"),
    ] {
        let original = refused(code, message);
        assert_eq!(gc_refusal("shop", original.clone()), original);
    }
}
