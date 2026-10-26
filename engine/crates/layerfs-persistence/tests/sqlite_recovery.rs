//! SIGKILL recovery of acknowledged units; no physical power-loss claim.
mod support;
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig, ReserveRequest};
use layerfs_persistence::{Handles, PersistenceConfig};
use layerfs_storage::{Storage, StoragePolicy};
use std::io::{BufRead, Write};
fn config() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"process-crash".to_vec(),
        cursor_key: [71; 32],
        incarnation: 1,
    }
}
fn object() -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(b"acknowledged-before-process-kill")
            .unwrap(),
    )
    .unwrap()
}
#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("LAYERFS_RECOVERY_CHILD_PATH") else {
        return;
    };
    let h = Handles::create(
        PersistenceConfig::sqlite(path),
        StoragePolicy::frozen_default(),
        &config(),
    )
    .unwrap();
    let storage = Storage::new(h.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object()).unwrap();
    save.finish().unwrap();
    h.history
        .reserve_inodes(&ReserveRequest {
            scope: ObjectId::for_bytes(b"crash-scope"),
            count: 7,
        })
        .unwrap();
    println!("ACKNOWLEDGED_RECOVERY_UNIT");
    std::io::stdout().flush().unwrap();
    // Parent kills this process while its live Store/connection are still owned.
    let mut byte = [0];
    std::io::Read::read_exact(&mut std::io::stdin(), &mut byte).unwrap();
    drop(storage);
    drop(h);
}
#[test]
fn sigkill_preserves_acknowledged_body_locator_and_reservation() {
    let t = support::Temp::new("sigkill");
    let path = t.join("db");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_child", "--nocapture"])
        .env("LAYERFS_RECOVERY_CHILD_PATH", &path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            output.read_line(&mut line).unwrap() > 0,
            "child ended before acknowledgement"
        );
        if line.contains("ACKNOWLEDGED_RECOVERY_UNIT") {
            break;
        }
    }
    child.kill().unwrap();
    assert!(!child.wait().unwrap().success());
    let h = Handles::open_writable(
        PersistenceConfig::sqlite(&path),
        &config().binding_key,
        [71; 32],
    )
    .unwrap();
    let storage = Storage::new(h.storage.clone()).unwrap();
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[object().id()])
            .unwrap(),
        vec![object().canonical().to_vec()]
    );
    assert_eq!(
        h.history
            .reserve_inodes(&ReserveRequest {
                scope: ObjectId::for_bytes(b"crash-scope"),
                count: 1
            })
            .unwrap()
            .start,
        8
    );
    drop(storage);
    h.seal().unwrap();
}
