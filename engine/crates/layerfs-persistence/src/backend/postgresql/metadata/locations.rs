//! Object location reads: locate, the locators a transaction holds, and the
//! prefix-ordered walk. The shared modules check every bound around them.
use super::super::session::transaction::Transaction;
use crate::backend::error::BackendError;
use crate::backend::seam::{pack_info, Record};
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    location::{LocatedObject, ObjectLocation},
    port::PersistenceError,
};
use std::collections::{BTreeSet, HashMap};

/// The locate statement, `{schema}` substituted; `$1` ids, `$2` their prefix
/// keys. The bench's `b8_locate.sql` is this text (checked by the
/// `postgresql_history_scaling` test).
pub(crate) const LOCATE: &str = include_str!("../../../../sql/postgresql/metadata/locate.sql");

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
    let sql = LOCATE.replace("{schema}", tx.schema);
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
            base: None,
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
            base: None,
        });
    }
    Ok(())
}
