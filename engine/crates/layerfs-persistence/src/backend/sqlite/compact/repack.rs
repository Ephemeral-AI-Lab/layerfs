//! §7 rebuild: a retained pack that holds dead records is written again
//! without them.
//!
//! The copy keeps a pack when any of its records is reached, and a pack is one
//! unit, so the records nothing reaches used to cross every rebuild. This
//! module takes the packs the mark found such records in
//! (`KeptObjects::partly_dead_packs`) out of the whole-pack copy and writes
//! each one again from the records the visited set keeps, which includes every
//! record kept only as a delta base. The pack bytes are built by the core
//! storage crate (`pack::repack::retain_records`), never here; this module
//! reads the old pack from the sealed Store, checks it against its recorded
//! digest, stores the new pack under the same `pack_id` with its own digest and
//! length, and moves the kept objects' locators to their new group and record
//! numbers.
//!
//! Two files are written, both artifacts of this run that a failed or crashed
//! run leaves for the next call to remove. The new packs and the moved
//! locators go to the fresh `<store>.compact` file. The working set goes to
//! the mark's visited scratch, `<store>.compact-visited`: [`plan`] adds the
//! tables `repack` and `repack_kept` there and deletes the planned packs from
//! its `live_pack`. The sealed Store is attached `immutable=1` and never
//! changes, so a failure or a crash at any point leaves it as it was, exactly
//! as for the whole-pack copy.
//!
//! The run is two steps around the bulk copy, because the copy appends in key
//! order and the rewritten rows must not be copied whole first: [`plan`] names
//! the packs and removes them from `scratch.live_pack`, the set the bulk copy
//! of `pack` and `pack_unit` selects from; [`apply`] runs after the table copy
//! and before the totals are summed.
//!
//! A pack this step cannot rewrite is copied whole, exactly as every rebuild
//! copied it before: one whose stored bytes are not the ones its row
//! describes (rewriting it would give damaged bytes a fresh, matching
//! digest), one whose kept locators disagree with it (two objects on one
//! record, or a record the pack does not hold), and one the core rebuild
//! refuses for any other reason. Its locators stay as they were, and its dead
//! bytes stay in the report's `keptWaste.deadInRetainedPacksBytes`.
//!
//! The kept locators are put in pack order by the scratch table's own key,
//! on disk, and read back one pack at a time, so the rewrite holds one pack's
//! locators and bytes in memory, never the whole kept set. No statement here
//! sorts or groups in a temporary b-tree.

use super::types::db_error;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_storage::encoding::{CompressionWorkspace, DecompressionWorkspace};
use layerfs_storage::pack::layout;
use layerfs_storage::pack::repack::{record_counts, retain_records};
use layerfs_storage::port::ObjectKey;
use rusqlite::{params, Connection};

/// The working tables in the visited scratch: the packs to write again, and
/// the locator of every kept object in them, keyed in the order [`apply`]
/// reads it. The object is part of the key, so a damaged Store in which two
/// objects name one record keeps both rows.
const TABLES: &str = "CREATE TABLE scratch.repack (pack_id INTEGER PRIMARY KEY) \
STRICT, WITHOUT ROWID;\
CREATE TABLE scratch.repack_kept (\
 pack_id INTEGER NOT NULL, group_number INTEGER NOT NULL, record_number INTEGER NOT NULL,\
 object_id BLOB NOT NULL,\
 PRIMARY KEY(pack_id,group_number,record_number,object_id)\
) STRICT, WITHOUT ROWID;";

/// Fills `repack_kept`: one pass over the visited set (`CROSS JOIN` fixes
/// that order), a key lookup per row in the planned packs and in the sealed
/// locators, and an insert into the keyed scratch table. `visited` holds the mark's whole §6.2 closure, so a record
/// reached only as a delta base is in this set.
const FILL: &str = "INSERT INTO scratch.repack_kept(pack_id,group_number,record_number,object_id) \
SELECT v.pack_id,l.group_number,l.record_number,v.object_id \
FROM scratch.visited v CROSS JOIN src.object_location l \
ON l.object_id=v.object_id AND l.pack_id=v.pack_id \
WHERE v.pack_id IN (SELECT pack_id FROM scratch.repack)";

/// The kept locators, one pack after another, in group and record order: a
/// scan of `repack_kept` in its key order.
const KEPT: &str = "SELECT pack_id,group_number,record_number,object_id \
FROM scratch.repack_kept ORDER BY pack_id,group_number,record_number,object_id";

/// One pack this rebuild wrote again, as the build computed it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rewritten {
    /// The pack's id, unchanged by the rewrite.
    pub pack_id: i64,
    /// The SHA-256 of the new pack bytes.
    pub digest: [u8; 32],
    /// Their length.
    pub length: u64,
    /// The records the new pack holds.
    pub records: u64,
}

/// What a rebuild's repack step did, for the report and the verification.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Repacked {
    /// The packs written again, in ascending pack order.
    pub(crate) packs: Vec<Rewritten>,
    /// Their stored bytes in the sealed Store.
    pub(crate) bytes_before: u64,
    /// Their stored bytes in the fresh Store.
    pub(crate) bytes_after: u64,
    /// The mark's `deadInRetainedPacksBytes` share of the packs written again:
    /// the part of that kept waste this rebuild no longer keeps.
    pub(crate) marked_dead: u64,
    /// Planned packs copied whole instead, because their stored bytes or
    /// their locators did not allow a rewrite.
    pub(crate) copied_whole: u64,
}

/// The packs one rebuild writes again, with the mark's dead bytes for each.
pub(crate) struct Plan {
    packs: Vec<(i64, u64)>,
}

/// Names the packs to rewrite and takes them out of the whole-pack copy.
///
/// `partly_dead` is the mark's list. Every pack in it is retained, so each one
/// must leave `scratch.live_pack`; a pack that does not is not one the mark
/// kept, and the rebuild stops instead of guessing.
pub(crate) fn plan(connection: &Connection, partly_dead: &[(i64, u64)]) -> HistoryResult<Plan> {
    if partly_dead.is_empty() {
        return Ok(Plan { packs: Vec::new() });
    }
    // The working tables live in the visited scratch, beside the set this
    // edits: a temp table would open the connection's temp database, which the
    // build's later WAL checkpoint cannot pass, and would sit outside the
    // run's artifact set.
    connection.execute_batch(TABLES).map_err(db_error)?;
    {
        let mut insert = connection
            .prepare("INSERT INTO scratch.repack(pack_id) VALUES(?1)")
            .map_err(db_error)?;
        for (pack_id, _) in partly_dead {
            insert.execute(params![pack_id]).map_err(db_error)?;
        }
    }
    let removed = connection
        .execute(
            "DELETE FROM scratch.live_pack WHERE pack_id IN (SELECT pack_id FROM scratch.repack)",
            params![],
        )
        .map_err(db_error)?;
    if removed != partly_dead.len() {
        return Err(HistoryError::Integrity("repack names a pack not retained"));
    }
    connection.execute(FILL, params![]).map_err(db_error)?;
    Ok(Plan {
        packs: partly_dead.to_vec(),
    })
}

/// One stored pack row with its complete body.
struct Source {
    domain: i64,
    digest: Vec<u8>,
    body: Vec<u8>,
}

/// Reads one pack whole from `schema` (`src`, the sealed Store, or `main`, a
/// staged file) and checks it against the length and SHA-256 its row records.
/// `None` is a pack whose stored bytes are not the ones its row describes.
fn read(
    connection: &Connection,
    schema: &str,
    pack_id: i64,
    units: bool,
) -> HistoryResult<Option<Source>> {
    let column = if units { "control" } else { "body" };
    let (domain, digest, length, mut body): (i64, Vec<u8>, i64, Vec<u8>) = connection
        .prepare_cached(&format!(
            "SELECT domain,digest,length,{column} FROM {schema}.pack WHERE pack_id=?1"
        ))
        .map_err(db_error)?
        .query_row(params![pack_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(db_error)?;
    if units {
        let mut statement = connection
            .prepare_cached(&format!(
                "SELECT group_number,offset,body FROM {schema}.pack_unit WHERE pack_id=?1 \
ORDER BY group_number"
            ))
            .map_err(db_error)?;
        let mut rows = statement.query(params![pack_id]).map_err(db_error)?;
        let mut expected = 0i64;
        while let Some(row) = rows.next().map_err(db_error)? {
            let group: i64 = row.get(0).map_err(db_error)?;
            let offset: i64 = row.get(1).map_err(db_error)?;
            let unit = row.get_ref(2).map_err(db_error)?;
            let Ok(unit) = unit.as_blob() else {
                return Ok(None);
            };
            if group != expected || usize::try_from(offset).ok() != Some(body.len()) {
                return Ok(None);
            }
            body.extend_from_slice(unit);
            expected += 1;
        }
    }
    if usize::try_from(length).ok() != Some(body.len())
        || ObjectKey::for_bytes(&body).as_bytes().as_slice() != digest.as_slice()
    {
        return Ok(None);
    }
    // A row may hold more bytes than the pack declares; the pack is what its
    // own control area says it is.
    let Ok(used) = layout::declared_length(&body) else {
        return Ok(None);
    };
    body.truncate(used);
    Ok(Some(Source {
        domain,
        digest,
        body,
    }))
}

/// Stores one rebuilt pack in the fresh file, in the Store's own pack layout.
fn write(
    connection: &Connection,
    pack_id: i64,
    domain: i64,
    digest: &[u8; 32],
    body: &[u8],
    units: bool,
) -> HistoryResult<()> {
    let length = i64::try_from(body.len()).map_err(|_| HistoryError::Integrity("repack length"))?;
    if !units {
        connection
            .prepare_cached(
                "INSERT INTO main.pack(pack_id,domain,digest,length,body) VALUES(?1,?2,?3,?4,?5)",
            )
            .map_err(db_error)?
            .execute(params![pack_id, domain, digest.as_slice(), length, body])
            .map_err(db_error)?;
        return Ok(());
    }
    let header = layout::parse_header(body).map_err(storage)?;
    let views = layout::directory_group_views(body, header).map_err(storage)?;
    connection
        .prepare_cached(
            "INSERT INTO main.pack(pack_id,domain,digest,length,control) VALUES(?1,?2,?3,?4,?5)",
        )
        .map_err(db_error)?
        .execute(params![
            pack_id,
            domain,
            digest.as_slice(),
            length,
            &body[..header.body_offset]
        ])
        .map_err(db_error)?;
    let mut insert = connection
        .prepare_cached(
            "INSERT INTO main.pack_unit(pack_id,group_number,offset,length,body) \
VALUES(?1,?2,?3,?4,?5)",
        )
        .map_err(db_error)?;
    for (number, view) in views.iter().enumerate() {
        insert
            .execute(params![
                pack_id,
                number as i64,
                view.start as i64,
                (view.end - view.start) as i64,
                &body[view.start..view.end]
            ])
            .map_err(db_error)?;
    }
    Ok(())
}

/// Copies one pack whole: the mark named it, and it holds nothing to drop
/// after all or cannot be rewritten. New unit rows take new ids, like the
/// rewritten ones.
fn copy_whole(connection: &Connection, pack_id: i64, units: bool) -> HistoryResult<()> {
    connection
        .prepare_cached("INSERT INTO main.pack SELECT * FROM src.pack WHERE pack_id=?1")
        .map_err(db_error)?
        .execute(params![pack_id])
        .map_err(db_error)?;
    if units {
        connection
            .prepare_cached(
                "INSERT INTO main.pack_unit(pack_id,group_number,offset,length,body) \
SELECT pack_id,group_number,offset,length,body FROM src.pack_unit WHERE pack_id=?1 \
ORDER BY group_number",
            )
            .map_err(db_error)?
            .execute(params![pack_id])
            .map_err(db_error)?;
    }
    Ok(())
}

/// The codec workspaces one repack step reuses for every pack.
struct Codecs {
    compression: CompressionWorkspace,
    decompression: DecompressionWorkspace,
}

/// Writes one pack again from its kept `(group, record, object)` locators and
/// moves those locators to the new ordinals. A pack that cannot be rewritten
/// is copied whole and counted in [`Repacked::copied_whole`].
fn rewrite(
    connection: &Connection,
    pack_id: i64,
    kept: &[(usize, usize, Vec<u8>)],
    units: bool,
    codecs: &mut Codecs,
    done: &mut Repacked,
) -> HistoryResult<()> {
    // The rewrite needs the pack's own bytes and a kept set the pack can
    // satisfy. `retain_records` refuses a record named twice (two objects on
    // one record arrive as neighbours in key order) or not held, and a pack
    // it cannot parse or decode.
    let rebuilt = read(connection, "src", pack_id, units)?.and_then(|source| {
        let header = layout::parse_header(&source.body).ok()?;
        let mut keep = vec![Vec::new(); header.group_count];
        for (group, record, _) in kept {
            keep.get_mut(*group)?.push(*record);
        }
        let rebuilt = retain_records(
            &source.body,
            &keep,
            &mut codecs.compression,
            &mut codecs.decompression,
        );
        Some((source, rebuilt.ok()??))
    });
    let Some((source, rebuilt)) = rebuilt else {
        done.copied_whole += 1;
        return copy_whole(connection, pack_id, units);
    };
    let digest = *ObjectKey::for_bytes(&rebuilt.body).as_bytes();
    write(
        connection,
        pack_id,
        source.domain,
        &digest,
        &rebuilt.body,
        units,
    )?;
    let mut relocate = connection
        .prepare_cached(
            "UPDATE main.object_location SET group_number=?1,record_number=?2 \
WHERE object_id=?3 AND pack_id=?4",
        )
        .map_err(db_error)?;
    let mut position = 0usize;
    let mut previous = None;
    for (group, record, object) in kept {
        if previous != Some(*group) {
            position = 0;
            previous = Some(*group);
        }
        let moved = rebuilt
            .groups
            .get(*group)
            .copied()
            .flatten()
            .ok_or(HistoryError::Integrity("repack group mapping"))?;
        if (moved, position) != (*group, *record) {
            let changed = relocate
                .execute(params![moved as i64, position as i64, object, pack_id])
                .map_err(db_error)?;
            if changed != 1 {
                return Err(HistoryError::Integrity("repack locator"));
            }
        }
        position += 1;
    }
    done.bytes_before = done.bytes_before.saturating_add(source.body.len() as u64);
    done.bytes_after = done.bytes_after.saturating_add(rebuilt.body.len() as u64);
    done.packs.push(Rewritten {
        pack_id,
        digest,
        length: rebuilt.body.len() as u64,
        records: rebuilt.records,
    });
    Ok(())
}

/// Writes every planned pack again into the fresh file. The caller has copied
/// the other tables, so each kept object's locator row is there to move.
pub(crate) fn apply(connection: &Connection, plan: &Plan, units: bool) -> HistoryResult<Repacked> {
    let mut done = Repacked::default();
    if plan.packs.is_empty() {
        return Ok(done);
    }
    let mut codecs = Codecs {
        compression: CompressionWorkspace::new().map_err(storage)?,
        decompression: DecompressionWorkspace::new().map_err(storage)?,
    };
    let mut statement = connection.prepare(KEPT).map_err(db_error)?;
    let mut rows = statement.query(params![]).map_err(db_error)?;
    let mut planned = plan.packs.iter();
    let mut current: Option<i64> = None;
    let mut kept: Vec<(usize, usize, Vec<u8>)> = Vec::new();
    let mut finish = |pack_id: i64,
                      kept: &mut Vec<(usize, usize, Vec<u8>)>,
                      done: &mut Repacked|
     -> HistoryResult<()> {
        // Both lists ascend by pack, and every planned pack holds a kept
        // object: a pack out of step means the mark and its scratch disagree.
        let (planned_id, marked) = planned
            .next()
            .ok_or(HistoryError::Integrity("repack plan"))?;
        if *planned_id != pack_id {
            return Err(HistoryError::Integrity("repack plan order"));
        }
        let before = done.packs.len();
        rewrite(connection, pack_id, kept, units, &mut codecs, done)?;
        if done.packs.len() > before {
            done.marked_dead = done.marked_dead.saturating_add(*marked);
        }
        kept.clear();
        Ok(())
    };
    while let Some(row) = rows.next().map_err(db_error)? {
        let pack_id: i64 = row.get(0).map_err(db_error)?;
        let group: i64 = row.get(1).map_err(db_error)?;
        let record: i64 = row.get(2).map_err(db_error)?;
        let object: Vec<u8> = row.get(3).map_err(db_error)?;
        if current.is_some_and(|open| open != pack_id) {
            finish(current.unwrap_or(pack_id), &mut kept, &mut done)?;
        }
        current = Some(pack_id);
        kept.push((
            usize::try_from(group).map_err(|_| HistoryError::Integrity("repack locator"))?,
            usize::try_from(record).map_err(|_| HistoryError::Integrity("repack locator"))?,
            object,
        ));
    }
    if let Some(pack_id) = current {
        finish(pack_id, &mut kept, &mut done)?;
    }
    if planned.next().is_some() {
        return Err(HistoryError::Integrity("repack plan incomplete"));
    }
    Ok(done)
}

/// Whether the staged file keeps pack payload in `pack_unit` rows.
fn unit_layout(connection: &Connection) -> HistoryResult<bool> {
    let tables: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='pack_unit'",
            params![],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    Ok(tables == 1)
}

/// A staged pack or locator that is not what the build wrote.
const STAGED_PACK: HistoryError = HistoryError::Integrity("staged Store repacked pack");
const STAGED_LOCATOR: HistoryError = HistoryError::Integrity("staged Store repacked locators");

/// §7 step 6 for the packs written again, read back from the staged file.
///
/// Each pack's stored bytes are read whole and hashed: they must carry the
/// SHA-256 and length of their row, and both must be the ones the build
/// computed. The pack's own directories then state how many records each
/// group holds (a compressed group is decoded for its count), and the staged
/// locators that name the pack must be exactly those records: every
/// `(group, record)` in range and named once, none left unnamed.
///
/// The locators are read in one plain scan of `object_location`, which has no
/// index by pack; the scan keeps no SQLite temporary structure. This pass
/// holds one pack body at a time, and for all rewritten packs together one
/// count per group and one bit per record.
///
/// `packs` ascends by pack id, as [`apply`] reports it.
pub fn verify(connection: &Connection, packs: &[Rewritten]) -> HistoryResult<()> {
    let (Some(first), Some(last)) = (packs.first(), packs.last()) else {
        return Ok(());
    };
    let units = unit_layout(connection)?;
    let mut decompression = DecompressionWorkspace::new().map_err(storage)?;
    // Per pack, where its groups start in `ends`; per group, the records the
    // pack holds up to and including it. A record's bit is its pack's first
    // bit plus the records before its group plus its own number.
    let mut slots: Vec<(usize, usize)> = Vec::with_capacity(packs.len());
    let mut ends: Vec<u32> = Vec::new();
    let mut records = 0usize;
    for pack in packs {
        let stored = read(connection, "main", pack.pack_id, units)?.ok_or(STAGED_PACK)?;
        if stored.digest.as_slice() != pack.digest.as_slice()
            || stored.body.len() as u64 != pack.length
        {
            return Err(STAGED_PACK);
        }
        let counts = record_counts(&stored.body, &mut decompression).map_err(|_| STAGED_PACK)?;
        slots.push((ends.len(), records));
        let mut held = 0usize;
        for count in counts {
            held += count;
            ends.push(u32::try_from(held).map_err(|_| STAGED_PACK)?);
        }
        if held as u64 != pack.records {
            return Err(STAGED_PACK);
        }
        records += held;
    }
    slots.push((ends.len(), records));
    let mut named = vec![0u64; records.div_ceil(64)];
    let mut seen = 0usize;
    let mut statement = connection
        .prepare(
            "SELECT pack_id,group_number,record_number FROM object_location \
WHERE pack_id BETWEEN ?1 AND ?2",
        )
        .map_err(db_error)?;
    let mut rows = statement
        .query(params![first.pack_id, last.pack_id])
        .map_err(db_error)?;
    while let Some(row) = rows.next().map_err(db_error)? {
        let pack_id: i64 = row.get(0).map_err(db_error)?;
        let Ok(at) = packs.binary_search_by_key(&pack_id, |pack| pack.pack_id) else {
            continue;
        };
        let group = usize::try_from(row.get::<_, i64>(1).map_err(db_error)?);
        let record = usize::try_from(row.get::<_, i64>(2).map_err(db_error)?);
        let (Ok(group), Ok(record)) = (group, record) else {
            return Err(STAGED_LOCATOR);
        };
        let (groups_at, first_bit) = slots[at];
        let groups = &ends[groups_at..slots[at + 1].0];
        let end = *groups.get(group).ok_or(STAGED_LOCATOR)? as usize;
        let start = if group == 0 {
            0
        } else {
            groups[group - 1] as usize
        };
        if record >= end - start {
            return Err(STAGED_LOCATOR);
        }
        let bit = first_bit + start + record;
        if named[bit / 64] & (1 << (bit % 64)) != 0 {
            return Err(STAGED_LOCATOR);
        }
        named[bit / 64] |= 1 << (bit % 64);
        seen += 1;
    }
    if seen != records {
        return Err(STAGED_LOCATOR);
    }
    Ok(())
}

/// A pack that cannot be read or rebuilt stops the rebuild; the sealed Store
/// is unchanged, so nothing is uncertain.
fn storage(_error: layerfs_storage::StorageError) -> HistoryError {
    HistoryError::Integrity("repack pack")
}
