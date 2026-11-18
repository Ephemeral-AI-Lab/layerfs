//! Reads change nothing (#536 §12 item 3): `store.list` and `store.get`
//! leave the home as it was, and a live read (`store get --with live`)
//! writes only its observation file: each leaves a stale lease and
//! `log/leases.jsonl` as they were and takes no lock, while its answer
//! still ignores the dead holder.
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_home::home::{paths, private_dir, write_entity};
use layerfs_store::get::{get, With};
use layerfs_store::list::list;
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
    std::fs::write(paths::store_file(&home, "s"), b"").unwrap();
    let mut sink = |_| {};
    let mut events = Events::new(&mut sink);

    let before = tree(&home);
    let list = list(&home, None, false, &layerfs_control::store::machine_state).unwrap();
    let get_records = get(
        &home,
        &mut events,
        "s",
        With::default(),
        &layerfs_control::store::machine_state,
    )
    .unwrap();
    assert_eq!(before, tree(&home), "a records-only read changed the home");
    let live = With {
        live: true,
        stats: false,
    };
    let seen = get(
        &home,
        &mut events,
        "s",
        live,
        &layerfs_control::store::machine_state,
    )
    .unwrap();
    let mut after = tree(&home);
    let observed = paths::store_dir(&home, "s").join("observed.json");
    assert!(after.remove(&observed).is_some(), "a live read observes");
    // the observation file's folder changed with it, and nothing else
    let folder = paths::store_dir(&home, "s");
    let mut before = before;
    before.remove(&folder);
    after.remove(&folder);
    assert_eq!(
        before, after,
        "a live read changed more than its observation"
    );
    assert!(stale.exists());
    assert!(!home.join("log/leases.jsonl").exists());
    assert!(!paths::store_dir(&home, "s").join("lock").exists());

    // the dead holder holds nothing
    let holders = seen.get("live").unwrap().get("holders");
    assert_eq!(holders, Some(&SdkValue::List(vec![])));
    assert_eq!(seen.get_text("state"), Some("ready"), "{seen:?}");
    let Some(SdkValue::List(items)) = list.get("items") else {
        panic!("no items: {list:?}");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].get_text("state"), Some("ready"));
    assert_eq!(get_records.get_text("store"), Some("s"));
    let _ = std::fs::remove_dir_all(&home);
}
