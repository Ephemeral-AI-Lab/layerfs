//! Fixture construction and Store probes for the GC benchmark home.
//!
//! This module is a library module of [`examples/gc_bench.rs`](../gc_bench.rs),
//! not a Cargo example target: the benchmark declares it with
//! `#[path = "gc_bench/gc_fixture.rs"] mod gc_fixture;`.
//!
//! The fixture is built through the public persistence/storage APIs, so the
//! measurement exercises the same construction and compaction paths as a real
//! Store. It models the four shapes the review of the whole-file-only fixture
//! found missing:
//!
//! - **history depth:** each LayerStack (project) publishes a chain of Layers,
//!   not one genesis Layer per project. A live Stack keeps all its Layers, so
//!   every published Layer root stays in the mark closure.
//! - **shared content:** one pool of whole-file objects is saved once and
//!   referenced by the inode leaves of many projects and Layers, so the mark
//!   meets many references to one object instead of one reference per object.
//! - **delta chains:** selected inode leaves reference a chain target whose
//!   bases are reached only through the stored PREFIX records (the depth-2 and
//!   deeper shape `keptWaste.deltaBases` exists for). Chains are published with
//!   explicit PREFIX records through the public `publish` API, the same
//!   construction the compaction fixtures use, so the depth is deterministic
//!   and the harness does not silently fall back to FULL records.
//! - **layer size:** every content object is still one whole-file payload of
//!   `object_bytes` (<= 131,071 B), because a chunked-file tree would add a
//!   second construction grammar; the report and `tools/ci/gc-bench.md` state
//!   that as the remaining representativeness gap.
//!
//! The dead payload is whole-file objects that no retained root reaches. Every
//! dead object is saved as its own pack, so `SELECT count(*), sum(length) FROM
//! pack` stays the exact pack-byte population and the section 11 dead-byte
//! bound/refined fields can be checked against it.
//!
//! Only the Normal (WAL/synchronous=NORMAL) profile is built and measured:
//! owner direction 2026-10-07 disabled Durable execution indefinitely.

use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::filesystem::directory::{encode_directory_page, DirectoryPage};
use layerfs_content::filesystem::inode::{encode_inode_page, InodePage};
use layerfs_content::filesystem::root::profile_id;
use layerfs_content::filesystem::{scope_for_seed, FilesystemRoot, InodeScope};
use layerfs_content::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitMeta, CommitOutcome, CommitRequest,
    CommitStatistics, CommitVia, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig,
    HistoryName, LayerStackId, StackInitialization,
};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::encoding::{encode_full, encode_prefix, raw_payload, CompressionWorkspace};
use layerfs_storage::location::{ObjectLocation, PackDomain};
use layerfs_storage::pack::{assemble, build_group, PackLane};
use layerfs_storage::port::{PackPersistence, Publication, PublishedPack, Reserve};
use layerfs_storage::save::SaveProfile;
use layerfs_storage::{Storage, StorageCapacities, StoragePolicy};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// Binding key of the benchmark Store; it is not a product binding.
pub const BINDING: &[u8] = b"gc-bench-fixture-v1";
/// Cursor key of the benchmark Store; it is not a product cursor.
pub const CURSOR_KEY: [u8; 32] = [44; 32];

/// Payload of one delta chain version before the appended version suffix. It is
/// below the 131,072 B whole-file cutoff, so a chain object is a WholeFile
/// object and can carry a PREFIX record.
pub const CHAIN_BASE_BYTES: usize = 8 * 1024;
/// Bytes appended for each later version; a PREFIX record then stores only this
/// suffix (plus framing) instead of the whole object.
pub const CHAIN_VERSION_BYTES: usize = 64;
/// Inode page cap: the directory root plus at most 99 file rows.
pub const MAX_GROUP_SIZE: u64 = 99;
/// Whole-file delta depth the frozen policy allows.
pub const MAX_CHAIN_DEPTH: u64 = 16;

const BATCH: usize = 100;

#[derive(Clone, Copy, Debug)]
pub struct FixtureSpec {
    /// Inode leaves, one per published Layer. `groups` is the benchmark's
    /// existing "groups" knob: `stacks * layers = groups`.
    pub groups: u64,
    /// LayerStacks (projects). History is `groups / stacks` Layers deep.
    pub stacks: u64,
    /// Whole-file entries per inode leaf, counting shared slots and a delta
    /// target. The page cap is [`MAX_GROUP_SIZE`].
    pub group_size: u64,
    /// Payload bytes per fresh and shared whole-file object.
    pub object_bytes: usize,
    /// Entries per leaf that reference the shared pool.
    pub shared_slots: u64,
    /// Distinct objects in the shared pool, saved once.
    pub shared_objects: u64,
    /// Layers whose inode leaf references one delta chain target.
    pub delta_chains: u64,
    /// Maximum PREFIX edges in a chain; chain `i` has `1 + i % delta_depth` of
    /// them, so the report carries a depth histogram.
    pub delta_depth: u64,
}

impl FixtureSpec {
    pub fn shared_per_leaf(&self) -> u64 {
        self.shared_slots.min(self.shared_objects)
    }

    pub fn chain_leaves(&self) -> u64 {
        self.delta_chains.min(self.groups)
    }

    pub fn chains(&self) -> u64 {
        self.chain_leaves()
    }

    /// One chain object count, summed over the mixed depths.
    pub fn chain_objects(&self) -> u64 {
        let depth = self.delta_depth.max(1);
        (0..self.chains()).map(|chain| 1 + (chain % depth)).sum()
    }

    pub fn fresh_objects(&self) -> u64 {
        let per_leaf = self.group_size.saturating_sub(self.shared_per_leaf());
        self.groups
            .saturating_mul(per_leaf)
            .saturating_sub(self.chain_leaves())
    }

    /// Unique content objects: fresh leaves, the shared pool and every chain
    /// object (target and bases).
    pub fn live_objects(&self) -> u64 {
        self.fresh_objects()
            .saturating_add(self.shared_objects)
            .saturating_add(self.chain_objects())
    }

    pub fn live_payload_bytes(&self) -> u64 {
        let whole = self
            .fresh_objects()
            .saturating_add(self.shared_objects)
            .saturating_mul(u64::try_from(self.object_bytes).unwrap_or(u64::MAX));
        let depth = self.delta_depth.max(1);
        let chain_bytes: u64 = (0..self.chains())
            .map(|chain| {
                let version = 1 + (chain % depth);
                let len = CHAIN_BASE_BYTES as u64 + CHAIN_VERSION_BYTES as u64 * (version + 1);
                // Each chain saves version 0 plus `version` successors.
                len.saturating_mul(version + 1)
            })
            .sum();
        whole.saturating_add(chain_bytes)
    }

    /// Live bytes including the per-Layer directory, inode leaf and root pages.
    pub fn live_structural_bytes(&self) -> u64 {
        // A 99-row inode page is ~14 KiB, the directory leaf and root are far
        // smaller; 16 KiB per Layer is a deliberately conservative bound.
        self.live_payload_bytes()
            .saturating_add(self.groups.saturating_mul(16 * 1024))
    }

    /// A deliberately conservative bound on what the fixture and its rebuild
    /// can need on disk. The 50 GB arm's receipt bound is 2.90x the old file;
    /// the runbook asks for >=155 GB free, which this covers with margin.
    pub fn disk_headroom_bytes(&self) -> u64 {
        self.live_structural_bytes()
            .saturating_mul(34)
            .div_ceil(10)
            .saturating_add(1024 * 1024 * 1024)
    }

    /// Refuses a spec that the construction below cannot realize, instead of
    /// silently building a different shape.
    pub fn validate(&self) -> Result<(), String> {
        if self.stacks == 0 {
            return Err("stacks must be >= 1".to_owned());
        }
        if self.groups < self.stacks {
            return Err("groups must be >= stacks (one Layer per Stack at least)".to_owned());
        }
        if self.group_size == 0 || self.group_size > MAX_GROUP_SIZE {
            return Err(format!(
                "group_size must be in 1..={MAX_GROUP_SIZE} (inode page cap)"
            ));
        }
        if self.object_bytes == 0 || self.object_bytes > 131_071 {
            return Err("object_bytes must be in 1..=131071 (whole-file payload)".to_owned());
        }
        if self.shared_slots > 0 && self.shared_objects == 0 {
            return Err("shared_slots > 0 needs shared_objects > 0".to_owned());
        }
        if self.shared_per_leaf() >= self.group_size {
            return Err("shared_slots must leave at least one entry for content".to_owned());
        }
        if self.groups.saturating_mul(self.shared_per_leaf()) < self.shared_objects {
            return Err(
                "the shared pool is larger than the references the leaves make; \
                 every shared object must be reachable"
                    .to_owned(),
            );
        }
        if self.delta_chains > 0 && self.delta_depth == 0 {
            return Err("delta_chains > 0 needs delta_depth >= 1".to_owned());
        }
        if self.delta_depth > MAX_CHAIN_DEPTH {
            return Err(format!(
                "delta_depth must be <= {MAX_CHAIN_DEPTH} (whole-file policy)"
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PackStats {
    pub count: u64,
    pub total: u64,
    pub min: u64,
    pub max: u64,
}

/// What `build_live` realized, so the report can carry the fixture shape next
/// to the timing instead of describing it only in prose.
#[derive(Clone, Debug, Default)]
pub struct BuildReport {
    pub seconds: f64,
    pub pack_before: PackStats,
    pub stacks: u64,
    pub layers_min: u64,
    pub layers_max: u64,
    pub history_commits: u64,
    pub history_layers: u64,
    pub fresh_objects: u64,
    pub shared_objects: u64,
    pub shared_references: u64,
    pub delta_chains: u64,
    pub delta_objects: u64,
    pub max_chain_depth: u64,
    /// `(PREFIX edges, count)` for every realized depth.
    pub chains_by_depth: Vec<(u64, u64)>,
}

pub fn create_store(path: &Path) -> SqliteHandles {
    SqliteHandles::create(
        PersistenceConfig::sqlite(path.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Normal),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR_KEY,
            incarnation: 44,
        },
    )
    .expect("Store creation")
}

pub fn reopen(path: &Path) -> SqliteHandles {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Normal),
        BINDING,
        CURSOR_KEY,
    )
    .expect("Store reopen")
}

/// Deterministic payload; the seed makes every object distinct. The first eight
/// bytes carry the object index so two seeds cannot collide by accident.
pub fn payload(seed: u64, len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    let head = len.min(8);
    out[..head].copy_from_slice(&seed.to_le_bytes()[..head]);
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    for chunk in out[head..].chunks_exact_mut(8) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        chunk.copy_from_slice(&state.to_le_bytes());
    }
    out
}

/// Payload of chain version `version`: a fixed base payload followed by one or
/// more 64 B markers, so every later version is a byte-prefix extension of the
/// previous one and a PREFIX record stores only the appended bytes.
pub fn chain_payload(seed: u64, version: u64) -> Vec<u8> {
    let mut out = payload(seed, CHAIN_BASE_BYTES);
    for mark in 0..=version {
        let mut marker = [0u8; CHAIN_VERSION_BYTES];
        marker[..8].copy_from_slice(&seed.to_le_bytes());
        marker[8..16].copy_from_slice(&mark.to_le_bytes());
        marker[16..24].copy_from_slice(b"gcbench0");
        out.extend_from_slice(&marker);
    }
    out
}

fn whole_file(bytes: &[u8]) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).expect("whole-file payload"),
    )
    .expect("finalized object")
}

/// Saves a batch of already-finalized objects in one Save.
fn save_batch(handles: &SqliteHandles, objects: Vec<FinalizedObject>) -> Vec<ObjectId> {
    let storage = Storage::new(handles.storage.clone()).expect("storage");
    let save = storage.begin_save().expect("save");
    let mut ids = Vec::with_capacity(objects.len());
    for object in objects {
        ids.push(object.id());
        save.accept(object).expect("accept");
    }
    save.finish().expect("save finish");
    ids
}

fn save_whole_files(handles: &SqliteHandles, payloads: &[Vec<u8>]) -> Vec<ObjectId> {
    let objects = payloads.iter().map(|bytes| whole_file(bytes)).collect();
    save_batch(handles, objects)
}

/// Seals one record as its own pack and registers its locator. This is the
/// public `publish` path the compaction fixtures use for records whose
/// representation must be pinned (FULL base or PREFIX successor).
fn publish_record(
    handles: &SqliteHandles,
    object: &FinalizedObject,
    lane: PackLane,
    record: Vec<u8>,
) -> ObjectId {
    let group = build_group(lane, &[record], None).expect("chain group");
    let body = assemble(lane, &[group]).expect("chain pack");
    let reserved = handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .expect("chain reservation");
    handles
        .storage
        .publish(&Publication {
            packs: vec![PublishedPack::seal(
                reserved.first_pack_id,
                PackDomain::for_lane(lane),
                Arc::new(body),
            )],
            objects: vec![ObjectLocation {
                object_id: object.id(),
                role: object.role(),
                canonical_length: object.canonical_len(),
                pack_id: reserved.first_pack_id,
                group_number: 0,
                record_number: 0,
            }],
            ..Publication::default()
        })
        .expect("chain publication");
    object.id()
}

/// Publishes one delta chain: version 0 as FULL, every successor as a PREFIX
/// record against its predecessor. Returns the target id (the one a tree
/// references) and the number of objects stored.
fn publish_chain(handles: &SqliteHandles, chain: u64, depth: u64) -> (ObjectId, u64) {
    let seed = 20_000_000u64.saturating_add(chain.saturating_mul(97));
    let capacities =
        StorageCapacities::from_policy(StoragePolicy::frozen_default()).expect("capacities");
    let mut workspace = CompressionWorkspace::new().expect("compression workspace");
    let mut profile = SaveProfile::default();

    let base = whole_file(&chain_payload(seed, 0));
    let mut previous_id = publish_record(
        handles,
        &base,
        PackLane::WholeFile,
        encode_full(
            base.canonical(),
            base.role(),
            &capacities,
            &mut workspace,
            &mut profile,
        )
        .expect("chain base record")
        .record,
    );
    let mut previous = base;
    let mut objects = 1u64;
    for version in 1..=depth {
        let target = whole_file(&chain_payload(seed, version));
        let base_raw =
            raw_payload(previous.canonical(), previous.role()).expect("chain base payload");
        let record = encode_prefix(
            target.canonical(),
            target.role(),
            previous_id,
            base_raw,
            &capacities,
            &mut workspace,
            &mut profile,
        )
        .expect("chain prefix record");
        assert_eq!(
            record.base,
            Some(previous_id),
            "chain {chain} version {version} must be a PREFIX record, not FULL"
        );
        previous_id = publish_record(handles, &target, record.lane, record.record);
        previous = target;
        objects = objects.saturating_add(1);
    }
    (previous_id, objects)
}

fn scope_for_stack(stack: u64) -> InodeScope {
    let mut seed = [0u8; 32];
    seed[0] = 1;
    seed[1..9].copy_from_slice(&stack.to_le_bytes());
    scope_for_seed(seed)
}

/// Publishes one Layer's tree: one inode leaf of `roots` under a directory
/// root, and the `FilesystemRoot` that names it. Returns the leaf and root ids.
fn publish_tree(
    handles: &SqliteHandles,
    scope: InodeScope,
    serial: u64,
    roots: &[ObjectId],
) -> (ObjectId, ObjectId) {
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: Vec::new(),
        })
        .expect("directory"),
    )
    .expect("directory leaf");
    let directory_id = directory.id();
    let mut entries = Vec::with_capacity(roots.len() + 1);
    entries.push((
        1,
        InodeValue {
            kind: InodeKind::Directory,
            namespace_ref_count: 0,
            content_root: directory_id,
            metadata_root: InodeMetadata::Inline(PortableMetadata {
                mode: 0o755,
                mtime_seconds: i64::try_from(serial).unwrap_or(0),
                mtime_nanoseconds: 0,
            }),
        },
    ));
    for (offset, root) in roots.iter().enumerate() {
        entries.push((
            u64::try_from(offset + 2).expect("inode serial"),
            InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: *root,
                metadata_root: InodeMetadata::Inline(PortableMetadata {
                    mode: 0o644,
                    mtime_seconds: i64::try_from(serial).unwrap_or(0),
                    mtime_nanoseconds: 0,
                }),
            },
        ));
    }
    // The same shared object can fill more than one slot; the reference list is
    // a set, so keep the first occurrence of each id.
    let mut references = Vec::with_capacity(roots.len() + 1);
    references.push(directory_id);
    for root in roots {
        if !references.contains(root) {
            references.push(*root);
        }
    }
    let leaf = FinalizedObject::new(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf { entries }).expect("inode page"),
    )
    .expect("inode leaf")
    .with_references(references);
    let leaf_id = leaf.id();
    let root = FilesystemRoot::new(profile_id(), scope, 1, leaf_id).expect("filesystem root");
    let root_object = FinalizedObject::new(
        ObjectRole::FilesystemRoot,
        root.encode().expect("root encode"),
    )
    .expect("filesystem root object")
    .with_references(vec![leaf_id]);
    let root_id = root_object.id();
    save_batch(handles, vec![directory, leaf, root_object]);
    (leaf_id, root_id)
}

fn stack_id(index: u64) -> LayerStackId {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&index.to_le_bytes());
    bytes[8..].copy_from_slice(b"g44ben00");
    LayerStackId::from_authority(bytes)
}

fn branch_id(index: u64) -> BranchId {
    let mut bytes = [0u8; 16];
    bytes[0] = 2;
    bytes[1..9].copy_from_slice(&index.to_le_bytes());
    BranchId::from_authority(bytes)
}

/// Genesis Layer for one project, plus the `main` Branch forked from it.
fn initialize_project(
    handles: &SqliteHandles,
    index: u64,
    scope: InodeScope,
    genesis_root: ObjectId,
) -> BranchId {
    let stack = stack_id(index);
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            stack,
            name: HistoryName::new(&format!("gc-bench-{index:07}")).expect("name"),
            scope: scope.object(),
            profile: profile_id(),
            genesis_root,
            logical: 0,
        })
        .expect("layerstack initialization");
    let branch = branch_id(index);
    let record = handles
        .history
        .layer_stack(stack)
        .expect("stack read")
        .expect("stack exists");
    handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name: HistoryName::new(&format!("gc-bench-main-{index:07}")).expect("name"),
            source: ForkSource::Layer(record.head_layer),
        })
        .expect("branch fork");
    branch
}

/// Commits `candidate_root` on the project's Branch and publishes it as a new
/// Layer, keeping the whole history depth in the live Stack.
fn commit_and_publish(
    handles: &SqliteHandles,
    stack: LayerStackId,
    branch: BranchId,
    scope: InodeScope,
    candidate_root: ObjectId,
    logical: u64,
) {
    let snapshot = handles
        .history
        .branch(branch)
        .expect("branch read")
        .expect("branch");
    let expected_root = match snapshot.head_commit {
        Some(head) => {
            handles
                .history
                .commit(head)
                .expect("head read")
                .expect("head commit")
                .root
        }
        None => {
            handles
                .history
                .layer(snapshot.base_layer)
                .expect("base layer read")
                .expect("base layer")
                .root
        }
    };
    let meta = CommitMeta {
        statistics: CommitStatistics {
            logical,
            ..CommitStatistics::ZERO
        },
        ..CommitMeta::new(CommitVia::Files)
    };
    match handles
        .history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: snapshot.head_commit,
            expected_base: snapshot.base_layer,
            expected_root,
            candidate_root,
            profile: profile_id(),
            scope: scope.object(),
            second_parent: None,
            meta,
            force: false,
        })
        .expect("commit")
    {
        CommitOutcome::Committed { .. } => {}
        other => panic!("commit: {other:?}"),
    }
    let branch_record = handles
        .history
        .branch(branch)
        .expect("branch read")
        .expect("branch");
    let stack_record = handles
        .history
        .layer_stack(stack)
        .expect("stack read")
        .expect("stack");
    match handles
        .history
        .add_layer(&AddLayerRequest {
            stack,
            branch,
            commit: branch_record.head_commit.expect("a committed Branch"),
            expected_stack_head: stack_record.head_layer,
            expected_branch_base: branch_record.base_layer,
        })
        .expect("add layer")
    {
        AddLayerOutcome::Added { .. } => {}
        other => panic!("add layer: {other:?}"),
    }
}

fn layers_per_stack(groups: u64, stacks: u64) -> Vec<u64> {
    let base = groups / stacks;
    let extra = groups % stacks;
    (0..stacks)
        .map(|index| base + u64::from(index < extra))
        .collect()
}

/// Reads the exact pack population straight from the Store file. The read-only
/// connection is closed before any compaction starts.
pub fn pack_stats(path: &Path) -> PackStats {
    let open = |flags| rusqlite::Connection::open_with_flags(path, flags);
    let connection = open(rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .or_else(|_| open(rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE))
        .expect("Store probe");
    let row: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT count(*), COALESCE(sum(length),0), COALESCE(min(length),0), \
             COALESCE(max(length),0) FROM pack",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("pack statistics");
    PackStats {
        count: u64::try_from(row.0).unwrap_or(0),
        total: u64::try_from(row.1).unwrap_or(0),
        min: u64::try_from(row.2).unwrap_or(0),
        max: u64::try_from(row.3).unwrap_or(0),
    }
}

pub fn physical_bytes(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// Physical bytes of the Store plus its WAL/journal sidecars.
pub fn path_bytes(path: &Path) -> u64 {
    let mut total = physical_bytes(path);
    if let Some(name) = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    {
        if let Some(parent) = path.parent() {
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let file = entry.file_name().to_string_lossy().into_owned();
                    if file.starts_with(&name) && file != name {
                        total = total
                            .saturating_add(entry.metadata().map(|meta| meta.len()).unwrap_or(0));
                    }
                }
            }
        }
    }
    total
}

/// Builds the live Store: the shared pool, the delta chains and the per-project
/// Layer history with their inode leaves.
pub fn build_live(store: &Path, spec: FixtureSpec) -> BuildReport {
    if let Err(error) = spec.validate() {
        panic!("gc_bench fixture spec: {error}");
    }
    let handles = create_store(store);
    let started = Instant::now();

    // 1. The shared content pool: saved once, then referenced by many projects
    //    and Layers.
    let mut shared = Vec::with_capacity(usize::try_from(spec.shared_objects).unwrap_or(0));
    let mut seed = 10_000_000u64;
    let mut left = spec.shared_objects;
    while left > 0 {
        let take = left.min(BATCH as u64);
        let payloads: Vec<Vec<u8>> = (0..take)
            .map(|offset| payload(seed + offset, spec.object_bytes))
            .collect();
        shared.extend(save_whole_files(&handles, &payloads));
        seed = seed.saturating_add(take);
        left -= take;
    }

    // 2. Delta chains. Chain `i` is referenced by leaf `i`; its depth varies so
    //    the report carries a histogram rather than one synthetic depth.
    let depth = spec.delta_depth.max(1);
    let mut chain_targets: Vec<ObjectId> = Vec::new();
    let mut delta_objects = 0u64;
    let mut chain_depth_counts = vec![0u64; usize::try_from(depth + 1).unwrap_or(2)];
    for chain in 0..spec.chains() {
        let edges = 1 + (chain % depth);
        let (target, objects) = publish_chain(&handles, chain, edges);
        chain_targets.push(target);
        delta_objects = delta_objects.saturating_add(objects);
        if let Some(slot) = chain_depth_counts.get_mut(usize::try_from(edges).unwrap_or(0)) {
            *slot = slot.saturating_add(1);
        }
    }

    let per_stack = layers_per_stack(spec.groups, spec.stacks);
    let mut fresh_objects = 0u64;
    let mut shared_references = 0u64;
    let mut history_commits = 0u64;
    let mut history_layers = 0u64;
    let mut leaf_index = 0u64;
    let mut fresh_seed = 0u64;
    let progress_every = (spec.groups / 20).max(1);

    for (stack_index, layers) in per_stack.iter().copied().enumerate() {
        let stack_index = stack_index as u64;
        let scope = scope_for_stack(stack_index);
        let stack = stack_id(stack_index);
        let mut branch = BranchId::from_authority([0; 16]);
        for layer in 0..layers {
            let chain_target = if leaf_index < spec.chains() {
                chain_targets
                    .get(usize::try_from(leaf_index).unwrap_or(usize::MAX))
                    .copied()
            } else {
                None
            };
            let fresh_slots = spec
                .group_size
                .saturating_sub(spec.shared_per_leaf())
                .saturating_sub(u64::from(chain_target.is_some()));
            let fresh_payloads: Vec<Vec<u8>> = (0..fresh_slots)
                .map(|offset| payload(fresh_seed + offset, spec.object_bytes))
                .collect();
            fresh_seed = fresh_seed.saturating_add(fresh_slots);
            fresh_objects = fresh_objects.saturating_add(fresh_slots);
            let mut roots: Vec<ObjectId> =
                Vec::with_capacity(usize::try_from(spec.group_size).unwrap_or(0));
            if !fresh_payloads.is_empty() {
                roots.extend(save_whole_files(&handles, &fresh_payloads));
            }
            if let Some(target) = chain_target {
                roots.push(target);
            }
            let shared_count = spec.shared_per_leaf();
            for slot in 0..shared_count {
                if shared.is_empty() {
                    break;
                }
                let index = ((leaf_index.saturating_mul(shared_count) + slot) % spec.shared_objects)
                    as usize;
                roots.push(shared[index.min(shared.len().saturating_sub(1))]);
                shared_references = shared_references.saturating_add(1);
            }
            let (_, root_id) = publish_tree(&handles, scope, layer, &roots);
            if layer == 0 {
                branch = initialize_project(&handles, stack_index, scope, root_id);
                history_layers = history_layers.saturating_add(1);
            } else {
                commit_and_publish(
                    &handles,
                    stack,
                    branch,
                    scope,
                    root_id,
                    spec.group_size
                        .saturating_mul(u64::try_from(spec.object_bytes).unwrap_or(0)),
                );
                history_commits = history_commits.saturating_add(1);
                history_layers = history_layers.saturating_add(1);
            }
            leaf_index = leaf_index.saturating_add(1);
            if leaf_index % progress_every == 0 || leaf_index == spec.groups {
                let written =
                    spec.live_payload_bytes().saturating_mul(leaf_index) / spec.groups.max(1);
                let elapsed = started.elapsed().as_secs_f64().max(1e-9);
                eprintln!(
                    "gc_bench: build live layers {}/{} ({:.3} GB, {:.1} MB/s)",
                    leaf_index,
                    spec.groups,
                    written as f64 / 1e9,
                    written as f64 / 1e6 / elapsed
                );
                let _ = std::io::stderr().flush();
            }
        }
    }

    let seconds = started.elapsed().as_secs_f64();
    let pack_before = pack_stats(store);
    drop(handles);
    let layers_min = per_stack.iter().copied().min().unwrap_or(0);
    let layers_max = per_stack.iter().copied().max().unwrap_or(0);
    BuildReport {
        seconds,
        pack_before,
        stacks: spec.stacks,
        layers_min,
        layers_max,
        history_commits,
        history_layers,
        fresh_objects,
        shared_objects: shared.len() as u64,
        shared_references,
        delta_chains: spec.chains(),
        delta_objects,
        max_chain_depth: depth,
        chains_by_depth: chain_depth_counts
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(edges, count)| (edges as u64, *count))
            .collect(),
    }
}

/// Saves `count` dead whole-file objects in batches.
fn add_dead(handles: &SqliteHandles, count: u64, seed_base: u64, object_bytes: usize) {
    let mut left = count;
    let mut seed = seed_base;
    while left > 0 {
        let take = left.min(BATCH as u64);
        let payloads: Vec<Vec<u8>> = (0..take)
            .map(|offset| payload(seed + offset, object_bytes))
            .collect();
        let _ = save_whole_files(handles, &payloads);
        seed += take;
        left -= take;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DeadShare {
    pub dead_bytes: u64,
    pub added_objects: u64,
    pub seconds: f64,
}

/// Adds dead whole-file objects until `dead_bytes * 1000 >= total * target_permille`.
/// `dead_bytes` is the current dead bound (0 when the Store is freshly
/// compacted). The function returns the measured dead bytes after the last add.
pub fn ensure_dead_share(
    store: &Path,
    target_permille: u64,
    dead_bytes: u64,
    seed_base: u64,
    object_bytes: usize,
) -> DeadShare {
    assert!(target_permille <= 1000, "dead share is a permille");
    let started = Instant::now();
    let mut dead = dead_bytes;
    let mut added = 0u64;
    let mut seed = seed_base;

    // Learn this fixture's exact one-object pack width instead of assuming the
    // pack framing: add one probe object and read the pack-total delta.
    let before = pack_stats(store);
    {
        let handles = reopen(store);
        add_dead(&handles, 1, seed, object_bytes);
    }
    seed = seed.saturating_add(1);
    added = added.saturating_add(1);
    let after = pack_stats(store);
    let unit = after.total.saturating_sub(before.total).max(1);
    dead = dead.saturating_add(after.total.saturating_sub(before.total));

    loop {
        let current = pack_stats(store);
        if current.total == 0
            || dead.saturating_mul(1000) >= current.total.saturating_mul(target_permille)
        {
            break;
        }
        let wanted = current.total.saturating_mul(target_permille) / 1000;
        let need = wanted.saturating_sub(dead).div_ceil(unit).max(1);
        let before_total = current.total;
        {
            let handles = reopen(store);
            add_dead(&handles, need, seed, object_bytes);
        }
        seed = seed.saturating_add(need);
        added = added.saturating_add(need);
        let after = pack_stats(store);
        dead = dead.saturating_add(after.total.saturating_sub(before_total));
        if added > 100_000_000 {
            panic!(
                "could not reach {target_permille} permille dead in a bounded number of objects"
            );
        }
    }
    DeadShare {
        dead_bytes: dead,
        added_objects: added,
        seconds: started.elapsed().as_secs_f64(),
    }
}
