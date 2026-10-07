//! Extra physical extents are released without touching logical SQLite bytes.
#![cfg(target_os = "macos")]
mod support;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig};
use layerfs_storage::{port::PackPersistence, Storage, StoragePolicy};
use nix::fcntl::{fcntl, FcntlArg};
use std::{fs::OpenOptions, os::unix::fs::MetadataExt};
fn create(path: &std::path::Path) -> Handles {
    Handles::create(
        PersistenceConfig::sqlite(path),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"allocation-release".to_vec(),
            cursor_key: [71; 32],
            incarnation: 1,
        },
    )
    .unwrap()
}
fn preallocate(path: &std::path::Path) {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut request = nix::libc::fstore_t {
        fst_flags: nix::libc::F_ALLOCATEALL,
        fst_posmode: nix::libc::F_PEOFPOSMODE,
        fst_offset: 0,
        fst_length: 16 * 1024 * 1024,
        fst_bytesalloc: 0,
    };
    fcntl(&file, FcntlArg::F_PREALLOCATE(&mut request)).unwrap();
}
#[test]
fn checkpoint_releases_extra_extents_and_preserves_acknowledged_canonical_bytes() {
    let t = support::Temp::new("allocation-release");
    let path = t.join("db");
    let h = create(&path);
    let storage = Storage::new(h.storage.clone()).unwrap();
    let object = FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(
            b"preserved-after-unused-extents-release",
        )
        .unwrap(),
    )
    .unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    h.checkpoint().unwrap();
    preallocate(&path);
    let before = path.metadata().unwrap();
    assert!(before.blocks() * 512 > before.len() + 8 * 1024 * 1024);
    h.checkpoint().unwrap();
    let after = path.metadata().unwrap();
    assert_eq!(after.len(), before.len());
    assert!(after.blocks() * 512 <= after.len() + 4096);
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[object.id()])
            .unwrap(),
        vec![object.canonical().to_vec()]
    );
    assert!(std::fs::read_dir(t.path()).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".layerfs-allocation-")));
    drop(storage);
    drop(h);
    let reopened = Handles::open_read_only(
        PersistenceConfig::sqlite(&path),
        b"allocation-release",
        [71; 32],
    )
    .unwrap();
    let storage = Storage::new(reopened.storage.clone()).unwrap();
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[object.id()])
            .unwrap(),
        vec![object.canonical().to_vec()]
    );
}

#[test]
fn readonly_checkpoint_refuses_before_sql_or_allocation_work() {
    let t = support::Temp::new("allocation-readonly");
    let path = t.join("db");
    let h = create(&path);
    h.checkpoint().unwrap();
    let reader = Handles::open_read_only(
        PersistenceConfig::sqlite(path),
        b"allocation-release",
        [71; 32],
    )
    .unwrap();
    let before = reader.diagnostics().unwrap();
    assert!(reader.checkpoint().is_err());
    assert_eq!(reader.diagnostics().unwrap().statements, before.statements);
}
#[test]
fn obstructed_checkpoint_does_not_release_extra_extents() {
    let t = support::Temp::new("allocation-busy");
    let path = t.join("db");
    let h = create(&path);
    h.checkpoint().unwrap();
    let reader = rusqlite::Connection::open(&path).unwrap();
    reader
        .execute_batch("BEGIN; SELECT * FROM store_policy;")
        .unwrap();
    h.storage
        .reserve(layerfs_storage::port::Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    preallocate(&path);
    let before = path.metadata().unwrap().blocks();
    let report = h.checkpoint().unwrap();
    assert!(report.busy);
    assert_eq!(report.allocation_before_bytes, None);
    assert_eq!(report.allocation_after_bytes, None);
    assert_eq!(path.metadata().unwrap().blocks(), before);
    reader.execute_batch("ROLLBACK").unwrap();
}
#[test]
fn scratch_creation_refusal_is_explicit_and_leaves_store_data_intact() {
    use std::os::unix::fs::PermissionsExt;
    if std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .unwrap()
        .stdout
        == b"0\n"
    {
        return;
    }
    let t = support::Temp::new("allocation-refusal");
    let path = t.join("db");
    let h = create(&path);
    h.checkpoint().unwrap();
    preallocate(&path);
    let old = t.path().metadata().unwrap().permissions();
    std::fs::set_permissions(t.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = h.checkpoint();
    std::fs::set_permissions(t.path(), old).unwrap();
    assert!(
        matches!(result,Err(layerfs_storage::port::PersistenceError::Refused {status}) if status.contains("Filesystem"))
    );
    assert_eq!(h.profile().journal_mode, "wal");
    assert!(h
        .storage
        .reserve(layerfs_storage::port::Reserve {
            packs: 1,
            ordinals: 0
        })
        .is_ok());
}

#[test]
fn disposable_finalization_releases_extents_without_wal_and_preserves_bytes() {
    let t = support::Temp::new("disposable-release");
    let path = t.join("db");
    let cfg = PersistenceConfig::sqlite(&path)
        .with_sqlite_profile(layerfs_persistence::SqlitePersistenceProfile::Disposable);
    let h = Handles::create(
        cfg.clone(),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"allocation-release".to_vec(),
            cursor_key: [71; 32],
            incarnation: 1,
        },
    )
    .unwrap();
    let storage = Storage::new(h.storage.clone()).unwrap();
    let object = FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(b"disposable-preserved").unwrap(),
    )
    .unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    preallocate(&path);
    let before = path.metadata().unwrap();
    assert!(before.blocks() * 512 > before.len() + 8 * 1024 * 1024);
    let completion = h.checkpoint().unwrap();
    assert!(!completion.wal_checkpoint_performed);
    let after = path.metadata().unwrap();
    assert_eq!(after.len(), before.len());
    assert!(after.blocks() * 512 <= after.len() + 4096);
    drop(storage);
    drop(h);
    let reopened = Handles::open_read_only(cfg, b"allocation-release", [71; 32]).unwrap();
    let storage = Storage::new(reopened.storage.clone()).unwrap();
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[object.id()])
            .unwrap(),
        vec![object.canonical().to_vec()]
    );
    assert!(std::fs::read_dir(t.path()).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".layerfs-allocation-")));
}

#[test]
fn disposable_completion_refuses_replaced_path_without_touching_replacement() {
    use layerfs_persistence::SqlitePersistenceProfile;
    let t = support::Temp::new("allocation-original-identity");
    let path = t.join("db");
    let cfg =
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable);
    let h = Handles::create(
        cfg,
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"identity".to_vec(),
            cursor_key: [71; 32],
            incarnation: 1,
        },
    )
    .unwrap();
    let original = t.join("original");
    std::fs::rename(&path, &original).unwrap();
    std::fs::write(&path, b"replacement must remain untouched").unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(h.checkpoint().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    drop(h);
    let reopened = Handles::open_read_only(
        PersistenceConfig::sqlite(&original)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        b"identity",
        [71; 32],
    )
    .unwrap();
    assert_eq!(reopened.profile().journal_mode, "memory");
}

#[test]
fn disposable_completion_refuses_new_hardlink_ownership() {
    use layerfs_persistence::SqlitePersistenceProfile;
    let t = support::Temp::new("allocation-exclusive-identity");
    let path = t.join("db");
    let h = Handles::create(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"exclusive".to_vec(),
            cursor_key: [71; 32],
            incarnation: 1,
        },
    )
    .unwrap();
    std::fs::hard_link(&path, t.join("alias")).unwrap();
    assert!(h.checkpoint().is_err());
    std::fs::remove_file(t.join("alias")).unwrap();
    assert!(!h.checkpoint().unwrap().busy);
}

#[test]
fn shared_pack_headroom_is_bounded_and_final_release_preserves_canonical_bytes() {
    for profile in DEVELOPMENT_PROFILES {
        let t = support::Temp::new("bounded-headroom");
        let path = t.join("db");
        let h = Handles::create(
            PersistenceConfig::sqlite(&path).with_sqlite_profile(profile),
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: b"headroom".to_vec(),
                cursor_key: [71; 32],
                incarnation: 1,
            },
        )
        .unwrap();
        let storage = Storage::new(h.storage.clone()).unwrap();
        let object = FinalizedObject::new(
            ObjectRole::FileState,
            layerfs_content::object::codec::encode_bytes_object(&vec![37; 32_000]).unwrap(),
        )
        .unwrap();
        let save = storage.begin_save().unwrap();
        save.accept(object.clone()).unwrap();
        save.finish().unwrap();
        let work = h.diagnostics().unwrap();
        assert!(work.preallocation_calls > 0);
        assert!(
            work.preallocation_bytes
                <= work.preallocation_calls
                    * (layerfs_storage::policy::SINGLETON_PACK_LIMIT as u64 + 3 * 1024 * 1024)
        );
        let before = path.metadata().unwrap();
        assert!(before.blocks() * 512 > before.len());
        let completion = h.checkpoint().unwrap();
        let source = completion.allocation_source.unwrap();
        assert_eq!((source.device, source.inode), (before.dev(), before.ino()));
        assert_eq!(source.logical_bytes, path.metadata().unwrap().len());
        if profile == layerfs_persistence::SqlitePersistenceProfile::Disposable {
            assert_eq!(path.metadata().unwrap().len(), before.len());
        }
        assert!(path.metadata().unwrap().blocks() * 512 <= path.metadata().unwrap().len() + 4096);
        assert_eq!(
            storage
                .reader()
                .unwrap()
                .read_objects(&[object.id()])
                .unwrap(),
            vec![object.canonical().to_vec()]
        );
    }
}

/// Owner direction 2026-10-07: Disposable is the sole active development
/// verification profile. Durable execution of these bodies is NOT_RUN —
/// deferred by owner for Disposable-only development. Durable support and its
/// retained historical receipts are unchanged.
const DEVELOPMENT_PROFILES: [layerfs_persistence::SqlitePersistenceProfile; 1] =
    [layerfs_persistence::SqlitePersistenceProfile::Disposable];
