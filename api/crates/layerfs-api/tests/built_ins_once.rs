//! The built-in machines' records are checked once per process for each
//! home, not on every call (#535 R6): a call after the first reads no
//! built-in record (here shown by one deleted meanwhile staying absent),
//! and a home made again at the same path, with a new id, gets them again.
#![cfg(feature = "machine")]
use layerfs_api::calls;
use layerfs_api::home::paths;
use layerfs_bridge::control::SdkValue;

#[test]
fn built_ins_are_checked_once_per_home() {
    let home = std::env::temp_dir().join(format!("lfs-builtins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let list = |home: &std::path::Path| {
        calls::call(Some(home), "machine.list", SdkValue::Null, &mut |_| {}).unwrap()
    };
    list(&home);
    let docker = paths::machine_record(&home, "docker");
    assert!(docker.is_file());
    std::fs::remove_file(&docker).unwrap();
    list(&home);
    assert!(!docker.exists(), "the second call did not check again");

    // the same path, a new home: checked again
    std::fs::remove_dir_all(&home).unwrap();
    list(&home);
    assert!(docker.is_file());
    std::fs::remove_dir_all(&home).unwrap();
}
