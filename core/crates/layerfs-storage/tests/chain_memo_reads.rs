//! A reader that keeps the chain elements it rebuilt answers exactly as one
//! that walks every chain to its end.
//!
//! An inode table is saved through the real writer and then changed one value
//! per Save, so its root branch and one leaf grow delta chains up to the
//! metadata depth cap. Every stored leaf and branch is then resolved through
//! two readers over the same Store, one plain and one made with
//! [`PoolReader::with_chain_memo`]. The proofs:
//!
//! * the canonical bytes, the identity and the chain's reported depth and
//!   charged bytes are equal for every object, in base-first and in
//!   dependent-first order, and with a memo too small to hold anything;
//! * the memo reader reads fewer records and applies fewer programs;
//! * under a lower depth cap both readers refuse the same objects with the
//!   same error: a chain that stops at a kept element is still measured to
//!   its end.
#[path = "support/memory_metadata.rs"]
mod metadata;

use layerfs_content::filesystem::attributes::portable::PortableMetadata;
use layerfs_content::filesystem::inode::apply_inode_values;
use layerfs_content::filesystem::{FilesystemObjects, MAXIMUM_SCRATCH_BYTES};
use layerfs_content::object::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
use layerfs_content::{ContentResult, FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::encoding::delta::read::{BodyCaches, ChainCounters, Resolver};
use layerfs_storage::encoding::pool::PoolReader;
use layerfs_storage::encoding::{DecompressionWorkspace, GroupCache, PackAcquisition, PackCache};
use layerfs_storage::location::{ObjectLocation, SignatureRow};
use layerfs_storage::source::Source;
use layerfs_storage::{Storage, StorageCapacities, StorageError, StoragePolicy, StorageResult};
use std::sync::Arc;

const INODES: u64 = 6_000;
const ROUNDS: u64 = 20;

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

fn value(serial: u64, version: u64) -> InodeValue {
    InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&serial.to_le_bytes()),
        metadata_root: InodeMetadata::Inline(PortableMetadata {
            mode: 0o644,
            mtime_seconds: 1_700_000_000 + version as i64,
            mtime_nanoseconds: 0,
        }),
    }
}

fn commit(storage: &Storage, base: Option<ObjectId>, changes: Vec<(u64, InodeValue)>) -> ObjectId {
    let reader = storage.reader().unwrap();
    let mut emitted = Emitted::default();
    let (root, _) = {
        let mut objects = FilesystemObjects::new(&reader, &mut emitted);
        apply_inode_values(
            &mut objects,
            base,
            changes
                .into_iter()
                .map(|(serial, value)| Ok((serial, Some(value)))),
            MAXIMUM_SCRATCH_BYTES,
        )
        .unwrap()
    };
    drop(reader);
    let save = storage.begin_save().unwrap();
    for object in emitted.0 {
        let predecessors = object.predecessors().clone();
        let parts = object.into_parts();
        save.accept(
            FinalizedObject::new(parts.role, parts.canonical)
                .unwrap()
                .with_predecessors(predecessors),
        )
        .unwrap();
    }
    save.finish().unwrap();
    root
}

/// The Store's rows as the encoding readers' physical input.
struct Rows(Arc<metadata::MemoryMetadata>);

impl Source for Rows {
    fn ordered_dependencies(&self) -> bool {
        false
    }
    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        let state = self.0.state.lock().unwrap();
        Ok(state
            .objects
            .get(&id)
            .filter(|location| location.pack_id <= ceiling)
            .copied())
    }
    fn pack_bytes(&self, id: i64) -> StorageResult<Vec<u8>> {
        let state = self.0.state.lock().unwrap();
        let pack = state
            .packs
            .get(&id)
            .ok_or(StorageError::Integrity("pack"))?;
        Ok(pack.body.to_vec())
    }
    fn acquire_groups(&self, id: i64, _: &[usize], _: bool) -> StorageResult<PackAcquisition> {
        let state = self.0.state.lock().unwrap();
        let pack = state
            .packs
            .get(&id)
            .ok_or(StorageError::Integrity("pack"))?;
        Ok(PackAcquisition::Whole {
            info: Some(pack.info),
            body: pack.body.to_vec(),
        })
    }
    fn inode_values(&self, ordinals: &[u32]) -> StorageResult<Vec<[u8; 73]>> {
        let state = self.0.state.lock().unwrap();
        ordinals
            .iter()
            .map(|ordinal| {
                state
                    .values
                    .get(ordinal)
                    .copied()
                    .ok_or(StorageError::Integrity("value"))
            })
            .collect()
    }
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>> {
        Err(StorageError::Integrity("unexpected fixture query"))
    }
    fn write_signatures(&self, _: &[SignatureRow]) -> StorageResult<usize> {
        Err(StorageError::Integrity("unexpected fixture query"))
    }
}

/// One reader's caches, kept across every object it resolves.
struct Reader {
    packs: PackCache,
    pool: PoolReader,
    groups: GroupCache,
    workspace: DecompressionWorkspace,
}

impl Reader {
    fn new(pool: PoolReader) -> Self {
        Self {
            packs: PackCache::new(),
            pool,
            groups: GroupCache::new(),
            workspace: DecompressionWorkspace::new().unwrap(),
        }
    }

    /// One resolution: the bytes and identity, and what the chain reported
    /// apart from the reader's own work counts; or the refusal's text.
    fn resolve(
        &mut self,
        source: &Rows,
        capacities: &StorageCapacities,
        location: ObjectLocation,
    ) -> Result<(Vec<u8>, ObjectId, [u64; 4]), String> {
        let mut counters = ChainCounters::default();
        let resolved = Resolver::new(
            source,
            i64::MAX,
            capacities,
            BodyCaches {
                packs: &mut self.packs,
                pool: &mut self.pool,
            },
            &mut self.groups,
            &mut self.workspace,
            &mut counters,
        )
        .resolve_at(location);
        match resolved {
            Ok((bytes, id)) => Ok((
                bytes,
                id,
                [
                    counters.objects,
                    counters.edges,
                    counters.max_depth,
                    counters.canonical_bytes,
                ],
            )),
            Err(error) => Err(format!("{error:?}")),
        }
    }
}

/// A Store whose root branch and first leaf carry chains up to the depth cap,
/// and every stored leaf and branch in locator order.
fn chained() -> (Rows, Vec<ObjectLocation>) {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = StoragePolicy::default();
    let storage = Storage::new(metadata.clone()).unwrap();
    let rows = (1..=INODES)
        .map(|serial| (serial, value(serial, 0)))
        .collect();
    let mut root = commit(&storage, None, rows);
    for round in 1..=ROUNDS {
        // The same few inodes of one leaf, so that leaf's chain grows too.
        let serial = 1 + round % 3;
        root = commit(&storage, Some(root), vec![(serial, value(serial, round))]);
    }
    drop(storage);
    let mut pages: Vec<ObjectLocation> = metadata
        .state
        .lock()
        .unwrap()
        .objects
        .values()
        .filter(|location| {
            matches!(
                location.role,
                ObjectRole::InodeLeaf | ObjectRole::InodeBranch
            )
        })
        .copied()
        .collect();
    pages.sort_by_key(|location| {
        (
            location.pack_id,
            location.group_number,
            location.record_number,
        )
    });
    (Rows(metadata), pages)
}

fn capacities(depth: u8) -> StorageCapacities {
    let mut capacities = StorageCapacities::from_policy(StoragePolicy::default()).unwrap();
    capacities.metadata_delta_max_depth = depth;
    capacities
}

#[test]
fn a_memo_reader_answers_as_a_plain_reader_with_less_work() {
    let (source, pages) = chained();
    let capacities = capacities(8);
    let mut plain = Reader::new(PoolReader::new());
    let expected: Vec<_> = pages
        .iter()
        .map(|location| plain.resolve(&source, &capacities, *location))
        .collect();
    let deepest = expected
        .iter()
        .map(|answer| answer.as_ref().unwrap().2[2])
        .max();
    assert_eq!(deepest, Some(8), "the fixture reaches the depth cap");
    let leaf_chains = plain.pool.counters().chain_edges;
    assert!(leaf_chains > 0, "the fixture has pooled leaf chains");

    // Base first: a dependent stops at the base resolved before it.
    let mut forward = Reader::new(PoolReader::with_chain_memo(1 << 20));
    for (location, expected) in pages.iter().zip(&expected) {
        assert_eq!(&forward.resolve(&source, &capacities, *location), expected);
    }
    let (plain_work, memo_work) = (plain.pool.counters(), forward.pool.counters());
    assert!(
        memo_work.physical_record_calls < plain_work.physical_record_calls,
        "{memo_work:?} against {plain_work:?}"
    );
    assert!(memo_work.branch_delta_applies < plain_work.branch_delta_applies);
    assert!(memo_work.chain_edges <= plain_work.chain_edges);

    // Dependent first: a base is answered from what its dependent rebuilt.
    let mut backward = Reader::new(PoolReader::with_chain_memo(1 << 20));
    for (location, expected) in pages.iter().zip(&expected).rev() {
        assert_eq!(&backward.resolve(&source, &capacities, *location), expected);
    }
    assert!(
        backward.pool.counters().branch_delta_applies < plain_work.branch_delta_applies,
        "{:?}",
        backward.pool.counters()
    );

    // A memo too small to keep a page changes nothing, in either direction.
    let mut small = Reader::new(PoolReader::with_chain_memo(64));
    for (location, expected) in pages.iter().zip(&expected) {
        assert_eq!(&small.resolve(&source, &capacities, *location), expected);
    }
    assert_eq!(small.pool.counters(), plain_work);
}

#[test]
fn a_memo_reader_refuses_the_chains_a_plain_reader_refuses() {
    let (source, pages) = chained();
    // The Store was written under a cap of eight; it is read under three.
    let capacities = capacities(3);
    let mut plain = Reader::new(PoolReader::new());
    let expected: Vec<_> = pages
        .iter()
        .map(|location| plain.resolve(&source, &capacities, *location))
        .collect();
    let refused = expected.iter().filter(|answer| answer.is_err()).count();
    let chained = expected
        .iter()
        .filter(|answer| answer.as_ref().is_ok_and(|read| read.2[2] > 0))
        .count();
    assert!(
        refused > 0 && chained > 0,
        "{refused} refused, {chained} read"
    );
    for forward in [true, false] {
        let mut memo = Reader::new(PoolReader::with_chain_memo(1 << 20));
        let mut order: Vec<usize> = (0..pages.len()).collect();
        if !forward {
            order.reverse();
        }
        for index in order {
            assert_eq!(
                memo.resolve(&source, &capacities, pages[index]),
                expected[index],
                "object {index}, forward {forward}"
            );
        }
    }
}
