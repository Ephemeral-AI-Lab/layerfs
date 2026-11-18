//! The walk of one mark: wave after wave from the roots to the fixpoint,
//! each wave in pack order, with nothing held that grows with the Store.
//!
//! A wave ends with the identities it reached and could not place: they are
//! sorted in the engine's locator order, folded, and looked up against the
//! locators block by block ([`super::table`]). Those the walk had not
//! reached are the next wave, sorted by locator. The wave is then walked a
//! bounded run of rows at a time.
use super::records::{self, Edge, Kept, Row};
use super::spill::{Merge, Sorter};
use super::table::{Locators, Marks};
use super::{edges, storage, ClosureSource};
use layerfs_content::filesystem::{FilesystemRoot, InodeScope};
use layerfs_content::object::decode_references;
use layerfs_content::{ConstructionPolicy, ObjectId, ObjectRole};
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_storage::{
    encoding::pool::PoolReader,
    encoding::{
        delta::read::{stored_edge, BodyCaches, ChainCounters, Resolver},
        DecompressionWorkspace, GroupCache, PackCache,
    },
    location::ObjectLocation,
    StorageCapacities,
};

/// Rows of a wave walked between two reads of record headers, at most.
const RUN_ROWS: usize = 16_384;

/// The caches of the shared encoding readers, kept for the whole walk.
struct Readers {
    packs: PackCache,
    pool: PoolReader,
    groups: GroupCache,
    workspace: DecompressionWorkspace,
    counters: ChainCounters,
    edge_decodes: u64,
}

impl Marks {
    /// Notes why the object at `rank` was reached, and says whether this is
    /// the first time it is.
    fn note(&mut self, rank: u64, kind: u8) -> bool {
        if kind & records::LOGICAL != 0 {
            self.logical.set(rank);
        }
        if kind & records::BASE != 0 {
            self.bases.set(rank);
        }
        self.reached.set(rank)
    }
}

/// Admits every object reachable from `roots` through logical references and
/// delta bases, exactly once, and records each in `kept`.
///
/// The walk is metadata-only: a role that names no other object
/// ([`ObjectRole::has_references`]) costs one record header read, not a payload
/// decode, and each wave is processed in locator order so one pack body answers
/// every object of its group. A metadata object reached in the pack being
/// walked is read in that same visit ([`ClosureSource::held_here`]).
/// Reference-bearing metadata pages are still authenticated and decoded, but
/// identity is not hashed a second time.
///
/// Returns the most bytes the walk's own sort files held.
#[allow(clippy::too_many_arguments)]
pub(super) fn walk(
    source: &ClosureSource<'_>,
    table: &mut Locators,
    marks: &mut Marks,
    inventory: &mut Sorter<Row>,
    kept: &mut Sorter<Kept>,
    capacities: &StorageCapacities,
    construction: ConstructionPolicy,
    roots: Vec<ObjectId>,
) -> HistoryResult<u64> {
    let mechanics = source.mechanics;
    let directory = mechanics.scratch_directory();
    let mut readers = Readers {
        packs: PackCache::new(),
        pool: PoolReader::with_chain_memo(super::CHAIN_MEMO_BYTES),
        groups: GroupCache::new(),
        workspace: DecompressionWorkspace::new().map_err(storage)?,
        counters: ChainCounters::default(),
        edge_decodes: 0,
    };
    // One hash for the whole walk, not one per metadata object.
    let unscoped = InodeScope::from_object(ObjectId::for_bytes(b"gc.mark/object-closure"));
    let mut reached: Sorter<Edge> = Sorter::new(
        &directory,
        super::EDGE_SORT_BYTES,
        if mechanics.locators_by_signed_prefix() {
            records::edges_by_signed_prefix
        } else {
            records::edges_by_bytes
        },
    );
    let mut frontier: Sorter<Row> = Sorter::new(
        &directory,
        super::FRONTIER_SORT_BYTES,
        records::rows_by_locator,
    );
    for id in roots {
        reached.push(Edge {
            id,
            kind: records::LOGICAL,
        })?;
    }
    loop {
        // Locate what the last wave reached, in the order the locators are
        // read in: each block of them is read once.
        {
            let mut edges = reached.sorted()?;
            while let Some(edge) = edges.next()? {
                let (rank, location) = table
                    .find(mechanics, &edge.id)?
                    .ok_or(HistoryError::Integrity("mark object locator"))?;
                if marks.note(rank, edge.kind) {
                    frontier.push(Row { location, rank })?;
                }
            }
        }
        reached.clear()?;
        if frontier.is_empty() {
            break;
        }
        {
            // Pack order: one body answers every object of its group, so a
            // wave sorted by locator reads each group once.
            let mut wave = frontier.sorted()?;
            let mut inventory = inventory.sorted()?;
            let mut rows: Vec<Row> = Vec::new();
            loop {
                // A run ends where the header reads of one call end: after
                // the payload packs one call reads the heads of.
                rows.clear();
                let (mut payload_packs, mut last) = (0, None);
                while rows.len() < RUN_ROWS {
                    let Some(next) = wave.peek() else {
                        break;
                    };
                    let pack_id = next.location.pack_id;
                    if !next.location.role.has_references() && last != Some(pack_id) {
                        if payload_packs == edges::HEAD_PACKS {
                            break;
                        }
                        payload_packs += 1;
                        last = Some(pack_id);
                    }
                    rows.extend(wave.next()?);
                }
                if rows.is_empty() {
                    break;
                }
                walk_rows(
                    source,
                    marks,
                    &rows,
                    &mut inventory,
                    &mut reached,
                    kept,
                    &mut readers,
                    capacities,
                    construction,
                    unscoped,
                )?;
            }
        }
        frontier.clear()?;
    }
    Ok(reached
        .peak_file_bytes()
        .saturating_add(frontier.peak_file_bytes())
        .saturating_add(inventory.peak_file_bytes()))
}

/// Walks one run of a wave: reads its payload record headers, then every
/// object of the run and what each one reaches in its own pack.
#[allow(clippy::too_many_arguments)]
fn walk_rows(
    source: &ClosureSource<'_>,
    marks: &mut Marks,
    rows: &[Row],
    inventory: &mut Merge<'_, Row>,
    reached: &mut Sorter<Edge>,
    kept: &mut Sorter<Kept>,
    readers: &mut Readers,
    capacities: &StorageCapacities,
    construction: ConstructionPolicy,
    unscoped: InodeScope,
) -> HistoryResult<()> {
    source.expect_rows(rows);
    // A file payload's edge is its record header: read just that, a pack
    // at a time, and leave the payload unread.
    let locations: Vec<ObjectLocation> = rows.iter().map(|row| row.location).collect();
    let mut headers = edges::wave_edges(source, &locations)?;
    drop(locations);
    // Objects admitted while their pack is being walked.
    let mut local: Vec<Row> = Vec::new();
    for row in rows {
        local.push(*row);
        while let Some(row) = local.pop() {
            let location = row.location;
            let id = location.object_id;
            source.enter(location.pack_id, inventory)?;
            let role = location.role;
            let references = if role.has_references() {
                let canonical = {
                    let mut resolver = Resolver::new(
                        source,
                        i64::MAX,
                        capacities,
                        BodyCaches {
                            packs: &mut readers.packs,
                            pool: &mut readers.pool,
                        },
                        &mut readers.groups,
                        &mut readers.workspace,
                        &mut readers.counters,
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
                    unscoped
                };
                decode_references(role, &canonical, construction, scope)
                    .map_err(|_| HistoryError::Integrity("mark object references"))?
            } else {
                Vec::new()
            };
            for reference in references {
                reach(
                    source,
                    marks,
                    reference,
                    records::LOGICAL,
                    &mut local,
                    reached,
                )?;
            }
            // The base identity lives in the record header. An encoded group
            // has no header to read on its own, so its edge borrows the group.
            let edge = match headers.remove(&id) {
                Some(edge) => edge,
                // The walk's own decoded-group cache: a compressed group is
                // decompressed once, not once more for each record's edge.
                None => stored_edge(
                    source,
                    &mut readers.packs,
                    &mut readers.groups,
                    &mut readers.workspace,
                    &mut readers.edge_decodes,
                    &location,
                )
                .map_err(storage)?,
            };
            kept.push(Kept {
                pack: location.pack_id,
                group: location.group_number as u64,
                record: location.record_number as u64,
                width: edge.width,
                rank: row.rank,
            })?;
            if let Some(base) = edge.base {
                reach(source, marks, base, records::BASE, &mut local, reached)?;
            }
        }
    }
    source.relieve()
}

/// Notes a newly found reference or delta base. An object of the pack being
/// walked that is not a file payload joins `local`, which the walk empties
/// before it leaves the pack. A file payload always waits for the next wave,
/// where payload headers are read a pack at a time; so does an object of any
/// other pack, and whether the walk already reached it is settled when the
/// wave's identities are located.
fn reach(
    source: &ClosureSource<'_>,
    marks: &mut Marks,
    id: ObjectId,
    kind: u8,
    local: &mut Vec<Row>,
    reached: &mut Sorter<Edge>,
) -> HistoryResult<()> {
    match source.held_here(&id) {
        Some(row) => {
            if marks.note(row.rank, kind) {
                local.push(row);
            }
            Ok(())
        }
        None => reached.push(Edge { id, kind }),
    }
}
