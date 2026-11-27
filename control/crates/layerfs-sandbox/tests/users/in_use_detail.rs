//! `InUse` from `store.disconnect` fills `detail.related` with
//! `{kind, name}` entries (#536 §10b refusal detail): another process's
//! lease on the Store.
use layerfs_home::disk::{paths, private_dir, write_entity};
use layerfs_home::reply::events::Events;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::PathBuf;

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn home(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("layerfs-in-use-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    home
}

/// `detail.related` as `(kind, name)` pairs.
fn related(refusal: &SdkRefusal) -> Vec<(String, String)> {
    let Some(SdkValue::List(items)) = refusal.detail.get("related") else {
        panic!("no detail.related: {refusal:?}");
    };
    let text = |v: &SdkValue, k: &str| v.get_text(k).unwrap().to_owned();
    items
        .iter()
        .map(|i| (text(i, "kind"), text(i, "name")))
        .collect()
}

fn refused(
    run: impl FnOnce(&mut Events<'_>) -> layerfs_home::reply::outcome::Outcome,
) -> SdkRefusal {
    let mut sink = |_| {};
    run(&mut Events::new(&mut sink)).unwrap_err()
}

#[test]
fn store_disconnect_names_another_process_s_lease() {
    let home = home("disconnect");
    let store = SdkValue::map([("name", t("g")), ("kind", t("postgresql"))]);
    write_entity(&paths::store_record(&home, "g"), &store).unwrap();
    // a live process other than this one holds the Store's lease
    let mut holder = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .unwrap();
    private_dir(&paths::lease_dir(&home, "g")).unwrap();
    std::fs::write(
        paths::lease_dir(&home, "g").join(holder.id().to_string()),
        "1\n",
    )
    .unwrap();
    let name = layerfs_home::names::StoreName::parse("g").unwrap();
    let refusal = refused(|_| {
        layerfs_store::admin::with_users::disconnect(&home, &name, &layerfs_sandbox::users::STORE)
    });
    let _ = holder.kill();
    let _ = holder.wait();
    assert_eq!(refusal.code, "InUse", "{refusal:?}");
    assert_eq!(
        related(&refusal),
        [("lease".to_owned(), holder.id().to_string())]
    );
    assert!(paths::store_record(&home, "g").is_file());
    let _ = std::fs::remove_dir_all(&home);
}
