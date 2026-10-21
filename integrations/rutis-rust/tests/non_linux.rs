//! Tier 0 is Linux-only; other hosts fail before any Store access.

#![cfg(not(target_os = "linux"))]

use layerfs_rutis::{Config, LayerFs};
use std::path::PathBuf;

#[test]
fn non_linux_is_unsupported() {
    let config = Config {
        project: "website".into(),
        home: PathBuf::from("/tmp/layerfs-home"),
        store_path: PathBuf::from("/tmp/layerfs-store.sqlite"),
    };
    match LayerFs::new(config) {
        Ok(_) => panic!("a non-Linux host must not open a Store"),
        Err(error) => assert_eq!(error.code(), "Unsupported"),
    }
}
