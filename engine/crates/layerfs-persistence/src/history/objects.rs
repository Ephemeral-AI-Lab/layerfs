//! The §6.2 object closure of a mark, its `keptWaste` and the §6.4 live-heads
//! digest.
//!
//! A mark starts at the roots of every Commit and Layer the §6.1 history
//! closure retained and follows each object's direct logical references and
//! its record-header delta base to the fixpoint. The walk is metadata-only: a
//! role that names no other object costs one header read, not a payload
//! decode, and reference-bearing pages are decoded without a second identity
//! hash. Nothing is deleted, rewritten or compacted here.
//!
//! Memory does not follow the Store's size: nothing a mark holds grows with
//! the objects, the packs or the Commits. The locators stay in the Store:
//! they are read in bounded pieces ([`table`]), and the walk keeps two bits
//! per object for what it reached, not the objects, in an array it holds a
//! fixed number of pages of ([`paged`]). The history closure stays in
//! `kept_node` and is read a page at a time. What a wave reached, the order
//! the next wave is walked in and the records the kept-waste pass attributes
//! are sorted in bounded memory and written to unnamed files beyond it
//! ([`spill`]), and so are the retained packs and the inode value ordinals
//! ([`lists`]). The reached objects, the history nodes, the packs and the
//! ordinals are listed, when a caller wants them, by reading them once more.
use super::kept::{self, KeptHistory};
use super::provider::HistoryProvider;
use super::rows::cell;
use super::transaction::Transaction;
use crate::backend::records::Param;
use crate::backend::sqlite::session::transaction::Completion;
use layerfs_content::inode_leaf::INODE_VALUE_BYTES;
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::records::GcIntent;
use layerfs_storage::{
    encoding::{delta::edge::RecordDirectory, PackAcquisition, PackUnit},
    location::{ObjectLocation, SignatureRow},
    pack::layout::{self, GroupCodec},
    port::AcquiredPackRead,
    source::Source,
    StorageCapacities, StorageError,
};
use std::path::Path;
use std::sync::{Arc, Mutex};

mod edges;
mod ids;
mod impls;
mod lists;
mod paged;
mod plan;
mod records;
pub(crate) mod spill;
mod table;
mod values;
mod walk;
mod waste;

use ids::IdMap;
pub use lists::{Sequence, Spans};
use plan::MarkGroups;
use spill::{Sealed, Sorter, Space};
use table::Locators;
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
///
/// What it always holds is bounded: a count of the reached objects, and the
/// retained packs, the inode value ordinals read and the partly dead packs
/// each in a list that leaves memory for a sort file when it is long
/// ([`lists`]). A mark that lists what it kept also holds them as slices.
#[derive(Clone, Debug)]
pub struct KeptObjects {
    /// The reached objects, when the mark listed them: [`HistoryProvider::mark`]
    /// does, a SQLite compaction mark writes them to its scratch instead.
    objects: Vec<ObjectId>,
    /// How many objects the mark reached, listed or not.
    count: u64,
    /// The retained packs: those holding at least one reached object.
    retained: Sequence,
    /// [`Self::retained`] as a slice, when the mark listed them.
    packs: Vec<i64>,
    waste: KeptWaste,
    /// Retained packs that hold a record no reached object owns, in ascending
    /// pack order, each with its share of
    /// [`KeptWaste::dead_in_retained_packs_bytes`].
    partly_dead: PartlyDead,
    /// [`Self::partly_dead`] as a list, when the mark listed them.
    partly_dead_listed: Vec<(i64, u64)>,
    /// The inode value ordinals the reached leaves read.
    ordinals: Spans,
    /// [`Self::ordinals`] as inclusive runs, when the mark listed them.
    inode_ordinals: Vec<(u32, u32)>,
}

/// The partly dead packs of one mark with their dead bytes, ascending.
#[derive(Clone, Default)]
pub(in crate::history) struct PartlyDead {
    packs: Option<Arc<Mutex<Sorter<records::Count>>>>,
    count: u64,
}

impl std::fmt::Debug for PartlyDead {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "PartlyDead({})", self.count)
    }
}

/// One complete mark: the §6.1 history closure, the §6.2 object closure and the
/// §6.4 digest of the live heads.
///
/// A rebuild path chooses its plan from [`Self::objects`] and reports
/// [`Self::live_heads_digest`] to prove the generation it installed; nothing in
/// this value has been compacted or deleted.
#[derive(Clone, Debug)]
pub struct MarkResult {
    history: KeptHistory,
    objects: KeptObjects,
    live_heads_digest: [u8; 32],
    mark_bytes: u64,
    /// Where the mark's sort files went, and how much they held together.
    /// A compaction puts its own sort files in the same space.
    space: Arc<Space>,
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
        let budget = self.mark_budget();
        self.write_snapshot(|tx| mark_gc(tx, budget))
    }

    /// The §6.4 live-heads digest of the Store as it is now, without a mark.
    pub fn live_heads_digest(&self) -> HistoryResult<[u8; 32]> {
        self.read(live_heads_digest)
    }
}

fn mark_gc(tx: &Transaction<'_>, budget: u64) -> HistoryResult<(MarkResult, GcTuple)> {
    let tuple = gc_tuple(tx)?;
    let mark = mark(tx, budget)?;
    Ok((mark, tuple))
}

/// [`HistoryProvider::mark_gc`] inside a caller's engine transaction, with the
/// reached set instead of a list of it ([`KeptObjects::objects`] is empty),
/// and no list of the retained packs either: the caller reads
/// [`KeptObjects::retained_packs`] as a stream.
/// A PostgreSQL compaction decides and fills its visited table from
/// [`Reach::each`] in the transaction that walked, so the reached identities
/// are never held: the ranks a `Reach` names are those of this snapshot only.
pub(crate) fn mark_gc_reached(
    backend: &dyn crate::backend::HistoryStatements,
) -> HistoryResult<(MarkResult, GcTuple, Reach)> {
    let tx = Transaction::new(backend);
    let tuple = gc_tuple(&tx)?;
    let (mark, reach) = mark_recorded(&tx, false, spill::BUDGET_BYTES)?;
    Ok((mark, tuple, reach))
}

/// The §6.4 live-heads digest inside a caller's engine transaction, read a
/// bounded page at a time: what a PostgreSQL swap compares with its mark's
/// under the Store-wide lock.
pub(crate) fn live_heads_digest_in(
    backend: &dyn crate::backend::HistoryStatements,
) -> HistoryResult<[u8; 32]> {
    live_heads_digest(&Transaction::new(backend))
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
        let budget = self.mark_budget();
        self.write_snapshot(|tx| mark(tx, budget))
    }

    /// The §6.3 compaction mark: the same closure and digest, with every
    /// reached object written to the compaction's temp-file visited table
    /// (`<store>.compact-visited`) when the walk is done. The §7 build attaches
    /// that same file, so it never rebuilds an in-memory `object_id -> pack`
    /// map from a second `object_location` scan. On PostgreSQL the visited set
    /// is the `pg_temp` table the Phase M mark fills, so the path is unused.
    ///
    /// A failed mark drops the sink, which removes the scratch; the caller's
    /// artifact set never keeps a half-written table.
    pub fn mark_with_scratch(&self, scratch: &Path) -> HistoryResult<MarkResult> {
        let budget = self.mark_budget();
        match &self.engine {
            crate::backend::Engine::Sqlite(session) => {
                self.sqlite_write_session(session)?.run(true, |backend| {
                    let tx = Transaction::new(backend);
                    let sink = crate::backend::sqlite::compact_visit::VisitSink::create(scratch)?;
                    let (mut mark, mut reach) = mark_recorded(&tx, false, budget)?;
                    mark.objects.list()?;
                    sink.finish(&mark.objects.retained, |row| {
                        reach.each(tx.mechanics(), row)
                    })?;
                    Ok(mark)
                })
            }
            crate::backend::Engine::Postgres(_) => self.write_snapshot(|tx| mark(tx, budget)),
        }
    }

    /// The §6.3 compaction mark under one admission intent, as one write
    /// transaction whose caller decides whether it survives.
    ///
    /// `decide` sees the completed [`MarkResult`] and the transaction that
    /// made it, for what it still has to read, and returns `true` to keep
    /// the transaction (a `rebuild`) or `false` to roll it back (a `none`). §5.1
    /// registers the exclusive operation intent inside the same transaction, so
    /// a rollback takes it with the closure: the live Store then keeps no
    /// `kept_node` row, no `gc_intent` row and no committed WAL frame, and the
    /// §3 `none` path writes nothing at all. The §6.3 visited scratch is a
    /// separate temp database that only a kept transaction writes, after the
    /// decision and before the commit, for the rebuild's build phase.
    ///
    /// On SQLite the result does not list the reached objects or the history
    /// nodes ([`KeptObjects::objects`] and [`KeptHistory::nodes`] are
    /// empty): the objects go to the scratch straight from the Store's
    /// locators, the nodes stay in `kept_node`, and no copy of either is
    /// held. Nor are the partly dead packs listed: a rebuild reads them as
    /// a stream ([`KeptObjects::each_partly_dead`]).
    pub(crate) fn mark_for_compaction(
        &self,
        scratch: &Path,
        intent: &GcIntent,
        decide: impl FnOnce(&Transaction<'_>, &MarkResult) -> HistoryResult<bool>,
    ) -> HistoryResult<(MarkResult, bool)> {
        let budget = self.mark_budget();
        match &self.engine {
            crate::backend::Engine::Sqlite(session) => self
                .sqlite_write_session(session)?
                .run_decided(true, |backend| {
                    let tx = Transaction::new(backend);
                    super::admission::register_in(&tx, intent, false)?;
                    let sink = crate::backend::sqlite::compact_visit::VisitSink::create(scratch)?;
                    let (mark, mut reach) = mark_recorded(&tx, false, budget)?;
                    let keep = decide(&tx, &mark)?;
                    let completion = if keep {
                        // Only a rebuild reads the scratch, so only a rebuild
                        // writes it; a `none` drops the sink and no file exists.
                        sink.finish(&mark.objects.retained, |row| {
                            reach.each(tx.mechanics(), row)
                        })?;
                        Completion::Commit
                    } else {
                        Completion::Rollback
                    };
                    Ok(((mark, keep), completion))
                }),
            crate::backend::Engine::Postgres(_) => self.write_snapshot(|tx| {
                let mark = mark(tx, budget)?;
                let keep = decide(tx, &mark)?;
                Ok((mark, keep))
            }),
        }
    }
}

/// One mark that also lists what it kept: every reached object
/// ([`KeptObjects::objects`], 32 bytes an object, read from the Store's
/// locators after the walk), every history node and the partly dead packs.
fn mark(tx: &Transaction<'_>, budget: u64) -> HistoryResult<MarkResult> {
    let (mut mark, mut reach) = mark_recorded(tx, true, budget)?;
    let mut objects = Vec::with_capacity(usize::try_from(reach.count).unwrap_or(0));
    reach.each(tx.mechanics(), &mut |row, _| {
        objects.push(row.object_id);
        Ok(())
    })?;
    mark.objects.objects = objects;
    mark.objects.list()?;
    Ok(mark)
}

/// Bytes of items each sorter of a mark holds before it writes a run
/// ([`spill`]): the identities a wave reached, the next wave by locator, the
/// reached records by locator, the Store's objects other than file payloads
/// by locator, and the small per-pack lists.
const EDGE_SORT_BYTES: usize = 8 * 1024 * 1024;
const FRONTIER_SORT_BYTES: usize = 8 * 1024 * 1024;
const KEPT_SORT_BYTES: usize = 4 * 1024 * 1024;
const INVENTORY_SORT_BYTES: usize = 4 * 1024 * 1024;
const PACK_SORT_BYTES: usize = 4 * 1024 * 1024;
const READ_SORT_BYTES: usize = 1024 * 1024;
const BASED_SORT_BYTES: usize = 1024 * 1024;
const PARTLY_DEAD_BYTES: usize = 256 * 1024;
/// Rows one read of the live heads returns.
const HEADS_PAGE_ROWS: usize = 1024;

/// The reached set of a finished walk, for listing it: the block boundaries
/// of the Store's locators and the walk's two bits per object.
pub(crate) struct Reach {
    table: Locators,
    marks: table::Marks,
    count: u64,
}

impl Reach {
    /// Every reached object's locator, in ascending identity order, read
    /// from the Store's locators a block at a time. The flag says whether
    /// the object was reached only as a delta base: the objects
    /// [`KeptWaste::delta_bases_bytes`] counts.
    pub(crate) fn each(
        &mut self,
        mechanics: &dyn crate::backend::Mechanics,
        each: &mut dyn FnMut(&ObjectLocation, bool) -> HistoryResult<()>,
    ) -> HistoryResult<()> {
        let mut listed = 0u64;
        self.table
            .each_set(mechanics, &mut self.marks, |_, row, kinds| {
                listed += 1;
                each(row, kinds == records::BASE)
            })?;
        if listed != self.count {
            return Err(HistoryError::Integrity("mark object locations"));
        }
        Ok(())
    }
}

/// One mark. `listed` also returns the history nodes
/// ([`KeptHistory::nodes`]); the reached set is returned for a caller that
/// lists the objects. `budget` sizes every buffer and cache of the mark and
/// of the compaction that shares its sort space.
fn mark_recorded(
    tx: &Transaction<'_>,
    listed: bool,
    budget: u64,
) -> HistoryResult<(MarkResult, Reach)> {
    let mechanics = tx.mechanics();
    let space = Space::sized(mechanics.scratch_directory(), budget);
    kept::close(tx, &space)?;
    let policy = mechanics.policy().map_err(storage)?;
    let capacities = StorageCapacities::from_policy(policy).map_err(storage)?;
    // Every locator is read once here, a bounded piece at a time. The pass
    // keeps the block boundaries the walk finds an identity by, counts each
    // pack's located records and puts the objects a pack visit reads
    // together (everything but file payloads) in locator order.
    let mut located = Sorter::new(&space, PACK_SORT_BYTES, records::counts_by_pack);
    let mut inventory = Sorter::new(&space, INVENTORY_SORT_BYTES, records::rows_by_locator);
    let mut table = Locators::load(mechanics, &space, |rank, location| {
        located.push(records::Count {
            pack: location.pack_id,
            records: 1,
        })?;
        if !matches!(location.role, ObjectRole::WholeFile | ObjectRole::Chunk) {
            inventory.push(records::Row {
                location: *location,
                rank,
                kind: 0,
            })?;
        }
        Ok(())
    })?;
    // A pack's objects are asked for each time a wave visits it: one merge
    // now, so that a visit reads that pack's part and not the whole list.
    let mut inventory = inventory.seal(records::row_pack)?;
    let mut marks = table::Marks::new(&space);
    let source = ClosureSource {
        mechanics,
        space: space.clone(),
        here: std::cell::RefCell::new(Here {
            pack: i64::MIN,
            rows: IdMap::default(),
        }),
        reads: std::cell::RefCell::new((
            Sorter::new(&space, READ_SORT_BYTES, records::reads_in_order),
            0,
        )),
        layouts: std::cell::RefCell::new(Sorter::new(
            &space,
            PACK_SORT_BYTES,
            records::layouts_by_group,
        )),
        upcoming: std::cell::RefCell::default(),
        based: std::cell::Cell::new((i64::MIN, 0)),
        values: std::cell::RefCell::new(values::HeldValues::holding(
            space.scaled(values::HELD_VALUES),
        )),
        ordinals: std::cell::RefCell::new(Spans::sorter(&space)),
    };
    // The roots of every retained Commit and Layer, straight from
    // `kept_node` into the first wave: the sorter folds a root named twice.
    let mut reached: Sorter<records::Edge> = Sorter::new(
        &space,
        EDGE_SORT_BYTES,
        if mechanics.locators_by_signed_prefix() {
            records::edges_by_signed_prefix
        } else {
            records::edges_by_bytes
        },
    );
    let mut history = KeptHistory::default();
    kept::each(tx, |node, root| {
        history.note(node, listed);
        match root {
            Some(id) => reached.push(records::Edge {
                id,
                kind: records::LOGICAL,
            }),
            None => Ok(()),
        }
    })?;
    let mut kept = Sorter::new(&space, KEPT_SORT_BYTES, records::kept_by_locator);
    let mut based = Sorter::new(&space, BASED_SORT_BYTES, records::based_by_rank);
    walk::walk(
        &source,
        &mut table,
        &mut marks,
        &mut inventory,
        (&mut kept, &mut based),
        &capacities,
        policy.construction(),
        reached,
    )?;
    drop(inventory);
    let found = waste::waste(
        tx,
        &source,
        &mut table,
        &mut marks,
        &mut located,
        (&mut kept, &mut based),
    )?;
    drop((located, kept, based));
    let mark_bytes = source.read_bytes()?;
    let digest = live_heads_digest(tx)?;
    let objects = KeptObjects {
        objects: Vec::new(),
        count: found.objects,
        retained: found.retained,
        packs: Vec::new(),
        waste: found.waste,
        partly_dead: found.partly_dead,
        partly_dead_listed: Vec::new(),
        ordinals: Spans::of(source.ordinals.replace(Spans::sorter(&space))),
        inode_ordinals: Vec::new(),
    };
    let reach = Reach {
        table,
        marks,
        count: found.objects,
    };
    Ok((
        MarkResult {
            history,
            objects,
            live_heads_digest: digest,
            mark_bytes,
            space,
        },
        reach,
    ))
}

/// The objects other than file payloads that one pack holds, by identity.
struct Here {
    pack: i64,
    rows: IdMap<records::Row>,
}

/// Physical input for the shared encoding readers over one mark transaction.
struct ClosureSource<'a> {
    mechanics: &'a dyn crate::backend::Mechanics,
    /// Where this mark's sort files go.
    space: Arc<Space>,
    /// The pack being walked and what it holds besides file payloads: what a
    /// reference found there is looked up in before the Store is asked.
    here: std::cell::RefCell<Here>,
    /// Every read of pack body bytes this mark made, and how many. Folded per
    /// pack in the order made, they give [`MarkResult::mark_bytes`]: a pack
    /// read in several groups counts each extent once, and a later whole-body
    /// read replaces the partial sum.
    reads: std::cell::RefCell<(Sorter<records::BodyRead>, u64)>,
    /// The group extents of every pack whose directory the walk read, kept so
    /// the kept-waste pass does not read it a second time.
    layouts: std::cell::RefCell<Sorter<records::Layout>>,
    /// The packs the rows being walked decode metadata from, ascending, and
    /// how many of them the engine has been told to expect.
    upcoming: std::cell::RefCell<(Vec<i64>, usize)>,
    /// The pack and group whose delta bases the engine was last told to
    /// expect lookups of ([`walk`]'s `expect_bases`).
    based: std::cell::Cell<(i64, usize)>,
    /// Inode values the engine has returned, asked for or not.
    values: std::cell::RefCell<values::HeldValues>,
    /// Every ordinal the walk asked for ([`ordinals`]), whether the engine or
    /// the held values answered it: the rebuild copies only the blocks these
    /// name.
    ordinals: std::cell::RefCell<Sorter<records::Span>>,
}

/// Packs announced to the engine at a time ([`ClosureSource::announce`]).
const AHEAD_PACKS: usize = 128;
/// Bytes of rebuilt chain elements the walk's reader keeps, so a page stored
/// as a delta is rebuilt from the base an earlier version already rebuilt
/// instead of from its whole chain. The base of a page is the version an
/// earlier Commit wrote, any number of packs back, and a pack's visit keeps
/// every page of its Commit: a base that has left the memo is read from its
/// own pack again, chain and all. The size is the largest single part of
/// what a mark holds.
const CHAIN_MEMO_BYTES: usize = 32 * 1024 * 1024;

impl ClosureSource<'_> {
    /// Notes the packs `rows`, which are in locator order, decode metadata
    /// from: the packs the walk reads one scoped acquisition at a time.
    fn expect_rows(&self, rows: &[records::Row]) {
        let mut packs: Vec<i64> = rows
            .iter()
            .filter(|row| row.location.role.has_references())
            .map(|row| row.location.pack_id)
            .collect();
        packs.dedup();
        *self.upcoming.borrow_mut() = (packs, 0);
    }

    /// Before `pack_id` is acquired: tells the engine which of the expected
    /// packs follow it, a window at a time, so an engine that pays a round
    /// trip per pack can read the window in one.
    fn announce(&self, pack_id: i64) -> Result<(), StorageError> {
        let mut upcoming = self.upcoming.borrow_mut();
        let (packs, told) = &mut *upcoming;
        let Ok(at) = packs.binary_search(&pack_id) else {
            return Ok(());
        };
        if at < *told {
            return Ok(());
        }
        *told = (at + AHEAD_PACKS).min(packs.len());
        self.mechanics
            .expect_scoped_packs(&packs[at..*told])
            .map_err(StorageError::from)
    }

    /// Makes `pack_id` the pack being walked: takes its objects other than
    /// file payloads from `inventory`, which finds a pack's rows by its id
    /// and reads on from where it stands when a wave's packs follow each
    /// other.
    fn enter(&self, pack_id: i64, inventory: &mut Sealed<records::Row>) -> HistoryResult<()> {
        let mut here = self.here.borrow_mut();
        if here.pack == pack_id {
            return Ok(());
        }
        here.pack = pack_id;
        here.rows.clear();
        let rows = &mut here.rows;
        inventory.each_of(pack_id, |row| {
            rows.insert(row.location.object_id, row);
        })
    }

    /// The object `id` when the pack being walked holds it and it is not a
    /// file payload: a Commit's root, inode pages, directory pages, file
    /// states and extent pages are written into one pack, so they are read
    /// in one visit of it and not in one visit per tree level.
    fn held_here(&self, id: &ObjectId) -> Option<records::Row> {
        self.here.borrow().rows.get(id).copied()
    }

    /// The objects of group `group` of the pack being walked that name other
    /// objects: those whose rebuild may look a delta base up.
    fn held_in_group(&self, group: usize) -> Vec<ObjectLocation> {
        let here = self.here.borrow();
        let mut rows: Vec<ObjectLocation> = here
            .rows
            .values()
            .map(|row| row.location)
            .filter(|row| row.group_number == group && row.role.has_references())
            .collect();
        rows.sort_unstable_by_key(|row| row.record_number);
        rows
    }

    /// Keeps the group extents the walk read for `pack_id`.
    fn keep_layout(&self, pack_id: i64, views: Vec<layout::GroupView>) {
        let mut layouts = self.layouts.borrow_mut();
        for (number, view) in views.into_iter().enumerate() {
            layouts.hold(records::Layout {
                pack: pack_id,
                number: number as u64,
                view,
            });
        }
    }

    fn note(&self, pack: i64, bytes: u64, limit: u64, whole: bool) {
        let mut reads = self.reads.borrow_mut();
        let sequence = reads.1;
        reads.1 += 1;
        reads.0.hold(records::BodyRead {
            pack,
            sequence,
            bytes,
            limit,
            whole,
        });
    }

    /// Records a whole-body read of `pack_id`. The full extent is the pack's
    /// body, so it replaces any group extents already counted for the pack.
    fn note_whole_body(&self, pack_id: i64, bytes: u64) {
        self.note(pack_id, bytes, 0, true);
    }

    /// Records one scoped acquisition's body bytes, capped at the pack's
    /// declared length. The group cache asks for only groups it does not hold,
    /// so the extents are disjoint; the controls each acquisition repeats can
    /// still not make a pack count for more than its body.
    fn note_scoped_body(&self, pack_id: i64, bytes: u64, length: u64) {
        self.note(pack_id, bytes, length, false);
    }

    /// Writes out what the reads and layouts noted since the last call hold
    /// beyond their bound. The notes themselves never write: they are made
    /// inside the storage readers.
    fn relieve(&self) -> HistoryResult<()> {
        self.reads.borrow_mut().0.relieve()?;
        self.ordinals.borrow_mut().relieve()?;
        self.layouts.borrow_mut().relieve()
    }

    /// The distinct pack body bytes this mark read.
    fn read_bytes(&self) -> HistoryResult<u64> {
        let mut reads = self.reads.borrow_mut();
        let mut total = 0u64;
        let mut pack: Option<(i64, u64)> = None;
        let mut merged = reads.0.sorted()?;
        while let Some(read) = merged.next()? {
            let mut entry = match pack {
                Some((id, entry)) if id == read.pack => entry,
                Some((_, entry)) => {
                    total = total.saturating_add(entry);
                    0
                }
                None => 0,
            };
            entry = if read.whole {
                entry.max(read.bytes)
            } else {
                entry.saturating_add(read.bytes).min(read.limit)
            };
            pack = Some((read.pack, entry));
        }
        drop(merged);
        Ok(total.saturating_add(pack.map_or(0, |(_, entry)| entry)))
    }
}

impl Source for ClosureSource<'_> {
    fn ordered_dependencies(&self) -> bool {
        false
    }

    fn location(&self, id: ObjectId, ceiling: i64) -> Result<Option<ObjectLocation>, StorageError> {
        // A chain's base is most often in the pack being read; any other is
        // one lookup in the Store's own locator index.
        if let Some(row) = self.held_here(&id) {
            return Ok(Some(row.location).filter(|location| location.pack_id <= ceiling));
        }
        Ok(self
            .mechanics
            .location(&id)
            .map_err(StorageError::from)?
            .filter(|location| location.pack_id <= ceiling))
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
        self.announce(pack_id)?;
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
        lists::hold_runs(&mut self.ordinals.borrow_mut(), ordinals);
        self.values.borrow_mut().values(self.mechanics, ordinals)
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

/// The §6.4 live-heads digest: BLAKE3 over `layerfs/live-heads/v1\0`, then the
/// non-retired stacks in identity order as `(stack_id, head_layer_id)`, then the
/// marker-free Branches in identity order as `(branch_id, head_commit_id or 33
/// zero bytes, base_layer_id)`. Every field is fixed width, so a retired row is
/// absent rather than hashed as zeros and timestamps never enter the vector.
///
/// The rows are read a bounded page at a time, in identity order, and hashed
/// as they come.
fn live_heads_digest(tx: &Transaction<'_>) -> HistoryResult<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/live-heads/v1\0");
    live_rows(tx, LIVE_STACKS, &mut |row| {
        let id: Vec<u8> = cell(row, 0)?;
        let head: Vec<u8> = cell(row, 1)?;
        if id.len() != 17 || head.len() != 33 {
            return Err(HistoryError::Integrity("live-heads stack width"));
        }
        hasher.update(&id);
        hasher.update(&head);
        Ok(())
    })?;
    live_rows(tx, LIVE_BRANCHES, &mut |row| {
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
        Ok(())
    })?;
    Ok(*hasher.finalize().as_bytes())
}

/// Hands the rows of one live-heads statement to `each`, a page at a time.
/// The first column is the identity the pages follow.
fn live_rows(
    tx: &Transaction<'_>,
    key: &str,
    each: &mut dyn FnMut(&crate::backend::records::Record) -> HistoryResult<()>,
) -> HistoryResult<()> {
    let mut after: Vec<u8> = Vec::new();
    loop {
        let rows = tx.query(
            key,
            vec![
                Param::Bytes(after.clone()),
                Param::I64(HEADS_PAGE_ROWS as i64),
            ],
        )?;
        for row in &rows {
            each(row)?;
        }
        match rows.last() {
            Some(last) if rows.len() == HEADS_PAGE_ROWS => after = cell(last, 0)?,
            _ => return Ok(()),
        }
    }
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
