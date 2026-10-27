//! Original-file allocation proofs through the consuming public seal operation.
#![cfg(target_os = "macos")]
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePackLayout, SqlitePersistenceProfile};
use layerfs_storage::{port::PersistenceError, Storage, StoragePolicy};
use nix::fcntl::{fcntl, FcntlArg};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{symlink, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "layerfs-seal-allocation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        eprintln!("SEAL_FIXTURE {}", directory.display());
        Self(directory)
    }
    fn store(&self, layout: SqlitePackLayout) -> (PathBuf, FinalizedObject) {
        let path = self.0.join("store.sqlite");
        let handles = Handles::create(
            config(&path).with_sqlite_pack_layout(layout),
            StoragePolicy::frozen_default(),
            &authority(),
        )
        .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let object = FinalizedObject::new(
            ObjectRole::FileState,
            layerfs_content::object::codec::encode_bytes_object(b"unchanged canonical file bytes")
                .unwrap(),
        )
        .unwrap();
        let save = storage.begin_save().unwrap();
        save.accept(object.clone()).unwrap();
        save.finish().unwrap();
        drop(storage);
        handles.seal().unwrap();
        (path, object)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("RETAINED_FAILED_SEAL_FIXTURE {}", self.0.display());
        } else {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn authority() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"seal-allocation".to_vec(),
        cursor_key: [73; 32],
        incarnation: 9,
    }
}
fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}
fn open(path: &Path) -> Handles {
    Handles::open_writable(
        config(path),
        &authority().binding_key,
        authority().cursor_key,
    )
    .unwrap()
}
fn hash(path: &Path) -> blake3::Hash {
    let mut file = fs::File::open(path).unwrap();
    let mut digest = blake3::Hasher::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            return digest.finalize();
        }
        digest.update(&buffer[..count]);
    }
}
fn preallocate(path: &Path) -> fs::Metadata {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(path)
        .unwrap();
    let before = file.metadata().unwrap();
    let mut request = nix::libc::fstore_t {
        fst_flags: nix::libc::F_ALLOCATEALL,
        fst_posmode: nix::libc::F_PEOFPOSMODE,
        fst_offset: 0,
        fst_length: 16 * 1024 * 1024,
        fst_bytesalloc: 0,
    };
    fcntl(&file, FcntlArg::F_PREALLOCATE(&mut request)).unwrap();
    nix::unistd::close(file).unwrap();
    let after = path.metadata().unwrap();
    assert_eq!(after.len(), before.len());
    assert!(after.blocks() * 512 > after.len() + 8 * 1024 * 1024);
    after
}

#[test]
fn seal_releases_excess_and_preserves_original_file_for_all_layouts() {
    for layout in [
        SqlitePackLayout::Monolithic,
        SqlitePackLayout::GroupRows,
        SqlitePackLayout::GroupRowsIndexed,
    ] {
        let fixture = Fixture::new();
        let (path, object) = fixture.store(layout);
        let before = preallocate(&path);
        let original = hash(&path);
        let sentinel = fixture.0.join(".layerfs-allocation-unrelated");
        fs::write(&sentinel, b"preserve unrelated file").unwrap();
        let sealed = open(&path).seal().unwrap();
        let after = path.metadata().unwrap();
        assert_eq!(
            (after.dev(), after.ino(), after.len()),
            (before.dev(), before.ino(), before.len())
        );
        assert_eq!(hash(&path), original);
        assert_eq!(sealed.bytes, before.len());
        assert!(after.blocks() * 512 <= after.len() + 4096);
        assert_eq!(fs::read(&sentinel).unwrap(), b"preserve unrelated file");
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 2);
        println!("SEAL_ALLOCATION layout={layout:?} logical={} before={} after={} same_hash=true same_inode=true",
                 after.len(), before.blocks() * 512, after.blocks() * 512);
        let handles = open(&path);
        let storage = Storage::new(handles.storage.clone()).unwrap();
        assert_eq!(
            storage
                .reader()
                .unwrap()
                .read_objects(&[object.id()])
                .unwrap(),
            vec![object.canonical().to_vec()]
        );
        drop(storage);
        handles.seal().unwrap();
    }
}

#[test]
fn shared_session_busy_leaves_allocation_untouched() {
    let fixture = Fixture::new();
    let (path, _) = fixture.store(SqlitePackLayout::GroupRowsIndexed);
    let before = preallocate(&path);
    let handles = open(&path);
    let held = handles.storage.clone();
    assert!(matches!(handles.seal(), Err(PersistenceError::Busy)));
    assert_eq!(path.metadata().unwrap().blocks(), before.blocks());
    drop(held);
}

#[test]
fn readonly_seal_does_not_release_extents() {
    let fixture = Fixture::new();
    let (path, _) = fixture.store(SqlitePackLayout::GroupRowsIndexed);
    let before = preallocate(&path);
    let writer = open(&path);
    let reader = Handles::open_read_only(
        config(&path),
        &authority().binding_key,
        authority().cursor_key,
    )
    .unwrap();
    assert!(matches!(
        reader.seal(),
        Err(PersistenceError::Refused { .. })
    ));
    assert_eq!(path.metadata().unwrap().blocks(), before.blocks());
    drop(writer);
}

#[test]
fn hard_link_refusal_preserves_original_bytes_and_allocation() {
    let fixture = Fixture::new();
    let (path, _) = fixture.store(SqlitePackLayout::GroupRowsIndexed);
    let before = preallocate(&path);
    let digest = hash(&path);
    let handles = open(&path);
    let alias = fixture.0.join("alias.sqlite");
    fs::hard_link(&path, &alias).unwrap();
    assert!(matches!(
        handles.seal(),
        Err(PersistenceError::Refused { .. })
    ));
    assert_eq!(hash(&path), digest);
    assert_eq!(path.metadata().unwrap().blocks(), before.blocks());
    fs::remove_file(alias).unwrap();
}

#[test]
fn symlink_refusal_preserves_original_bytes_and_allocation() {
    let fixture = Fixture::new();
    let (path, _) = fixture.store(SqlitePackLayout::GroupRowsIndexed);
    let before = preallocate(&path);
    let digest = hash(&path);
    let alias = fixture.0.join("alias.sqlite");
    symlink(&path, &alias).unwrap();
    assert!(matches!(
        open(&alias).seal(),
        Err(PersistenceError::Refused { .. })
    ));
    assert_eq!(hash(&path), digest);
    assert_eq!(path.metadata().unwrap().blocks(), before.blocks());
    fs::remove_file(alias).unwrap();
}
