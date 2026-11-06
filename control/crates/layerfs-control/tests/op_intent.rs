//! The one in-flight intent (#536 §7.1) and the per-entity lock (§8).
use layerfs_bridge::control::SdkValue;
use layerfs_control::common::home::entity_lock;
use layerfs_control::common::op::{Op, Standing};
use std::time::Duration;

/// A record whose intent names a process that is not alive (id 0).
fn gone(uncertain: bool) -> SdkValue {
    let op = SdkValue::map([
        ("id", SdkValue::text("op-00000000000000aa")),
        ("name", SdkValue::text("store.remove")),
        ("phase", SdkValue::text("delete")),
        ("args", SdkValue::Null),
        ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
        ("started", SdkValue::Int(1)),
        ("updated", SdkValue::Int(2)),
        ("uncertain", SdkValue::Bool(uncertain)),
    ]);
    SdkValue::map([("name", SdkValue::text("app")), ("op", op)])
}

#[test]
fn a_live_owner_is_busy_and_a_gone_one_blocks_only_when_uncertain() {
    let mine = Op::begin("store.remove", "check", SdkValue::Null).unwrap();
    assert!(
        mine.id().starts_with("op-") && mine.id().len() == 19,
        "{}",
        mine.id()
    );
    assert!(mine.mine());
    assert_eq!(mine.standing(), Standing::Running);
    let busy = mine.blocks("store", "app", "store status", "store remove");
    assert_eq!(busy.unwrap_err().code, "Busy");
    let moved = mine.clone().at("delete");
    assert_eq!((moved.phase(), moved.id()), ("delete", mine.id()));

    let stale = Op::of(&gone(false)).unwrap();
    assert_eq!(stale.standing(), Standing::Stale);
    stale
        .blocks("store", "app", "store status", "store remove")
        .unwrap();

    let unresolved = Op::of(&gone(true)).unwrap();
    assert_eq!(unresolved.standing(), Standing::Unresolved);
    let refusal = unresolved
        .blocks("store", "app", "store status", "store remove")
        .unwrap_err();
    assert_eq!(refusal.code, "Unresolved");
    // the detail names the entity, the phase and the intent (#536 §10)
    let entity = refusal.detail.get("entity").unwrap();
    assert_eq!(entity.get_text("kind"), Some("store"));
    assert_eq!(entity.get_text("name"), Some("app"));
    assert_eq!(refusal.detail.get_text("phase"), Some("delete"));
    assert_eq!(refusal.detail.get_text("op"), Some("op-00000000000000aa"));
    assert!(
        refusal.message.contains("`layerfs store status app`"),
        "{}",
        refusal.message
    );
    assert!(
        refusal.message.contains("`layerfs store remove app`"),
        "{}",
        refusal.message
    );

    let shown = unresolved.to_sdk();
    assert_eq!(shown.get_text("name"), Some("store.remove"));
    assert_eq!(shown.get("uncertain"), Some(&SdkValue::Bool(true)));
    assert!(shown.get("owner").is_none());
    assert!(Op::of(&SdkValue::map([("name", SdkValue::text("app"))])).is_none());
}

#[test]
fn an_entity_lock_excludes_a_second_holder_until_dropped() {
    let dir = std::env::temp_dir().join(format!("layerfs-entity-lock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lock");
    let held = entity_lock::take(&path, Duration::ZERO, "Store app was held").unwrap();
    let refusal = entity_lock::take(&path, Duration::ZERO, "Store app was held").unwrap_err();
    assert_eq!(refusal.code, "Unavailable");
    assert!(
        refusal.message.starts_with("Store app was held"),
        "{}",
        refusal.message
    );
    drop(held);
    entity_lock::take(&path, Duration::ZERO, "Store app was held").unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}
