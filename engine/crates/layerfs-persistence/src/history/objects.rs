//! The §6.2 object closure of a mark, its `keptWaste` and the §6.4 live-heads
//! digest.
//!
//! A mark starts at the roots of every Commit and Layer the §6.1 history
//! closure retained. Every locator is loaded once, in pack order, and the walk
//! follows each object's direct logical references and its record-header delta
//! base to the fixpoint. The walk is metadata-only: a role that names no other
//! object costs one header read, not a payload decode, and reference-bearing
//! pages are decoded without a second identity hash. The retained pack of every
//! reached object is recorded, and the bytes of records in those packs that no
//! reached object owns are reported as `keptWaste` from a bounded directory
//! prefix read. Nothing is deleted, rewritten or compacted here; the closure is
//! kept in memory for the rebuild path to use.
use super::kept::{self, KeptHistory};
use super::provider::HistoryProvider;
use super::rows::{cell, object};
use super::transaction::Transaction;
use crate::backend::records::Param;
use crate::backend::sqlite::session::transaction::Completion;
use layerfs_content::filesystem::{FilesystemRoot, InodeScope};
use layerfs_content::inode_leaf::INODE_VALUE_BYTES;
use layerfs_content::object::decode_references;
use layerfs_content::{ConstructionPolicy, ObjectId, ObjectRole};
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::records::GcIntent;
use layerfs_storage::{
    encoding::pool::PoolReader,
    encoding::{
        delta::read::{locator_key, BodyCaches, ChainBases, ChainCounters, Resolver},
        framed_record, framed_record_count, DecompressionWorkspace, GroupCache, PackAcquisition,
        PackCache, PackUnit,
    },
    location::{ObjectLocation, PackInfo, SignatureRow},
    pack::layout::{self, GroupCodec, PackLane},
    port::{
        AcquiredPackRead, PackRange, PackReadChoice, PackReadPlan, PersistenceError,
        PACK_READ_PREFIX_BYTES,
    },
    source::Source,
    StorageCapacities, StorageError,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

mod impls;
const ROOTS: &str = "gc_kept_roots";
const LOCATIONS: &str = "gc_object_locations";
const LIVE_STACKS: &str = "gc_live_stacks";
const LIVE_BRANCHES: &str = "gc_live_branches";
const INODE_BLOCKS: &str = "gc_inode_blocks";
const TUPLE: &str = "gc_tuple_read";
/// The reported kept delta-base list is capped here; the byte total is not.
const MAX_KEPT_WASTE_REFERENCES: usize = 1024;

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
    /// Whether [`Self::delta_base_references`] was cut at
    /// [`MAX_KEPT_WASTE_REFERENCES`] before it was stored.
    delta_base_references_capped: bool,
    inode_blocks_bytes: u64,
}

/// The object half of a mark: every object and pack the retained history
/// reaches (§6.2), in ascending order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeptObjects {
    objects: Vec<ObjectId>,
    packs: Vec<i64>,
    waste: KeptWaste,
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
    mark_bytes: u64,
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
        self.write_snapshot(mark_gc)
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
        self.write_snapshot(mark)
    }

    /// The §6.3 compaction mark: the same closure and digest, with every
    /// reached object streamed into the compaction's temp-file visited table
    /// (`<store>.compact-visited`) as it is discovered. The §7 build attaches
    /// that same file, so it never rebuilds an in-memory `object_id -> pack`
    /// map from a second `object_location` scan. On PostgreSQL the visited set
    /// is the `pg_temp` table the Phase M mark fills, so the path is unused.
    ///
    /// A failed mark drops the sink, which removes the scratch; the caller's
    /// artifact set never keeps a half-written table.
    pub fn mark_with_scratch(&self, scratch: &Path) -> HistoryResult<MarkResult> {
        match &self.engine {
            crate::backend::Engine::Sqlite(session) => session.run(true, |backend| {
                let tx = Transaction::new(backend);
                let sink = crate::backend::sqlite::compact_visit::VisitSink::create(scratch)?;
                let mark = mark_recorded(&tx, Some(&sink))?;
                sink.finish()?;
                Ok(mark)
            }),
            #[cfg(feature = "postgresql")]
            crate::backend::Engine::Postgres(_) => self.write_snapshot(mark),
        }
    }

    /// The §6.3 compaction mark under one admission intent, as one write
    /// transaction whose caller decides whether it survives.
    ///
    /// `decide` sees the completed [`MarkResult`] and returns `true` to keep
    /// the transaction (a `rebuild`) or `false` to roll it back (a `none`). §5.1
    /// registers the exclusive operation intent inside the same transaction, so
    /// a rollback takes it with the closure: the live Store then keeps no
    /// `kept_node` row, no `gc_intent` row and no committed WAL frame, and the
    /// §3 `none` path writes nothing that survives the call. The §6.3 visited
    /// scratch is a separate temp database on both paths; the caller deletes it
    /// after a rollback and keeps it for the rebuild's build phase.
    pub(crate) fn mark_for_compaction(
        &self,
        scratch: &Path,
        intent: &GcIntent,
        decide: impl FnOnce(&MarkResult) -> HistoryResult<bool>,
    ) -> HistoryResult<(MarkResult, bool)> {
        match &self.engine {
            crate::backend::Engine::Sqlite(session) => session.run_decided(true, |backend| {
                let tx = Transaction::new(backend);
                super::admission::register_in(&tx, intent, false)?;
                let sink = crate::backend::sqlite::compact_visit::VisitSink::create(scratch)?;
                let mark = mark_recorded(&tx, Some(&sink))?;
                sink.finish()?;
                let keep = decide(&mark)?;
                let completion = if keep {
                    Completion::Commit
                } else {
                    Completion::Rollback
                };
                Ok(((mark, keep), completion))
            }),
            #[cfg(feature = "postgresql")]
            crate::backend::Engine::Postgres(_) => {
                let mark = self.write_snapshot(mark)?;
                let keep = decide(&mark)?;
                Ok((mark, keep))
            }
        }
    }
}

fn mark(tx: &Transaction<'_>) -> HistoryResult<MarkResult> {
    mark_recorded(tx, None)
}

fn mark_recorded(
    tx: &Transaction<'_>,
    visit: Option<&crate::backend::sqlite::compact_visit::VisitSink>,
) -> HistoryResult<MarkResult> {
    let history = kept::mark(tx)?;
    let policy = tx.mechanics().policy().map_err(storage)?;
    let capacities = StorageCapacities::from_policy(policy).map_err(storage)?;
    // Every locator is loaded once, in pack order. The walk's queue and the
    // kept-waste attribution share that one read, so a mark never asks the
    // engine for a single object location again.
    let located = locate_all(tx)?;
    let source = ClosureSource {
        mechanics: tx.mechanics(),
        locations: &located.locations,
        read_bytes: std::cell::RefCell::new(BTreeMap::new()),
    };
    let roots = roots(tx)?;
    let closure = walk(&source, &capacities, policy.construction(), roots, visit)?;
    let waste = waste(tx, &source, &located, &closure)?;
    let mark_bytes = source.read_bytes();
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
        mark_bytes,
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

/// Every stored locator, read once in pack order.
///
/// `locations` answers the walk; `inventory` answers kept-waste attribution.
/// They are two views of the same single `gc_object_locations` read, so the
/// mark no longer loads `object_location` twice (once per object and once for
/// the waste inventory).
struct Located {
    /// The winning locator of every object, keyed by identity.
    locations: BTreeMap<ObjectId, ObjectLocation>,
    /// pack -> group -> (record, object): the pack/object inventory.
    inventory: BTreeMap<i64, BTreeMap<usize, BTreeMap<usize, ObjectId>>>,
}

fn locate_all(tx: &Transaction<'_>) -> HistoryResult<Located> {
    let rows = tx.query(LOCATIONS, Vec::<Param>::new())?;
    let mut located = Located {
        locations: BTreeMap::new(),
        inventory: BTreeMap::new(),
    };
    for row in &rows {
        let id = object(row, 0)?;
        let pack: i64 = cell(row, 1)?;
        let group = usize::try_from(cell::<i64>(row, 2)?)
            .map_err(|_| HistoryError::Integrity("mark pack inventory"))?;
        let record = usize::try_from(cell::<i64>(row, 3)?)
            .map_err(|_| HistoryError::Integrity("mark pack inventory"))?;
        let role = ObjectRole::from_code(
            u8::try_from(cell::<i64>(row, 4)?)
                .map_err(|_| HistoryError::Integrity("mark object role"))?,
        )
        .map_err(|_| HistoryError::Integrity("mark object role"))?;
        let canonical_length = usize::try_from(cell::<i64>(row, 5)?)
            .map_err(|_| HistoryError::Integrity("mark object length"))?;
        located
            .inventory
            .entry(pack)
            .or_default()
            .entry(group)
            .or_default()
            .insert(record, id);
        located.locations.entry(id).or_insert(ObjectLocation {
            object_id: id,
            role,
            canonical_length,
            pack_id: pack,
            group_number: group,
            record_number: record,
        });
    }
    Ok(located)
}

/// The in-memory closure of one mark, before `keptWaste` is attributed./// The in-memory closure of one mark, before `keptWaste` is attributed.
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
    /// Every locator, exactly as the one `gc_object_locations` read returned it.
    locations: &'a BTreeMap<ObjectId, ObjectLocation>,
    /// pack -> the body bytes this mark read from it: a whole body, or the
    /// disjoint group extents it acquired. The sum is
    /// [`MarkResult::mark_bytes`]: a pack read in several groups counts each
    /// extent once, and a later whole-body read replaces the partial sum.
    read_bytes: std::cell::RefCell<BTreeMap<i64, u64>>,
}

impl ClosureSource<'_> {
    /// Records a whole-body read of `pack_id`. The full extent is the pack's
    /// body, so it replaces any group extents already counted for the pack.
    fn note_whole_body(&self, pack_id: i64, bytes: u64) {
        let mut read = self.read_bytes.borrow_mut();
        let entry = read.entry(pack_id).or_insert(0);
        *entry = (*entry).max(bytes);
    }

    /// Records one scoped acquisition's body bytes, capped at the pack's
    /// declared length. The group cache asks for only groups it does not hold,
    /// so the extents are disjoint; the controls each acquisition repeats can
    /// still not make a pack count for more than its body.
    fn note_scoped_body(&self, pack_id: i64, bytes: u64, length: u64) {
        let mut read = self.read_bytes.borrow_mut();
        let entry = read.entry(pack_id).or_insert(0);
        *entry = entry.saturating_add(bytes).min(length);
    }

    /// The distinct pack body bytes this mark read.
    fn read_bytes(&self) -> u64 {
        self.read_bytes.borrow().values().copied().sum()
    }
}

/// The complete encoded groups one mark walk demands from one pack.
///
/// A metadata-only walk needs one record header per object and one delta edge
/// per record, so the demand is sparse; the plan selects those exact group
/// ranges and the backend reads only them, never the body behind the
/// directory. A dense demand or a cache that already holds another group of
/// the pack still materializes the whole body once.
struct MarkGroups<'a> {
    pack_id: i64,
    groups: &'a [usize],
    whole: bool,
}

impl PackReadPlan for MarkGroups<'_> {
    fn select(
        &mut self,
        info: PackInfo,
        prefix: &[u8],
    ) -> Result<PackReadChoice, PersistenceError> {
        if info.pack_id != self.pack_id || self.groups.is_empty() {
            return Err(PersistenceError::Malformed);
        }
        let header = layout::parse_directory_header(prefix, info.length)
            .map_err(|_| PersistenceError::Malformed)?;
        if self.whole || header.lane == PackLane::Singleton || info.length <= PACK_READ_PREFIX_BYTES
        {
            return Ok(PackReadChoice::Whole);
        }
        let views = layout::directory_group_views(prefix, header)
            .map_err(|_| PersistenceError::Malformed)?;
        let mut ranges: Vec<PackRange> = Vec::new();
        let mut useful = 0usize;
        let mut previous = None;
        for &group in self.groups {
            if previous.is_some_and(|prior| prior >= group) {
                return Err(PersistenceError::Malformed);
            }
            previous = Some(group);
            let view = *views.get(group).ok_or(PersistenceError::Malformed)?;
            useful = useful
                .checked_add(view.end - view.start)
                .ok_or(PersistenceError::Malformed)?;
            if let Some(prior) = ranges
                .last_mut()
                .filter(|prior| prior.offset + prior.length == view.start)
            {
                prior.length += view.end - view.start;
            } else {
                ranges.push(PackRange {
                    offset: view.start,
                    length: view.end - view.start,
                });
            }
        }
        if useful >= info.length.div_ceil(2) || ranges.len() > 4 {
            Ok(PackReadChoice::Whole)
        } else {
            Ok(PackReadChoice::Ranges(ranges))
        }
    }
}

impl Source for ClosureSource<'_> {
    fn ordered_dependencies(&self) -> bool {
        false
    }

    fn location(&self, id: ObjectId, ceiling: i64) -> Result<Option<ObjectLocation>, StorageError> {
        Ok(self
            .locations
            .get(&id)
            .filter(|location| location.pack_id <= ceiling)
            .copied())
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
        let body = pack.into_parts().1;
        self.note_whole_body(pack_id, u64::try_from(body.len()).unwrap_or(u64::MAX));
        Ok(body)
    }

    fn pack_prefix(
        &self,
        pack_id: i64,
        limit: usize,
    ) -> Result<Option<(usize, Vec<u8>)>, StorageError> {
        self.mechanics
            .pack_prefix(pack_id, limit)
            .map_err(StorageError::from)
    }
    fn acquire_groups(
        &self,
        pack_id: i64,
        groups: &[usize],
        whole_for_reuse: bool,
    ) -> Result<PackAcquisition, StorageError> {
        let mut plan = MarkGroups {
            pack_id,
            groups,
            whole: whole_for_reuse,
        };
        let reply = self
            .mechanics
            .read_scoped_pack(pack_id, &mut plan)
            .map_err(StorageError::from)?;
        match reply {
            AcquiredPackRead::Whole(row) => {
                let (info, body) = row.into_parts();
                self.note_whole_body(pack_id, u64::try_from(body.len()).unwrap_or(u64::MAX));
                Ok(PackAcquisition::Whole {
                    info: Some(info),
                    body,
                })
            }
            AcquiredPackRead::Units(row) => {
                // The selected complete groups are the physical body bytes
                // this scoped read transfers; the group cache asks for each
                // group once, so the extents do not repeat.
                let length = u64::try_from(row.info().length).unwrap_or(u64::MAX);
                self.note_scoped_body(
                    pack_id,
                    u64::try_from(row.acquired_bytes()).unwrap_or(u64::MAX),
                    length,
                );
                Ok(PackAcquisition::Units(PackUnit::from_acquired(row)?))
            }
        }
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
///
/// The walk is metadata-only: a role that names no other object
/// ([`ObjectRole::has_references`]) costs one record header read, not a payload
/// decode, and each wave is processed in locator order so one pack body answers
/// every object of its group. Reference-bearing metadata pages are still
/// authenticated and decoded, but identity is not hashed a second time.
fn walk(
    source: &ClosureSource<'_>,
    capacities: &StorageCapacities,
    construction: ConstructionPolicy,
    roots: Vec<ObjectId>,
    visit: Option<&crate::backend::sqlite::compact_visit::VisitSink>,
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
    let mut frontier: Vec<ObjectId> = roots;
    while !frontier.is_empty() {
        // Pack order: one body answers every object of its group, so a wave
        // sorted by locator reads each group once instead of once per identity.
        frontier.sort_unstable_by_key(|id| {
            source
                .locations
                .get(id)
                .map_or((i64::MAX, usize::MAX, usize::MAX), locator_key)
        });
        let mut next = Vec::new();
        for id in std::mem::take(&mut frontier) {
            let location = source
                .location(id, i64::MAX)
                .map_err(storage)?
                .ok_or(HistoryError::Integrity("mark object locator"))?;
            let role = location.role;
            let references = if role.has_references() {
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
                    resolver.resolve_at(location).map_err(storage)?.0
                };
                let scope = if role == ObjectRole::FilesystemRoot {
                    FilesystemRoot::decode(&canonical)
                        .map_err(|_| HistoryError::Integrity("mark root scope"))?
                        .scope()
                } else {
                    // No other admitted role reads the scope; the leaf encoders
                    // that derive references ignore it.
                    InodeScope::from_object(ObjectId::for_bytes(b"gc.mark/object-closure"))
                };
                decode_references(role, &canonical, construction, scope)
                    .map_err(|_| HistoryError::Integrity("mark object references"))?
            } else {
                Vec::new()
            };
            for reference in references {
                closure.logical.insert(reference);
                if queued.insert(reference) {
                    next.push(reference);
                }
            }
            // The base identity lives in the record header, so the edge is read
            // from the header and never by decoding the chain payload.
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
                    next.push(base);
                }
            }
            if let Some(visit) = visit {
                // Streaming rows as objects are discovered keeps the scratch a
                // complete §6.2 closure at every step, not a post-pass copy.
                visit.visit(id, location.pack_id)?;
            }
            closure.visited.insert(id, location);
        }
        frontier = next;
    }
    Ok(closure)
}

/// Attributes the bytes a retained pack keeps for records no reached object
/// owns, the delta-base-only bytes and the inode-value block bytes.
///
/// The pack directory sits at the head of every body, so the common cases are
/// answered from a bounded prefix read: a wholly dead group's extent and a
/// retained compressed group (which contributes nothing) need no body. Only a
/// retained uncompressed group that still holds a dead record needs the body,
/// because its exact per-record split lives inside the group.
fn waste(
    tx: &Transaction<'_>,
    source: &ClosureSource<'_>,
    located: &Located,
    closure: &Closure,
) -> HistoryResult<KeptWaste> {
    let mut dead_in_retained_packs_bytes = 0u64;
    for pack_id in closure.packs() {
        let groups = located.inventory.get(&pack_id);
        // The pack row may reserve more capacity than it uses; the header names
        // the used length. Everything below parses against `used`, never the
        // reserved capacity. A prefix that already covers `used` is the body.
        let Some((capacity, prefix)) = source
            .pack_prefix(pack_id, PACK_READ_PREFIX_BYTES)
            .map_err(storage)?
        else {
            return Err(HistoryError::Integrity("mark pack directory"));
        };
        if prefix.len() < layout::USED_OFFSET + 4 {
            return Err(HistoryError::Integrity("mark pack length"));
        }
        let used = {
            let field = &prefix[layout::USED_OFFSET..layout::USED_OFFSET + 4];
            u32::from_le_bytes(
                field
                    .try_into()
                    .map_err(|_| HistoryError::Integrity("mark pack length"))?,
            ) as usize
        };
        if used < layout::HEADER_LEN || used > capacity {
            return Err(HistoryError::Integrity("mark pack length"));
        }
        if prefix.len() >= used {
            dead_in_retained_packs_bytes = dead_in_retained_packs_bytes.saturating_add(pack_waste(
                &prefix[..used],
                groups,
                closure,
            )?);
            continue;
        }
        let header = layout::parse_directory_header(&prefix, used).map_err(storage)?;
        let views = layout::directory_group_views(&prefix, header).map_err(storage)?;
        // A wholly dead group's stored extent is known from the directory. A
        // retained Raw group whose inventory no longer accounts for every
        // record is read on its own, so its exact per-record split never costs
        // the whole pack body.
        let mut mixed_raw = Vec::new();
        for (number, view) in views.iter().enumerate() {
            let records = groups.and_then(|groups| groups.get(&number));
            let live = records
                .is_some_and(|records| records.values().any(|id| closure.visited.contains_key(id)));
            if !live {
                dead_in_retained_packs_bytes = dead_in_retained_packs_bytes
                    .saturating_add(u64::try_from(view.end - view.start).unwrap_or(u64::MAX));
            } else if view.codec == GroupCodec::Raw
                && group_may_hide_dead_records(*view, records, closure)
            {
                mixed_raw.push((number, *view));
            }
        }
        if mixed_raw.is_empty() {
            continue;
        }
        let numbers: Vec<usize> = mixed_raw.iter().map(|(number, _)| *number).collect();
        let mut cache = PackCache::new();
        cache
            .acquire_groups(source, pack_id, &numbers)
            .map_err(storage)?;
        for (number, view) in mixed_raw {
            let slice = cache
                .group(source, pack_id, number)
                .map_err(storage)?
                .ok_or(HistoryError::Integrity("mark group acquisition"))?;
            if slice.view.start != view.start || slice.view.end != view.end {
                return Err(HistoryError::Integrity("mark group acquisition"));
            }
            let records = groups.and_then(|groups| groups.get(&number));
            dead_in_retained_packs_bytes = dead_in_retained_packs_bytes
                .saturating_add(dead_raw_record_bytes(slice.bytes, records, closure)?);
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
    // The report names at most this many kept delta bases. The byte total stays
    // exact; only the list is bounded, and [`KeptWaste::from_parts`] records
    // that it was cut so the §11 projection can render `{value, cap}`.
    let delta_base_references = deltas;
    let inode = tx.query(INODE_BLOCKS, Vec::<Param>::new())?;
    let mut inode_blocks_bytes = 0u64;
    for row in &inode {
        let length: i64 = cell(row, 0)?;
        inode_blocks_bytes =
            inode_blocks_bytes.saturating_add(u64::try_from(length).unwrap_or(u64::MAX));
    }
    Ok(KeptWaste::from_parts(
        dead_in_retained_packs_bytes,
        delta_bases_bytes,
        delta_base_references,
        inode_blocks_bytes,
    ))
}

/// Whether a retained Raw group's `object_location` inventory still explains
/// its whole stored extent. A rebuild copies a retained pack whole but keeps
/// only the reached records' rows, so a later mark sees a dead record that no
/// row names; comparing the directory extent with the live records' framing and
/// widths detects that case without reading the body.
fn group_may_hide_dead_records(
    view: layout::GroupView,
    records: Option<&BTreeMap<usize, ObjectId>>,
    closure: &Closure,
) -> bool {
    let (live_records, live_width) = records.map_or((0u64, 0u64), |records| {
        records.values().fold((0u64, 0u64), |(count, width), id| {
            if closure.visited.contains_key(id) {
                (
                    count + 1,
                    width.saturating_add(closure.widths.get(id).copied().unwrap_or(0)),
                )
            } else {
                (count, width)
            }
        })
    });
    let extent = u64::try_from(view.end - view.start).unwrap_or(u64::MAX);
    extent != 4 + 4 * live_records + live_width
}

/// The framed bytes of every record in a Raw group that no reached object owns,
/// including a record whose `object_location` row a rebuild did not retain. The
/// caller passes a group body it already read.
fn dead_raw_record_bytes(
    group: &[u8],
    records: Option<&BTreeMap<usize, ObjectId>>,
    closure: &Closure,
) -> HistoryResult<u64> {
    let count = framed_record_count(group).map_err(storage)?;
    let mut dead = 0u64;
    for record in 0..count {
        let live = records
            .and_then(|records| records.get(&record))
            .is_some_and(|id| closure.visited.contains_key(id));
        if live {
            continue;
        }
        let framed = framed_record(group, record).map_err(storage)?;
        dead = dead.saturating_add(u64::try_from(framed.len()).unwrap_or(u64::MAX));
    }
    Ok(dead)
}

/// One retained pack's dead bytes, exact over a complete body.
fn pack_waste(
    body: &[u8],
    groups: Option<&BTreeMap<usize, BTreeMap<usize, ObjectId>>>,
    closure: &Closure,
) -> HistoryResult<u64> {
    let header = layout::parse_header(body).map_err(storage)?;
    let mut dead = 0u64;
    for group_number in 0..header.group_count {
        let view = layout::group_view(body, header, group_number).map_err(storage)?;
        let records = groups.and_then(|groups| groups.get(&group_number));
        let live = records
            .is_some_and(|records| records.values().any(|id| closure.visited.contains_key(id)));
        if !live {
            // No reached object owns any record of this group, so its stored
            // extent stays only because the pack is retained.
            dead = dead.saturating_add(u64::try_from(view.end - view.start).unwrap_or(u64::MAX));
        } else if view.codec == GroupCodec::Raw
            && group_may_hide_dead_records(view, records, closure)
        {
            // An exact per-record split is possible only when the body is
            // stored uncompressed. A compressed group's bytes cannot be
            // attributed to one record, so a retained group contributes
            // nothing and the error stays conservative.
            dead = dead.saturating_add(dead_raw_record_bytes(
                &body[view.start..view.end],
                records,
                closure,
            )?);
        }
    }
    Ok(dead)
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
