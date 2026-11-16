//! The §6.2 object closure of a mark, its `keptWaste` and the §6.4 live-heads
//! digest.
//!
//! A mark starts at the roots of every Commit and Layer the §6.1 history
//! closure retained. Each object is admitted once through `FinalizedObject`, so
//! its logical references (`references()`) are followed, and each locator's
//! direct delta base (`ChainBases::edge`) is followed, to the fixpoint. The
//! retained pack of every reached object is recorded, and the bytes of records
//! in those packs that no reached object owns are reported as `keptWaste`.
//! Nothing is deleted, rewritten or compacted here; the closure is kept in
//! memory for the rebuild path to use.
use super::kept::{self, KeptHistory};
use super::provider::HistoryProvider;
use super::rows::{cell, object};
use super::transaction::Transaction;
use crate::backend::records::Param;
use layerfs_content::filesystem::{FilesystemRoot, InodeScope};
use layerfs_content::inode_leaf::INODE_VALUE_BYTES;
use layerfs_content::{ConstructionPolicy, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_storage::{
    encoding::pool::PoolReader,
    encoding::{
        delta::read::{BodyCaches, ChainBases, ChainCounters, Resolver},
        framed_record, DecompressionWorkspace, GroupCache, PackCache,
    },
    location::{ObjectLocation, SignatureRow},
    pack::layout::{self, GroupCodec},
    source::Source,
    StorageCapacities, StorageError,
};
use layerfs_telemetry::timer::Timing;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const ROOTS: &str = "gc_kept_roots";
const LOCATIONS: &str = "gc_object_locations";
const LIVE_STACKS: &str = "gc_live_stacks";
const LIVE_BRANCHES: &str = "gc_live_branches";
const INODE_BLOCKS: &str = "gc_inode_blocks";
const TUPLE: &str = "gc_tuple_read";

/// The bytes a mark keeps that no retained root actually needs (§6.2, §11).
///
/// The report names three parts: bytes of unreferenced records inside a pack
/// the mark retains, bytes of objects reached only as a delta base, and the
/// inode-value block bytes 2.0 keeps for every live Store. A wholly dead pack is
/// not part of this value: it is not retained, so the rebuild path decides on it
/// from the refined live set.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeptWaste {
    dead_in_retained_packs_bytes: u64,
    delta_bases_bytes: u64,
    delta_base_references: Vec<ObjectId>,
    inode_blocks_bytes: u64,
}

impl KeptWaste {
    /// Stored bytes of records in retained packs that no reached object owns.
    pub fn dead_in_retained_packs_bytes(&self) -> u64 {
        self.dead_in_retained_packs_bytes
    }

    /// Stored bytes of objects reached only as a delta base.
    pub fn delta_bases_bytes(&self) -> u64 {
        self.delta_bases_bytes
    }

    /// The objects counted by [`Self::delta_bases_bytes`], in ascending order.
    pub fn delta_base_references(&self) -> &[ObjectId] {
        &self.delta_base_references
    }

    /// Bytes of `inode_value_block` rows, which 2.0 keeps in full (§6.4).
    pub fn inode_blocks_bytes(&self) -> u64 {
        self.inode_blocks_bytes
    }

    /// Whether every part is zero.
    pub fn is_empty(&self) -> bool {
        self.dead_in_retained_packs_bytes == 0
            && self.delta_bases_bytes == 0
            && self.inode_blocks_bytes == 0
    }
}

/// The object half of a mark: every object and pack the retained history
/// reaches (§6.2), in ascending order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeptObjects {
    objects: Vec<ObjectId>,
    packs: Vec<i64>,
    waste: KeptWaste,
}

impl KeptObjects {
    /// Every reached canonical object, in ascending identity order.
    pub fn objects(&self) -> &[ObjectId] {
        &self.objects
    }

    /// Every pack holding a reached object, in ascending pack order.
    pub fn packs(&self) -> &[i64] {
        &self.packs
    }

    /// The bytes the mark keeps without a retained root needing them.
    pub fn waste(&self) -> &KeptWaste {
        &self.waste
    }

    /// Whether `id` is reached.
    pub fn contains(&self, id: ObjectId) -> bool {
        self.objects.binary_search(&id).is_ok()
    }

    /// How many objects the closure holds.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Whether the closure holds no object.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

/// One complete mark: the §6.1 history closure, the §6.2 object closure and the
/// §6.4 digest of the live heads.
///
/// A rebuild path chooses its plan from [`Self::objects`] and reports
/// [`Self::live_heads_digest`] to prove the generation it installed; nothing in
/// this value has been compacted or deleted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkResult {
    history: KeptHistory,
    objects: KeptObjects,
    live_heads_digest: [u8; 32],
}

impl MarkResult {
    /// The retained history closure, exactly as written to `kept_node`.
    pub fn history(&self) -> &KeptHistory {
        &self.history
    }

    /// The retained object and pack closure.
    pub fn objects(&self) -> &KeptObjects {
        &self.objects
    }

    /// BLAKE3 over the §6.4 live-heads vector at the mark's fixed generation.
    pub fn live_heads_digest(&self) -> [u8; 32] {
        self.live_heads_digest
    }
}

/// The Store tuple a compaction records as its mark's first statement and
/// re-checks before the swap (§8 Phase B): the generation, the serial and pack
/// allocators, the four Store totals, the signature stamp and the history row
/// count the mark ran at.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GcTuple {
    /// The catalog incarnation (the Store generation G).
    pub generation: i64,
    /// The next history serial the mark saw.
    pub next_history_serial: i64,
    /// store_policy.next_pack_id at the mark.
    pub next_pack_id: i64,
    /// store_policy.next_ordinal at the mark.
    pub next_ordinal: i64,
    /// store_totals.logical at the mark.
    pub logical: i64,
    /// store_totals.canonical at the mark.
    pub canonical: i64,
    /// store_totals.stored at the mark.
    pub stored: i64,
    /// store_totals.metadata at the mark.
    pub metadata: i64,
    /// The highest content_signature stamp at the mark.
    pub stamp: i64,
    /// count(commit) + count(layer) at the mark.
    pub history_rows: i64,
}

impl HistoryProvider {
    /// The T7.2 mark with the §8 mark tuple read as its first statement.
    ///
    /// The tuple and the closure are one write transaction: a publication that
    /// lands before the tuple is read is part of the mark, and one that lands
    /// after it moves at least one tuple field, so Phase B abandons.
    pub fn mark_gc(&self) -> HistoryResult<(MarkResult, GcTuple)> {
        self.write(mark_gc)
    }

    /// The §6.4 live-heads digest of the Store as it is now, without a mark.
    pub fn live_heads_digest(&self) -> HistoryResult<[u8; 32]> {
        self.read(live_heads_digest)
    }
}

fn mark_gc(tx: &Transaction<'_>) -> HistoryResult<(MarkResult, GcTuple)> {
    let tuple = gc_tuple(tx)?;
    let mark = mark(tx)?;
    Ok((mark, tuple))
}

fn gc_tuple(tx: &Transaction<'_>) -> HistoryResult<GcTuple> {
    let rows = tx.query(TUPLE, Vec::<Param>::new())?;
    if rows.len() != 1 {
        return Err(HistoryError::Integrity("Store tuple"));
    }
    let row = &rows[0];
    Ok(GcTuple {
        generation: cell(row, 0)?,
        next_history_serial: cell(row, 1)?,
        next_pack_id: cell(row, 2)?,
        next_ordinal: cell(row, 3)?,
        logical: cell(row, 4)?,
        canonical: cell(row, 5)?,
        stored: cell(row, 6)?,
        metadata: cell(row, 7)?,
        stamp: cell(row, 8)?,
        history_rows: cell(row, 9)?,
    })
}

impl HistoryProvider {
    /// Recomputes the §6.1 history closure, then the §6.2 object closure over
    /// its retained Commit and Layer roots, and the §6.4 live-heads digest.
    ///
    /// It runs in the same write transaction as [`Self::mark_history_closure`],
    /// so a missing locator, a corrupt dependency or a failed admission aborts
    /// the mark and leaves no `kept_node` row behind. Nothing is compacted; the
    /// returned [`MarkResult`] is the rebuild path's input.
    pub fn mark(&self) -> HistoryResult<MarkResult> {
        self.write(mark)
    }
}

fn mark(tx: &Transaction<'_>) -> HistoryResult<MarkResult> {
    let history = kept::mark(tx)?;
    let policy = tx.mechanics().policy().map_err(storage)?;
    let capacities = StorageCapacities::from_policy(policy).map_err(storage)?;
    let source = ClosureSource {
        mechanics: tx.mechanics(),
    };
    let roots = roots(tx)?;
    let closure = walk(&source, &capacities, policy.construction(), roots)?;
    let waste = waste(tx, &source, &closure)?;
    let digest = live_heads_digest(tx)?;
    let objects = KeptObjects {
        objects: closure.visited.keys().copied().collect(),
        packs: closure.packs(),
        waste,
    };
    Ok(MarkResult {
        history,
        objects,
        live_heads_digest: digest,
    })
}

/// The roots of every retained Commit and Layer, deduplicated and ordered.
fn roots(tx: &Transaction<'_>) -> HistoryResult<Vec<ObjectId>> {
    let rows = tx.query(ROOTS, Vec::<Param>::new())?;
    let mut roots = BTreeSet::new();
    for row in &rows {
        roots.insert(object(row, 0)?);
    }
    Ok(roots.into_iter().collect())
}

/// The in-memory closure of one mark, before `keptWaste` is attributed.
struct Closure {
    /// Every reached object and its winning locator.
    visited: BTreeMap<ObjectId, ObjectLocation>,
    /// The stored width a read charges each reached object's record.
    widths: BTreeMap<ObjectId, u64>,
    /// Objects reached as a direct delta base.
    bases: BTreeSet<ObjectId>,
    /// Objects reached as a root or through a logical reference.
    logical: BTreeSet<ObjectId>,
}

impl Closure {
    /// The retained packs: those holding at least one reached object.
    fn packs(&self) -> Vec<i64> {
        self.visited
            .values()
            .map(|location| location.pack_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

/// Physical input for the shared encoding readers over one mark transaction.
struct ClosureSource<'a> {
    mechanics: &'a dyn crate::backend::Mechanics,
}

impl Source for ClosureSource<'_> {
    fn ordered_dependencies(&self) -> bool {
        false
    }

    fn location(&self, id: ObjectId, ceiling: i64) -> Result<Option<ObjectLocation>, StorageError> {
        let mut found = Vec::new();
        self.mechanics
            .locate(&[id], &mut found)
            .map_err(StorageError::from)?;
        Ok(found
            .into_iter()
            .find(|row| row.location.object_id == id && row.location.pack_id <= ceiling)
            .map(|row| row.location))
    }

    fn pack_bytes(&self, pack_id: i64) -> Result<Vec<u8>, StorageError> {
        let mut found = Vec::new();
        self.mechanics
            .read_packs(&[pack_id], &mut found)
            .map_err(StorageError::from)?;
        let pack = found.pop().ok_or(StorageError::Integrity("mark pack"))?;
        if !found.is_empty() || pack.info().pack_id != pack_id {
            return Err(StorageError::Integrity("mark pack binding"));
        }
        Ok(pack.into_parts().1)
    }

    fn inode_values(&self, ordinals: &[u32]) -> Result<Vec<[u8; INODE_VALUE_BYTES]>, StorageError> {
        let mut rows = Vec::new();
        self.mechanics
            .inode_values(ordinals, &mut rows)
            .map_err(StorageError::from)?;
        ordinals
            .iter()
            .map(|ordinal| {
                rows.iter()
                    .find(|row| row.ordinal == *ordinal)
                    .map(|row| row.value)
                    .ok_or(StorageError::Integrity("mark inode value"))
            })
            .collect()
    }

    fn signatures(&self) -> Result<Vec<SignatureRow>, StorageError> {
        let mut rows = Vec::new();
        self.mechanics
            .signatures(&mut rows)
            .map_err(StorageError::from)?;
        Ok(rows)
    }

    fn write_signatures(&self, _rows: &[SignatureRow]) -> Result<usize, StorageError> {
        Err(StorageError::Integrity("mark is read-only"))
    }
}

/// Admits every object reachable from `roots` through logical references and
/// delta bases, exactly once, in a queue that runs to empty.
fn walk(
    source: &ClosureSource<'_>,
    capacities: &StorageCapacities,
    construction: ConstructionPolicy,
    roots: Vec<ObjectId>,
) -> HistoryResult<Closure> {
    let mut packs = PackCache::new();
    let mut pool = PoolReader::new();
    let mut groups = GroupCache::new();
    let mut workspace = DecompressionWorkspace::new().map_err(storage)?;
    let mut counters = ChainCounters::default();
    let mut closure = Closure {
        visited: BTreeMap::new(),
        widths: BTreeMap::new(),
        bases: BTreeSet::new(),
        logical: roots.iter().copied().collect(),
    };
    let mut queued: BTreeSet<ObjectId> = roots.iter().copied().collect();
    let mut queue: VecDeque<ObjectId> = roots.into_iter().collect();
    while let Some(id) = queue.pop_front() {
        let location = source
            .location(id, i64::MAX)
            .map_err(storage)?
            .ok_or(HistoryError::Integrity("mark object locator"))?;
        let canonical = {
            let mut resolver = Resolver::new(
                source,
                i64::MAX,
                capacities,
                BodyCaches {
                    packs: &mut packs,
                    pool: &mut pool,
                },
                &mut groups,
                &mut workspace,
                &mut counters,
            );
            resolver.resolve(id).map_err(storage)?.0
        };
        let role = location.role;
        let scope = if role == ObjectRole::FilesystemRoot {
            FilesystemRoot::decode(&canonical)
                .map_err(|_| HistoryError::Integrity("mark root scope"))?
                .scope()
        } else {
            // No other admitted role reads the scope; the leaf encoders that
            // derive references ignore it.
            InodeScope::from_object(ObjectId::for_bytes(b"gc.mark/object-closure"))
        };
        let admitted = Timing::disabled("gc.mark.admit", |timing| {
            FinalizedObject::admit(
                id,
                role,
                canonical,
                construction,
                scope,
                timing.child("admit"),
            )
        })
        .0
        .map_err(|_| HistoryError::Integrity("mark object admission"))?;
        for reference in admitted.references() {
            closure.logical.insert(*reference);
            if queued.insert(*reference) {
                queue.push_back(*reference);
            }
        }
        let edge = {
            let mut chain = ChainBases::new(&mut packs);
            chain
                .edge(source, &mut workspace, &location)
                .map_err(storage)?
        };
        closure.widths.insert(id, edge.width);
        if let Some(base) = edge.base {
            closure.bases.insert(base);
            if queued.insert(base) {
                queue.push_back(base);
            }
        }
        closure.visited.insert(id, location);
    }
    Ok(closure)
}

/// Attributes the bytes a retained pack keeps for records no reached object
/// owns, the delta-base-only bytes and the inode-value block bytes.
fn waste(
    tx: &Transaction<'_>,
    source: &ClosureSource<'_>,
    closure: &Closure,
) -> HistoryResult<KeptWaste> {
    let rows = tx.query(LOCATIONS, Vec::<Param>::new())?;
    // pack -> group -> (record, object): the pack/object inventory.
    let mut inventory: BTreeMap<i64, BTreeMap<usize, BTreeMap<usize, ObjectId>>> = BTreeMap::new();
    for row in &rows {
        let id = object(row, 0)?;
        let pack: i64 = cell(row, 1)?;
        let group = usize::try_from(cell::<i64>(row, 2)?)
            .map_err(|_| HistoryError::Integrity("mark pack inventory"))?;
        let record = usize::try_from(cell::<i64>(row, 3)?)
            .map_err(|_| HistoryError::Integrity("mark pack inventory"))?;
        inventory
            .entry(pack)
            .or_default()
            .entry(group)
            .or_default()
            .insert(record, id);
    }
    let mut dead_in_retained_packs_bytes = 0u64;
    for pack_id in closure.packs() {
        let body = source.pack_bytes(pack_id).map_err(storage)?;
        let header = layout::parse_header(&body).map_err(storage)?;
        let groups = inventory.get(&pack_id);
        for group_number in 0..header.group_count {
            let view = layout::group_view(&body, header, group_number).map_err(storage)?;
            let records = groups.and_then(|groups| groups.get(&group_number));
            let live = records
                .is_some_and(|records| records.values().any(|id| closure.visited.contains_key(id)));
            if !live {
                // No reached object owns any record of this group, so its
                // stored extent stays only because the pack is retained.
                dead_in_retained_packs_bytes = dead_in_retained_packs_bytes
                    .saturating_add(u64::try_from(view.end - view.start).unwrap_or(u64::MAX));
            } else if view.codec == GroupCodec::Raw {
                // An exact per-record split is possible only when the body is
                // stored uncompressed. A compressed group's bytes cannot be
                // attributed to one record, so a retained group contributes
                // nothing and the error stays conservative.
                if let Some(records) = records {
                    for (record, id) in records {
                        if closure.visited.contains_key(id) {
                            continue;
                        }
                        let framed =
                            framed_record(&body[view.start..view.end], *record).map_err(storage)?;
                        dead_in_retained_packs_bytes = dead_in_retained_packs_bytes
                            .saturating_add(u64::try_from(framed.len()).unwrap_or(u64::MAX));
                    }
                }
            }
        }
    }
    let deltas: Vec<ObjectId> = closure
        .bases
        .difference(&closure.logical)
        .copied()
        .collect();
    let delta_bases_bytes = deltas.iter().fold(0u64, |total, id| {
        total.saturating_add(closure.widths.get(id).copied().unwrap_or(0))
    });
    let inode = tx.query(INODE_BLOCKS, Vec::<Param>::new())?;
    let mut inode_blocks_bytes = 0u64;
    for row in &inode {
        let length: i64 = cell(row, 0)?;
        inode_blocks_bytes =
            inode_blocks_bytes.saturating_add(u64::try_from(length).unwrap_or(u64::MAX));
    }
    Ok(KeptWaste {
        dead_in_retained_packs_bytes,
        delta_bases_bytes,
        delta_base_references: deltas,
        inode_blocks_bytes,
    })
}

/// The §6.4 live-heads digest: BLAKE3 over `layerfs/live-heads/v1\0`, then the
/// non-retired stacks in identity order as `(stack_id, head_layer_id)`, then the
/// marker-free Branches in identity order as `(branch_id, head_commit_id or 33
/// zero bytes, base_layer_id)`. Every field is fixed width, so a retired row is
/// absent rather than hashed as zeros and timestamps never enter the vector.
fn live_heads_digest(tx: &Transaction<'_>) -> HistoryResult<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/live-heads/v1\0");
    let stacks = tx.query(LIVE_STACKS, Vec::<Param>::new())?;
    for row in &stacks {
        let id: Vec<u8> = cell(row, 0)?;
        let head: Vec<u8> = cell(row, 1)?;
        if id.len() != 17 || head.len() != 33 {
            return Err(HistoryError::Integrity("live-heads stack width"));
        }
        hasher.update(&id);
        hasher.update(&head);
    }
    let branches = tx.query(LIVE_BRANCHES, Vec::<Param>::new())?;
    for row in &branches {
        let id: Vec<u8> = cell(row, 0)?;
        let head: Option<Vec<u8>> = cell(row, 1)?;
        let base: Vec<u8> = cell(row, 2)?;
        if id.len() != 17 || base.len() != 33 {
            return Err(HistoryError::Integrity("live-heads branch width"));
        }
        hasher.update(&id);
        match head {
            Some(head) if head.len() == 33 => hasher.update(&head),
            Some(_) => return Err(HistoryError::Integrity("live-heads commit width")),
            None => hasher.update(&[0u8; 33]),
        };
        hasher.update(&base);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// One physical read failure as a mark failure; an unproven outcome is never
/// folded into an integrity refusal.
fn storage(error: impl Into<StorageError>) -> HistoryError {
    let error = error.into();
    if error.is_unknown_outcome() {
        HistoryError::UnknownOutcome
    } else {
        HistoryError::Integrity("mark object closure")
    }
}
