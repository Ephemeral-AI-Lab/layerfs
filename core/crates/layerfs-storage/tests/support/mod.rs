//! Shared external-test helpers for the `layerfs-storage` public API.
//!
//! Nothing here is product code: it creates disposable directories, builds
//! canonical objects through the real C1 API, and drives the real C2 save/read
//! entry points.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use layerfs_content::{
    construct_bytes, read_all, AdvisoryPredecessors, AuthenticatedObjects, ConstructionPolicy,
    ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_storage::{StorageError, StoragePolicy, Store};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Disposable directory removed when it is dropped.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a fresh unique directory under the system temporary directory.
    pub fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "layerfs-stage02-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temporary directory");
        Self { path }
    }

    /// Path of the directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Path of a file inside the directory.
    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    /// Path of a Store database inside the directory.
    pub fn store_path(&self, name: &str) -> PathBuf {
        self.path.join(format!("{name}.sqlite"))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// In-memory consumer that keeps every emitted canonical object.
///
/// The advisory predecessor list is retained too: the C1-to-C2 handoff is defined
/// to carry it, so a collected set can be offered to the Store exactly as the
/// producer emitted it, hints included.
#[derive(Clone, Debug, Default)]
pub struct Collected {
    objects: Vec<(ObjectId, ObjectRole, Vec<u8>, Vec<ObjectId>)>,
    advisories: Vec<AdvisoryPredecessors>,
}

impl Collected {
    /// Empty collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Every collected object in emission order as `(id, role, bytes, refs)`.
    pub fn objects(&self) -> &[(ObjectId, ObjectRole, Vec<u8>, Vec<ObjectId>)] {
        &self.objects
    }

    /// Advisory predecessor list of the object emitted at `index`.
    pub fn advisories(&self) -> &[AdvisoryPredecessors] {
        &self.advisories
    }

    /// Only the objects with `role`, in emission order.
    pub fn with_role(&self, role: ObjectRole) -> Vec<FinalizedObject> {
        self.objects
            .iter()
            .filter(|(_, candidate, _, _)| *candidate == role)
            .map(|(_, candidate, bytes, references)| {
                FinalizedObject::new(*candidate, bytes.clone())
                    .expect("canonical object")
                    .with_references(references.clone())
            })
            .collect()
    }

    /// Every collected object as finalized values, references and advisory
    /// predecessors preserved.
    pub fn finalized(&self) -> Vec<FinalizedObject> {
        self.objects
            .iter()
            .zip(&self.advisories)
            .map(|((_, role, bytes, references), predecessors)| {
                FinalizedObject::new(*role, bytes.clone())
                    .expect("canonical object")
                    .with_references(references.clone())
                    .with_predecessors(predecessors.clone())
            })
            .collect()
    }

    /// Total canonical bytes.
    pub fn canonical_bytes(&self) -> u64 {
        self.objects
            .iter()
            .map(|(_, _, bytes, _)| bytes.len() as u64)
            .sum()
    }
}

impl FinalizedConsumer for Collected {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let predecessors = object.predecessors().clone();
        let (id, role, bytes, references) = object.into_parts();
        self.objects.push((id, role, bytes, references));
        self.advisories.push(predecessors);
        Ok(())
    }
}

/// Provider serving a collected object set.
pub struct Provider<'a>(pub &'a Collected);

impl AuthenticatedObjects for Provider<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter()
            .map(|id| {
                self.0
                    .objects()
                    .iter()
                    .find(|(candidate, _, _, _)| candidate == id)
                    .map(|(_, _, bytes, _)| bytes.clone())
                    .ok_or(ContentError::MissingObject)
            })
            .collect()
    }
}

/// Provider that serves canonical bytes from a real Store.
///
/// This is the product bridge an adapter uses, not a second implementation of
/// it: the test drives the same object the runtime does.
pub use layerfs_storage::StoreProvider;

/// Reads the whole logical file through the real C1 read path, with every
/// canonical object acquired from the real Store.
pub fn read_logical(store: &Store, root: ObjectId) -> Vec<u8> {
    let mut bytes = Vec::new();
    disabled(|scope| {
        read_all(
            &StoreProvider::new(store),
            root,
            &mut bytes,
            scope.child("content.read"),
        )
    })
    .expect("logical read through the store");
    bytes
}

/// A root scope that records nothing.
pub fn disabled<T, E>(
    body: impl FnOnce(
        &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>,
    ) -> Result<T, E>,
) -> Result<T, E> {
    layerfs_telemetry::timer::Timing::disabled("test.operation", body).0
}

/// Builds a complete file through the real C1 constructor.
pub fn construct_file(bytes: &[u8]) -> (Collected, ObjectId, u64) {
    let policy = ConstructionPolicy::frozen_default();
    let mut collected = Collected::new();
    let constructed = disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            bytes,
            &mut collected,
            scope.child("content"),
        )
    })
    .expect("complete-file construction");
    (collected, constructed.root, constructed.logical_len)
}

/// Creates a fresh Store with the frozen policy.
pub fn create_store(path: &Path) -> Store {
    disabled(|scope| Store::create(path, StoragePolicy::frozen_default(), scope.child("store")))
        .expect("store creation")
}

/// Opens an existing Store.
pub fn open_store(path: &Path) -> Store {
    disabled(|scope| Store::open(path, scope.child("store"))).expect("store open")
}

/// Saves every collected object in emission order and acknowledges completion.
pub fn save_all(
    store: &Store,
    collected: &Collected,
) -> Result<layerfs_storage::SaveOutcome, StorageError> {
    let objects = collected.finalized();
    disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in objects {
            operation.accept(object, scope.child("storage.accept"))?;
        }
        operation.finish(scope.child("storage.finish"))
    })
}

/// Saves one object with its own bounded operation.
pub fn save_one(
    store: &Store,
    object: FinalizedObject,
) -> Result<layerfs_storage::SaveOutcome, StorageError> {
    disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        operation.accept(object, scope.child("storage.accept"))?;
        operation.finish(scope.child("storage.finish"))
    })
}

/// Saves a whole collected object set through the C1 handoff adapter.
pub fn save_via_handoff(
    store: &Store,
    collected: &Collected,
) -> Result<layerfs_storage::SaveOutcome, StorageError> {
    let objects = collected.finalized();
    disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in objects {
            operation.accept(object, scope.child("storage.accept"))?;
        }
        operation.finish(scope.child("storage.finish"))
    })
}

/// Reads canonical bytes for `ids` from an independent Store connection.
pub fn read_objects(
    store: &Store,
    ids: &[ObjectId],
) -> Result<(Vec<Vec<u8>>, layerfs_storage::StoreReadCounters), StorageError> {
    disabled(|scope| store.read_batch(ids, scope.child("storage.read")))
}

/// Builds a canonical whole-file object directly from raw payload bytes.
pub fn assembled_small_object(raw: &[u8]) -> Vec<u8> {
    let mut value = Vec::new();
    value.extend_from_slice(b"LFS5SML\0");
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(raw);
    layerfs_content::object::codec::encode_bytes_object(&value).expect("canonical envelope")
}

/// Reader that serves `bytes` and then reports an I/O failure at `fail_at`.
pub struct FailingAfter {
    bytes: Vec<u8>,
    position: usize,
    fail_at: usize,
}

impl FailingAfter {
    /// Serves `bytes` until `fail_at` have been delivered, then fails.
    pub fn new(bytes: Vec<u8>, fail_at: usize) -> Self {
        Self {
            bytes,
            position: 0,
            fail_at,
        }
    }
}

impl std::io::Read for FailingAfter {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.fail_at {
            return Err(std::io::Error::other("late input failure"));
        }
        let available = (self.fail_at - self.position).min(output.len());
        let end = (self.position + available).min(self.bytes.len());
        let count = end - self.position;
        output[..count].copy_from_slice(&self.bytes[self.position..end]);
        self.position += count;
        Ok(count)
    }
}

/// Deterministic incompressible input.
pub fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    (0..len)
        .map(|_| {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            state as u8
        })
        .collect()
}

/// Deterministic compressible input.
pub fn repeat(len: usize, byte: u8) -> Vec<u8> {
    vec![byte; len]
}

/// Deterministic structured input.
pub fn patterned(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect()
}

/// Flips one byte of the first stored pack body, simulating damaged storage.
pub fn corrupt_first_pack(path: &Path) {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    let data: Vec<u8> = connection
        .query_row(
            "SELECT data FROM object_packs ORDER BY pack_id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("pack row");
    let mut damaged = data.clone();
    let index = damaged.len() - 1;
    damaged[index] ^= 0xff;
    let affected = connection
        .execute(
            "UPDATE object_packs SET data = ?1 WHERE pack_id = (SELECT MIN(pack_id) FROM object_packs)",
            rusqlite::params![damaged],
        )
        .expect("pack update");
    assert_eq!(affected, 1);
}

/// Damages the digest of the first stored value-group catalogue row.
pub fn corrupt_value_group_digest(path: &Path) {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    let affected = connection
        .execute(
            "UPDATE metadata_value_groups SET digest = ?1 WHERE first_ordinal = \
             (SELECT MIN(first_ordinal) FROM metadata_value_groups)",
            rusqlite::params![vec![0xab_u8; 32]],
        )
        .expect("digest damage");
    assert_eq!(affected, 1);
}

pub mod filesystem;
