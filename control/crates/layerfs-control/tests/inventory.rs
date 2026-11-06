//! The relationships no record stores (#536 §5, §9.2): which Stores and
//! Sandboxes a machine holds, which Sandboxes a Store or a project has, and
//! the `InUse` refusals built from them, with their texts. Layout 3: a
//! Sandbox is read from `sandbox/<name>/` alone; a layout 2 folder under a
//! machine is never read.
use layerfs_bridge::control::SdkValue;
use layerfs_control::inventory;
use layerfs_home::home::{paths, write_record};
use layerfs_home::json::parse;
use layerfs_store::record::StoreRecord;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// One home per call: parallel tests can read the same clock value (macOS
/// gives microseconds), so a counter keeps their homes apart.
fn tempdir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "layerfs-inventory-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn record(path: &Path, text: &str) {
    write_record(path, &parse(text).unwrap()).unwrap();
}

/// Machines `native` and `dev` (a Lima host of `fc`); Stores `shop` (on
/// `dev`), `docs` (no machine: `native`) and `pg` (PostgreSQL); Sandboxes
/// `a` (shop/api, on dev), `b` (docs/web, on native) and `c` (shop/api, on
/// dev); a folder `d` with no record and a layout 2 record `z` under
/// `machine/dev/sandbox/`, which no call reads.
fn home() -> PathBuf {
    let home = tempdir();
    record(
        &paths::machine_record(&home, "native"),
        r#"{"platform":"native"}"#,
    );
    record(
        &paths::machine_record(&home, "dev"),
        r#"{"platform":"lima","nested":true}"#,
    );
    record(
        &paths::machine_record(&home, "fc"),
        r#"{"platform":"firecracker","host":"dev"}"#,
    );
    record(
        &paths::store_record(&home, "shop"),
        r#"{"kind":"sqlite","machine":"dev"}"#,
    );
    record(&paths::store_record(&home, "docs"), r#"{"kind":"sqlite"}"#);
    record(
        &paths::store_record(&home, "pg"),
        r#"{"kind":"postgresql"}"#,
    );
    record(
        &paths::sandbox_record(&home, "a"),
        r#"{"v":1,"name":"a","project":"shop/api","machine":"dev"}"#,
    );
    record(
        &paths::sandbox_record(&home, "b"),
        r#"{"v":1,"name":"b","project":"docs/web","machine":"native"}"#,
    );
    record(
        &paths::sandbox_record(&home, "c"),
        r#"{"v":1,"name":"c","project":"shop/api","machine":"dev"}"#,
    );
    std::fs::create_dir_all(paths::sandbox_dir(&home, "d")).unwrap();
    record(
        &paths::machine_dir(&home, "dev").join("sandbox/z/record.json"),
        r#"{"name":"z","project":"shop/api","machine":"dev"}"#,
    );
    home
}

#[test]
fn relationships_are_read_from_the_records() {
    let home = home();
    assert_eq!(inventory::stores_on(&home, "dev").unwrap(), ["shop"]);
    assert_eq!(inventory::stores_on(&home, "native").unwrap(), ["docs"]);
    assert_eq!(inventory::sandboxes_on(&home, "dev").unwrap(), ["a", "c"]);
    assert_eq!(inventory::hosted_on(&home, "dev").unwrap(), ["fc"]);
    let names: Vec<_> = inventory::sandboxes(&home)
        .unwrap()
        .iter()
        .map(|s| (s.machine().to_owned(), s.name().to_owned()))
        .collect();
    assert_eq!(
        names,
        [
            ("dev".into(), "a".into()),
            ("native".into(), "b".into()),
            ("dev".into(), "c".into())
        ]
    );
    assert_eq!(
        inventory::sandbox(&home, "b").unwrap().unwrap().machine(),
        "native"
    );
    assert!(inventory::sandbox(&home, "d").unwrap().is_none());
    assert!(inventory::sandbox(&home, "z").unwrap().is_none());
    assert_eq!(
        inventory::sandboxes_of_store(&home, "shop").unwrap(),
        ["a", "c"]
    );
    assert_eq!(
        inventory::sandboxes_of_project(&home, "docs/web").unwrap(),
        ["b"]
    );
    assert!(inventory::sandboxes_of_project(&home, "docs/api")
        .unwrap()
        .is_empty());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_checks_refuse_in_use_with_the_same_texts() {
    let home = home();
    let refusal = inventory::can_remove_machine(&home, "dev").unwrap_err();
    assert_eq!(refusal.code, "InUse");
    assert_eq!(
        refusal.message,
        "machine dev holds Stores [shop], Sandboxes [a, c] and hosts [fc]"
    );
    // #536 §10b: what holds it, in `detail.related`
    let held = |kind: &str, name: &str| {
        SdkValue::map([
            ("kind", SdkValue::text(kind)),
            ("name", SdkValue::text(name)),
        ])
    };
    assert_eq!(
        refusal.detail.get("related"),
        Some(&SdkValue::List(vec![
            held("store", "shop"),
            held("sandbox", "a"),
            held("sandbox", "c"),
            held("machine", "fc"),
        ]))
    );
    let refusal = inventory::can_stop_machine(&home, "dev").unwrap_err();
    assert_eq!(
        refusal.message,
        "machine dev runs Sandboxes [a, c]; close them first"
    );
    let related = refusal.detail.get("related").unwrap();
    assert_eq!(
        related,
        &SdkValue::List(
            ["a", "c"]
                .iter()
                .map(|n| SdkValue::map([
                    ("kind", SdkValue::text("sandbox")),
                    ("name", SdkValue::text(*n))
                ]))
                .collect()
        )
    );
    assert!(inventory::can_stop_machine(&home, "fc").is_ok());
    let refusal = inventory::can_change_machine(&home, "native").unwrap_err();
    assert_eq!(
        refusal.message,
        "Sandboxes [b] run on native; close them before changing it"
    );
    let refusal = inventory::store_idle(&home, "shop").unwrap_err();
    assert_eq!(refusal.message, "Store shop has running Sandboxes: a, c");
    let names = |related: &SdkValue| match related {
        SdkValue::List(items) => items
            .iter()
            .map(|i| i.get_text("name").unwrap().to_owned())
            .collect(),
        _ => Vec::new(),
    };
    assert_eq!(names(refusal.detail.get("related").unwrap()), ["a", "c"]);
    assert!(inventory::store_idle(&home, "pg").is_ok());
    let refusal = inventory::can_remove_project(&home, "docs/web").unwrap_err();
    assert_eq!(refusal.message, "Sandbox b of docs/web is running");
    assert_eq!(names(refusal.detail.get("related").unwrap()), ["b"]);
    let refusal = inventory::can_remove_project(&home, "shop/api").unwrap_err();
    assert_eq!(refusal.message, "Sandboxes a, c of shop/api are running");
    assert_eq!(names(refusal.detail.get("related").unwrap()), ["a", "c"]);
    assert!(inventory::can_remove_project(&home, "shop/web").is_ok());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn push_and_pull_check_the_stores_side() {
    let home = home();
    let store = |name: &str, text: &str| StoreRecord::from_record(name, parse(text).unwrap());
    let on_dev = store("shop", r#"{"name":"shop","kind":"sqlite","machine":"dev"}"#);
    let here = store(
        "app",
        r#"{"name":"app","kind":"sqlite","machine":"native"}"#,
    );
    let pg = store(
        "pg",
        r#"{"name":"pg","kind":"postgresql","url":"postgresql://u@h/d"}"#,
    );
    let code = |result: Result<(), layerfs_bridge::control::SdkRefusal>| result.unwrap_err().code;
    assert_eq!(
        layerfs_store::sqlite::transfer::resolve::can_pull(&on_dev).unwrap(),
        "dev"
    );
    assert_eq!(
        layerfs_store::sqlite::transfer::resolve::can_pull(&here)
            .unwrap_err()
            .code,
        "InvalidInput"
    );
    let refusal = layerfs_store::sqlite::transfer::resolve::can_pull(&pg).unwrap_err();
    assert!(
        refusal.message.contains("does not move"),
        "{}",
        refusal.message
    );
    let refusal =
        layerfs_store::sqlite::transfer::resolve::can_push(&home, &on_dev, "fc").unwrap_err();
    assert!(
        refusal.message.contains("`layerfs store pull shop`"),
        "{}",
        refusal.message
    );
    assert!(layerfs_store::sqlite::transfer::resolve::can_push(&home, &here, "dev").is_ok());
    assert_eq!(
        code(layerfs_store::sqlite::transfer::resolve::can_push(
            &home, &here, "native"
        )),
        "InvalidInput"
    );
    assert_eq!(
        code(layerfs_store::sqlite::transfer::resolve::can_push(
            &home, &here, "nosuch"
        )),
        "NotFound"
    );
    assert_eq!(
        code(layerfs_store::sqlite::transfer::resolve::can_push(
            &home, &pg, "dev"
        )),
        "InvalidInput"
    );
    let _ = std::fs::remove_dir_all(&home);
}
