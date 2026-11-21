//! The relationships no record stores (#536 §5, §9.2): which Stores and
//! Sandboxes a machine holds, which Sandboxes a Store or a project has, and
//! the `InUse`/`Busy` refusals built from them, with their texts. Layout 3: a
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

/// Machines `native`, `dev` (Lima) and `fc` (an old Firecracker record
/// hosted on `dev`); Stores `shop` (on
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
        // a 2.0 record names its VM (one without is an earlier build's)
        r#"{"platform":"lima","vm_name":"layerfs-00000000-dev"}"#,
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
    // §5: an open Sandbox of the project refuses `project.delete` with
    // `Busy`, even with nothing mounted.
    let refusal = inventory::can_remove_project(&home, "docs/web").unwrap_err();
    assert_eq!(refusal.code, "Busy");
    assert_eq!(refusal.message, "Sandbox b of docs/web is running");
    // #536 §10: the detail names the entity and phase/op, not only `related`
    let entity = refusal.detail.get("entity").expect("entity");
    assert_eq!(entity.get_text("kind"), Some("project"));
    assert_eq!(entity.get_text("name"), Some("docs/web"));
    assert_eq!(refusal.detail.get("phase"), Some(&SdkValue::Null));
    assert_eq!(refusal.detail.get("op"), Some(&SdkValue::Null));
    assert_eq!(
        refusal.detail.get("related"),
        Some(&SdkValue::List(vec![SdkValue::map([
            ("kind", SdkValue::text("sandbox")),
            ("name", SdkValue::text("b"))
        ])]))
    );
    let refusal = inventory::can_remove_project(&home, "shop/api").unwrap_err();
    assert_eq!(refusal.code, "Busy");
    assert!(inventory::can_remove_project(&home, "docs/api").is_ok());
    let _ = std::fs::remove_dir_all(&home);
}

/// A Store record can name a machine with no record of its own: the doctor's
/// data-volume check inventories those sqlite Stores before an orphan volume
/// is removed.
#[test]
fn stores_on_a_machine_with_no_record_are_still_read() {
    let home = home();
    record(
        &paths::store_record(&home, "orphan"),
        r#"{"kind":"sqlite","machine":"ghost"}"#,
    );
    assert_eq!(inventory::stores_on(&home, "ghost").unwrap(), ["orphan"]);
    let _ = std::fs::remove_dir_all(&home);
}

/// A PostgreSQL Store is on no machine: a machine's own connection to one
/// (its record's `machines`) and a Sandbox of one hold no Store on the
/// machine, so only its sqlite Stores, Sandboxes and hosted records refuse
/// its removal.
#[test]
fn a_postgresql_store_holds_no_machine() {
    let home = home();
    record(
        &paths::store_record(&home, "team"),
        r#"{"kind":"postgresql","machines":{"dev":{"url":"postgresql://lfs_dev@10.0.0.5/team"}}}"#,
    );
    assert_eq!(inventory::stores_on(&home, "dev").unwrap(), ["shop"]);
    let refusal = inventory::can_remove_machine(&home, "dev").unwrap_err();
    assert!(
        refusal.message.contains("Stores shop;"),
        "{}",
        refusal.message
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_checks_refuse_in_use_with_the_same_texts() {
    let home = home();
    let refusal = inventory::can_remove_machine(&home, "dev").unwrap_err();
    assert_eq!(refusal.code, "InUse");
    assert_eq!(
        refusal.message,
        "machine dev holds Stores shop; Sandboxes a, c; old Firecracker machines fc"
    );
    // each kind as a next field; Stores hold it too, so no single fix
    let listed =
        |names: &[&str]| SdkValue::List(names.iter().map(|n| SdkValue::text(*n)).collect());
    let next = refusal.detail.get("next").unwrap();
    assert_eq!(next.get("stores"), Some(&listed(&["shop"])));
    assert_eq!(next.get("sandboxes"), Some(&listed(&["a", "c"])));
    assert_eq!(next.get("machines"), Some(&listed(&["fc"])));
    assert_eq!(refusal.detail.get("fix"), None);
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
        "machine dev runs Sandboxes a, c: remove them first, or stop it with --force"
    );
    let next = refusal.detail.get("next").unwrap();
    assert_eq!(next.get("sandboxes"), Some(&listed(&["a", "c"])));
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs sandbox remove a && layerfs sandbox remove c && layerfs machine stop dev")
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
    let refusal = inventory::can_change_machine(&home, "native").unwrap_err();
    assert_eq!(
        refusal.message,
        "Sandbox b runs on machine native: remove it before changing it"
    );
    let next = refusal.detail.get("next").unwrap();
    assert_eq!(next.get("sandboxes"), Some(&listed(&["b"])));
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs sandbox remove b")
    );
    let refusal = inventory::store_idle(&home, "shop").unwrap_err();
    assert_eq!(refusal.message, "Store shop has running Sandboxes: a, c");
    let next = refusal.detail.get("next").unwrap();
    assert_eq!(next.get("sandboxes"), Some(&listed(&["a", "c"])));
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs sandbox remove a && layerfs sandbox remove c")
    );
    let names = |related: &SdkValue| match related {
        SdkValue::List(items) => items
            .iter()
            .map(|i| i.get_text("name").unwrap().to_owned())
            .collect(),
        _ => Vec::new(),
    };
    assert_eq!(names(refusal.detail.get("related").unwrap()), ["a", "c"]);
    assert!(inventory::store_idle(&home, "pg").is_ok());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_move_checks_the_stores_side() {
    use layerfs_store::sqlite::transfer::resolve::can_move;
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
    let code =
        |result: Result<String, layerfs_bridge::control::SdkRefusal>| result.unwrap_err().code;
    // to or from this computer: the machine it goes from
    assert_eq!(can_move(&home, &on_dev, "native").unwrap(), "dev");
    assert_eq!(can_move(&home, &here, "dev").unwrap(), "native");
    assert_eq!(code(can_move(&home, &here, "native")), "InvalidInput");
    let refusal = can_move(&home, &pg, "native").unwrap_err();
    assert!(
        refusal.message.contains("does not move"),
        "{}",
        refusal.message
    );
    // between two other machines: refused, naming the move here first (C13)
    let refusal = can_move(&home, &on_dev, "fc").unwrap_err();
    assert_eq!(refusal.code, "InvalidInput");
    assert!(
        refusal.message.contains("`layerfs store move shop native`"),
        "{}",
        refusal.message
    );
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs store move shop native")
    );
    assert_eq!(code(can_move(&home, &here, "nosuch")), "NotFound");
    assert_eq!(code(can_move(&home, &pg, "dev")), "InvalidInput");
    let _ = std::fs::remove_dir_all(&home);
}

/// Held by Sandboxes alone, a machine's removal is fixed by removing them
/// first; a stopped machine gets no such fix, since `sandbox remove` cannot
/// reach a daemon on it (LF2 review).
#[test]
fn a_stopped_machine_offers_no_sandbox_removal() {
    let home = home();
    let machine = paths::machine_record(&home, "box");
    record(
        &machine,
        r#"{"platform":"lima","vm_name":"layerfs-00000000-box","state":"running"}"#,
    );
    record(
        &paths::sandbox_record(&home, "s1"),
        r#"{"v":1,"name":"s1","project":"docs/web","machine":"box"}"#,
    );
    let refusal = inventory::can_remove_machine(&home, "box").unwrap_err();
    assert_eq!(refusal.message, "machine box holds Sandboxes s1");
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs sandbox remove s1 && layerfs machine remove box")
    );
    record(
        &machine,
        r#"{"platform":"lima","vm_name":"layerfs-00000000-box","state":"stopped"}"#,
    );
    let refusal = inventory::can_remove_machine(&home, "box").unwrap_err();
    assert_eq!(refusal.code, "InUse");
    assert!(
        refusal
            .message
            .starts_with("machine box holds Sandboxes s1; it is stopped"),
        "{}",
        refusal.message
    );
    assert_eq!(refusal.detail.get("fix"), None);
    let next = refusal.detail.get("next").unwrap();
    assert_eq!(
        next.get("sandboxes"),
        Some(&SdkValue::List(vec![SdkValue::text("s1")]))
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// A machine whose record has no 2.0 name (`vm_name`) and still holds
/// Sandboxes, a Store and an old Firecracker record: its state, as the
/// records give it, shows the same fix chain as its remove's `InUse`
/// refusal (RT5 note 3), built by the one builder; with nothing on it the
/// fix is the bare remove.
#[test]
fn a_nameless_machines_state_shows_its_refusals_chain() {
    let home = home();
    let machine = paths::machine_record(&home, "dev");
    record(&machine, r#"{"platform":"lima","state":"running"}"#);
    let chain = "layerfs sandbox remove a && layerfs sandbox remove c && \
                 layerfs store remove shop && layerfs machine remove fc && \
                 layerfs machine remove dev";
    let refusal = inventory::can_remove_machine(&home, "dev").unwrap_err();
    assert_eq!(refusal.code, "InUse");
    assert_eq!(refusal.detail.get_text("fix"), Some(chain));
    let read = layerfs_machine::record::MachineRecord::read(&home, "dev").unwrap();
    let state = inventory::machine_state(&home, "dev", read.as_ref()).unwrap();
    let fields = state.write(SdkValue::Map(Vec::new()));
    assert_eq!(fields.get_text("reason"), Some("outdated"), "{fields:?}");
    assert_eq!(fields.get_text("fix"), Some(chain));
    // the same machine with nothing recorded on it
    let bare = tempdir();
    record(
        &paths::machine_record(&bare, "dev"),
        r#"{"platform":"lima","state":"running"}"#,
    );
    let read = layerfs_machine::record::MachineRecord::read(&bare, "dev").unwrap();
    let state = inventory::machine_state(&bare, "dev", read.as_ref()).unwrap();
    let fields = state.write(SdkValue::Map(Vec::new()));
    assert_eq!(fields.get_text("fix"), Some("layerfs machine remove dev"));
    inventory::can_remove_machine(&bare, "dev").unwrap();
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&bare);
}
