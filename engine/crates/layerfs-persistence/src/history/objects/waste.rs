//! Kept waste of one mark: what the retained packs hold beyond the objects
//! the walk reached. Split from the parent module, which owns the walk; the
//! types and statements used here are the parent's.
use super::*;

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
    located: &Located,
    closure: &Closure,
) -> HistoryResult<(KeptWaste, Vec<(i64, u64)>)> {
    let mut dead_in_retained_packs_bytes = 0u64;
    // Reached records per retained pack, in pack order: the same single pass
    // over the closure that names the retained packs.
    let mut reached: BTreeMap<i64, usize> = BTreeMap::new();
    for location in closure.visited.values() {
        *reached.entry(location.pack_id).or_default() += 1;
    }
    // Packs the walk did not lay out have their directory prefix read here:
    // a bounded batch of packs per call, not one call per pack.
    let mut prefixes = {
        let layouts = source.layouts.borrow();
        let unread = reached.keys().filter(|pack| !layouts.contains_key(pack));
        edges::Prefixes::new(source, unread.copied().collect())
    };
    let mut partly_dead = Vec::new();
    let mut counted = (0i64, 0u64, false);
    for (pack_id, reached) in reached {
        // The pack before this one is complete: record what it added.
        if counted.2 || dead_in_retained_packs_bytes > counted.1 {
            partly_dead.push((counted.0, dead_in_retained_packs_bytes - counted.1));
        }
        let groups = located.inventory.get(&pack_id);
        // A pack that locates more records than the walk reached in it holds
        // a dead one. That is the only sign a compressed group gives; its
        // bytes are not attributed.
        let located_records = groups.map_or(0, |groups| groups.values().map(BTreeMap::len).sum());
        counted = (
            pack_id,
            dead_in_retained_packs_bytes,
            located_records > reached,
        );
        // The walk already read this pack's directory when it read its
        // record headers; only a pack it reached another way is read here.
        let kept = source.layouts.borrow().get(&pack_id).cloned();
        let views = if let Some(views) = kept {
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
                    .saturating_add(pack_waste(&prefix[..used], groups, closure)?);
                continue;
            }
            let header = layout::parse_directory_header(&prefix, used).map_err(storage)?;
            layout::directory_group_views(&prefix, header).map_err(storage)?
        };
        // A wholly dead group's stored extent is known from the directory. A
        // retained Raw group whose inventory no longer accounts for every
        // record has its own record directory read, so its exact per-record
        // split costs neither the pack body nor the group's.
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
        // Each record's extent is in the group's directory, so the records
        // themselves stay unread.
        let views: Vec<layout::GroupView> = mixed_raw.iter().map(|(_, view)| *view).collect();
        let directories = edges::group_directories(source, pack_id, &views)?;
        for ((number, _), directory) in mixed_raw.iter().zip(&directories) {
            let records = groups.and_then(|groups| groups.get(number));
            dead_in_retained_packs_bytes = dead_in_retained_packs_bytes
                .saturating_add(dead_raw_record_bytes(directory, records, closure)?);
        }
    }
    if counted.2 || dead_in_retained_packs_bytes > counted.1 {
        partly_dead.push((counted.0, dead_in_retained_packs_bytes - counted.1));
    }
    let mut deltas: Vec<ObjectId> = closure
        .bases
        .difference(&closure.logical)
        .copied()
        .collect();
    deltas.sort_unstable();
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
    Ok((
        KeptWaste::from_parts(
            dead_in_retained_packs_bytes,
            delta_bases_bytes,
            delta_base_references,
            inode_blocks_bytes,
        ),
        partly_dead,
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
/// group's directory names every record's extent, so this is one pass over it.
fn dead_raw_record_bytes(
    directory: &RecordDirectory,
    records: Option<&BTreeMap<usize, ObjectId>>,
    closure: &Closure,
) -> HistoryResult<u64> {
    let mut dead = 0u64;
    for record in 0..directory.count() {
        let live = records
            .and_then(|records| records.get(&record))
            .is_some_and(|id| closure.visited.contains_key(id));
        if live {
            continue;
        }
        let (start, end) = directory.range(record).map_err(storage)?;
        dead = dead.saturating_add(u64::try_from(end - start).unwrap_or(u64::MAX));
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
            let directory = RecordDirectory::of(&body[view.start..view.end]).map_err(storage)?;
            dead = dead.saturating_add(dead_raw_record_bytes(&directory, records, closure)?);
        }
    }
    Ok(dead)
}
