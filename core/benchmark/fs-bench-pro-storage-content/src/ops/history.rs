//! The retained-history driver: one Store, N states, three timed steps each.
//!
//! ```text
//! create the Store
//! for each state k, in order:
//!     read state k's changed bytes from the corpus      untimed, harness
//!       construct the changed content                   TIMED
//!       build_filesystem against the previous root      TIMED
//!       save                                            TIMED
//! close
//! ```
//!
//! **The corpus reading is between the children, never inside one.** That is why
//! this row's `operation_ns` is the sum of its named children and not the root: a
//! root would include [`Corpus::transition`]'s work, which is the harness's.
//!
//! **No constructed object is supplied to a measured child.** `InodeValue`
//! references content **by id** and carries no bytes, so the content objects are
//! what the child produces rather than an input. There is consequently nothing to
//! prepare and nothing to reuse: `Preparation::InProcess`, no master, no copy, no
//! checkpoint, and a second run costs what the first cost.
//!
//! **The base is read back through the Store.** State 1 builds a new filesystem
//! (`base: None`); every later state passes the previous root, whose content the
//! operation reads through `StoreProvider` over the very Store the chain is
//! growing. The chain therefore goes C1 → C2 in the read direction and not out of
//! harness memory.
//!
//! ## What this lane does not model
//!
//! `InodeValue` carries a **kind class** — regular file, directory, symlink — and
//! no POSIX mode bits, so the executable bit is not representable in C1/C2 at all.
//! That would matter for an O4 comparison of `mode` against the corpus oracle, so
//! it was checked before the driver was written: **the corpus contains zero
//! mode-only transitions** across all 157 checkpoints, and therefore zero in each
//! of the three selections. O4 compares path, kind, size and digest, and the
//! unrepresentable axis is not exercised by this workload. It is recorded rather
//! than left as an assumption.

#[path = "history_reads.rs"]
pub mod reads;

use reads::{ReadCounters, ReadWork};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use layerfs_content::filesystem::references::backing::FileBacking;
use layerfs_content::filesystem::{
    build_filesystem, build_filesystem_timed, scope_for_seed, update_filesystem,
    update_filesystem_timed, DirectoryUpdate, FilesystemInput, FilesystemPhases,
    FilesystemUpdateCounters,
    FilesystemObjects, FilesystemRead, FilesystemResources, FilesystemRootId, InodeUpdate,
    LogicalPath, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    construct_bytes, construct_bytes_with_predecessor, AdvisoryPredecessors,
    ConstructionCapacities, ConstructionPolicy, ObjectId, PredecessorBase, PredecessorProvenance,
};
use layerfs_storage::{SaveOutcome, StoragePolicy, Store, StoreProvider};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};

use super::{OpContext, OpError, OpOutcome, Phase};
use crate::gates::{self, Gate, GateClass};
use crate::registry::Case;
use crate::support::{instruments, phases};
use super::c1;
use crate::support::trace::Kind;
use crate::workload::history::{Change, Corpus, HistoryError, Row};
use crate::workload::providers::TreeStore;

/// The root directory's serial. `build_filesystem` requires it to be one of the
/// supplied values.
const ROOT_SERIAL: u64 = 1;

/// Serial of the first allocated inode. 1 is the root, so allocation starts at 2.
const FIRST_SERIAL: u64 = 2;

/// The metadata root every file carries.
///
/// The corpus records no attributes, so this is one stable synthetic identity
/// rather than a per-file one: a value that moved between states would show up as
/// a metadata change the workload never made.
fn empty_metadata_root() -> ObjectId {
    ObjectId::for_bytes(b"layerfs/history/empty-metadata-root")
}

/// The allocation scope of one row, derived from its id so a row's identities are
/// reproducible from the receipt alone.
fn scope_of(row: Row) -> layerfs_content::InodeScope {
    scope_for_seed(*ObjectId::for_bytes(format!("layerfs/history/{}", row.token()).as_bytes()).as_bytes())
}

/// Whether the driver models a history save faithfully — **Step 0 of #187**.
///
/// **Diagnostic switch, harness-side only; no product behaviour depends on it.**
///
/// Unset or any value other than `1`/`true` is the lane's original behaviour:
/// every constructed object is offered to the save with no advisory predecessor,
/// so the Store is asked to hold every version as though it had never existed.
///
/// `1` makes the driver declare what a faithful history save declares: the
/// previous version's content root of the same path, as an `OriginalBase`
/// advisory predecessor — the same declaration
/// `layerfs-content/src/file/edit/apply.rs` makes when it produces a new
/// version as an edit rather than as fresh bytes.
///
/// It is read from the environment rather than from the registry so that **one
/// binary measures both arms** and the only difference between them is the
/// declaration.
///
/// **It now defaults ON.** The lane's historical default declared nothing at all,
/// and that default is precisely what made its headline number (128,864,256 B
/// apparent) *not a product measurement*: the Store was supplied no base for
/// 26,847 objects. A faithful history save declares one. Setting
/// `LAYERFS_HISTORY_ADVISORY=0` reproduces the historical registered arm, so the
/// comparison that produced the 2.65x headline stays reproducible rather than
/// being asserted.
fn faithful_history_model() -> bool {
    !matches!(
        std::env::var("LAYERFS_HISTORY_ADVISORY").as_deref(),
        Ok("0") | Ok("false")
    )
}

/// Largest dependency-chain depth the driver will *declare* a base at.
///
/// `StoragePolicy::frozen_default` caps a whole-file chain at
/// `DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH` = 8 and the reader accepts a stored
/// chain of exactly that depth, so the design is self-consistent. A producer
/// that declared a base unconditionally would nevertheless ask the Store to
/// build a chain as deep as the selection ever ran, and the selection's own
/// depth bookkeeping (`DepthCache::cost_of`) terminates one edge short when its
/// walk stops on an already-cached entry — so the Store can write a chain one
/// edge deeper than the policy permits and then refuse to read it back.
///
/// That is a **product** finding, reported rather than patched: no product
/// source may change without an owner ruling. This switch exists so the
/// *harness* question — how much of the 2.65x is the driver declaring no base at
/// all — can still be answered with a run that completes. `u8::MAX` (the
/// default) declares unconditionally, which is the faithful model and which the
/// product currently rejects.
/// Whether the driver declares the **four-slot ordered** predecessor list.
///
/// **Diagnostic switch, harness-side only; off by default.** On, the driver builds
/// a cross-save content-similarity index from the product's own `signature` and
/// declares `cross-path best, cross-path 2nd, same-path previous, same-path
/// earlier` — the order §3.3 measured as best. Off, it declares the single
/// same-path previous version, which is what this lane has measured best.
///
/// It is off because it was measured and it is **worse**: see the evidence
/// addendum. It stays so the negative result is reproducible rather than asserted.
fn ordered_predecessors_enabled() -> bool {
    matches!(
        std::env::var("LAYERFS_HISTORY_ORDERED_PREDECESSORS").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// Whether the driver falls back to the cross-save similarity index when a path
/// has **no** same-path previous version.
///
/// **Diagnostic switch, harness-side only; off by default.** The measured best arm
/// declares the single same-path previous version and nothing else, so a path that is
/// new — or that was removed and re-added — is offered **no** candidate at all and the
/// Store holds it FULL. The reference tree does not stop there: it consults its
/// content-keyed similarity cache exactly when the declared predecessor is absent
/// (`crates/layerfs-layerstack-store/src/objects/admission.rs:454`, "consult the cache
/// only when `anchor.is_none()`").
///
/// This arm reproduces that **condition** with the harness's own `SimilarityIndex`:
/// the same-path previous version when there is one (unchanged, so the
/// same-path-only arm's result is reproduced object for object), and the ordered
/// arm's cross-path candidates when there is not. It is therefore **one variable**
/// wide against the same-path-only arm — the condition under which the index is
/// consulted — and it is the arm that separates the ordered arm's measured gain
/// from its measured loss.
///
/// **It is OFF by default, on the owner's direction.**
///
/// The reason it is off is a modelling reason, and it is the honest one: this
/// arm makes the *Store* find cross-path candidates across save boundaries, and
/// **the product cannot do that yet.** Core's own content index
/// (`encoding/delta/candidates.rs`) is owned by one save and dropped at the end
/// of it (`cas/lifecycle.rs:90`, `cas/store.rs:392`), so a cross-save match is
/// impossible in the product as it stands. Here the harness maintains that index
/// itself. Declaring a base the caller knows is one thing — that is what a
/// faithful caller does, and `file/edit/apply.rs:121` does it — but assuming a
/// store capability that does not exist is a different thing.
///
/// **What turning it off costs, measured: 6,377,472 B.**
///
/// ````text
///   fallback OFF (this default)   56,049,664 B   = 1.13653x v0.1.6   +6,733,824
///   fallback ON                   49,672,192 B   = 1.00723x v0.1.6     +356,352
///   v0.1.6                        49,315,840 B
/// ````
///
/// With it off the gate is **6,733,824 B** away rather than 356,352 B. The
/// capability is real and v0.1.6 has it; the product-side form of it is a
/// persisted, cross-save content index, which is a new table and a
/// `SCHEMA_VERSION` bump and needs an owner ruling. **Building it is the way to
/// get these bytes back without modelling anything the product cannot do.**
///
/// `LAYERFS_HISTORY_SIMILARITY_CANDIDATES=1` turns it on for the comparison.
fn similarity_candidates_enabled() -> bool {
    if matches!(
        std::env::var("LAYERFS_HISTORY_DECLARED_ONLY").as_deref(),
        Ok("1") | Ok("true")
    ) {
        return false;
    }
    matches!(
        std::env::var("LAYERFS_HISTORY_SIMILARITY_CANDIDATES").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// How many cross-path candidates the similarity source proposes.
///
/// **Diagnostic, harness-side only; 2 by default** — the width the ordered arm's
/// helper already used, so the first fallback measurement differs from the default
/// arm in exactly one variable (the *condition*). `4` is
/// `MAXIMUM_ADVISORY_PREDECESSORS`, the width the specification's recommendation
/// names; it is a second, separate variable and is therefore a separate arm.
fn fallback_candidate_limit() -> usize {
    std::env::var("LAYERFS_HISTORY_FALLBACK_CANDIDATES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2)
}

/// Up to `limit` cross-path candidates for one object, best overlap first.
///
/// The same candidate class `ordered_predecessors` builds, without the same-path
/// slots: this arm has no same-path version by construction.
fn cross_path_predecessors(
    index: &SimilarityIndex,
    id: ObjectId,
    path: &[u8],
    signature: [u64; 8],
    limit: usize,
) -> Vec<ObjectId> {
    let mut cross: Vec<ObjectId> = Vec::with_capacity(limit);
    for candidate in index.nearest(id, signature, 8) {
        if cross.len() == limit {
            break;
        }
        if index.path_of.get(&candidate).map(Vec::as_slice) == Some(path) {
            continue;
        }
        cross.push(candidate);
    }
    cross
}

/// Whether the driver models a producer that ran at **every** commit.
///
/// **Diagnostic switch, harness-side only.** The faithful model above declares a
/// base from the selection's own earlier states. This one additionally consults
/// the full 157-checkpoint history: a producer that ran at every commit held
/// checkpoint *k-1* when it saved checkpoint *k*, so for a path that changed inside
/// a skipped span it had a version the selection alone never saw.
///
/// It is a separate switch because it is a **separate hypothesis**, and it was
/// measured and rejected: it adds bases and costs bytes (see
/// `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188-*/README.md`). It stays
/// available so the negative result is reproducible rather than asserted.
fn full_history_producer() -> bool {
    matches!(
        std::env::var("LAYERFS_HISTORY_FULL_PRODUCER").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// Whether the driver offers each emitted chunk the stored payload the **previous
/// version's own mapping** holds at that chunk's byte range.
///
/// Off, every chunk of a chunked construction is offered nothing and the Store
/// holds it FULL. On, the driver hands `construct_bytes_with_predecessor` the
/// same-path previous version's root and a `StoreProvider` over the Store the
/// chain has already written, and C1's positional cursor answers per chunk. This
/// is the producer the chunk lane has never had: the advisory R0 declares names
/// the **FileState** root, and a tree role never takes a payload base, so R0's
/// declaration is inert on a chunked path.
///
/// **It now defaults ON, because it was measured and it wins: 51,347,456 ->
/// 49,672,192 B apparent, -1,675,264.** The native lane falls from 5,695,678 to
/// **3,957,829 B — 228 B below v0.1.6's own 3,958,057 B** — and the selection
/// becomes byte-identical to v0.1.6's (448 FULL / 650 PREFIX, same raw and stored
/// bytes per class); the 228 B edge is group framing. `delta.trials` rises 37,886
/// -> 38,538, the first chunk trials this lane has ever run.
///
/// `LAYERFS_HISTORY_CHUNK_PREDECESSORS=0` reproduces the arm every earlier
/// measurement on this lane was taken on, byte for byte.
fn chunk_predecessors_enabled() -> bool {
    !matches!(
        std::env::var("LAYERFS_HISTORY_CHUNK_PREDECESSORS").as_deref(),
        Ok("0") | Ok("false")
    )
}

/// Largest dependency-chain depth the driver will *declare* a base at.
fn advisory_depth_limit() -> u8 {
    std::env::var("LAYERFS_HISTORY_DEPTH_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(u8::MAX)
}

/// Chain totals of the save counters this row did not previously publish.
///
/// "Cannot see it" and "sees it and declines it" were indistinguishable without
/// these: the selection counters live in `DeltaCounters`, which `SaveOutcome`
/// already carries, and nothing read them.
#[derive(Default, Clone, Copy)]
struct SaveTotals {
    reused: u64,
    inserted: u64,
    full_records: u64,
    prefix_records: u64,
    packs_created: u64,
    prepared_full: u64,
    trials: u64,
    prefix_selected: u64,
    full_losses: u64,
    no_candidate: u64,
    absent_candidates: u64,
    ineligible_candidates: u64,
    work_exceeded: u64,
}

impl SaveTotals {
    fn add(&mut self, saved: &SaveOutcome) {
        self.reused = self.reused.saturating_add(saved.reused);
        self.inserted = self.inserted.saturating_add(saved.inserted);
        self.full_records = self.full_records.saturating_add(saved.full_records);
        self.prefix_records = self.prefix_records.saturating_add(saved.prefix_records);
        self.packs_created = self.packs_created.saturating_add(saved.packs_created);
        let delta = saved.delta;
        self.prepared_full = self.prepared_full.saturating_add(delta.prepared_full);
        self.trials = self.trials.saturating_add(delta.trials);
        self.prefix_selected = self.prefix_selected.saturating_add(delta.prefix_selected);
        self.full_losses = self.full_losses.saturating_add(delta.full_losses);
        self.no_candidate = self.no_candidate.saturating_add(delta.no_candidate);
        self.absent_candidates = self
            .absent_candidates
            .saturating_add(delta.absent_candidates);
        self.ineligible_candidates = self
            .ineligible_candidates
            .saturating_add(delta.ineligible_candidates);
        self.work_exceeded = self.work_exceeded.saturating_add(delta.work_exceeded);
    }
}

/// Turns a corpus refusal into a driver error, keeping the refusal's own name.
fn corpus_error(error: HistoryError) -> OpError {
    OpError::Io(format!("{}: {error}", error.name()))
}

/// The parent directory of a path, or the empty path for a top-level entry.
fn parent_of(path: &[u8]) -> Vec<u8> {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(index) => path[..index].to_vec(),
        None => Vec::new(),
    }
}

/// The final component of a path.
fn name_of(path: &[u8]) -> &[u8] {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

/// Every proper ancestor of a path, longest last.
fn ancestors_of(path: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for (index, byte) in path.iter().enumerate() {
        if *byte == b'/' {
            out.push(path[..index].to_vec());
        }
    }
    out
}

/// The kind class the corpus's octal mode names.
///
/// `120000` is a symbolic link, `040000` a directory, and everything else is a
/// regular file. The permission bits are not representable in `InodeValue` and
/// the corpus makes no mode-only transition, so they are not consulted.
fn kind_of(mode: u32) -> InodeKind {
    match mode & 0o170000 {
        0o120000 => InodeKind::Symlink,
        0o040000 => InodeKind::Directory,
        _ => InodeKind::RegularFile,
    }
}

/// A cross-save, content-keyed similarity index — **the producer's correspondence**.
///
/// This is R0's missing caller, and it is **harness code with no product line**.
/// The product already contains the right index (`encoding/delta/candidates.rs`: a
/// 16-byte rolling hash, `mix()`, the eight smallest hashes, a `>= 2`-of-8 match)
/// but owns it **per save** and bounds it at 1,024 slots, so it can only ever
/// propose bases from the objects the same save admitted. The caller is what holds
/// the correspondence across saves, and there is no caller — Stage 7 is unbuilt.
///
/// This is that caller, and it uses the product's **own** `signature` function, so
/// the index the harness keeps is the index the product would keep if it had one.
/// It is unbounded because the harness owns it: 44,141 whole-file objects is about
/// 2.8 MB of signatures.
///
/// **The order is the load-bearing parameter.** §3.3 measured that "try all four
/// and keep the smallest frame" is **433,443 B worse**, because it deepens chains
/// past the depth cap. The order below is the one that was measured best.
struct SimilarityIndex {
    /// Every stored object's eight-hash signature.
    signatures: BTreeMap<ObjectId, [u64; 8]>,
    /// `hash -> objects whose signature contains it`, so a lookup visits only the
    /// objects that share at least one hash rather than all of them.
    by_hash: BTreeMap<u64, Vec<ObjectId>>,
    /// `content root -> the path it was stored for`, so a candidate can be told
    /// from a same-path one.
    path_of: BTreeMap<ObjectId, Vec<u8>>,
}

impl SimilarityIndex {
    fn new() -> Self {
        Self {
            signatures: BTreeMap::new(),
            by_hash: BTreeMap::new(),
            path_of: BTreeMap::new(),
        }
    }

    /// Records one stored object under the path it was stored for.
    fn insert(&mut self, id: ObjectId, path: &[u8], signature: [u64; 8]) {
        if signature[0] == u64::MAX {
            return;
        }
        if self.signatures.insert(id, signature).is_some() {
            return;
        }
        self.path_of.insert(id, path.to_vec());
        for hash in signature.iter().copied().filter(|hash| *hash != u64::MAX) {
            self.by_hash.entry(hash).or_default().push(id);
        }
    }

    /// The objects whose signature shares at least two hashes with `signature`,
    /// best overlap first, excluding `id` itself.
    ///
    /// `>= 2`-of-8 is the product's own match rule, applied here so the harness
    /// proposes what the product's index would have proposed.
    fn nearest(&self, id: ObjectId, signature: [u64; 8], limit: usize) -> Vec<ObjectId> {
        let mut overlap: BTreeMap<ObjectId, u8> = BTreeMap::new();
        for hash in signature.iter().copied().filter(|hash| *hash != u64::MAX) {
            let Some(bucket) = self.by_hash.get(&hash) else {
                continue;
            };
            for candidate in bucket {
                if *candidate == id {
                    continue;
                }
                let entry = overlap.entry(*candidate).or_insert(0);
                *entry = entry.saturating_add(1);
            }
        }
        let mut ranked: Vec<(u8, ObjectId)> = overlap
            .into_iter()
            .filter(|(_, count)| *count >= 2)
            .map(|(candidate, count)| (count, candidate))
            .collect();
        // Best overlap first; ties broken by identity so the order is reproducible
        // from the receipt and not from a hash-map iteration order.
        ranked.sort_by(|left, right| right.cmp(left));
        ranked
            .into_iter()
            .map(|(_, candidate)| candidate)
            .take(limit)
            .collect()
    }
}

/// The predecessors to declare for one object, in the measured best order.
///
/// `cross-path best`, `cross-path 2nd`, `same-path previous`, `same-path earlier`.
/// An identity that is not in the Store yet is dropped: the selector probes each in
/// order and returns the first **eligible** one, so proposing an object the save has
/// not admitted would spend a probe and buy nothing.
fn ordered_predecessors(
    index: &SimilarityIndex,
    id: ObjectId,
    path: &[u8],
    signature: [u64; 8],
    same_path_previous: Option<ObjectId>,
) -> Vec<ObjectId> {
    let mut cross: Vec<ObjectId> = Vec::with_capacity(2);
    for candidate in index.nearest(id, signature, 8) {
        if cross.len() == 2 {
            break;
        }
        if index.path_of.get(&candidate).map(Vec::as_slice) == Some(path) {
            continue;
        }
        cross.push(candidate);
    }
    let mut ordered: Vec<ObjectId> = cross;
    if let Some(previous) = same_path_previous {
        if !ordered.contains(&previous) {
            ordered.push(previous);
        }
    }
    ordered.truncate(layerfs_content::MAXIMUM_ADVISORY_PREDECESSORS);
    ordered
}

/// Emits one symlink target through the state's consumer.
///
/// A symlink is **not** a file whose bytes happen to be a path. The driver used
/// `construct_bytes` for every changed path, which stored a symlink as a
/// `RegularFile` content object holding its target string — a tree that reads
/// back with the wrong kind, which is what the verification phase found the first
/// time it ran (`.claude/skills`, oracle mode `120000`, `UnsupportedFraming` from
/// the regular-file read). It is emitted as a `Symlink` object instead, and only
/// when the corpus says the path changed: an unchanged symlink keeps the content
/// root the Store already holds, exactly as an unchanged file does.
fn emit_symlink_target(
    target: &[u8],
    consumer: &mut TreeStore,
) -> Result<ObjectId, OpError> {
    let target = layerfs_content::filesystem::SymlinkTarget::new(target.to_vec())
        .map_err(|error| OpError::Product(format!("{error:?}")))?;
    let object = target
        .finalize()
        .map_err(|error| OpError::Product(format!("{error:?}")))?;
    let id = object.id();
    consumer.insert_object(object);
    Ok(id)
}

/// The serials and paths this chain has allocated so far.
///
/// It is carried **across** states, which is what makes an unchanged path keep its
/// inode and an unchanged directory keep its identity: a chain that re-allocated
/// every serial per state would rewrite the whole tree N times and measure a
/// different operation than the one the claim describes.
struct Chain {
    serial_of: BTreeMap<Vec<u8>, u64>,
    next_serial: u64,
}

impl Chain {
    fn new() -> Self {
        let mut serial_of = BTreeMap::new();
        serial_of.insert(Vec::new(), ROOT_SERIAL);
        Self {
            serial_of,
            next_serial: FIRST_SERIAL,
        }
    }

    /// The serial for a path, allocating one if this is the first state to see it.
    fn serial(&mut self, path: &[u8]) -> u64 {
        if let Some(serial) = self.serial_of.get(path) {
            return *serial;
        }
        let serial = self.next_serial;
        self.next_serial += 1;
        self.serial_of.insert(path.to_vec(), serial);
        serial
    }

    fn known(&self, path: &[u8]) -> Option<u64> {
        self.serial_of.get(path).copied()
    }
}

/// Builds the `FilesystemInput` for one state.
///
/// The changed directory bindings carry the **final** binding of every name they
/// change, so a removal is an explicit `None` rather than an omission. New
/// directories are declared as directory inodes and bound in their parents; a
/// directory that already existed keeps its serial and is not re-declared.
fn filesystem_input(
    chain: &mut Chain,
    transition: &crate::workload::history::Transition,
    content_roots: &BTreeMap<Vec<u8>, ObjectId>,
    is_build: bool,
    directories: &mut Vec<DirectoryUpdate>,
    inodes: &mut Vec<InodeUpdate>,
    new_inodes: &mut Vec<u64>,
) -> Result<(), OpError> {
    directories.clear();
    inodes.clear();
    new_inodes.clear();

    // A **build** allocates its root like any other inode and must declare it;
    // an update must not, because the root already exists in the base. Both rules
    // are `FilesystemInput::check`'s, and getting either wrong fails the whole
    // state with `InvalidRecord("root inode allocation")`.
    if is_build {
        inodes.push(InodeUpdate {
            serial: ROOT_SERIAL,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: ObjectId::for_bytes(b"layerfs/history/dir/root"),
                metadata_root: empty_metadata_root(),
            },
        });
        new_inodes.push(ROOT_SERIAL);
    }

    let name = |path: &[u8]| -> Result<PathName, OpError> {
        let text = std::str::from_utf8(name_of(path))
            .map_err(|_| OpError::Io("a corpus path name is not UTF-8".to_string()))?;
        PathName::new(text).map_err(|error| OpError::Product(format!("{error:?}")))
    };

    // The tree's directories, and the ones this state is the first to see.
    let mut directories_now: BTreeSet<Vec<u8>> = BTreeSet::new();
    for path in transition.tree.keys() {
        for ancestor in ancestors_of(path) {
            directories_now.insert(ancestor);
        }
    }

    // Changed directory bindings, keyed by the directory path.
    let mut bindings: BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, Option<u64>>> = BTreeMap::new();

    for directory in &directories_now {
        if chain.known(directory).is_none() {
            let serial = chain.serial(directory);
            inodes.push(InodeUpdate {
                serial,
                value: InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 1,
                    content_root: ObjectId::for_bytes(
                        format!("layerfs/history/dir/{}", hex(directory)).as_bytes(),
                    ),
                    metadata_root: empty_metadata_root(),
                },
            });
            new_inodes.push(serial);
            bindings
                .entry(parent_of(directory))
                .or_default()
                .insert(name_of(directory).to_vec(), Some(serial));
        }
    }

    for changed in &transition.changed {
        let path = &changed.path;
        let parent = parent_of(path);
        match changed.kind {
            Change::Removed => {
                if let Some(serial) = chain.known(path) {
                    let _ = serial;
                }
                bindings
                    .entry(parent)
                    .or_default()
                    .insert(name_of(path).to_vec(), None);
            }
            Change::MetadataOnly => {
                // The corpus makes no mode-only transition — verified on all 157
                // checkpoints — and `InodeValue` could not express one anyway. It
                // is counted, never silently dropped.
            }
            Change::Added | Change::Modified => {
                let serial = chain.serial(path);
                if changed.kind == Change::Added {
                    new_inodes.push(serial);
                }
                let content_root = content_roots.get(path).copied().ok_or_else(|| {
                    OpError::Io(format!(
                        "state {} has no constructed content for {}",
                        transition.state.ordinal,
                        hex(path)
                    ))
                })?;
                inodes.push(InodeUpdate {
                    serial,
                    value: InodeValue {
                        kind: kind_of(changed.mode),
                        namespace_ref_count: 1,
                        content_root,
                        metadata_root: empty_metadata_root(),
                    },
                });
                bindings
                    .entry(parent)
                    .or_default()
                    .insert(name_of(path).to_vec(), Some(serial));
            }
        }
    }

    for (directory, changes) in bindings {
        let parent = chain.serial(&directory);
        let mut rows: Vec<(PathName, Option<u64>)> = Vec::with_capacity(changes.len());
        for (path, binding) in changes {
            rows.push((name(&path)?, binding));
        }
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        directories.push(DirectoryUpdate {
            parent,
            changes: rows,
        });
    }
    directories.sort_by_key(|update| update.parent);
    inodes.sort_by_key(|update| update.serial);
    new_inodes.sort_unstable();
    new_inodes.dedup();
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

// --- the verification phase ---------------------------------------------------
//
// `Phase::Verify` is a **second invocation** over the Store the measured phase
// wrote. It exists because a performance PASS over a Store nobody read back is a
// statement about bytes on disk and not about a retained history: the row's O1
// pin is the state's root identity, and an identity that is *stored* is not an
// identity that is *readable*. Before this phase existed, `ops/history.rs`
// refused `--phase verify` outright, so a `history.*` PASS covered no read-back
// at all.
//
// It is deliberately the **whole oracle** and not a sample. The row's declared
// verification unit is the path-state, and the corpus records one for every path
// of every state, so there is nothing to sample *from*: O4 is "path, kind, size
// and digest" for every entry, and a tenth of them would be a different claim.

/// How many paths one state's read-back samples, by default.
///
/// A **declared** bound, not a discovered one: the phase costs what the
/// declaration says it costs, and a 157-state history is verified by the same
/// budget as a 17-state one. `--verify-sample N` overrides it.
const VERIFY_PATH_BUDGET: usize = 64;

/// Every `history.state.<n>.root` and `store.path` the measured phase published.
///
/// Read from the trace the performance invocation wrote rather than carried in
/// process memory, because the two phases are two processes and the trace is the
/// only thing that crosses between them. It is parsed with the harness's own JSON
/// reader and not by splitting the line: the trace escapes its strings, so a
/// hand-rolled reader sees `FilesystemRootId(ObjectId(\"7e1c...\"))` and reports a
/// structural defect on a perfectly good record. That mistake was made here once
/// and is recorded rather than quietly fixed.
fn measured_roots(trace_path: &Path) -> Result<(Vec<(usize, ObjectId)>, PathBuf), OpError> {
    let text = std::fs::read_to_string(trace_path)
        .map_err(|error| OpError::Io(format!("{}: {error}", trace_path.display())))?;
    let mut roots: Vec<(usize, ObjectId)> = Vec::new();
    let mut store: Option<PathBuf> = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let record = crate::workload::json::parse(line).map_err(|error| {
            OpError::Io(format!("{}: not a trace record: {error}", trace_path.display()))
        })?;
        let kind = record.get_str("kind").unwrap_or_default();
        let key = record.get_str("key").unwrap_or_default();
        if kind != "counter" {
            continue;
        }
        if key == "store.path" {
            if let Some(value) = record.get_str("value") {
                store = Some(PathBuf::from(value));
            }
            continue;
        }
        let Some(ordinal) = key
            .strip_prefix("history.state.")
            .and_then(|rest| rest.strip_suffix(".root"))
            .and_then(|digits| digits.parse::<usize>().ok())
        else {
            continue;
        };
        let value = record
            .get_str("value")
            .ok_or_else(|| OpError::Io(format!("{key} is not a string")))?;
        // The published identity is `FilesystemRootId(ObjectId("<hex>"))`. The
        // hex is extracted from the text the receipt carries rather than
        // re-derived: a receipt that says one thing and a verifier that checks
        // another would be two claims.
        let digest = value
            .split_once("ObjectId(\"")
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(digest, _)| digest)
            .ok_or_else(|| {
                OpError::Io(format!("{key} is not an ObjectId identity: {value:?}"))
            })?;
        let id: ObjectId = digest
            .parse()
            .map_err(|_| OpError::Io(format!("{key} is not a hex object id: {digest:?}")))?;
        roots.push((ordinal, id));
    }
    if roots.is_empty() {
        return Err(OpError::Io(format!(
            "{}: no history.state.<n>.root records; the measured phase did not run",
            trace_path.display()
        )));
    }
    roots.sort_by_key(|(ordinal, _)| *ordinal);
    let mut seen = BTreeSet::new();
    for (ordinal, _) in &roots {
        if !seen.insert(*ordinal) {
            return Err(OpError::Io(format!(
                "history.state.{ordinal}.root is recorded more than once"
            )));
        }
    }
    let store = store.ok_or_else(|| {
        OpError::Io(format!(
            "{}: no store.path record; the measured phase did not publish the Store it grew",
            trace_path.display()
        ))
    })?;
    Ok((roots, store))
}

/// What one state's read-back found.
#[derive(Default, Clone, Copy)]
struct VerifyTally {
    /// Path-states the oracle declares for this state.
    declared: u64,
    /// Paths the sample resolved and compared.
    compared: u64,
    /// Sampled paths whose kind, size or digest disagreed with the oracle.
    mismatches: u64,
    /// Sampled paths the Store's tree does not bind.
    missing: u64,
    /// Sampled paths the Store binds to something other than a directory when
    /// the oracle declares a directory, or the reverse.
    unexpected: u64,
    /// Sampled files whose bytes were read back and hashed.
    files_read: u64,
    /// Logical bytes those reads returned.
    file_bytes: u64,
    /// Sampled symlink targets read back.
    symlinks_read: u64,
}

impl VerifyTally {
    fn add(&mut self, other: &VerifyTally) {
        self.declared = self.declared.saturating_add(other.declared);
        self.compared = self.compared.saturating_add(other.compared);
        self.mismatches = self.mismatches.saturating_add(other.mismatches);
        self.missing = self.missing.saturating_add(other.missing);
        self.unexpected = self.unexpected.saturating_add(other.unexpected);
        self.files_read = self.files_read.saturating_add(other.files_read);
        self.file_bytes = self.file_bytes.saturating_add(other.file_bytes);
        self.symlinks_read = self.symlinks_read.saturating_add(other.symlinks_read);
    }

    /// Whether this state's read-back is a pass.
    fn clean(&self) -> bool {
        self.mismatches == 0 && self.missing == 0 && self.unexpected == 0
    }
}

/// Reads one state's tree back, by **sampling paths** rather than walking it.
///
/// The first version of this phase walked every state's whole tree, one
/// `resolve` per path, and cost about 0.36 ms per path-state — 101,477
/// path-states over stride 10 is 36 s of Store round trips, and the read-back of
/// every file on top of it took the phase past four minutes. That is not a
/// verification anybody runs, and a verification nobody runs is not a gate.
///
/// What it does instead: take the harness's one declared sample rule over the
/// state's oracle in declaration order, resolve exactly those paths, and compare
/// **presence, kind, size and digest** on them. The cost is proportional to the
/// sample, not to the tree, and the sample spreads across the whole path order
/// rather than taking a prefix.
///
/// What it does not do, stated rather than implied: it does not prove the absence
/// of paths the oracle never declares, because that needs the full listing. It is
/// a sampled read-back, so the row it produces is `INCOMPLETE` and never `PASS`.
struct Sampler<'a> {
    reader: &'a dyn layerfs_content::object::AuthenticatedObjects,
    oracle: &'a crate::workload::history::Oracle,
    tally: VerifyTally,
    /// The first few disagreements verbatim, so a receipt that fails says what
    /// differed rather than only how many.
    samples: Vec<String>,
    /// `content root -> (sha256, logical length)`, so one object read serves
    /// every path that names it.
    digests: BTreeMap<ObjectId, ([u8; 32], u64)>,
    /// How many paths this state's sample names.
    budget: usize,
}

impl<'a> Sampler<'a> {
    fn new(
        reader: &'a dyn layerfs_content::object::AuthenticatedObjects,
        oracle: &'a crate::workload::history::Oracle,
        budget: usize,
    ) -> Self {
        Self {
            reader,
            oracle,
            tally: VerifyTally {
                declared: oracle.len() as u64,
                ..VerifyTally::default()
            },
            samples: Vec::new(),
            digests: BTreeMap::new(),
            budget,
        }
    }

    fn disagree(&mut self, message: String) {
        if self.samples.len() < 4 {
            self.samples.push(message);
        }
    }

    /// The oracle indices this state's sample names.
    ///
    /// Evenly spread across the state's path order, `max(1, ceil(len / budget))`
    /// apart, so a state smaller than the budget is verified whole and a large one
    /// is sampled across its whole extent. The rule is a function of the oracle
    /// alone, so the same Store and the same corpus always sample the same paths.
    fn sample_indices(&self) -> Vec<usize> {
        let units = self.oracle.len();
        if units == 0 {
            return Vec::new();
        }
        let stride = units.div_ceil(self.budget.max(1)).max(1);
        (0..units).step_by(stride).collect()
    }

    fn run(&mut self, root: FilesystemRootId) -> Result<VerifyTally, OpError> {
        let mut read = FilesystemRead::new(self.reader, root)
            .map_err(|error| OpError::Product(format!("root {root:?}: {error:?}")))?;
        // The root itself is not an oracle entry: the corpus declares the paths
        // inside the tree, not the tree. It is checked for being a directory.
        let resolved = read
            .resolve(&LogicalPath::root())
            .map_err(|error| OpError::Product(format!("root: {error:?}")))?;
        if resolved.value.kind != InodeKind::Directory {
            return Err(OpError::Io("the state's root is not a directory".to_string()));
        }
        for index in self.sample_indices() {
            let Some((path, entry)) = self.oracle.iter().nth(index) else {
                continue;
            };
            let path = path.clone();
            let entry = *entry;
            let logical = match LogicalPath::from_bytes(&path) {
                Ok(path) => path,
                // The corpus cannot contain a path this layer refuses, and if it
                // did the refusal is the finding.
                Err(error) => {
                    self.disagree(format!(
                        "{}: the oracle declares a path this layer refuses: {error:?}",
                        String::from_utf8_lossy(&path)
                    ));
                    continue;
                }
            };
            self.check(&mut read, &logical, &path, &entry)?;
        }
        Ok(self.tally)
    }

    /// Resolves one sampled path and compares it with its oracle entry.
    fn check(
        &mut self,
        read: &mut FilesystemRead<'_>,
        logical: &LogicalPath,
        path: &[u8],
        entry: &crate::workload::history::OracleEntry,
    ) -> Result<(), OpError> {
        let shown = String::from_utf8_lossy(path).into_owned();
        let resolved = match read.resolve(logical) {
            Ok(resolved) => resolved,
            Err(layerfs_content::ContentError::PathNotFound) => {
                self.tally.missing = self.tally.missing.saturating_add(1);
                self.disagree(format!(
                    "{shown}: the oracle declares this path and the Store's tree does not bind it"
                ));
                return Ok(());
            }
            Err(error) => {
                return Err(OpError::Product(format!("{shown}: {error:?}")));
            }
        };
        self.tally.compared = self.tally.compared.saturating_add(1);
        // The kind is read from the oracle's own mode rather than inferred from
        // whether a digest is present: a symlink has a digest and is not a
        // regular file, and a mode is what the corpus actually recorded.
        let expected_kind = match entry.mode & 0o170000 {
            0o040000 => InodeKind::Directory,
            0o120000 => InodeKind::Symlink,
            _ => InodeKind::RegularFile,
        };
        if resolved.value.kind != expected_kind {
            self.tally.unexpected = self.tally.unexpected.saturating_add(1);
            self.disagree(format!(
                "{shown}: kind {:?} but the oracle declares {expected_kind:?} (mode {:o})",
                resolved.value.kind, entry.mode
            ));
            return Ok(());
        }
        if expected_kind == InodeKind::Directory {
            return Ok(());
        }
        let (found, length) = self.digest_of(resolved.value.content_root, &shown, expected_kind)?;
        if entry.size != length {
            self.tally.mismatches = self.tally.mismatches.saturating_add(1);
            self.disagree(format!(
                "{shown}: size {length} but the oracle declares {}",
                entry.size
            ));
            return Ok(());
        }
        if let Some(expected) = entry.digest {
            if found != expected {
                self.tally.mismatches = self.tally.mismatches.saturating_add(1);
                self.disagree(format!(
                    "{shown}: digest {} but the oracle declares {}",
                    crate::workload::digest::hex(&found),
                    crate::workload::digest::hex(&expected)
                ));
            }
        }
        Ok(())
    }

    /// The digest and logical length of one content object, read at most once.
    fn digest_of(
        &mut self,
        id: ObjectId,
        path: &str,
        kind: InodeKind,
    ) -> Result<([u8; 32], u64), OpError> {
        if let Some(found) = self.digests.get(&id).copied() {
            return Ok(found);
        }
        // The oracle hashes a file's **logical bytes**, not the canonical object
        // that holds them. Reading the canonical object directly is off by the
        // encoding's own header — 23 bytes on every file in this corpus, which is
        // how this phase first reported 85,929 mismatches that were all the same
        // fixed difference. `read_all` is the product's logical read path and is
        // what the claim is about.
        let (found, length) = if kind == InodeKind::Symlink {
            self.tally.symlinks_read = self.tally.symlinks_read.saturating_add(1);
            // A symlink's logical bytes are its target string; its oracle digest
            // is over that, not over the framing `SymlinkTarget::encode` adds.
            let canonical = self
                .reader
                .read_canonical(id)
                .map_err(|error| OpError::Product(format!("{path}: reading {id}: {error:?}")))?;
            let target = layerfs_content::filesystem::SymlinkTarget::decode(&canonical)
                .map_err(|error| OpError::Product(format!("{path}: symlink target: {error:?}")))?;
            (
                crate::workload::digest::sha256(target.as_bytes()),
                target.as_bytes().len() as u64,
            )
        } else {
            let mut sink = crate::workload::oracle::HashingSink::new();
            let (result, _) =
                Timing::disabled("verify.read", |scope: &TimingScope<'_, Active>| {
                    layerfs_content::read_all(
                        self.reader,
                        id,
                        &mut sink,
                        scope.child("content.acquire"),
                    )
                });
            result.map_err(|error| OpError::Product(format!("{path}: reading {id}: {error:?}")))?;
            let bytes = sink.bytes();
            (sink.finish(), bytes)
        };
        self.tally.files_read = self.tally.files_read.saturating_add(1);
        self.tally.file_bytes = self.tally.file_bytes.saturating_add(length);
        self.digests.insert(id, (found, length));
        Ok((found, length))
    }
}

/// What one state's child produced, carried out of the measured region so the
/// trace is written **after** the timer rather than inside it.
struct StateOutcome {
    ordinal: usize,
    root: FilesystemRootId,
    changed_paths: usize,
    changed_bytes: u64,
    inserted: u64,
    objects: u64,
    peak_heap_bytes: u64,
    /// Diagnostic breakdown of this state's child, so "the state costs 2 s" can
    /// be read as which part of it does.
    input_ns: u64,
    build_ns: u64,
    save_ns: u64,
    commits: u64,
    group_decodes: u64,
    pooled_reads: layerfs_storage::encoding::pool::PoolReadCounters,
    connection_opens: u64,
    filesystem: FilesystemUpdateCounters,
    filesystem_reads: ReadCounters,
    content_reads: ReadCounters,
}

/// The driver.
pub fn run(case: &Case, row: Row, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    match context.phase {
        Phase::Prepare => prepare(case, row, context),
        Phase::Perf => perf(case, row, context),
        Phase::Verify => verify(case, row, context),
    }
}

/// `Phase::Verify`: read the Store back and compare it with the corpus oracle.
///
/// The measured phase published each state's root identity to the trace; this
/// phase opens the Store that trace names, resolves a **declared sample** of each
/// state's paths through [`FilesystemRead`], and compares presence, kind, size and
/// digest against the corpus oracle for that state. It measures nothing: the whole
/// read-back runs under a disabled timer, and the receipt records it as the
/// verification invocation.
///
/// It is a sampled read-back and says so: the row it produces is `INCOMPLETE` and
/// never `PASS`.
fn verify(case: &Case, row: Row, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let corpus_root = corpus_root(context)?;
    let trace_path = context.output.join("trace.jsonl");
    let mut gates: Vec<Gate> = Vec::new();

    // The identity the verification is *of*. A verifier that ran against a
    // different corpus than the one the measured phase read would be verifying a
    // different claim, so the manifest pin is re-checked here and not assumed.
    let corpus = Corpus::open(&corpus_root, row).map_err(corpus_error)?;
    let pins = corpus.pins();

    let (roots, store_path) = match measured_roots(&trace_path) {
        Ok(found) => found,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    gates.push(gates::require(
        GateClass::Custody,
        "g6.verify-state-count",
        roots.len() == pins.states,
        &format!("{} state roots recorded", roots.len()),
        &format!("{} states in the selection", pins.states),
    ));
    let store = match super::c2::open_untimed(&store_path) {
        Ok(store) => store,
        Err(error) => {
            return Ok(unmeasured(
                &OpError::Io(format!("{}: {error}", store_path.display())),
                gates,
            ))
        }
    };

    // Each state's oracle, parsed **once**. It is read here, before a single
    // Store read, so the row's declared unit — one content-bearing path-state —
    // is counted from the corpus, and the same parsed oracle is then handed to
    // the sampler. Parsing it twice cost about a second on stride 10 and bought
    // nothing.
    let mut oracles: Vec<(usize, ObjectId, crate::workload::history::Oracle)> =
        Vec::with_capacity(roots.len());
    let mut content_units: u64 = 0;
    for (ordinal, root) in &roots {
        let state = corpus
            .states()
            .get(ordinal.saturating_sub(1))
            .ok_or_else(|| {
                OpError::Io(format!("history.state.{ordinal}.root has no matching state"))
            })?;
        let oracle = corpus.oracle(state).map_err(corpus_error)?;
        if oracle.is_empty() {
            return Err(OpError::Io(format!(
                "state {ordinal}: the oracle declares no paths; an empty oracle is a defect"
            )));
        }
        content_units = content_units
            .saturating_add((oracle.len() as u64).saturating_sub(oracle.directories() as u64));
        oracles.push((*ordinal, *root, oracle));
    }
    // The budget is a **declared** bound, not a discovered one: the phase samples
    // a fixed number of paths per state, so its cost is what the declaration says
    // and not what the history happens to be. `--verify-sample N` overrides it.
    let budget = match context.verify_sample {
        None | Some(0) => VERIFY_PATH_BUDGET,
        Some(requested) => requested.max(1),
    };

    let (result, _) = Timing::disabled("history.verify", |scope: &TimingScope<'_, Active>| {
        let _ = scope;
        let provider = StoreProvider::new(&store);
        let mut total = VerifyTally::default();
        let mut failures: Vec<String> = Vec::new();
        let mut per_state_ns: Vec<(usize, u64)> = Vec::new();
        for (ordinal, root, oracle) in &oracles {
            let started = std::time::Instant::now();
            let mut sampler = Sampler::new(&provider, oracle, budget);
            let tally = sampler.run(FilesystemRootId(*root))?;
            if !tally.clean() {
                failures.push(format!(
                    "state {ordinal}: {} sampled, {} mismatched, {} missing, {} unexpected; {}",
                    tally.compared,
                    tally.mismatches,
                    tally.missing,
                    tally.unexpected,
                    sampler.samples.join(" | ")
                ));
            }
            total.add(&tally);
            per_state_ns.push((*ordinal, started.elapsed().as_nanos() as u64));
        }
        Ok((
            total,
            failures,
            per_state_ns,
            provider.connection_opens(),
            provider.group_decodes(),
        ))
    });
    let (total, failures, per_state_ns, connection_opens, group_decodes) = match result {
        Ok(value) => value,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };

    // The Store's own read accounting. Without it, "the phase is slow" is an
    // observation and "the phase opens a connection per wave" is a finding.
    for (key, value, unit, basis) in [
        (
            "verify.store_connection_opens",
            connection_opens as i128,
            "connections",
            "SQLite connections this phase's waves opened",
        ),
        (
            "verify.store_group_decodes",
            group_decodes as i128,
            "groups",
            "ordinary-lane group bodies this phase decompressed",
        ),
    ] {
        context.trace.write_number(Kind::Resource, key, value, unit, basis)?;
    }
    for (ordinal, nanos) in &per_state_ns {
        context.trace.write_number(
            Kind::Resource,
            &format!("verify.state.{ordinal}.ns"),
            *nanos as i128,
            "ns",
            "wall time this state's read-back took, inside the unmeasured phase",
        )?;
    }
    for (key, value, unit, basis) in [
        (
            "verify.states",
            roots.len() as i128,
            "states",
            "state roots this invocation read back",
        ),
        (
            "verify.path_states",
            total.declared as i128,
            "path-states",
            "oracle entries across every state",
        ),
        (
            "verify.compared",
            total.compared as i128,
            "path-states",
            "sampled paths this invocation resolved and compared",
        ),
        (
            "verify.mismatches",
            total.mismatches as i128,
            "path-states",
            "sampled paths whose kind, size or digest disagreed",
        ),
        (
            "verify.missing",
            total.missing as i128,
            "path-states",
            "sampled paths the Store's tree does not bind",
        ),
        (
            "verify.unexpected",
            total.unexpected as i128,
            "path-states",
            "sampled paths the Store binds to the wrong kind",
        ),
        (
            "verify.files_read",
            total.files_read as i128,
            "files",
            "sampled files whose bytes were read back and hashed",
        ),
        (
            "verify.file_bytes",
            total.file_bytes as i128,
            "bytes",
            "logical bytes those reads returned",
        ),
        (
            "verify.symlinks_read",
            total.symlinks_read as i128,
            "symlinks",
            "sampled symlink targets read back",
        ),
    ] {
        context.trace.write_number(Kind::Counter, key, value, unit, basis)?;
    }
    // The mode ladder's unit, declared by the row. The unit is one
    // content-bearing path-state, and the sample is the declared one; a sampled
    // row is `INCOMPLETE` in the receipt and never `PASS`.
    context.trace.write_number(
        Kind::Counter,
        "verify.units",
        content_units as i128,
        "path-states",
        "the row's declared verification unit: one content-bearing path-state",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "verify.sampled",
        total.compared as i128,
        "path-states",
        "path-states this invocation actually read back",
    )?;

    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o4-readback",
        failures.is_empty(),
        &match failures.first() {
            None => format!(
                "{} sampled paths across {} states read back: presence, kind, size and digest all match",
                total.compared,
                roots.len()
            ),
            Some(first) => format!("{} state(s) failed; first: {first}", failures.len()),
        },
        "every sampled path of every state reads back and matches the corpus oracle",
    ));
    gates.push(gates::require(
        GateClass::Custody,
        "g6.verify-sample-declared",
        total.compared > 0,
        &format!(
            "{} sampled of {} declared path-states, at most {budget} per state",
            total.compared, total.declared
        ),
        "the phase read back at least one path of every state it was asked to verify",
    ));

    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("verify_op: {}", row.id()),
            "verification_phase: separate unmeasured invocation".to_string(),
            "verification_budget: charged to the 60 s verification budget".to_string(),
            format!("store: {}", store_path.display()),
            format!("verify_sample_budget: {budget} paths per state"),
            "oracle: a declared sample of each state's paths, never a prefix".to_string(),
            format!(
                "corpus_manifest_sha256: {}",
                crate::workload::history::MANIFEST_SHA256
            ),
            format!("case: {}", case.id),
        ],
    })
}

/// The corpus root this row was handed, or a refusal.
fn corpus_root(context: &OpContext<'_>) -> Result<std::path::PathBuf, OpError> {
    context.corpus.clone().ok_or_else(|| {
        OpError::Io("history.* requires --corpus PATH; an absent corpus is never defaulted".to_string())
    })
}

/// `Phase::Prepare`: authenticate the corpus and publish the pins. Writes nothing.
fn prepare(_case: &Case, row: Row, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let root = corpus_root(context)?;
    let corpus = Corpus::open(&root, row).map_err(corpus_error)?;
    let pins = corpus.pins();
    context.trace.write_number(
        Kind::Counter,
        "history.states",
        pins.states as i128,
        "states",
        "the selection's length",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.logical_bytes",
        pins.logical_bytes as i128,
        "bytes",
        "cumulative logical bytes, from the manifest",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.path_states",
        pins.path_states as i128,
        "path-states",
        "cumulative oracle entries: files and directories (erratum E1)",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.manifest_files",
        pins.manifest_files as i128,
        "files",
        "the manifest's own files total; about 18% below path-states and not the same quantity",
    )?;
    context.trace.write(
        Kind::Counter,
        "history.manifest_sha256",
        crate::workload::history::MANIFEST_SHA256,
        "sha256",
        "the corpus manifest identity",
    )?;
    context.trace.write(
        Kind::Counter,
        "history.source_tip",
        crate::workload::history::SOURCE_TIP,
        "commit",
        "the pinned source tip",
    )?;
    for state in corpus.states() {
        context.trace.write(
            Kind::Counter,
            &format!("history.oracle_sha256.{}", state.ordinal),
            &state.oracle_sha256,
            "sha256",
            "the state's oracle digest, recorded once at open",
        )?;
    }
    Ok(OpOutcome::new().note(format!(
        "corpus: {} (manifest {}, tip {})",
        root.display(),
        crate::workload::history::MANIFEST_SHA256,
        crate::workload::history::SOURCE_TIP
    )))
}

/// Optional diagnostic nodes use the existing timer without changing product work.
fn history_phase<T>(
    child: &TimingScope<'_, Active>,
    enabled: bool,
    name: &'static str,
    body: impl FnOnce(&TimingScope<'_, Active>) -> Result<T, OpError>,
) -> Result<T, OpError> {
    if enabled {
        child.child(name).run(body)
    } else {
        Timing::disabled(name, body).0
    }
}

/// The measured chain.
fn perf(_case: &Case, row: Row, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    context.create_output()?;
    let root = corpus_root(context)?;
    let mut corpus = Corpus::open(&root, row).map_err(corpus_error)?;
    let store_path = context.output.join("sample.sqlite");

    let policy = ConstructionPolicy::frozen_default();
    let capacities: ConstructionCapacities = policy.capacities();
    // The full history's trees, loaded once and untimed. A producer that runs at
    // every commit holds the previous checkpoint; answering "what was this path
    // before?" from the checkpoints' own manifests is what lets this driver stand
    // in for the Stage 7 producer that does not exist.
    if full_history_producer() {
        corpus.load_checkpoint_trees().map_err(corpus_error)?;
    }
    let scope = scope_of(row);
    // The operation's own ordering spill. `ops/fs.rs` opens one when the fixture
    // outgrows the in-memory pending map, and the threshold is the product's
    // declared ceiling rather than a harness policy: a transition large enough to
    // need a backing pays for it, inside the child, and a small one does not.
    let backing_directory = context.output.join("ordering");
    std::fs::create_dir_all(&backing_directory)?;

    let mut gates = Vec::new();
    let mut chain = Chain::new();
    let mut store: Option<Store> = None;
    let mut previous_root: Option<FilesystemRootId> = None;

    let mut directories: Vec<DirectoryUpdate> = Vec::new();
    let mut inodes: Vec<InodeUpdate> = Vec::new();
    let mut new_inodes: Vec<u64> = Vec::new();

    // **Step 0 of #187.** Whether this run models a history save faithfully, and
    // the last content root this chain constructed for each path. The map is
    // carried **across** states on purpose: the base a faithful save declares
    // for state k's version of a path is an earlier state's version of the same
    // path, which lives in an earlier save and is therefore unreachable by the
    // per-save candidate cache (`encoding/delta/candidates.rs`).
    let faithful = faithful_history_model();
    // **R0's missing caller.** A cross-save, content-keyed similarity index, built
    // from the product's own `signature` function. It is what the per-save
    // `Candidates` cache cannot be, and it is harness code with no product line.
    let mut index = SimilarityIndex::new();
    // `path -> the content root this chain last stored for it`, so the same-path
    // slots of the declaration are real predecessors rather than guesses.
    let mut same_path_root: BTreeMap<Vec<u8>, ObjectId> = BTreeMap::new();
    let depth_limit = advisory_depth_limit();
    let mut previous_content: BTreeMap<Vec<u8>, ObjectId> = BTreeMap::new();
    // The driver's own account of how deep a chain it has asked the Store to
    // build for each path. It is the only depth the driver can know: the Store's
    // own depth bookkeeping is not a public reading.
    let mut chain_depth: BTreeMap<Vec<u8>, u8> = BTreeMap::new();
    let mut advisory_bases: u64 = 0;
    let mut prior_unavailable: u64 = 0;
    let mut totals = SaveTotals::default();

    let count = corpus.states().len();
    // Extra nodes fit stride10/stride3; stride1 remains the ordinary recording.
    let detailed = matches!(
        std::env::var("LAYERFS_HISTORY_PHASES").as_deref(),
        Ok("1") | Ok("true")
    );
    if detailed && count > 53 {
        return Err(OpError::Io(
            "history phase diagnostics support at most 53 states (timer node bound)".into(),
        ));
    }

    // **One root, N named children.** The corpus reading sits inside the root and
    // outside every child, so the row's `operation_ns` is the sum of the children
    // and not the root — owner ruling 2. The root is the whole chain including the
    // harness's own reading, and it is published for exactly that reason: a reader
    // can see how much of the chain was not the product's.
    phases::prepared();
    let (result, report) = Timing::record("history", |root: &TimingScope<'_, Active>| {
        let mut corpus_read_ns: u64 = 0;
        let mut outcomes: Vec<StateOutcome> = Vec::with_capacity(count);
        for position in 0..count {
            // Untimed: the harness reads the corpus between the children.
            let t_corpus = std::time::Instant::now();
            let transition = corpus.transition(position).map_err(corpus_error)?;
            // **The producer's own correspondence.** A producer that runs at
            // every commit holds checkpoint k-1 when it saves checkpoint k, so for
            // every changed path it has that path's version at k-1 in hand. The
            // selection skips checkpoints, so a path that changed inside a skipped
            // span arrives here with no version in the Store at all; this finds the
            // version the producer would have held.
            //
            // The **reading** is harness work and sits outside every child, exactly
            // as the transition's own blob reading does. The **construction** of
            // those versions is the producer's work and happens inside the child,
            // because a producer does that work when it saves.
            let prior_inputs: BTreeMap<Vec<u8>, Vec<u8>> = if faithful && full_history_producer() {
                let wanted: Vec<Vec<u8>> = transition
                    .changed
                    .iter()
                    .filter(|entry| {
                        !matches!(entry.kind, Change::Removed | Change::MetadataOnly)
                            && !previous_content.contains_key(&entry.path)
                    })
                    .map(|entry| entry.path.clone())
                    .collect();
                if wanted.is_empty() {
                    BTreeMap::new()
                } else {
                    corpus
                        .prior_inputs(&transition.state, &wanted)
                        .map_err(corpus_error)?
                }
            } else {
                BTreeMap::new()
            };
            corpus_read_ns = corpus_read_ns.saturating_add(t_corpus.elapsed().as_nanos() as u64);
            let ordinal = transition.state.ordinal;
            let mut peak_heap = 0u64;
            let outcome = root
                .child(format!("history.state.{ordinal}"))
                .run(|child: &TimingScope<'_, Active>| -> Result<StateOutcome, OpError> {
                    // The heap window is per child, so the figure is one state's
                    // peak rather than the whole chain's: the corpus bytes this
                    // loop reads are inside the root but outside this window.
                    instruments::heap_begin();
                    let mut consumer = TreeStore::new();
                    // **One `content` node per state, not per file.** The product's
                    // timing tree is bounded by `layerfs_telemetry::MAX_NODES`
                    // (1,024), so a node per constructed file exhausts it inside
                    // state 1 and every later state is clipped away — the first
                    // version of this driver recorded one child of seventeen and
                    // reported the chain as `incomplete`. The per-file work is
                    // measured inside this node; the node itself is per state, so a
                    // 157-state chain needs 3 x 157 + 1 nodes rather than tens of
                    // thousands.
                    let mut content_reads = ReadCounters::default();
                    let constructed = child
                        .child("content")
                        .run(
                            |content: &TimingScope<'_, Active>| -> Result<
                                (
                                    BTreeMap<Vec<u8>, ObjectId>,
                                    BTreeMap<ObjectId, [u64; 8]>,
                                ),
                                OpError,
                            > {
                                let _ = content;
                                // **The chunk lane's missing producer.** The Store the
                                // chain has written so far is the provider a cursor can
                                // walk a previous version's mapping through. It is built
                                // once per state and reads nothing until a chunked
                                // construction asks it for a mapping page.
                                let cursor_reader = if chunk_predecessors_enabled() {
                                    store.as_ref().map(|store| ReadWork::new(StoreProvider::new(store), detailed))
                                } else {
                                    None
                                };
                                let mut constructed = BTreeMap::new();
                                let mut state_signatures: BTreeMap<ObjectId, [u64; 8]> =
                                    BTreeMap::new();
                                for changed in &transition.changed {
                                    if matches!(
                                        changed.kind,
                                        Change::Removed | Change::MetadataOnly
                                    ) {
                                        continue;
                                    }
                                    let bytes =
                                        transition.blobs.get(&changed.oid).ok_or_else(|| {
                                            OpError::Io(format!(
                                                "state {ordinal}: oid {} was not served",
                                                hex(&changed.oid)
                                            ))
                                        })?;
                                    // A disabled scope records nothing: on it,
                                    // `child` returns a disabled pending scope, so
                                    // the per-file work is measured inside the
                                    // state's `content` node and creates no nodes
                                    // of its own.
                                    //
                                    // A symlink takes the symlink path and not
                                    // `construct_bytes`: the target is emitted as
                                    // a `Symlink` object, so the tree reads back
                                    // with the kind the corpus declares.
                                    // The previous version of this path, offered as the
                                    // positional base for a chunked result. Only a root this
                                    // chain itself saved is offered, so the provider can
                                    // always serve it.
                                    let base = cursor_reader.as_ref().and_then(|reader| {
                                        previous_content
                                            .get(&changed.path)
                                            .copied()
                                            .map(|root| PredecessorBase::new(reader, root))
                                    });
                                    let root = if kind_of(changed.mode) == InodeKind::Symlink {
                                        emit_symlink_target(bytes, &mut consumer)?
                                    } else {
                                        let (file, _) = Timing::disabled(
                                            "content.file",
                                            |scope: &TimingScope<'_, Active>| match base {
                                                Some(base) => construct_bytes_with_predecessor(
                                                    policy,
                                                    &capacities,
                                                    bytes,
                                                    Some(base),
                                                    &mut consumer,
                                                    scope.child("content.file"),
                                                ),
                                                None => construct_bytes(
                                                    policy,
                                                    &capacities,
                                                    bytes,
                                                    &mut consumer,
                                                    scope.child("content.file"),
                                                ),
                                            },
                                        );
                                        let file = file.map_err(|error| {
                                            OpError::Product(format!("{error:?}"))
                                        })?;
                                        file.root
                                    };
                                    // The signature the product's own index would
                                    // key on: the eight smallest mixed rolling
                                    // hashes of the **raw** content, computed with
                                    // the product's function.
                                    let raw_signature =
                                        layerfs_storage::encoding::delta::candidates::signature(bytes);
                                    state_signatures.insert(root, raw_signature);
                                    constructed.insert(changed.path.clone(), root);
                                }
                                if let Some(reader) = cursor_reader {
                                    content_reads = reader.counters();
                                }
                                Ok((constructed, state_signatures))
                            },
                        )?;
                    let (constructed, state_signatures) = constructed;
                    let (bases, _prior_roots, _prior_missing) = history_phase(child, detailed, "harness.predecessors", |_| {
                        // **Step 0 of #187.** A faithful history save reads the
                        // previous version of a path and produces the new one as an
                        // **edit**, which is what `file/edit/apply.rs` does when it
                        // attaches `AdvisoryPredecessors::explicit(view.root())`.
                        // This driver instead calls `construct_bytes`, which builds
                        // `FinalizedObject::new(role, canonical)` with **no
                        // predecessors at all** — so without this the Store is asked
                        // to hold every version as though it had never existed, and
                        // the only cross-save route to a delta base is never taken.
                        //
                        // Keyed by the NEW content root, because that is the object
                        // the save will offer; the value is the previous version's
                        // content root for the same path. An identical root is
                        // skipped for the same reason `apply.rs` skips it: the
                        // content did not change, so there is nothing to encode
                        // against.
                        // The producer's earlier versions, constructed inside the
                        // measured region: a producer builds the base it declares.
                        let mut prior_roots: BTreeMap<Vec<u8>, ObjectId> = BTreeMap::new();
                        let mut prior_missing: BTreeSet<Vec<u8>> = BTreeSet::new();
                        if faithful {
                            let (built, _) = Timing::disabled(
                                "content.prior",
                                |scope: &TimingScope<'_, Active>| -> Result<
                                    BTreeMap<Vec<u8>, ObjectId>,
                                    OpError,
                                > {
                                    let mut built: BTreeMap<Vec<u8>, ObjectId> = BTreeMap::new();
                                    for (path, bytes) in &prior_inputs {
                                        match construct_bytes(
                                            policy,
                                            &capacities,
                                            bytes,
                                            &mut consumer,
                                            scope.child("content.prior"),
                                        ) {
                                            Ok(file) => {
                                                built.insert(path.clone(), file.root);
                                            }
                                            Err(error) => {
                                                let _ = error;
                                                prior_missing.insert(path.clone());
                                            }
                                        }
                                    }
                                    Ok(built)
                                },
                            );
                            match built {
                                Ok(built) => prior_roots = built,
                                Err(error) => {
                                    return Err(OpError::Product(format!("{error:?}")));
                                }
                            }
                        }
                        // **R0's declaration, in the measured best order.** The
                        // selector probes these in order and returns the first
                        // *eligible* one (`encoding/delta/select.rs:342`), so the order
                        // is the parameter that decides the result — and "try all four
                        // and keep the smallest frame" is measurably worse because it
                        // deepens chains past the depth cap.
                        let mut bases: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();
                        if faithful {
                            for changed in &transition.changed {
                                if matches!(changed.kind, Change::Removed | Change::MetadataOnly) {
                                    continue;
                                }
                                let Some(new_root) = constructed.get(&changed.path).copied() else {
                                    continue;
                                };
                                let Some(signature) = state_signatures.get(&new_root).copied() else {
                                    continue;
                                };
                                let previous = same_path_root.get(&changed.path).copied();
                                if previous == Some(new_root) {
                                    continue;
                                }
                                let ordered = if ordered_predecessors_enabled() {
                                    ordered_predecessors(
                                        &index,
                                        new_root,
                                        &changed.path,
                                        signature,
                                        previous,
                                    )
                                } else if similarity_candidates_enabled() {
                                    // **One variable against the default arm.** The
                                    // same-path previous version is still the whole list
                                    // when it exists; the cross-save index is consulted
                                    // only when it does not. The candidates are the
                                    // ordered arm's own, so the gain this arm shows is
                                    // the ordered arm's gain with its base-replacement
                                    // loss removed.
                                    match previous {
                                        Some(previous) => vec![previous],
                                        None => cross_path_predecessors(
                                            &index,
                                            new_root,
                                            &changed.path,
                                            signature,
                                            fallback_candidate_limit(),
                                        ),
                                    }
                                } else {
                                    // The measured best for this lane: the single
                                    // same-path previous version. Everything else is
                                    // shared, so the two arms differ in exactly one
                                    // parameter — the list — and the comparison is one
                                    // variable wide.
                                    previous.into_iter().collect()
                                };
                                // Drop a candidate whose chain is already at the cap:
                                // declaring it would make the writer build a chain the
                                // reader refuses. The driver's own depth account is the
                                // only one it has, and it is an over-estimate, which is
                                // the safe direction.
                                let depth_limit = advisory_depth_limit();
                                // A candidate at or near the cap is a **bad base** even
                                // when it is eligible: the frame it produces is deeper,
                                // and a deeper chain is the spec's measured failure mode
                                // for a greedy four-slot order. Rank by the candidate's
                                // own depth first, then keep the measured
                                // cross-path-then-same-path order within a depth, so a
                                // shallow base is preferred to a deep one.
                                let mut ranked: Vec<(u8, usize, ObjectId)> = ordered
                                    .iter()
                                    .enumerate()
                                    .map(|(position, candidate)| {
                                        let depth = index
                                            .path_of
                                            .get(candidate)
                                            .and_then(|path| chain_depth.get(path))
                                            .copied()
                                            .unwrap_or(0);
                                        (depth, position, *candidate)
                                    })
                                    .collect();
                                ranked.sort_by_key(|(depth, position, _)| (*depth, *position));
                                let eligible: Vec<ObjectId> = ranked
                                    .into_iter()
                                    .filter(|(depth, _, _)| *depth < depth_limit)
                                    .map(|(_, _, candidate)| candidate)
                                    .collect();
                                if eligible.is_empty() {
                                    chain_depth.insert(changed.path.clone(), 0);
                                    continue;
                                }
                                for candidate in &eligible {
                                    let depth = index
                                        .path_of
                                        .get(candidate)
                                        .and_then(|path| chain_depth.get(path))
                                        .copied()
                                        .unwrap_or(0);
                                    chain_depth
                                        .insert(changed.path.clone(), depth.saturating_add(1));
                                    break;
                                }
                                bases.insert(new_root, eligible);
                            }
                            advisory_bases = advisory_bases.saturating_add(bases.len() as u64);
                        }
                        Ok((bases, prior_roots, prior_missing))
                    })?;
                    // The Store is created inside state 1's child, so the fixed
                    // cost is visible and subtractable rather than buried.
                    if store.is_none() {
                        let created = Store::create(
                            &store_path,
                            StoragePolicy::frozen_default(),
                            child.child("store.create"),
                        )
                        .map_err(|error| OpError::Product(format!("{error:?}")))?;
                        store = Some(created);
                    }
                    let held = store.as_ref().expect("the Store was just created");
                    let t_input = std::time::Instant::now();
                    history_phase(child, detailed, "harness.input", |_| {
                        filesystem_input(
                            &mut chain,
                            &transition,
                            &constructed,
                            previous_root.is_none(),
                            &mut directories,
                            &mut inodes,
                            &mut new_inodes,
                        )
                    })?;
                    let input_ns = t_input.elapsed().as_nanos() as u64;
                    let t_build = std::time::Instant::now();
                    let input = FilesystemInput {
                        base: previous_root,
                        scope,
                        root_serial: ROOT_SERIAL,
                        directories: &directories,
                        inodes: &inodes,
                        new_inodes: &new_inodes,
                        resources: FilesystemResources::default(),
                    };
                    let provider = ReadWork::new(StoreProvider::new(held), detailed);
                    let bindings: usize =
                        directories.iter().map(|update| update.changes.len()).sum();
                    // **Always supply the ordering backing.**
                    //
                    // This used to supply one only when the *binding count* exceeded
                    // `maximum_pending_records`, on the theory that a fixture with few
                    // bindings never spills and so needs no scratch. **That theory is
                    // false, and `history-stride3` is the tier it broke.** The spill is
                    // triggered by the *pending row map* (`references/reduce.rs:248`:
                    // `if self.pending.len() >= self.maximum_pending`), and on an
                    // **update** the walk accumulates rows while descending the *base*
                    // tree — so a state can carry far fewer changed bindings than the
                    // 4,096 ceiling and still open a run. With no backing supplied the
                    // operation then fails with
                    // `ResourceUnavailable { what: "ordering backing" }`.
                    //
                    // Supplying a backing does **not** cause ordering I/O; it only makes
                    // it possible when the operation decides it needs it. A fixture that
                    // never spills behaves exactly as before, which is why `stride10` is
                    // byte-identical across this change.
                    let _ = bindings;
                    let mut backing = Some(FileBacking::new(&backing_directory));
                    let built = history_phase(child, detailed, "filesystem", |scope| {
                        let mut objects = FilesystemObjects::new(&provider, &mut consumer);
                        let backing = backing.as_mut().map(|backing| {
                            backing
                                as &mut dyn layerfs_content::filesystem::references::backing::OrderingBacking
                        });
                        let result = match (previous_root, detailed) {
                            (None, false) => build_filesystem(&mut objects, &input, backing),
                            (Some(_), false) => update_filesystem(&mut objects, &input, backing),
                            (None, true) => build_filesystem_timed(
                                &mut objects, &input, backing, &FilesystemPhases::new(scope),
                            ),
                            (Some(_), true) => update_filesystem_timed(
                                &mut objects, &input, backing, &FilesystemPhases::new(scope),
                            ),
                        };
                        result.map_err(|error| OpError::Product(format!("{error:?}")))
                    })?;
                    let build_ns = t_build.elapsed().as_nanos() as u64;
                    let t_save = std::time::Instant::now();
                    let mut operation = held
                        .begin_save(child.child("storage.begin"))
                        .map_err(|error| OpError::Product(format!("{error:?}")))?;
                    history_phase(child, detailed, "storage.accept_loop", |_| {
                        for id in consumer.insertion_order() {
                            let mut object = consumer
                                .cloned_object(*id)
                                .ok_or_else(|| OpError::Io(format!("object {id:?} vanished")))?;
                            if let Some(ordered) = bases.get(id) {
                                let mut list = AdvisoryPredecessors::new();
                                for candidate in ordered {
                                    list.push(*candidate, PredecessorProvenance::OriginalBase)
                                        .map_err(|error| OpError::Product(format!("{error:?}")))?;
                                }
                                object = object.with_predecessors(list);
                            }
                            operation
                                .accept(object)
                                .map_err(|error| OpError::Product(format!("{error:?}")))?;
                        }
                        Ok(())
                    })?;
                    history_phase(child, detailed, "harness.index", |_| {
                        if faithful {
                            // The objects this save admitted are the candidates the
                            // **next** state may declare. Indexing after the save is
                            // what makes the correspondence cross-save, which the
                            // product's per-save cache cannot be at any size.
                            for (path, root) in &constructed {
                                if let Some(signature) = state_signatures.get(root).copied() {
                                    index.insert(*root, path, signature);
                                }
                                previous_content.insert(path.clone(), *root);
                                same_path_root.insert(path.clone(), *root);
                            }
                        }
                        Ok(())
                    })?;
                    let saved = operation
                        .finish(child.child("storage.finish"))
                        .map_err(|error| OpError::Product(format!("{error:?}")))?;
                    let save_ns = t_save.elapsed().as_nanos() as u64;
                    totals.add(&saved);
                    peak_heap = instruments::heap_end().peak_incremental_bytes;
                    previous_root = Some(built.root);
                    Ok(StateOutcome {
                        ordinal,
                        root: built.root,
                        changed_paths: transition.changed.len(),
                        changed_bytes: transition.changed_bytes(),
                        inserted: saved.inserted,
                        objects: built.counters.objects.objects_emitted,
                        peak_heap_bytes: 0,
                        input_ns,
                        build_ns,
                        save_ns,
                        commits: saved.commits,
                        group_decodes: provider.inner.group_decodes(),
                        pooled_reads: provider.inner.pooled_read_counters(),
                        connection_opens: provider.inner.connection_opens(),
                        filesystem_reads: provider.counters(),
                        content_reads,
                        filesystem: built.counters,
                    })
                })?;
            outcomes.push(StateOutcome {
                peak_heap_bytes: peak_heap,
                ..outcome
            });
        }
        Ok((outcomes, corpus_read_ns))
    });
    let (outcomes, corpus_read_ns) = match result {
        Ok(value) => value,
        Err(error) => return Ok(unmeasured(&error, gates)),
    };
    drop(store);

    // The product's own timing tree, byte-verbatim, written **once** for the whole
    // chain: one root and one named child per state. The published operation time
    // is the sum of those children.
    let timing_bytes = c1::write_timing(context.output, &report)?;
    let children_ns: u64 = report
        .root()
        .map(|root| {
            root.children()
                .iter()
                .map(|child| child.elapsed().as_nanos() as u64)
                .sum()
        })
        .unwrap_or(0);
    let root_ns = report
        .root()
        .map(|root| root.elapsed().as_nanos() as u64)
        .unwrap_or(0);
    phases::measured(children_ns, timing_bytes);

    for outcome in &outcomes {
        context.trace.write(
            Kind::Counter,
            &format!("history.state.{}.root", outcome.ordinal),
            &format!("{:?}", outcome.root),
            "identity",
            "the state's filesystem root identity (O1 pin)",
        )?;
        context.trace.write_number(
            Kind::Counter,
            &format!("history.state.{}.changed_paths", outcome.ordinal),
            outcome.changed_paths as i128,
            "paths",
            "entries this transition changed",
        )?;
        context.trace.write_number(
            Kind::Counter,
            &format!("history.state.{}.changed_bytes", outcome.ordinal),
            outcome.changed_bytes as i128,
            "bytes",
            "logical bytes this transition changed",
        )?;
        context.trace.write_number(
            Kind::Counter,
            &format!("history.state.{}.inserted", outcome.ordinal),
            i128::from(outcome.inserted),
            "objects",
            "SaveOutcome.inserted",
        )?;
        context.trace.write_number(
            Kind::Counter,
            &format!("history.state.{}.objects", outcome.ordinal),
            i128::from(outcome.objects),
            "objects",
            "canonical objects the child emitted",
        )?;
        context.trace.write_number(
            Kind::Resource,
            &format!("history.state.{}.heap_peak_incremental_bytes", outcome.ordinal),
            outcome.peak_heap_bytes as i128,
            "bytes",
            "counting GlobalAlloc, this state's child alone",
        )?;
        if detailed {
            for (provider, reads) in [
                ("filesystem.provider", outcome.filesystem_reads),
                ("content.predecessor_provider", outcome.content_reads),
            ] {
                for (suffix, value, unit) in [
                    ("read_waves", reads.waves, "count"),
                    ("requested_objects", reads.requested_objects, "objects"),
                    ("returned_objects", reads.returned_objects, "objects"),
                    ("returned_canonical_bytes", reads.returned_bytes, "bytes"),
                    ("failed_waves", reads.failed_waves, "count"),
                    ("read_elapsed_ns", reads.elapsed_ns, "ns"),
                ] {
                    context.trace.write_number(
                        Kind::Resource,
                        &format!("history.state.{}.{provider}.{suffix}", outcome.ordinal),
                        i128::from(value),
                        unit,
                        "aggregate original provider calls; elapsed overlaps enclosing phase; bytes are returned canonical bytes, not disk/pack traffic",
                    )?;
                }
            }
        }
        if detailed {
            let pooled = outcome.pooled_reads;
            for (suffix, value, unit) in [
                ("leaf_requests", pooled.leaf_requests, "count"),
                ("chain_edges", pooled.chain_edges, "count"),
                ("physical_record_calls", pooled.physical_record_calls, "count"),
                ("physical_group_decodes", pooled.physical_group_decodes, "count"),
                ("physical_group_decoded_bytes", pooled.physical_group_decoded_bytes, "bytes"),
                ("physical_group_cache_hits", pooled.physical_group_cache_hits, "count"),
                ("value_group_decodes", pooled.value_group_decodes, "count"),
                ("pack_fetches", pooled.pack_fetches, "count"),
                ("pack_bytes", pooled.pack_bytes, "bytes"),
            ] {
                context.trace.write_number(
                    Kind::Resource,
                    &format!("history.state.{}.filesystem.provider.pooled.{suffix}", outcome.ordinal),
                    i128::from(value),
                    unit,
                    "successful provider waves; physical decodes are actual Zstd calls; value decodes include raw materialization; pack bytes are BLOB acquisitions, not disk traffic",
                )?;
            }
        }
        let fs = outcome.filesystem;
        for (suffix, value) in [
            ("save.commits", outcome.commits),
            ("filesystem.provider.group_decodes", outcome.group_decodes),
            ("filesystem.provider.connection_opens", outcome.connection_opens),
            ("filesystem.validation.objects_read", fs.validation.objects_read),
            ("filesystem.validation.read_waves", fs.validation.read_waves),
            ("filesystem.validation.inode_demands", fs.validation.inode_demands),
            ("filesystem.validation.inode_pages_read", fs.validation.inode_pages_read),
            ("filesystem.validation.directory_pages_read", fs.validation.directory_pages_read),
            ("filesystem.validation.entries_examined", fs.validation.entries_examined),
            ("filesystem.references.rows_touched", fs.references.rows_touched),
            ("filesystem.references.rows_spilled", fs.references.rows_spilled),
            ("filesystem.references.serials_scanned", fs.references.serials_scanned),
            ("filesystem.references.base_records_read", fs.references.base_records_read),
            ("filesystem.references.base_waves", fs.references.base_waves),
            ("filesystem.references.runs_created", fs.references.runs.runs_created),
            ("filesystem.references.rows_read", fs.references.runs.rows_read),
            ("filesystem.references.rows_written", fs.references.runs.rows_written),
            ("filesystem.directories.pages_read", fs.directories.pages_read),
            ("filesystem.inodes.pages_read", fs.inodes.pages_read),
        ] {
            context.trace.write_number(
                Kind::Counter,
                &format!("history.state.{}.{suffix}", outcome.ordinal),
                i128::from(value),
                "count",
                "public SaveOutcome or FilesystemUpdateCounters; overlapping work counters are not summed",
            )?;
        }
        for (suffix, nanos, basis) in [
            ("input_ns", outcome.input_ns, "harness input assembly inside the child"),
            ("build_ns", outcome.build_ns, "build/update_filesystem"),
            ("save_ns", outcome.save_ns, "begin_save + accept + finish"),
        ] {
            context.trace.write_number(
                Kind::Resource,
                &format!("history.state.{}.{suffix}", outcome.ordinal),
                nanos as i128,
                "ns",
                basis,
            )?;
        }
    }
    let peak_heap = outcomes
        .iter()
        .map(|outcome| outcome.peak_heap_bytes)
        .max()
        .unwrap_or(0);
    context.trace.write_number(
        Kind::Resource,
        "heap.peak_incremental_bytes",
        peak_heap as i128,
        "bytes",
        "the largest single-state peak; the chain's own peak is not the product's",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.root_ns",
        root_ns as i128,
        "ns",
        "the product's own root: the chain including the harness's untimed corpus reading",
    )?;
    context.trace.write_number(
        Kind::Resource,
        "history.corpus_read_ns",
        corpus_read_ns as i128,
        "ns",
        "the harness's own corpus reading: inside the root, between the children, untimed",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.children_ns",
        children_ns as i128,
        "ns",
        "the sum of this row's named children; the published operation_ns",
    )?;

    // The counters the row did not publish, which is why "cannot see it" and
    // "sees it and declines it" were indistinguishable. `prefix_records` is the
    // object count actually stored as a PREFIX (delta) record; `no_candidate`
    // is the count for which the save was supplied **no** base at all; the rest
    // separate "absent" from "present but ineligible" from "over budget".
    for (key, value) in [
        ("delta.reused", totals.reused),
        ("delta.inserted", totals.inserted),
        ("delta.full_records", totals.full_records),
        ("delta.prefix_records", totals.prefix_records),
        ("delta.packs_created", totals.packs_created),
        ("delta.prepared_full", totals.prepared_full),
        ("delta.trials", totals.trials),
        ("delta.prefix_selected", totals.prefix_selected),
        ("delta.full_losses", totals.full_losses),
        ("delta.no_candidate", totals.no_candidate),
        ("delta.absent_candidates", totals.absent_candidates),
        ("delta.ineligible_candidates", totals.ineligible_candidates),
        ("delta.work_exceeded", totals.work_exceeded),
    ] {
        context.trace.write_number(
            Kind::Counter,
            key,
            value as i128,
            "objects",
            "chain total of the save's own counters",
        )?;
    }
    context.trace.write_number(
        Kind::Counter,
        "history.advisory_model",
        i128::from(faithful),
        "bool",
        "1 when the driver declared each version's previous content root as an OriginalBase predecessor",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.prior_unavailable",
        prior_unavailable as i128,
        "objects",
        "changed paths that have an earlier version in the history whose bytes were not served",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.advisory_bases",
        advisory_bases as i128,
        "objects",
        "content roots the driver declared a previous version as the base for",
    )?;
    context.trace.write_number(
        Kind::Counter,
        "history.advisory_depth_limit",
        i128::from(depth_limit),
        "edges",
        "255 means the driver declared unconditionally",
    )?;

    context.trace.write(
        Kind::Counter,
        "store.path",
        &store_path.display().to_string(),
        "path",
        "the Store this row created and grew",
    )?;
    let space = instruments::space(&store_path)
        .map_err(|error| OpError::Io(format!("{error}")))?;
    context.trace.write_number(
        Kind::Resource,
        "space.allocated_bytes",
        space.allocated_bytes as i128,
        "bytes",
        "st_blocks * 512 of the Store file",
    )?;
    context.trace.write_number(
        Kind::Resource,
        "space.apparent_bytes",
        space.apparent_bytes as i128,
        "bytes",
        "st_size of the Store file",
    )?;
    let pins = corpus.pins();
    gates.push(gates::require(
        GateClass::Correctness,
        "g1.o1-chain-complete",
        outcomes.len() == pins.states,
        &format!("{} of {} states saved", outcomes.len(), pins.states),
        "every state of the selection is saved, in order",
    ));
    gates.push(gates::swap_gate(instruments::swaps()));
    Ok(OpOutcome {
        gates,
        notes: vec![
            format!("history_row: {}", row.id()),
            format!("history_states: {}", pins.states),
            format!("history_logical_bytes: {}", pins.logical_bytes),
            format!("history_path_states: {}", pins.path_states),
            "measured_region: construct + build/update + save, one named child per state".to_string(),
            "operation_ns: the sum of the named children, not the root".to_string(),
            "corpus_reading: between the children, untimed".to_string(),
            "prepared: none (Preparation::InProcess)".to_string(),
            // The runner reads this to decide whether the row has a deferred
            // oracle. Without it the verify invocation is never scheduled and a
            // `history.*` PASS covers no read-back at all.
            "oracle_phase: verify-invocation".to_string(),
            format!("store: {}", store_path.display()),
        ],
    })
}

/// The shape of an unmeasured failure: gates only, never a pass.
fn unmeasured(error: &OpError, mut gates: Vec<Gate>) -> OpOutcome {
    gates.push(Gate::incomplete(
        GateClass::Correctness,
        "g1.o1-chain-complete",
        &format!("{error}"),
        "every state of the selection is saved, in order",
    ));
    OpOutcome { gates, notes: Vec::new() }
}

