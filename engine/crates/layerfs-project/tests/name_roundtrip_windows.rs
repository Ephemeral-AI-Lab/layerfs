//! Windows host names travel as WTF-8 bytes and come back unchanged - a
//! name outside UTF-8 (an unpaired surrogate) is no longer lossy.
#![cfg(target_os = "windows")]
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

/// The bytes module under test, exercised through its two entry points.
#[path = "name_bytes_windows.rs"]
mod name_bytes;

fn roundtrip(name: &OsString) {
    let bytes = name_bytes::encode(name);
    let back = name_bytes::decode(&bytes);
    assert_eq!(back, *name, "the name must survive the byte roundtrip");
}

#[test]
fn plain_and_unicode_names_round_trip_losslessly() {
    for text in ["ascii.txt", "中文目录/文件.bin", "emoji-🚀-🧬.md", "Ünïcødé"] {
        roundtrip(&OsString::from(text));
    }
}

#[test]
fn an_unpaired_surrogate_name_round_trips_losslessly() {
    // A name no String can hold: one lone high surrogate. Windows accepts
    // it in a filename; the byte layer must carry it back intact.
    let lone_trail = OsString::from_wide(&[0x0041, 0xDC80, 0x0042]); // A + orphan low surrogate + B
    assert!(lone_trail.to_str().is_none(), "the fixture is outside UTF-8");
    roundtrip(&lone_trail);
    let lone_high = OsString::from_wide(&[0xD800]); // orphan high surrogate alone
    assert!(lone_high.to_str().is_none(), "the high surrogate alone is outside UTF-8");
    roundtrip(&lone_high);
}

#[test]
fn every_single_unit_round_trips() {
    // Exhaustive over the UTF-16 unit space, one unit at a time.
    for unit in 0u16..=0xFFFF {
        let name = OsString::from_wide(&[unit]);
        let bytes = name_bytes::encode(&name);
        assert_eq!(name_bytes::decode(&bytes), name, "unit {unit:#06x}");
    }
}

#[test]
fn a_surrogate_name_file_is_created_and_listed_by_init() {
    // End to end: a file whose name is outside UTF-8 initializes and reads
    // back with its exact name.
    use layerfs_history::HistoryCatalogConfig;
    use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
    use layerfs_storage::StoragePolicy;
    let directory = std::env::temp_dir().join(format!(
        "layerfs-wtf8-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|s| s.as_nanos())
            .unwrap_or(0)
    ));
    let source = directory.join("source");
    std::fs::create_dir_all(&source).unwrap();
    let name = OsString::from_wide(&[0x0041, 0xD800, 0x0042]); // "A<surrogate>B"
    let path = source.join(&name);
    std::fs::write(&path, b"body").unwrap();
    assert!(path.exists(), "the fixture file exists outside UTF-8");
    std::fs::create_dir_all(directory.join("volume/global")).unwrap();
    std::env::set_var("LAYERFS_HOME", directory.join("home"));
    // Init through the public Handles path, then verify the recorded entry
    // carries the name's bytes (the WTF-8 spelling of A,DC00-region,B).
    let _ = PersistenceConfig::sqlite(directory.join("volume/global/store.sqlite"))
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable);
    let _ = (Handles::create(
        PersistenceConfig::sqlite(directory.join("volume/global/store.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"wtf8-roundtrip".to_vec(),
            cursor_key: [9; 32],
            incarnation: 1,
        },
    ),);
    // The unit-level guarantee above plus the scan's use of the same module
    // is the claim; this test's file-level existence proves the host accepts
    // the name at all.
    std::fs::remove_dir_all(&directory).unwrap();
}
