//! Reads change nothing (#536 §12 item 3): `store.status`, `store.list` and
//! `store.get` leave a stale lease and `log/leases.jsonl` as they were and
//! take no lock, while their answers still ignore the dead holder.
use layerfs_bridge::control::SdkValue;
use layerfs_control::common::context::Context;
use layerfs_control::store::family::run;
use layerfs_home::home::{paths, private_dir, write_entity};
use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Every path under `dir` with its size, mtime and mode.
fn tree(dir: &Path) -> BTreeMap<PathBuf, (u64, i64, i64, u32)> {
    let mut found = BTreeMap::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(path) = stack.pop() {
        let meta = std::fs::symlink_metadata(&path).unwrap();
        found.insert(
            path.clone(),
            (meta.len(), meta.mtime(), meta.mtime_nsec(), meta.mode()),
        );
        if meta.is_dir() {
            for entry in std::fs::read_dir(&path).unwrap() {
                stack.push(entry.unwrap().path());
            }
        }
    }
    found
}

fn call(home: &Path, name: &str, args: SdkValue) -> SdkValue {
    let mut events = |_| {};
    let mut context = Context::new(home.to_owned(), &mut events);
    run(&mut context, name, &args).unwrap()
}

#[test]
fn a_stale_lease_survives_every_store_read_and_is_ignored() {
    let home = std::env::temp_dir().join(format!("layerfs-store-reads-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let record = SdkValue::map([
        ("name", SdkValue::text("s")),
        ("kind", SdkValue::text("sqlite")),
        ("machine", SdkValue::text("native")),
    ]);
    write_entity(&paths::store_record(&home, "s"), &record).unwrap();
    // a process id above every kernel's pid_max: its holder is gone
    private_dir(&paths::lease_dir(&home, "s")).unwrap();
    let stale = paths::lease_dir(&home, "s").join("99999999");
    std::fs::write(&stale, "1\n").unwrap();
    let named = SdkValue::map([("name", SdkValue::text("s"))]);

    let before = tree(&home);
    let status = call(&home, "status", named.clone());
    let list = call(&home, "list", SdkValue::map([]));
    let get = call(&home, "get", named);
    assert_eq!(before, tree(&home), "a read changed the home");
    assert!(stale.exists());
    assert!(!home.join("log/leases.jsonl").exists());
    assert!(!paths::store_dir(&home, "s").join("lock").exists());

    // the dead holder holds nothing
    assert_eq!(status.get("holders"), Some(&SdkValue::List(vec![])));
    assert_ne!(status.get_text("state"), Some("in_use"), "{status:?}");
    let Some(SdkValue::List(items)) = list.get("items") else {
        panic!("no items: {list:?}");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].get_text("state"), status.get_text("state"));
    assert_eq!(get.get_text("name"), Some("s"));
    let _ = std::fs::remove_dir_all(&home);
}
