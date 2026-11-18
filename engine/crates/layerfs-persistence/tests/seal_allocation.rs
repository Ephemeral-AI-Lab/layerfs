//! Original-file allocation proofs through the consuming public seal operation.
#![cfg(target_os = "macos")]
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{
    HistoryCatalog, HistoryCatalogConfig, HistoryError, HistoryName, LayerStackId,
    StackInitialization,
};
use layerfs_persistence::{
    CompactPath, CompactReceipt, CompactRequest, PersistenceConfig, SqliteHandles,
    SqlitePackLayout, SqlitePersistenceProfile,
};
use layerfs_storage::{
    port::{PackPersistence, PersistenceError},
    Storage, StoragePolicy,
};
use nix::fcntl::{fcntl, FcntlArg};
use std::{
    ffi::CString,
    fs::{self, OpenOptions},
    io::Read,
    os::unix::ffi::OsStrExt,
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
        let handles = SqliteHandles::create(
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
fn open(path: &Path) -> SqliteHandles {
    SqliteHandles::open_writable(
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
    let reader = SqliteHandles::open_read_only(
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

/// Makes `target` an APFS copy-on-write clone of `source`. From then on both
/// files may share blocks, which is what makes the extent transfer refuse.
fn clone_file(source: &Path, target: &Path) {
    let from = CString::new(source.as_os_str().as_bytes()).unwrap();
    let to = CString::new(target.as_os_str().as_bytes()).unwrap();
    // SAFETY: both arguments are NUL-terminated paths that outlive the call.
    let status = unsafe { nix::libc::clonefile(from.as_ptr(), to.as_ptr(), 0) };
    assert_eq!(
        status,
        0,
        "clonefile needs a cloning filesystem (APFS): {}",
        std::io::Error::last_os_error()
    );
}

/// The names in the fixture directory, sorted.
fn names(fixture: &Fixture) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(&fixture.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A file that was ever one side of a clone refuses the extent transfer with
/// ENOENT before it moves anything. The seal reports that as nothing
/// released: the Store is sealed and unchanged, and no scratch file remains.
#[test]
fn seal_of_a_cloned_store_releases_nothing_and_leaves_no_scratch() {
    for sealed_side in ["clone", "origin"] {
        let fixture = Fixture::new();
        let (origin, object) = fixture.store(SqlitePackLayout::GroupRowsIndexed);
        let clone = fixture.0.join("clone.sqlite");
        clone_file(&origin, &clone);
        let (path, other) = match sealed_side {
            "clone" => (clone, origin),
            _ => (origin, clone),
        };
        let before = preallocate(&path);
        let original = hash(&path);
        let other_before = other.metadata().unwrap();
        let sealed = open(&path)
            .seal()
            .unwrap_or_else(|error| panic!("seal of the {sealed_side}: {error:?}"));
        let after = path.metadata().unwrap();
        assert_eq!(
            (after.dev(), after.ino(), after.len()),
            (before.dev(), before.ino(), before.len())
        );
        assert_eq!(sealed.bytes, before.len());
        assert_eq!(hash(&path), original);
        assert!(after.blocks() <= before.blocks());
        let other_after = other.metadata().unwrap();
        assert_eq!(
            (other_after.ino(), other_after.len(), other_after.blocks()),
            (
                other_before.ino(),
                other_before.len(),
                other_before.blocks()
            )
        );
        assert_eq!(hash(&other), original);
        assert_eq!(names(&fixture), ["clone.sqlite", "store.sqlite"]);
        println!(
            "SEAL_CLONED side={sealed_side} logical={} before={} after={}",
            after.len(),
            before.blocks() * 512,
            after.blocks() * 512
        );
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

/// Saves one filesystem root with its inode and directory pages.
fn filesystem_root(handles: &SqliteHandles, seed: u8) -> ObjectId {
    use layerfs_content::{
        filesystem::{
            attributes::PortableMetadata,
            directory::{encode_directory_page, DirectoryPage},
            inode::{encode_inode_page, InodePage},
            root::profile_id,
            scope_for_seed, FilesystemRoot,
        },
        inode_leaf::{InodeKind, InodeMetadata, InodeValue},
    };
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: Vec::new(),
        })
        .unwrap(),
    )
    .unwrap();
    let directory_id = directory.id();
    let inode = FinalizedObject::new(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: vec![(
                1,
                InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 0,
                    content_root: directory_id,
                    metadata_root: InodeMetadata::Inline(PortableMetadata {
                        mode: 0o755,
                        mtime_seconds: i64::from(seed),
                        mtime_nanoseconds: 0,
                    }),
                },
            )],
        })
        .unwrap(),
    )
    .unwrap()
    .with_references(vec![directory_id]);
    let inode_id = inode.id();
    let root = FilesystemRoot::new(profile_id(), scope_for_seed([1; 32]), 1, inode_id).unwrap();
    let object = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode().unwrap())
        .unwrap()
        .with_references(vec![inode_id]);
    let root_id = object.id();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    for object in [directory, inode, object] {
        save.accept(object).unwrap();
    }
    save.finish().unwrap();
    root_id
}

fn located(handles: &SqliteHandles, id: ObjectId) -> bool {
    let mut found = Vec::new();
    handles.storage.locate(&[id], &mut found).unwrap();
    !found.is_empty()
}

/// A sealed Store a compaction rebuilds: forty live projects and four packs
/// nothing reaches.
struct Rebuildable {
    path: PathBuf,
    live: Vec<ObjectId>,
    dead: Vec<ObjectId>,
    generation: u64,
}

fn rebuildable(fixture: &Fixture) -> Rebuildable {
    let (path, _) = fixture.store(SqlitePackLayout::GroupRowsIndexed);
    let handles = open(&path);
    // The bytes do not compress: these packs are most of the Store's pack
    // bytes, well over the tenth a rebuild needs.
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut dead = Vec::new();
    for _ in 0..4 {
        let noise: Vec<u8> = (0..100_000)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state.to_le_bytes()[0]
            })
            .collect();
        let object = FinalizedObject::new(
            ObjectRole::WholeFile,
            layerfs_content::encode_whole_file_payload(&noise).unwrap(),
        )
        .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let save = storage.begin_save().unwrap();
        save.accept(object.clone()).unwrap();
        save.finish().unwrap();
        dead.push(object.id());
    }
    let mut live = Vec::new();
    for project in 1..=40u8 {
        let root = filesystem_root(&handles, project);
        handles
            .history
            .initialize_layerstack(&StackInitialization {
                logical: 10,
                stack: LayerStackId::from_authority([project; 16]),
                name: HistoryName::new(&format!("project-{project}")).unwrap(),
                scope: layerfs_content::filesystem::scope_for_seed([1; 32]).object(),
                profile: layerfs_content::filesystem::root::profile_id(),
                genesis_root: root,
            })
            .unwrap();
        live.push(root);
    }
    let generation = handles.history.incarnation();
    handles.seal().unwrap();
    Rebuildable {
        path,
        live,
        dead,
        generation,
    }
}

fn compact(path: &Path) -> Result<CompactReceipt, HistoryError> {
    open(path).compact(&CompactRequest {
        op_id: [0x41; 16],
        ..CompactRequest::default()
    })
}

/// A compaction replaces the sealed file by its rename, so it does not release
/// extents at all. Before, the release ran on the file about to be replaced,
/// and on a cloned Store with allocation past its end its refusal ended the
/// run `UnknownOutcome` with the op record, the mark scratch and an empty
/// `.layerfs-allocation-*` file left beside the Store.
#[test]
fn compaction_of_a_cloned_store_with_allocation_past_its_end_rebuilds() {
    for cloned in [true, false] {
        let fixture = Fixture::new();
        let store = rebuildable(&fixture);
        let path = if cloned {
            let clone = fixture.0.join("clone.sqlite");
            clone_file(&store.path, &clone);
            clone
        } else {
            store.path.clone()
        };
        preallocate(&path);
        let receipt = compact(&path).unwrap_or_else(|error| panic!("cloned={cloned}: {error:?}"));
        assert_eq!(receipt.path, CompactPath::Rebuild);
        assert_eq!(receipt.generation_after, store.generation + 1);
        // No op record, mark scratch, staged file, sidecar or allocation
        // scratch is left beside the Store.
        let expected: &[&str] = if cloned {
            &["clone.sqlite", "store.sqlite"]
        } else {
            &["store.sqlite"]
        };
        assert_eq!(names(&fixture), expected);
        let after = path.metadata().unwrap();
        assert!(after.blocks() * 512 <= after.len() + 4096);
        let handles = open(&path);
        for root in &store.live {
            assert!(located(&handles, *root), "a live root survives");
        }
        for object in &store.dead {
            assert!(!located(&handles, *object), "a dead object is gone");
        }
        handles.seal().unwrap();
    }
}

/// The compaction's seal releases nothing, but it keeps the seal's macOS
/// refusal of a Store that has a second hard link or is reached through a
/// symbolic link: the rename would replace one name and leave the other on
/// the old file. The refusal is definite and leaves the Store as it was.
#[test]
fn compaction_refuses_a_linked_store_and_leaves_it_unchanged() {
    for link in ["hard link", "symbolic link"] {
        let fixture = Fixture::new();
        let store = rebuildable(&fixture);
        let before = preallocate(&store.path);
        let alias = fixture.0.join("alias.sqlite");
        let path = if link == "hard link" {
            fs::hard_link(&store.path, &alias).unwrap();
            &store.path
        } else {
            symlink(&store.path, &alias).unwrap();
            &alias
        };
        let error = compact(path).unwrap_err();
        assert!(
            matches!(error, HistoryError::Integrity("Store seal")),
            "{link}: {error:?}"
        );
        assert_eq!(names(&fixture), ["alias.sqlite", "store.sqlite"]);
        let after = store.path.metadata().unwrap();
        assert_eq!((after.dev(), after.ino()), (before.dev(), before.ino()));
        fs::remove_file(&alias).unwrap();
        let handles = open(&store.path);
        assert_eq!(handles.history.incarnation(), store.generation);
        for object in store.live.iter().chain(&store.dead) {
            assert!(located(&handles, *object), "{link}: nothing was collected");
        }
        handles.seal().unwrap();
    }
}
