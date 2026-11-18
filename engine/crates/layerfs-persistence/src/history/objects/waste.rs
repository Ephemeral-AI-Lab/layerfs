//! Kept waste of one mark: what the retained packs hold beyond the objects
//! the walk reached. Split from the parent module, which owns the walk; the
//! types and statements used here are the parent's.
//!
//! The pass reads three lists in pack order, each sorted in bounded memory:
//! the records the walk reached, the count of records the Store locates in
//! each pack, and the group extents of the packs whose directory the walk
//! read. It holds a bounded run of packs at a time.
use super::records::{Based, Count, Kept};
use super::table::{Locators, Marks};
use super::*;

/// Reached records of the packs one round holds, at most, under the default
/// budget; a pack is never split.
const ROUND_RECORDS: usize = 262_144;
/// Packs one round holds, at most, under the default budget. Each costs its
/// own small lists whatever it holds, so a Store of one-record packs is
/// bounded by this and not by the records.
const ROUND_PACKS: usize = 4_096;
/// The least of both, whatever the budget.
const LEAST_ROUND_RECORDS: usize = 1_024;
const LEAST_ROUND_PACKS: usize = 16;

/// What the pass found: the kept waste, and the retained packs it met.
pub(super) struct Found {
    pub(super) waste: KeptWaste,
    /// Retained packs that hold a record no reached object owns, ascending,
    /// each with its dead bytes.
    pub(super) partly_dead: PartlyDead,
    /// The retained packs: those holding at least one reached object.
    pub(super) retained: Sequence,
    /// How many objects the walk reached.
    pub(super) objects: u64,
}

/// One retained pack of a round.
struct Retained {
    pack_id: i64,
    /// Its reached records, in group and record order.
    records: Vec<Kept>,
    /// How many records the Store locates in it.
    located: u64,
    /// Its group extents, when the walk read its directory.
    views: Option<Vec<layout::GroupView>>,
}

/// The reached records of group `number`, in record order.
fn group(records: &[Kept], number: usize) -> &[Kept] {
    let number = number as u64;
    let from = records.partition_point(|kept| kept.group < number);
    let to = records.partition_point(|kept| kept.group <= number);
    &records[from..to]
}

/// Attributes the bytes a retained pack keeps for records no reached object
/// owns, the delta-base-only bytes and the inode-value block bytes.
///
/// The pack directory sits at the head of every body, so the common cases are
/// answered from a bounded prefix read: a wholly dead group's extent and a
/// retained compressed group (which contributes nothing) need no body. Only a
/// retained uncompressed group that still holds a dead record needs the body,
/// because its exact per-record split lives inside the group.
pub(super) fn waste(
    tx: &Transaction<'_>,
    source: &ClosureSource<'_>,
    table: &mut Locators,
    marks: &mut Marks,
    located: &mut Sorter<Count>,
    kept: (&mut Sorter<Kept>, &mut Sorter<Based>),
) -> HistoryResult<Found> {
    let (kept, based) = kept;
    let mut dead_in_retained_packs_bytes = 0u64;
    let mut delta_bases_bytes = 0u64;
    let mut objects = 0u64;
    let mut retained = Sequence::new(&source.space);
    // Written in pack order, a pack once: the order only merges its runs.
    let mut partly_dead = Sorter::new(&source.space, PARTLY_DEAD_BYTES, records::counts_by_pack);
    let mut partly_dead_count = 0u64;
    let mut counted = (0i64, 0u64, false);
    // The walk is over: nothing adds to its layouts while they are read.
    let mut layouts = std::mem::replace(
        &mut *source.layouts.borrow_mut(),
        Sorter::new(&source.space, PACK_SORT_BYTES, records::layouts_by_group),
    );
    let round_records = source.space.scaled(ROUND_RECORDS).max(LEAST_ROUND_RECORDS);
    let round_packs = source.space.scaled(ROUND_PACKS).max(LEAST_ROUND_PACKS);
    let mut layouts = layouts.sorted()?;
    let mut located = located.sorted()?;
    let mut kept = kept.sorted()?;
    loop {
        // One round: retained packs in pack order, until the packs whose
        // directory is still unread fill one read or the round is full.
        let mut round: Vec<Retained> = Vec::new();
        let mut unread = Vec::new();
        let mut held = 0;
        while unread.len() < edges::HEAD_PACKS && held < round_records && round.len() < round_packs
        {
            let Some(first) = kept.next()? else {
                break;
            };
            let pack_id = first.pack;
            let mut records = vec![first];
            while kept.peek().is_some_and(|next| next.pack == pack_id) {
                records.extend(kept.next()?);
            }
            let mut count = 0;
            while located.peek().is_some_and(|next| next.pack <= pack_id) {
                if let Some(next) = located.next()? {
                    if next.pack == pack_id {
                        count = next.records;
                    }
                }
            }
            let mut views = Vec::new();
            while layouts.peek().is_some_and(|next| next.pack <= pack_id) {
                if let Some(next) = layouts.next()? {
                    if next.pack == pack_id {
                        if next.number != views.len() as u64 {
                            return Err(HistoryError::Integrity("mark pack directory"));
                        }
                        views.push(next.view);
                    }
                }
            }
            if views.is_empty() {
                unread.push(pack_id);
            }
            held += records.len();
            round.push(Retained {
                pack_id,
                records,
                located: count,
                views: (!views.is_empty()).then_some(views),
            });
        }
        if round.is_empty() {
            break;
        }
        // Packs the walk did not lay out have their directory prefix read
        // here: one call for the round's, not one call per pack.
        let mut prefixes = edges::Prefixes::new(source, unread);
        for pack in round {
            let pack_id = pack.pack_id;
            let records = pack.records.as_slice();
            // The pack before this one is complete: record what it added.
            if counted.2 || dead_in_retained_packs_bytes > counted.1 {
                partly_dead.push(Count {
                    pack: counted.0,
                    records: dead_in_retained_packs_bytes - counted.1,
                })?;
                partly_dead_count += 1;
            }
            // The reached records come in pack order: a pack once.
            if pack_id < 0 || retained.push(pack_id as u64).is_err() {
                return Err(HistoryError::Integrity("mark pack"));
            }
            objects += records.len() as u64;
            // A pack that locates more records than the walk reached in it
            // holds a dead one. That is the only sign a compressed group
            // gives; its bytes are not attributed.
            counted = (
                pack_id,
                dead_in_retained_packs_bytes,
                pack.located > records.len() as u64,
            );
            // The walk already read this pack's directory when it read its
            // record headers; only a pack it reached another way is read here.
            let views = if let Some(views) = pack.views {
                views
            } else {
                // The pack row may reserve more capacity than it uses; the header names
                // the used length. Everything below parses against `used`, never the
                // reserved capacity. A prefix that already covers `used` is the body.
                let (capacity, prefix) = prefixes.take(pack_id)?;
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
                    dead_in_retained_packs_bytes = dead_in_retained_packs_bytes
                        .saturating_add(pack_waste(&prefix[..used], records)?);
                    continue;
                }
                let header = layout::parse_directory_header(&prefix, used).map_err(storage)?;
                layout::directory_group_views(&prefix, header).map_err(storage)?
            };
            // A wholly dead group's stored extent is known from the directory. A
            // retained Raw group whose reached records do not account for its
            // whole extent has its own record directory read, so its exact
            // per-record split costs neither the pack body nor the group's.
            let mut mixed_raw = Vec::new();
            for (number, view) in views.iter().enumerate() {
                let live = group(records, number);
                if live.is_empty() {
                    dead_in_retained_packs_bytes = dead_in_retained_packs_bytes
                        .saturating_add(u64::try_from(view.end - view.start).unwrap_or(u64::MAX));
                } else if view.codec == GroupCodec::Raw && group_may_hide_dead_records(*view, live)
                {
                    mixed_raw.push((number, *view));
                }
            }
            if mixed_raw.is_empty() {
                continue;
            }
            // Each record's extent is in the group's directory, so the records
            // themselves stay unread.
            let views: Vec<layout::GroupView> = mixed_raw.iter().map(|(_, view)| *view).collect();
            let directories = edges::group_directories(source, pack_id, &views)?;
            for ((number, _), directory) in mixed_raw.iter().zip(&directories) {
                dead_in_retained_packs_bytes = dead_in_retained_packs_bytes
                    .saturating_add(dead_raw_record_bytes(directory, group(records, *number))?);
            }
        }
        source.relieve()?;
    }
    if counted.2 || dead_in_retained_packs_bytes > counted.1 {
        partly_dead.push(Count {
            pack: counted.0,
            records: dead_in_retained_packs_bytes - counted.1,
        })?;
        partly_dead_count += 1;
    }
    drop((layouts, located, kept));
    // The objects first reached as a delta base, in rank order: those no
    // logical reference reached later are kept only as a base. The bits are
    // read forward, a page once.
    {
        let mut based = based.sorted()?;
        while let Some(object) = based.next()? {
            if marks.kinds(object.rank)? == records::BASE {
                delta_bases_bytes = delta_bases_bytes.saturating_add(object.width);
            }
        }
    }
    // The report names at most this many kept delta bases, the first in
    // identity order. The byte total stays exact; only the list is bounded,
    // and [`KeptWaste::from_parts`] records that it was cut so the §11
    // projection can render `{value, cap}`. One more than the cap is read so
    // the cut is known.
    let mut ranks = Vec::new();
    for (from, to) in table.byte_order() {
        marks.each(from, to, |rank, kinds| {
            if kinds == records::BASE {
                ranks.push(rank);
            }
            Ok(ranks.len() <= MAX_KEPT_WASTE_REFERENCES)
        })?;
    }
    let mut delta_base_references = Vec::new();
    for rank in ranks.into_iter().take(MAX_KEPT_WASTE_REFERENCES + 1) {
        delta_base_references.push(table.at(source.mechanics, rank)?.object_id);
    }
    let inode = tx.query(INODE_BLOCKS, Vec::<Param>::new())?;
    let mut inode_blocks_bytes = 0u64;
    for row in &inode {
        let length: i64 = cell(row, 0)?;
        inode_blocks_bytes =
            inode_blocks_bytes.saturating_add(u64::try_from(length).unwrap_or(u64::MAX));
    }
    Ok(Found {
        waste: KeptWaste::from_parts(
            dead_in_retained_packs_bytes,
            delta_bases_bytes,
            delta_base_references,
            inode_blocks_bytes,
        ),
        partly_dead: PartlyDead::new(partly_dead, partly_dead_count),
        retained,
        objects,
    })
}

/// Whether a retained Raw group's reached records explain its whole stored
/// extent. A rebuild copies a retained pack whole but keeps only the reached
/// records' rows, so a later mark sees a dead record that no row names;
/// comparing the directory extent with the live records' framing and widths
/// detects that case without reading the body. `live` is the group's reached
/// records.
fn group_may_hide_dead_records(view: layout::GroupView, live: &[Kept]) -> bool {
    let live_width = live
        .iter()
        .fold(0u64, |width, kept| width.saturating_add(kept.width));
    let extent = u64::try_from(view.end - view.start).unwrap_or(u64::MAX);
    extent != 4 + 4 * live.len() as u64 + live_width
}

/// The framed bytes of every record in a Raw group that no reached object owns,
/// including a record whose `object_location` row a rebuild did not retain. The
/// group's directory names every record's extent, so this is one pass over it.
/// `live` is the group's reached records, in record order.
fn dead_raw_record_bytes(directory: &RecordDirectory, live: &[Kept]) -> HistoryResult<u64> {
    let mut dead = 0u64;
    for record in 0..directory.count() {
        if live
            .binary_search_by_key(&(record as u64), |kept| kept.record)
            .is_ok()
        {
            continue;
        }
        let (start, end) = directory.range(record).map_err(storage)?;
        dead = dead.saturating_add(u64::try_from(end - start).unwrap_or(u64::MAX));
    }
    Ok(dead)
}

/// One retained pack's dead bytes, exact over a complete body. `records` are
/// the pack's reached records.
fn pack_waste(body: &[u8], records: &[Kept]) -> HistoryResult<u64> {
    let header = layout::parse_header(body).map_err(storage)?;
    let mut dead = 0u64;
    for group_number in 0..header.group_count {
        let view = layout::group_view(body, header, group_number).map_err(storage)?;
        let live = group(records, group_number);
        if live.is_empty() {
            // No reached object owns any record of this group, so its stored
            // extent stays only because the pack is retained.
            dead = dead.saturating_add(u64::try_from(view.end - view.start).unwrap_or(u64::MAX));
        } else if view.codec == GroupCodec::Raw && group_may_hide_dead_records(view, live) {
            // An exact per-record split is possible only when the body is
            // stored uncompressed. A compressed group's bytes cannot be
            // attributed to one record, so a retained group contributes
            // nothing and the error stays conservative.
            let directory = RecordDirectory::of(&body[view.start..view.end]).map_err(storage)?;
            dead = dead.saturating_add(dead_raw_record_bytes(&directory, live)?);
        }
    }
    Ok(dead)
}
