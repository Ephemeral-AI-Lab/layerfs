//! Windows host names travel as WTF-8 bytes and come back unchanged - a
//! name outside UTF-8 (an unpaired surrogate) is no longer lossy.
#![cfg(target_os = "windows")]
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

mod support;

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
    for text in [
        "ascii.txt",
        "中文目录/文件.bin",
        "emoji-🚀-🧬.md",
        "Ünïcødé",
    ] {
        roundtrip(&OsString::from(text));
    }
}

#[test]
fn a_well_formed_pair_encodes_as_its_utf_8_scalar() {
    // The bytes Linux stores for 🚀 are its UTF-8 spelling; Windows must
    // agree byte for byte or one name acquires as two different trees
    // (second review, finding 1).
    let rocket = OsString::from("🚀");
    let bytes = name_bytes::encode(&rocket);
    assert_eq!(bytes, "🚀".as_bytes());
    let dna = OsString::from("🧬");
    assert_eq!(name_bytes::encode(&dna), "🧬".as_bytes());
    // A pair next to a lone unit keeps both spellings in one name.
    let mixed = OsString::from_wide(&[0xD83D, 0xDE80, 0xDC80]);
    let mixed_bytes = name_bytes::encode(&mixed);
    assert_eq!(
        mixed_bytes,
        [b"\xf0\x9f\x9a\x80".as_slice(), b"\xed\xb2\x80".as_slice()].concat(),
        "the pair combines, the unpaired low surrogate keeps its form"
    );
    assert_eq!(name_bytes::decode(&mixed_bytes), mixed);
}

#[test]
fn an_unpaired_surrogate_name_round_trips_losslessly() {
    // A name no String can hold: one lone high surrogate. Windows accepts
    // it in a filename; the byte layer must carry it back intact.
    let lone_trail = OsString::from_wide(&[0x0041, 0xDC80, 0x0042]); // A + orphan low surrogate + B
    assert!(
        lone_trail.to_str().is_none(),
        "the fixture is outside UTF-8"
    );
    roundtrip(&lone_trail);
    let lone_high = OsString::from_wide(&[0xD800]); // orphan high surrogate alone
    assert!(
        lone_high.to_str().is_none(),
        "the high surrogate alone is outside UTF-8"
    );
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
fn init_acquires_a_surrogate_named_file_end_to_end() {
    // End to end through the public init: the scan stamps every entry's
    // identity through CreateFileW, which must receive the name's raw units
    // (K3-R3 F2) - a lossy spelling opened a nonexistent path and refused
    // the whole Init on a legal NTFS name.
    use layerfs_history::{HistoryCatalogConfig, HistoryName, LayerStackId};
    use layerfs_persistence::{
        Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
    };
    use layerfs_project::{init, InitRequest};
    use layerfs_storage::StoragePolicy;
    use support::{memory_acquisition::MemoryAcquisition, Fixture};
    let fixture = Fixture::new(5);
    let name = OsString::from_wide(&[0x0041, 0xD800, 0x0042]); // "A<surrogate>B"
    let path = fixture.source.join(&name);
    std::fs::write(&path, b"body").unwrap();
    assert!(path.exists(), "the fixture file exists outside UTF-8");
    let volume = fixture.path.join("volume");
    std::fs::create_dir_all(&volume).unwrap();
    let handles = Handles::create(
        PersistenceConfig::sqlite(fixture.path.join("store.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"wtf8-init".to_vec(),
            cursor_key: [83; 32],
            incarnation: 1,
        },
    )
    .unwrap();
    let storage = support::storage(handles.storage.clone());
    let initialized = layerfs_telemetry::timer::Timing::disabled("project.init", |scope| {
        init(
            &storage,
            &handles.history,
            InitRequest {
                source: &fixture.source,
                acquisition: &MemoryAcquisition::placed_in(volume),
                stack: LayerStackId::from_authority([37; 16]),
                name: HistoryName::new("main").unwrap(),
                scope_seed: [41; 32],
                deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
            },
            scope,
        )
    })
    .0
    .unwrap();
    assert!(
        initialized.entries >= 6,
        "the surrogate-named file initializes with the rest of the tree: {}",
        initialized.entries
    );
}
