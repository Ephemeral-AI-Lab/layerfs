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
//!
//! A rewrite also changes what the mark's `keptWaste.deltaBases.bytes` says
//! of the pack. That figure charges a record kept only as a delta base the
//! stored width a read of it pays, and a record in a compressed group pays
//! the group's whole decoded length, which shrinks when the group is written
//! again without its dead records. Each kept row carries the mark's
//! delta-base-only flag, and the rewrite sums the width of those records in
//! the old pack and in the new one ([`Repacked::bases_before`],
//! [`Repacked::bases_after`]) with the function the mark charges them by, so
//! the receipt states the figure a mark of the rebuilt Store would.

use super::types::db_error;
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_storage::encoding::delta::read::record_width;
use layerfs_storage::encoding::{CompressionWorkspace, DecompressionWorkspace};
use layerfs_storage::location::ObjectLocation;
use layerfs_storage::pack::layout;
use layerfs_storage::pack::repack::retain_records;
use layerfs_storage::port::ObjectKey;
use rusqlite::{params, Connection};

/// The working tables in the visited scratch: the packs to write again, and
/// the locator of every kept object in them, keyed in the order [`apply`]
/// reads it. The object is part of the key, so a damaged Store in which two
/// objects name one record keeps both rows. `base_only` is the mark's flag
/// for an object reached only as a delta base.
const TABLES: &str = "CREATE TABLE scratch.repack (pack_id INTEGER PRIMARY KEY) \
STRICT, WITHOUT ROWID;\
CREATE TABLE scratch.repack_kept (\
 pack_id INTEGER NOT NULL, group_number INTEGER NOT NULL, record_number INTEGER NOT NULL,\
 object_id BLOB NOT NULL, role INTEGER NOT NULL, canonical_length INTEGER NOT NULL,\
 base_only INTEGER NOT NULL,\
 PRIMARY KEY(pack_id,group_number,record_number,object_id)\
) STRICT, WITHOUT ROWID;";

/// Fills `repack_kept`: one pass over the visited set (`CROSS JOIN` fixes
/// that order), a key lookup per row in the planned packs and in the sealed
/// locators, and an insert into the keyed scratch table. `visited` holds the
/// mark's whole §6.2 closure, so a record reached only as a delta base is in
/// this set.
const FILL: &str = "INSERT INTO scratch.repack_kept\
(pack_id,group_number,record_number,object_id,role,canonical_length,base_only) \
SELECT v.pack_id,l.group_number,l.record_number,v.object_id,l.role,l.canonical_length,\
v.base_only \
FROM scratch.visited v CROSS JOIN src.object_location l \
ON l.object_id=v.object_id AND l.pack_id=v.pack_id \
WHERE v.pack_id IN (SELECT pack_id FROM scratch.repack)";

/// The kept locators, one pack after another, in group and record order: a
/// scan of `repack_kept` in its key order.
const KEPT: &str = "SELECT pack_id,group_number,record_number,object_id,role,canonical_length,\
base_only FROM scratch.repack_kept ORDER BY pack_id,group_number,record_number,object_id";

/// One kept object of a pack being written again: its locator in the sealed
/// Store, and whether the mark reached it only as a delta base.
struct Kept {
    location: ObjectLocation,
    base_only: bool,
}

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
    /// What the mark's `keptWaste.deltaBases.bytes` charged the records of
    /// the packs written again that are kept only as a delta base.
    pub(crate) bases_before: u64,
    /// What a mark charges the same records in the packs as written again.
    pub(crate) bases_after: u64,
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
pub(super) struct Source {
    domain: i64,
    pub(super) digest: Vec<u8>,
    pub(super) body: Vec<u8>,
}

/// Reads one pack whole from `schema` (`src`, the sealed Store, or `main`, a
/// staged file) and checks it against the length and SHA-256 its row records.
/// `None` is a pack whose stored bytes are not the ones its row describes.
pub(super) fn read(
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

/// Writes one pack again from its kept locators, which are in group and
/// record order, and moves those locators to the new ordinals. A pack that
/// cannot be rewritten is copied whole and counted in
/// [`Repacked::copied_whole`].
fn rewrite(
    connection: &Connection,
    pack_id: i64,
    kept: &[Kept],
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
        for kept in kept {
            keep.get_mut(kept.location.group_number)?
                .push(kept.location.record_number);
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
    for kept in kept {
        let (group, record) = (kept.location.group_number, kept.location.record_number);
        if previous != Some(group) {
            position = 0;
            previous = Some(group);
        }
        let moved = rebuilt
            .groups
            .get(group)
            .copied()
            .flatten()
            .ok_or(HistoryError::Integrity("repack group mapping"))?;
        if (moved, position) != (group, record) {
            let changed = relocate
                .execute(params![
                    moved as i64,
                    position as i64,
                    kept.location.object_id.as_bytes().as_slice(),
                    pack_id
                ])
                .map_err(db_error)?;
            if changed != 1 {
                return Err(HistoryError::Integrity("repack locator"));
            }
        }
        if kept.base_only {
            // The width the mark charged this record, and the one it charges
            // the same record where the new pack holds it.
            let before = record_width(&source.body, &kept.location).map_err(storage)?;
            let after = record_width(
                &rebuilt.body,
                &ObjectLocation {
                    group_number: moved,
                    record_number: position,
                    ..kept.location
                },
            )
            .map_err(storage)?;
            done.bases_before = done.bases_before.saturating_add(before);
            done.bases_after = done.bases_after.saturating_add(after);
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
    let mut kept: Vec<Kept> = Vec::new();
    let mut finish =
        |pack_id: i64, kept: &mut Vec<Kept>, done: &mut Repacked| -> HistoryResult<()> {
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
        const LOCATOR: HistoryError = HistoryError::Integrity("repack locator");
        let pack_id: i64 = row.get(0).map_err(db_error)?;
        let number = |column: usize| -> HistoryResult<usize> {
            usize::try_from(row.get::<_, i64>(column).map_err(db_error)?).map_err(|_| LOCATOR)
        };
        let object = row.get_ref(3).map_err(db_error)?;
        let location = ObjectLocation {
            object_id: ObjectId::from_bytes(object.as_blob().map_err(|_| LOCATOR)?)
                .map_err(|_| LOCATOR)?,
            role: u8::try_from(number(4)?)
                .ok()
                .and_then(|code| ObjectRole::from_code(code).ok())
                .ok_or(LOCATOR)?,
            canonical_length: number(5)?,
            pack_id,
            group_number: number(1)?,
            record_number: number(2)?,
        };
        let base_only = row.get::<_, i64>(6).map_err(db_error)? != 0;
        if current.is_some_and(|open| open != pack_id) {
            finish(current.unwrap_or(pack_id), &mut kept, &mut done)?;
        }
        current = Some(pack_id);
        kept.push(Kept {
            location,
            base_only,
        });
    }
    if let Some(pack_id) = current {
        finish(pack_id, &mut kept, &mut done)?;
    }
    if planned.next().is_some() {
        return Err(HistoryError::Integrity("repack plan incomplete"));
    }
    Ok(done)
}

/// A pack that cannot be read or rebuilt stops the rebuild; the sealed Store
/// is unchanged, so nothing is uncertain.
fn storage(_error: layerfs_storage::StorageError) -> HistoryError {
    HistoryError::Integrity("repack pack")
}
