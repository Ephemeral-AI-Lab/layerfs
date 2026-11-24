//! Place evidence on the Sandbox side (#783 gaps 1–3): a worker's own record
//! keeps the machine and generation its caller minted, so its mounts carry
//! them; a tombstone is written on the removal paths and goes once the name
//! is recreated with a newer generation. Missing evidence is never death.
use layerfs_bridge::control::SdkValue;
use layerfs_home::json::parse;
use layerfs_home::op::Op;
use layerfs_sandbox::record::{mount_place, retire, SandboxRecord, SandboxTombstone};
use std::path::PathBuf;

/// A fresh folder of its own under the temp directory: a worker's root
/// (not a home) unless `home` opens it as one.
fn scratch(tag: &str, home: bool) -> PathBuf {
    let root = std::env::temp_dir().join(format!("lfs-place-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    if home {
        layerfs_home::home::layout::open(&root).expect("a new home");
    }
    root
}

#[test]
fn a_worker_record_keeps_the_callers_place_and_its_mounts_read_it() {
    let root = scratch("worker", false);
    let directory = root.join("sandbox").join("t1");
    std::fs::create_dir_all(&directory).unwrap();
    // the worker's record as its start writes it: no machine, no generation
    SandboxRecord::from_record(
        "t1",
        parse(r#"{"name":"t1","project":"shop/api","backend":"namespaces"}"#).unwrap(),
    )
    .write_in(&directory)
    .unwrap();
    let before = mount_place(&root, &directory, "t1").unwrap();
    assert_eq!(before.generation, None, "no evidence before the place");
    assert_eq!(before.machine, None);

    let generation = "p1:aa11.sandbox:0000000000000010:aa".to_owned();
    layerfs_sandbox::files::save_place(&directory, "t1", &("box".to_owned(), generation.clone()));
    let placed = mount_place(&root, &directory, "t1").unwrap();
    assert_eq!(placed.sandbox.as_deref(), Some("t1"));
    assert_eq!(placed.machine.as_deref(), Some("box"));
    assert_eq!(placed.generation.as_deref(), Some(generation.as_str()));
    // a worker's root keeps no machine record: no incarnation is guessed
    assert_eq!(placed.machine_incarnation, None);

    // a record naming a generation already (native's home record) is left
    let other = (
        "other".to_owned(),
        "p1:aa11.sandbox:0000000000000020:bb".to_owned(),
    );
    layerfs_sandbox::files::save_place(&directory, "t1", &other);
    let kept = SandboxRecord::read_in(&directory, "t1").unwrap().unwrap();
    assert_eq!(kept.generation(), Some(generation.as_str()));
    assert_eq!(kept.machine(), "box");
    let _ = std::fs::remove_dir_all(&root);
}

/// The tombstone is a home record: a worker's root, which no place view
/// reads, gets none (its caller writes the home's), while a home does.
#[test]
fn a_tombstone_is_written_in_a_home_and_never_in_a_worker_root() {
    let record = SandboxRecord::from_record(
        "t1",
        parse(
            r#"{"name":"t1","machine":"box","generation":"p1:aa11.sandbox:0000000000000010:aa"}"#,
        )
        .unwrap(),
    );
    let root = scratch("root", false);
    layerfs_home::home::guest::serve_root(&root);
    retire(&root, &record).unwrap();
    assert!(!layerfs_home::home::paths::sandbox_tombstone(&root, "box", "t1").exists());
    let _ = std::fs::remove_dir_all(&root);

    let home = scratch("home", true);
    retire(&home, &record).unwrap();
    let tombstone = SandboxTombstone::read(&home, "box", "t1").unwrap().unwrap();
    assert_eq!(tombstone.generation, "p1:aa11.sandbox:0000000000000010:aa");
    let _ = std::fs::remove_dir_all(&home);
}

/// A Sandbox recreated under a retired name mints a newer generation; once
/// it is recorded the old tombstone goes (the newer record proves the old
/// generation's mounts dead by itself), so tombstones do not pile up.
#[test]
fn a_recreated_name_drops_its_older_tombstone() {
    let home = scratch("recreate", true);
    let id = layerfs_home::home::opened(&home).unwrap().id;
    let old = format!("p1:{id}.sandbox:0000000000000010:aa");
    SandboxTombstone {
        name: "t1".to_owned(),
        machine: "box".to_owned(),
        generation: old.clone(),
        retired_ms: 1,
    }
    .write(&home)
    .unwrap();
    let op = Op::begin("sandbox.create", "claim", SdkValue::Null).unwrap();
    let record = SandboxRecord::creating(&home, "t1", "shop/api", "box", &op).unwrap();
    // a tombstone newer than the record (a restore) is kept
    let newer = format!("p1:{id}.sandbox:{:016}:bb", u64::MAX / 4);
    SandboxTombstone {
        name: "t1".to_owned(),
        machine: "box".to_owned(),
        generation: newer.clone(),
        retired_ms: 2,
    }
    .write(&home)
    .unwrap();
    layerfs_sandbox::record::drop_retired(&home, &record);
    let kept = SandboxTombstone::read(&home, "box", "t1").unwrap().unwrap();
    assert_eq!(kept.generation, newer, "a newer tombstone stays");

    // the older one goes
    SandboxTombstone {
        name: "t1".to_owned(),
        machine: "box".to_owned(),
        generation: old,
        retired_ms: 1,
    }
    .write(&home)
    .unwrap();
    layerfs_sandbox::record::drop_retired(&home, &record);
    assert!(SandboxTombstone::read(&home, "box", "t1")
        .unwrap()
        .is_none());
    let _ = std::fs::remove_dir_all(&home);
}
