//! Bounded Metadata reads; the shared modules check every bound around them.
use super::super::{schema::SCHEMA_VERSION, session::transaction::Transaction};
use crate::backend::records::{pack_info, BackendError, Record};
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    location::{InodeValueRow, LocatedObject, ObjectLocation, SignatureRow},
    port::PersistenceError,
    StoragePolicy,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub(crate) fn policy(tx: &Transaction<'_>) -> Result<StoragePolicy, PersistenceError> {
    let rows = tx.query("SELECT format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth,schema_version FROM store_policy WHERE id=1", vec![])?;
    let r = rows
        .first()
        .filter(|_| rows.len() == 1)
        .ok_or(PersistenceError::Missing)?;
    let byte = |i| u8::try_from(r.get::<i64>(i)?).map_err(|_| BackendError::Integrity);
    let threshold = u64::try_from(r.get::<i64>(1)?).map_err(|_| BackendError::Integrity)?;
    if r.get::<i64>(5)? != SCHEMA_VERSION {
        return Err(PersistenceError::Malformed);
    }
    StoragePolicy::new(byte(0)?, threshold, byte(2)?, byte(3)?)
        .with_metadata_depth(byte(4)?)
        .validated()
        .map_err(|_| PersistenceError::Malformed)
}

pub(crate) fn cursor(tx: &Transaction<'_>) -> Result<(i64, i64), BackendError> {
    let rows = tx.query(
        "SELECT next_pack_id,next_ordinal FROM store_policy WHERE id=1",
        vec![],
    )?;
    let r = rows.first().ok_or(BackendError::Integrity)?;
    Ok((r.get(0)?, r.get(1)?))
}

/// Every stored location among `ids`, in one statement that names the
/// schema, so it also runs outside a transaction.
pub(crate) fn locate(
    tx: &Transaction<'_>,
    ids: &[ObjectId],
    out: &mut Vec<LocatedObject>,
) -> Result<(), PersistenceError> {
    let ids: Vec<Vec<u8>> = ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|id| id.as_bytes().to_vec())
        .collect();
    let keys: Vec<i64> = ids
        .iter()
        .map(|id| ObjectId::from_bytes(id).map(|id| super::super::objects::publish::prefix(&id)))
        .collect::<Result<_, _>>()
        .map_err(|_| BackendError::Integrity)?;
    // Each id finds its rows through the prefix index, then the full id is
    // compared (#90); `OFFSET 0` keeps that a probe per id rather than a hash
    // of the whole table.
    let sql = format!(
        "SELECT CASE WHEN octet_length(o.object_id)=32 THEN o.object_id END,o.role::bigint,o.canonical_length::bigint,o.group_number::bigint,o.record_number::bigint,p.pack_id,p.domain,CASE WHEN octet_length(p.digest)=32 THEN p.digest END,p.length \
         FROM unnest($1::bytea[],$2::bigint[]) k(i,k) CROSS JOIN LATERAL (SELECT * FROM {s}.object_location x \
         WHERE ('x'||encode(substring(x.object_id FROM 1 FOR 8),'hex'))::bit(64)::bigint=k.k AND x.object_id=k.i OFFSET 0) o \
         JOIN {s}.pack p ON p.pack_id=o.pack_id",
        s = tx.schema
    );
    let rows = tx.typed(&sql, &[&ids, &keys], ids.len() as u64 * 40)?;
    for row in rows {
        let r = super::super::session::rows::record(&row)?;
        out.push(location(&r)?);
    }
    Ok(())
}

fn location(r: &Record) -> Result<LocatedObject, BackendError> {
    let object_id =
        ObjectId::from_bytes(&r.get::<Vec<u8>>(0)?).map_err(|_| BackendError::Integrity)?;
    let pack = pack_info(r.get(5)?, r.get(6)?, &r.get::<Vec<u8>>(7)?, r.get(8)?)?;
    let role =
        ObjectRole::from_code(u8::try_from(r.get::<i64>(1)?).map_err(|_| BackendError::Integrity)?)
            .map_err(|_| BackendError::Integrity)?;
    let number = |index| usize::try_from(r.get::<i64>(index)?).map_err(|_| BackendError::Integrity);
    Ok(LocatedObject {
        location: ObjectLocation {
            object_id,
            role,
            canonical_length: number(2)?,
            pack_id: pack.pack_id,
            group_number: number(3)?,
            record_number: number(4)?,
        },
        pack,
    })
}

/// Locators one transaction keeps between [`expect_locations`] and the
/// lookups it announced, in two generations: when the young one holds half
/// of this, the old one is dropped. About 120 bytes each.
const HELD_LOCATIONS: usize = 16_384;
/// Identities one [`expect_locations`] statement carries.
const LOCATE_IDS: usize = 4096;

/// The locators a transaction was told to expect lookups of.
///
/// A published locator never changes inside one snapshot, so a held one is
/// the row a lookup would return. A walk that rebuilds the records of a pack
/// asks for the locator of each one's delta base, one identity at a time;
/// announced together they cost one round trip and not one each.
#[derive(Default)]
pub(crate) struct HeldLocations {
    young: HashMap<ObjectId, ObjectLocation>,
    old: HashMap<ObjectId, ObjectLocation>,
}

impl HeldLocations {
    fn get(&self, id: &ObjectId) -> Option<ObjectLocation> {
        self.young.get(id).or_else(|| self.old.get(id)).copied()
    }

    fn keep(&mut self, location: ObjectLocation) {
        if self.young.len() >= HELD_LOCATIONS / 2 {
            self.old = std::mem::take(&mut self.young);
        }
        // The first row found for an identity answers for it, as it does
        // for a lookup on its own.
        self.young.entry(location.object_id).or_insert(location);
    }
}

/// Finds the locators of `ids` this transaction does not hold, a bounded
/// piece per statement, and keeps them for [`held_location`]. Inside a
/// transaction only: an autocommit statement is fenced on its own.
pub(crate) fn expect_locations(
    tx: &Transaction<'_>,
    ids: &[ObjectId],
) -> Result<(), PersistenceError> {
    if !tx.unfenced() {
        return Ok(());
    }
    let missing: Vec<ObjectId> = {
        let held = tx.located.borrow();
        ids.iter()
            .copied()
            .filter(|id| held.get(id).is_none())
            .collect()
    };
    for ids in missing.chunks(LOCATE_IDS) {
        let mut found = Vec::with_capacity(ids.len());
        locate(tx, ids, &mut found)?;
        let mut held = tx.located.borrow_mut();
        for located in found {
            held.keep(located.location);
        }
    }
    Ok(())
}

/// The locator of `id` when this transaction holds it.
pub(crate) fn held_location(tx: &Transaction<'_>, id: &ObjectId) -> Option<ObjectLocation> {
    tx.located.borrow().get(id)
}

/// Up to `limit` locators that follow `after` in the order of the prefix
/// index: the identity's first eight bytes as a signed number, then its
/// bytes. The table has no index on the whole identity, so the walk follows
/// the prefix index from the prefix of `after` and orders only the rows that
/// share a prefix. Nothing but the writer keeps an identity to one row here,
/// so one row is returned for an identity, the one with the least locator:
/// a second row is for Phase A to refuse, and a piece that ended between the
/// two would lose it. The statement names the schema.
pub(crate) fn locations_after(
    tx: &Transaction<'_>,
    after: Option<&ObjectId>,
    limit: usize,
    out: &mut Vec<ObjectLocation>,
) -> Result<(), PersistenceError> {
    const PREFIX: &str =
        "(('x'||encode(substring(object_id FROM 1 FOR 8),'hex'))::bit(64)::bigint)";
    let sql = format!(
        "SELECT DISTINCT ON ({PREFIX},object_id) object_id,pack_id,group_number::bigint,record_number::bigint,role::bigint,canonical_length::bigint \
         FROM {s}.object_location WHERE {PREFIX}>=$1 AND ({PREFIX}>$1 OR object_id>$2) \
         ORDER BY {PREFIX},object_id,pack_id,group_number,record_number LIMIT $3",
        s = tx.schema
    );
    let prefix = after.map_or(i64::MIN, super::super::objects::publish::prefix);
    let key: &[u8] = after.map_or(&[], |id| id.as_bytes().as_slice());
    let count = i64::try_from(limit).map_err(|_| BackendError::Integrity)?;
    for row in tx.typed(&sql, &[&prefix, &key, &count], 48)? {
        let r = super::super::session::rows::record(&row)?;
        let number =
            |index| usize::try_from(r.get::<i64>(index)?).map_err(|_| BackendError::Integrity);
        out.push(ObjectLocation {
            object_id: ObjectId::from_bytes(&r.get::<Vec<u8>>(0)?)
                .map_err(|_| BackendError::Integrity)?,
            pack_id: r.get(1)?,
            group_number: number(2)?,
            record_number: number(3)?,
            role: ObjectRole::from_code(
                u8::try_from(r.get::<i64>(4)?).map_err(|_| BackendError::Integrity)?,
            )
            .map_err(|_| BackendError::Integrity)?,
            canonical_length: number(5)?,
        });
    }
    Ok(())
}

/// Every stored value among ascending distinct ordinals, in ordinal order,
/// in one statement that names the schema. Each run of consecutive ordinals
/// seeks the last block starting at or before it and takes the blocks from
/// there to its end; each block is returned once, in ordinal order.
pub(crate) fn inode_values(
    tx: &Transaction<'_>,
    ordinals: &[u32],
    out: &mut Vec<InodeValueRow>,
) -> Result<(), PersistenceError> {
    crate::blocks::values::pick(ordinals, value_blocks(tx, ordinals, 0, &[])?, out)?;
    Ok(())
}

/// Bytes of value-block bodies one transaction keeps between
/// [`inode_values_ahead`] calls, in two generations: when the young one
/// has reached half of this, the next call drops the old one. A call never
/// drops a block it read, so what it keeps can exceed this by its own
/// statement.
const HELD_BLOCK_BYTES: usize = 16 * 1024 * 1024;

/// The value blocks one transaction has read, by first ordinal, each with
/// its last ordinal and body.
///
/// A block is written once and never changes, so a held block answers for
/// its ordinals as the server would. A walk that names the same ordinals
/// again and again, scattered over the blocks they were first stored in,
/// then pays one round trip per block and not one per visit.
#[derive(Default)]
pub(crate) struct HeldBlocks {
    young: BTreeMap<u32, (u32, Vec<u8>)>,
    old: BTreeMap<u32, (u32, Vec<u8>)>,
    young_bytes: usize,
}

impl HeldBlocks {
    /// The first ordinal of the held block that holds `ordinal`.
    fn find(&self, ordinal: u32) -> Option<u32> {
        [&self.young, &self.old].into_iter().find_map(|blocks| {
            let (first, (last, _)) = blocks.range(..=ordinal).next_back()?;
            (ordinal <= *last).then_some(*first)
        })
    }

    /// The first ordinals of the held blocks starting after `after` and no
    /// later than `through`.
    fn firsts(&self, after: u32, through: u32) -> Vec<i64> {
        let Some(from) = after.checked_add(1).filter(|from| *from <= through) else {
            return Vec::new();
        };
        [&self.young, &self.old]
            .into_iter()
            .flat_map(|blocks| blocks.range(from..=through))
            .map(|(first, _)| i64::from(*first))
            .collect()
    }

    fn get(&self, first: u32) -> Option<&(u32, Vec<u8>)> {
        self.young.get(&first).or_else(|| self.old.get(&first))
    }

    /// Ages the generations, at the start of a call: every block a call
    /// finds or reads stays held until that call has handed it back.
    fn turn(&mut self) {
        if self.young_bytes > HELD_BLOCK_BYTES / 2 {
            self.old = std::mem::take(&mut self.young);
            self.young_bytes = 0;
        }
    }

    fn keep(&mut self, first: u32, last: u32, body: Vec<u8>) {
        self.young_bytes += body.len();
        if let Some((_, replaced)) = self.young.insert(first, (last, body)) {
            self.young_bytes -= replaced.len();
        }
    }
}

/// [`inode_values`], and every other value of the blocks that hold the asked
/// ordinals. The blocks the transaction has not read yet come in one
/// statement, with those starting up to `ahead` ordinals past the last
/// missing one; inside a transaction every block read is kept
/// ([`HeldBlocks`]), so the blocks read ahead answer later calls and the
/// caller is handed only the blocks it asked for.
pub(crate) fn inode_values_ahead(
    tx: &Transaction<'_>,
    ordinals: &[u32],
    ahead: u32,
    out: &mut Vec<InodeValueRow>,
) -> Result<(), PersistenceError> {
    let mut held = tx.values.borrow_mut();
    held.turn();
    let missing: Vec<u32> = ordinals
        .iter()
        .copied()
        .filter(|ordinal| held.find(*ordinal).is_none())
        .collect();
    if let Some(last) = missing.last().copied() {
        // Blocks read ahead by an earlier call lie in this call's window too.
        let skip = held.firsts(last, last.saturating_add(ahead));
        let mut end = 0u32;
        for block in value_blocks(tx, &missing, ahead, &skip)? {
            let (first, last, body) = block?;
            // The same order rule `pick` applies: blocks ascend and never overlap.
            if first <= end {
                return Err(BackendError::Integrity.into());
            }
            end = last;
            // An autocommit read keeps nothing: each of its statements is
            // fenced on its own.
            if tx.unfenced() {
                held.keep(first, last, body);
            } else {
                push_block(first, last, &body, out)?;
            }
        }
        if !tx.unfenced() {
            return Ok(());
        }
    }
    // The blocks of the asked ordinals, each once, in ordinal order. An
    // ordinal no block holds has no value and is left out, as `pick` does.
    let mut end = 0u32;
    for ordinal in ordinals {
        if *ordinal <= end {
            continue;
        }
        let Some(first) = held.find(*ordinal) else {
            continue;
        };
        let (last, body) = held.get(first).ok_or(BackendError::Integrity)?;
        push_block(first, *last, body, out)?;
        end = *last;
    }
    Ok(())
}

/// Every value of one block, in ordinal order.
fn push_block(
    first: u32,
    last: u32,
    body: &[u8],
    out: &mut Vec<InodeValueRow>,
) -> Result<(), PersistenceError> {
    let values = crate::blocks::values::decode(first, last, body)?;
    out.extend(
        (first..=last)
            .zip(values)
            .map(|(ordinal, value)| InodeValueRow { ordinal, value }),
    );
    Ok(())
}

/// One stored block: its first and last ordinal and its body.
type ValueBlock = Result<(u32, u32, Vec<u8>), BackendError>;

/// The value blocks covering `ordinals`, and those starting within `ahead`
/// ordinals past the last one, in ascending first-ordinal order. A block
/// whose first ordinal is in `held` is left out: the caller has it.
fn value_blocks(
    tx: &Transaction<'_>,
    ordinals: &[u32],
    ahead: u32,
    held: &[i64],
) -> Result<Vec<ValueBlock>, PersistenceError> {
    let (mut lows, mut highs) = (Vec::new(), Vec::new());
    for &ordinal in ordinals {
        match highs.last_mut() {
            Some(high) if *high + 1 == i64::from(ordinal) => *high += 1,
            _ => {
                lows.push(i64::from(ordinal));
                highs.push(i64::from(ordinal));
            }
        }
    }
    let sql = format!(
        "SELECT b.first_ordinal,b.last_ordinal,CASE WHEN octet_length(b.body) BETWEEN 9 AND 4000 THEN b.body END FROM {s}.inode_value_block b WHERE b.first_ordinal IN (\
         SELECT x.first_ordinal FROM unnest($1::bigint[],$2::bigint[]) r(lo,hi) JOIN {s}.inode_value_block x \
         ON x.first_ordinal BETWEEN coalesce((SELECT max(y.first_ordinal) FROM {s}.inode_value_block y \
         WHERE y.first_ordinal<=r.lo),0) AND r.hi) AND NOT (b.first_ordinal = ANY($3::bigint[])) \
         ORDER BY b.first_ordinal",
        s = tx.schema
    );
    if let Some(high) = highs.last_mut() {
        *high += i64::from(ahead);
    }
    let rows = tx.typed(
        &sql,
        &[&lows, &highs, &held],
        (lows.len() as u64 * 2 + held.len() as u64) * 8,
    )?;
    let ordinal = |v: i64| u32::try_from(v).map_err(|_| BackendError::Integrity);
    Ok(rows
        .iter()
        .map(|row| {
            let first: i64 = row.try_get(0).map_err(|_| BackendError::Integrity)?;
            let last: i64 = row.try_get(1).map_err(|_| BackendError::Integrity)?;
            let body: Vec<u8> = row.try_get(2).map_err(|_| BackendError::Integrity)?;
            Ok((ordinal(first)?, ordinal(last)?, body))
        })
        .collect())
}

/// The signature ring in insertion (stamp) order.
pub(crate) fn signatures(
    tx: &Transaction<'_>,
    out: &mut Vec<SignatureRow>,
) -> Result<(), PersistenceError> {
    let rows = tx.query(
        "SELECT slot,stamp,CASE WHEN octet_length(object_id)=32 THEN object_id END,CASE WHEN octet_length(signature)=32 THEN signature END FROM content_signature ORDER BY slot LIMIT 8193",
        vec![],
    )?;
    if rows.len() > 8192 {
        return Err(PersistenceError::Malformed);
    }
    for r in rows {
        let number =
            |index| u64::try_from(r.get::<i64>(index)?).map_err(|_| BackendError::Integrity);
        out.push(SignatureRow {
            slot: usize::try_from(number(0)?).map_err(|_| BackendError::Integrity)?,
            stamp: number(1)?,
            object_id: ObjectId::from_bytes(&r.get::<Vec<u8>>(2)?)
                .map_err(|_| BackendError::Integrity)?,
            signature: r
                .get::<Vec<u8>>(3)?
                .try_into()
                .map_err(|_| BackendError::Integrity)?,
        });
    }
    out.sort_unstable_by_key(|row| row.stamp);
    Ok(())
}
