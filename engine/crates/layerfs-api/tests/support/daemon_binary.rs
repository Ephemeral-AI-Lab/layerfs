//! Locate the ordinary daemon binary built alongside these SDK integration tests.
use std::path::PathBuf;

pub fn path() -> PathBuf {
    if let Some(path) = std::env::var_os("LAYERFS_TEST_DAEMON") {
        return path.into();
    }
    let executable = std::env::current_exe().expect("integration test executable");
    let binary = executable
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("layerfs-daemon");
    assert!(
        binary.is_file(),
        "build layerfs-daemon before SDK tests: {}",
        binary.display()
    );
    binary
}
