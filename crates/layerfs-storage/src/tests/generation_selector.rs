//! Selector framing, typed opening, and legacy installation regression tests.

use crate::generation::create::opened_file_identity;
use crate::generation::selector::*;
use crate::generation::{compact, directory, open_or_create, InstallBehavior, TestDriver};
use crate::generation::{NativeGenerationDriver, StoreGenerationDriver};
use crate::integrity::IntegrityMode;
use crate::{Engine, EngineError};
use rusqlite::Connection;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

fn current_generation(directory: &Path) -> u64 {
    read_selector(&directory.join("CURRENT"))
        .unwrap()
        .generation
}

fn legacy_store(label: &str, mode: IntegrityMode) -> (PathBuf, TestDriver, Engine) {
    let directory = directory(label);
    let driver = TestDriver::native();
    let engine = open_or_create(&directory, &driver, mode).unwrap();
    (directory, driver, engine)
}

#[test]
fn selector_is_exact_154_bytes_and_strictly_checksummed() {
    let selector = StoreSelector {
        generation: 7,
        schema_version: 1,
        store_id: [3; 32],
        profile_id: [4; 32],
    };
    let bytes = selector.encode();
    assert_eq!(bytes.len(), 154);
    assert_eq!(&bytes[20..54], b"generation-0000000000000007.sqlite");
    assert_eq!(StoreSelector::decode(&bytes).unwrap(), selector);
    let mut corrupt = bytes;
    corrupt[54] ^= 1;
    assert!(StoreSelector::decode(&corrupt).is_err());
}

#[test]
fn genesis_current_compaction_and_reopen_preserve_store_identity() {
    let (directory, driver, engine) = legacy_store("identity", IntegrityMode::TrustedLocalDev);
    let store_id = engine.store_id().unwrap();
    assert_eq!(current_generation(&directory), 0);
    let engine = compact(engine, &directory, &driver).unwrap();
    assert_eq!(engine.mode, IntegrityMode::TrustedLocalDev);
    let observation = engine.last_compaction_observation().unwrap();
    assert!(observation.old_generation_bytes > 0);
    assert!(observation.new_generation_bytes > 0);
    assert!(observation.mark_database_bytes > 0);
    assert_eq!(observation.selector_temporary_bytes, SELECTOR_BYTES as u64);
    assert_eq!(
        observation.total_peak_bytes,
        observation.old_generation_bytes
            + observation.new_generation_bytes
            + observation.mark_database_bytes
            + observation.candidate_journal_temp_peak_bytes
            + observation.verification_scratch_peak_bytes
            + observation.selector_temporary_bytes
    );
    assert_eq!(engine.store_id().unwrap(), store_id);
    assert_eq!(current_generation(&directory), 1);
    assert!(!directory.join(generation_filename(0)).exists());
    drop(engine);
    assert_eq!(
        open_current(&directory, IntegrityMode::Verified)
            .unwrap()
            .store_id()
            .unwrap(),
        store_id
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn genesis_handoff_reopens_the_generation_selected_after_maintenance() {
    let directory = directory("handoff");
    let engine = open_or_create(
        &directory,
        &TestDriver::new(InstallBehavior::Advance),
        IntegrityMode::TrustedLocalDev,
    )
    .unwrap();
    assert_eq!(current_generation(&directory), 1);
    assert_eq!(engine.path(), directory.join(generation_filename(1)));
    drop(engine);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn selector_directory_sync_failure_is_ambiguous_and_preserves_prior_generation() {
    let (directory, native, engine) = legacy_store("sync-reconcile", IntegrityMode::Verified);
    assert!(matches!(
        compact(engine, &directory, &TestDriver::fail_sync(0)),
        Err(EngineError::AmbiguousDurability)
    ));
    assert_eq!(current_generation(&directory), 1);
    assert!(directory.join(generation_filename(0)).exists());
    assert!(directory.join(generation_filename(1)).exists());
    let recovered = open_or_create(&directory, &native, IntegrityMode::Verified).unwrap();
    assert_eq!(
        recovered.store_id().unwrap(),
        read_selector(&directory.join("CURRENT")).unwrap().store_id
    );
    assert!(!directory.join(generation_filename(0)).exists());
    drop(recovered);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn trusted_recovery_reopens_in_the_requested_store_lifetime_mode() {
    let (directory, driver, engine) =
        legacy_store("trusted-recovery", IntegrityMode::TrustedLocalDev);
    let namespace = layerfs_core::namespace_codec::encode_namespace_root(
        layerfs_core::namespace::NamespaceRootV1 {
            profile_id: layerfs_core::namespace_codec::profile_id(),
            root_directory_inode: layerfs_core::inode::InodeId::allocate([0x61; 32], 0),
            inode_table_root: layerfs_core::ObjectId::for_bytes(b"missing inode table"),
        },
    )
    .unwrap();
    let state = engine
        .begin_publication(None, "main")
        .unwrap()
        .publish_namespace(&namespace)
        .unwrap();
    drop(engine);
    let residue = directory.join(".layerfs-foreign-trigger.sqlite");
    fs::write(&residue, b"not owned scratch").unwrap();
    let recovered = open_or_create(&directory, &driver, IntegrityMode::TrustedLocalDev).unwrap();
    assert_eq!(recovered.read_ref("main").unwrap(), Some(state));
    drop(recovered);
    assert!(open_or_create(&directory, &driver, IntegrityMode::Verified).is_err());
    fs::remove_file(residue).unwrap();
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn selector_reader_rejects_oversize_without_read_to_end() {
    let path = directory("selector-oversize");
    fs::write(&path, vec![0_u8; SELECTOR_BYTES + 1]).unwrap();
    assert!(matches!(
        read_selector(&path),
        Err(EngineError::Sqlite {
            kind: crate::SqliteErrorKind::Io,
            ..
        })
    ));
    let handle = fs::File::open(&path).unwrap();
    assert_eq!(
        opened_file_identity(&handle).unwrap(),
        NativeGenerationDriver.file_identity(&path).unwrap()
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn missing_selector_fails_closed_and_exact_candidate_residue_recovers() {
    let genesis = directory("genesis-residue");
    fs::create_dir(&genesis).unwrap();
    let generation = genesis.join(generation_filename(0));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&generation)
        .unwrap();
    drop(Engine::open(&generation).unwrap());
    let before = fs::read(&generation).unwrap();
    assert!(matches!(
        open_or_create(&genesis, &TestDriver::native(), IntegrityMode::Verified),
        Err(EngineError::InvalidRecord(
            "missing CURRENT in nonempty Store"
        ))
    ));
    assert!(!genesis.join("CURRENT").exists());
    assert_eq!(fs::read(&generation).unwrap(), before);
    fs::remove_dir_all(genesis).unwrap();

    for partial_selector in [false, true] {
        let (directory, driver, engine) = legacy_store(
            &format!("candidate-residue-{partial_selector}"),
            IntegrityMode::Verified,
        );
        let candidate = directory.join(generation_filename(1));
        engine.compact_to(&candidate).unwrap();
        if partial_selector {
            fs::write(directory.join("CURRENT.tmp"), b"partial").unwrap();
        }
        let unknown = directory.join(generation_filename(9));
        engine.compact_to(&unknown).unwrap();
        assert!(compact(engine, &directory, &driver).is_err());
        assert_eq!(current_generation(&directory), 0);
        assert!(candidate.exists(), "unproven candidate was removed");
        assert!(unknown.exists(), "cleanup removed unknown generation");
        if partial_selector {
            assert_eq!(fs::read(directory.join("CURRENT.tmp")).unwrap(), b"partial");
        }
        fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn candidate_inspection_never_mutates_empty_or_foreign_sqlite() {
    for kind in ["empty", "foreign", "sqliteX"] {
        let (directory, driver, engine) = legacy_store(
            &format!("foreign-candidate-{kind}"),
            IntegrityMode::Verified,
        );
        let candidate = directory.join(generation_filename(1));
        if kind == "empty" {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
                .unwrap();
        } else {
            let table = if kind == "sqliteX" {
                "sqliteX_data"
            } else {
                "caller_data"
            };
            Connection::open(&candidate)
                .unwrap()
                .execute_batch(&format!(
                    "CREATE TABLE {table} (value TEXT); INSERT INTO {table} VALUES ('keep');"
                ))
                .unwrap();
        }
        let before = fs::read(&candidate).unwrap();
        assert!(compact(engine, &directory, &driver).is_err());
        assert_eq!(fs::read(&candidate).unwrap(), before);
        fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn cleanup_waits_for_selected_verified_authority() {
    let (directory, driver, mut engine) = legacy_store("corrupt-selected", IntegrityMode::Verified);
    let candidate = directory.join(generation_filename(1));
    engine.compact_to(&candidate).unwrap();
    let candidate_engine = Engine::open(&candidate).unwrap();
    fs::write(
        directory.join("CURRENT.tmp"),
        selector(&candidate_engine, 1).unwrap().encode(),
    )
    .unwrap();
    let selected = engine.path().to_owned();
    engine.maintenance_pin.take();
    drop(engine);
    fs::write(&selected, b"corrupt selected generation").unwrap();
    assert!(open_or_create(&directory, &driver, IntegrityMode::Verified).is_err());
    assert!(candidate.exists());
    assert!(directory.join("CURRENT.tmp").exists());
    fs::remove_dir_all(directory).unwrap();
}
