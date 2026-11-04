//! Storage placement (#545 B0-150, B7-223): a sqlite Store's file stays on a
//! machine that keeps it. `sandbox.create` and `store.push` refuse
//! `StoragePlacement` — naming the Store, the machine, its backend and the
//! reason — instead of silently routing the Store to a Family 2 machine,
//! one whose backend has no worker and no disk of ours; a PostgreSQL Store
//! is on no machine and is never refused, and `store.pull` stays the way off
//! such a machine.
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use std::path::PathBuf;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-placement-{tag}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn sqlite_record(machine: &str) -> SdkValue {
    SdkValue::map([
        ("name", t("shop")),
        ("kind", t("sqlite")),
        ("machine", t(machine)),
    ])
}

fn postgresql_record() -> SdkValue {
    SdkValue::map([
        ("name", t("shop")),
        ("kind", t("postgresql")),
        ("url", t("postgresql://db.example/layerfs")),
        ("schema", t("layerfs")),
    ])
}

/// A home with Store `shop` (project `api`) and machine `dev`, an ssh host
/// whose record says whether it keeps a sqlite Store's file: `false` is the
/// Family 2 machine a provider with no worker and no disk of ours records,
/// `None` leaves the field out so the platform's support row answers.
fn home_with(store: SdkValue, dev_keeps_a_store: Option<bool>) -> PathBuf {
    let home = tempdir("home");
    home::open(&home).unwrap();
    write_record(&paths::store_record(&home, "shop"), &store).unwrap();
    write_record(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    let mut fields: Vec<(String, SdkValue)> = vec![
        ("name".to_owned(), t("dev")),
        ("platform".to_owned(), t("ssh")),
        ("created".to_owned(), SdkValue::Int(1)),
    ];
    if let Some(kept) = dev_keeps_a_store {
        fields.push(("local_sqlite".to_owned(), SdkValue::Bool(kept)));
    }
    write_record(&paths::machine_record(&home, "dev"), &SdkValue::Map(fields)).unwrap();
    home
}

fn call(
    home: &std::path::Path,
    name: &str,
    args: SdkValue,
) -> Result<SdkValue, (String, SdkValue, String)> {
    layerfs_api::call(Some(home), name, args, &mut |_| {})
        .map_err(|refusal| (refusal.code, refusal.detail, refusal.message))
}

fn create_on_dev(home: &std::path::Path) -> Result<SdkValue, (String, SdkValue, String)> {
    call(
        home,
        "sandbox.create",
        SdkValue::map([
            ("project", t("shop/api")),
            ("name", t("box")),
            ("machine", t("dev")),
        ]),
    )
}

/// The refusal's detail, as the contract names it: the Store, the machine,
/// the machine's backend and the reason.
fn assert_placement_detail(detail: &SdkValue, message: &str) {
    assert_eq!(detail.get_text("store"), Some("shop"), "{detail:?}");
    assert_eq!(detail.get_text("machine"), Some("dev"), "{detail:?}");
    let reason = detail.get_text("reason").expect("a reason");
    assert!(reason.contains("local_sqlite"), "{reason}");
    assert!(message.contains(reason), "{message}");
    // the backend is the platform table's; the guest worker's build holds
    // no table, so it names none
    if cfg!(feature = "machine") {
        assert_eq!(detail.get_text("backend"), Some("namespaces"), "{detail:?}");
        assert!(message.contains("backend namespaces"), "{message}");
    } else {
        assert_eq!(detail.get("backend"), Some(&SdkValue::Null), "{detail:?}");
    }
}

#[test]
fn a_sqlite_store_is_not_placed_on_a_machine_that_keeps_no_disk() {
    let home = home_with(sqlite_record("native"), Some(false));
    let (code, detail, message) = create_on_dev(&home).unwrap_err();
    assert_eq!(code, "StoragePlacement", "{message}");
    assert_placement_detail(&detail, &message);
    assert!(
        message.contains("cannot be placed on machine dev"),
        "{message}"
    );
    assert!(message.contains("Store shop"), "{message}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_postgresql_store_is_never_refused_by_placement() {
    let home = home_with(postgresql_record(), Some(false));
    // a PostgreSQL Store is on no machine (G3): placement does not apply,
    // so the create goes on to its own machine checks, and is refused by
    // one of those, never by this guard
    let refused = create_on_dev(&home).unwrap_err();
    assert_ne!(refused.0, "StoragePlacement", "{}", refused.2);
    assert!(!refused.2.contains("cannot be placed"), "{}", refused.2);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_push_to_a_machine_that_keeps_no_disk_is_refused() {
    let home = home_with(sqlite_record("native"), Some(false));
    let (code, detail, message) = call(
        &home,
        "store.push",
        SdkValue::map([("name", t("shop")), ("machine", t("dev"))]),
    )
    .unwrap_err();
    assert_eq!(code, "StoragePlacement", "{message}");
    assert_placement_detail(&detail, &message);
    assert!(message.contains("store push"), "{message}");
    std::fs::remove_dir_all(home).unwrap();

    // a machine that keeps the Store is not refused here: the same push
    // goes on to dial its worker, which is not recorded
    let keeps = home_with(sqlite_record("native"), Some(true));
    let live = call(
        &keeps,
        "store.push",
        SdkValue::map([("name", t("shop")), ("machine", t("dev"))]),
    )
    .unwrap_err();
    assert_eq!(live.0, "Unavailable", "{}", live.2);
    std::fs::remove_dir_all(keeps).unwrap();
}

#[test]
fn a_pull_from_a_machine_that_keeps_no_disk_stays_the_way_off() {
    let home = home_with(sqlite_record("dev"), Some(false));
    let refused = call(&home, "store.pull", SdkValue::map([("name", t("shop"))])).unwrap_err();
    assert_ne!(refused.0, "StoragePlacement", "{}", refused.2);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_machine_record_that_does_not_say_whether_it_keeps_a_store_is_refused() {
    let home = home_with(sqlite_record("native"), None);
    write_record(
        &paths::machine_record(&home, "dev"),
        &SdkValue::map([
            ("name", t("dev")),
            ("platform", t("ssh")),
            ("created", SdkValue::Int(1)),
            ("local_sqlite", t("yes")),
        ]),
    )
    .unwrap();
    let refused = create_on_dev(&home).unwrap_err();
    assert_eq!(refused.0, "InvalidInput", "{}", refused.2);
    assert!(refused.2.contains("local_sqlite"), "{}", refused.2);
    std::fs::remove_dir_all(home).unwrap();
}
