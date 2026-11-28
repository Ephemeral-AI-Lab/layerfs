//! The built-in machines' records are checked once per process for each
//! home, not on every call (#535 R6): a call after the first reads no
//! built-in record (here shown by one deleted meanwhile staying absent),
//! and a home made again at the same path, with a new id, gets them again.
#![cfg(feature = "machine")]
use layerfs_api::home::paths;
use layerfs_protocol::sdk::SdkValue;

#[test]
fn built_ins_are_checked_once_per_home() {
    let root = std::env::temp_dir().join(format!("lfs-builtins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    let list =
        || layerfs_api::call(Some(&root), "machine.list", SdkValue::Null, &mut |_| {}).unwrap();
    list();
    let docker = paths::machine_record(&home, "docker");
    assert!(docker.is_file());
    std::fs::remove_file(&docker).unwrap();
    list();
    assert!(!docker.exists(), "the second call did not check again");

    // the same path, a new home: checked again
    std::fs::remove_dir_all(&home).unwrap();
    list();
    assert!(docker.is_file());
    std::fs::remove_dir_all(&root).unwrap();
}
