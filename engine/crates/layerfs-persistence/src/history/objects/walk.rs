//! The walk of one mark: wave after wave from the roots to the fixpoint,
//! each wave in pack order, with nothing held that grows with the Store.
//!
//! A wave ends with the identities it reached and could not place: they are
//! sorted in the engine's locator order, folded, and looked up against the
//! locators block by block ([`super::table`]). Those the walk had not
//! reached are the next wave, sorted by locator. The wave is then walked a
//! bounded run of rows at a time.
use super::records::{self, Based, Edge, Kept, Row};
use super::spill::{Sealed, Sorter};
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

/// Rows of a wave walked between two reads of record headers, at most,
/// under the default budget; and the least, whatever the budget.
const RUN_ROWS: usize = 16_384;
const LEAST_RUN_ROWS: usize = 64;
/// The least the chain memo holds, whatever the budget.
const LEAST_MEMO_BYTES: usize = 64 * 1024;
/// A wave that reached fewer identities than one in this many locators asks
/// for each one alone instead of reading the locators block by block: most
/// blocks then hold none of them.
const SPARSE_SHARE: u64 = 128;

/// The caches of the shared encoding readers, kept for the whole walk.
struct Readers {
    packs: PackCache,
    pool: PoolReader,
    groups: GroupCache,
    workspace: DecompressionWorkspace,
    counters: ChainCounters,
    edge_decodes: u64,
}

/// Admits every object reachable from `roots` through logical references and
/// delta bases, exactly once, and records each in `kept`: the record it is
/// stored in, and apart from that its rank when a delta base was the first
/// to reach it.
///
/// The walk is metadata-only: a role that names no other object
/// ([`ObjectRole::has_references`]) costs one record header read, not a payload
/// decode, and each wave is processed in locator order so one pack body answers
/// every object of its group. A metadata object reached in the pack being
/// walked is read in that same visit ([`ClosureSource::held_here`]).
/// Reference-bearing metadata pages are still authenticated and decoded, but
/// identity is not hashed a second time.
///
/// `reached` holds the roots: the first wave.
#[allow(clippy::too_many_arguments)]
pub(super) fn walk(
    source: &ClosureSource<'_>,
    table: &mut Locators,
    marks: &mut Marks,
    inventory: &mut Sealed<Row>,
    kept: (&mut Sorter<Kept>, &mut Sorter<Based>),
    capacities: &StorageCapacities,
    construction: ConstructionPolicy,
    mut reached: Sorter<Edge>,
) -> HistoryResult<()> {
    let mechanics = source.mechanics;
    let mut readers = Readers {
        packs: PackCache::new(),
        pool: PoolReader::with_chain_memo(
            source
                .space
                .scaled(super::CHAIN_MEMO_BYTES)
                .max(LEAST_MEMO_BYTES),
        ),
        groups: GroupCache::new(),
        workspace: DecompressionWorkspace::new().map_err(storage)?,
        counters: ChainCounters::default(),
        edge_decodes: 0,
    };
    // One hash for the whole walk, not one per metadata object.
    let unscoped = InodeScope::from_object(ObjectId::for_bytes(b"gc.mark/object-closure"));
    let run_rows = source.space.scaled(RUN_ROWS).max(LEAST_RUN_ROWS);
    let mut frontier: Sorter<Row> = Sorter::new(
        &source.space,
        super::FRONTIER_SORT_BYTES,
        records::rows_by_locator,
    );
    loop {
        // Locate what the last wave reached, in the order the locators are
        // read in: each block of them is read once, and a wave that reached
        // little asks for its identities one by one.
        {
            let sparse = reached.added().saturating_mul(SPARSE_SHARE) < table.count();
            let mut edges = reached.sorted()?;
            while let Some(edge) = edges.next()? {
                let (rank, location) = table
                    .find(mechanics, &edge.id, sparse)?
                    .ok_or(HistoryError::Integrity("mark object locator"))?;
                if marks.note(rank, edge.kind)? {
                    frontier.push(Row {
                        location,
                        rank,
                        kind: edge.kind,
                    })?;
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
            let mut rows: Vec<Row> = Vec::new();
            loop {
                // A run ends where the header reads of one call end: after
                // the payload packs one call reads the heads of.
                rows.clear();
                let (mut payload_packs, mut last) = (0, None);
                while rows.len() < run_rows {
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
                    inventory,
                    &mut reached,
                    (&mut *kept.0, &mut *kept.1),
                    &mut readers,
                    capacities,
                    construction,
                    unscoped,
                )?;
            }
        }
        frontier.clear()?;
    }
    Ok(())
}

/// Walks one run of a wave: reads its payload record headers, then every
/// object of the run and what each one reaches in its own pack.
#[allow(clippy::too_many_arguments)]
fn walk_rows(
    source: &ClosureSource<'_>,
    marks: &mut Marks,
    rows: &[Row],
    inventory: &mut Sealed<Row>,
    reached: &mut Sorter<Edge>,
    kept: (&mut Sorter<Kept>, &mut Sorter<Based>),
    readers: &mut Readers,
    capacities: &StorageCapacities,
    construction: ConstructionPolicy,
    unscoped: InodeScope,
) -> HistoryResult<()> {
    let (kept, based) = kept;
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
                expect_bases(source, readers, &location)?;
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
            })?;
            // An object kept only as a delta base was first reached as one.
            if row.kind & records::LOGICAL == 0 {
                based.push(Based {
                    rank: row.rank,
                    width: edge.width,
                })?;
            }
            if let Some(base) = edge.base {
                reach(source, marks, base, records::BASE, &mut local, reached)?;
            }
        }
    }
    source.relieve()
}

/// Levels of delta bases [`expect_bases`] follows below one group: no chain
/// is deeper than a depth cap, which is a byte.
const BASE_LEVELS: usize = u8::MAX as usize;

/// Before a record of `location`'s group is rebuilt, on an engine that pays
/// a round trip per locator lookup: tells it the lookups the group's chains
/// are about to make, a level of delta bases at a time, so they cost one
/// statement a level and not one for each chain element
/// ([`crate::backend::Mechanics::expect_locations`]).
///
/// A rebuild walks from a record to its base, and on from there until it
/// meets an element the reader's memo holds or a record stored whole. This
/// walks the same way for every record of the group at once: it reads each
/// element's base from the record the rebuild is about to read, through the
/// caches the rebuild reads through, and stops where the memo holds the
/// element. It is a hint only. What the memo holds can change before a
/// rebuild gets there; a lookup that was not announced is then made on its
/// own, and an element read here that no rebuild reads costs one record
/// header.
fn expect_bases(
    source: &ClosureSource<'_>,
    readers: &mut Readers,
    location: &ObjectLocation,
) -> HistoryResult<()> {
    let mechanics = source.mechanics;
    if !mechanics.batches_locations() {
        return Ok(());
    }
    let group = (location.pack_id, location.group_number);
    if source.based.replace(group) == group {
        return Ok(());
    }
    let engine = |error: layerfs_storage::port::PersistenceError| {
        storage(layerfs_storage::StorageError::from(error))
    };
    let mut level = source.held_in_group(location.group_number);
    for _ in 0..BASE_LEVELS {
        // A rebuild locates the base of every element it reads, and reads
        // on only through a base its memo does not hold.
        let (mut next, mut outside) = (Vec::new(), Vec::new());
        for row in &level {
            let edge = stored_edge(
                source,
                &mut readers.packs,
                &mut readers.groups,
                &mut readers.workspace,
                &mut readers.edge_decodes,
                row,
            )
            .map_err(storage)?;
            let Some(base) = edge.base else {
                continue;
            };
            match source.held_here(&base) {
                Some(held) if !readers.pool.remembers(&base) => next.push(held.location),
                Some(_) => {}
                None => outside.push(base),
            }
        }
        outside.sort_unstable();
        outside.dedup();
        mechanics.expect_locations(&outside).map_err(engine)?;
        for base in outside.iter().filter(|base| !readers.pool.remembers(base)) {
            // Held by the engine now; one it does not locate is the
            // rebuild's refusal to make, not this hint's.
            next.extend(mechanics.location(base).map_err(engine)?);
        }
        if next.is_empty() {
            break;
        }
        next.sort_unstable_by_key(|row| (row.pack_id, row.group_number, row.record_number));
        next.dedup();
        let mut packs: Vec<i64> = next.iter().map(|row| row.pack_id).collect();
        packs.dedup();
        mechanics.expect_scoped_packs(&packs).map_err(engine)?;
        level = next;
    }
    Ok(())
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
            if marks.note(row.rank, kind)? {
                local.push(Row { kind, ..row });
            }
            Ok(())
        }
        None => reached.push(Edge { id, kind }),
    }
}
