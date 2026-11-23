//! `locate_chains` on PostgreSQL (#758, gate amendment design B): one chain
//! locate in at most two statements.
//!
//! 1. [`ROOTS`] probes every root through the prefix index, as `locate` does,
//!    and returns each located row with its raw ancestor list, then one row
//!    per distinct pack those rows name, with its directory copy.
//! 2. The lists are decoded here (D1 chained zigzag varints; decoding them in
//!    SQL costs about 2-3 us an entry, an order of magnitude over the probes).
//!    [`PACKS`] then reads, by `pack_id` primary key, the rows and directories
//!    of the member packs statement 1 did not return. It is skipped when there
//!    are none, so a wave without chains costs one statement.
//!
//! Nothing is kept between calls: a cold read pays for every pack row it is
//! handed. A chain is left out (the reader then locates its bases level by
//! level, counted as an unhinted root) when its list is NULL (unrecorded),
//! malformed, longer than the root role's depth bound, or names a pack that
//! is not stored; it is never repaired or guessed. A FULL root (`''`) has no
//! chain.
use super::super::session::transaction::Transaction;
use crate::backend::records::{pack_info, BackendError};
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    location::{
        ChainMember, LocatedChain, LocatedChains, LocatedObject, ObjectLocation, PackDirectory,
        PackInfo,
    },
    policy::{CANONICAL_LIMIT, GROUP_COUNT_LIMIT, RECORD_COUNT_LIMIT},
    port::PersistenceError,
    StoragePolicy,
};
use postgres::Row;
use std::collections::{BTreeMap, BTreeSet};

/// Statement 1, `{schema}` substituted; `$1` root ids, `$2` their prefix
/// keys. Rows of kind 0 are located rows (`object_id, role,
/// canonical_length, group_number, record_number, pack_id, ancestors`),
/// rows of kind 1 their packs (`pack_id, directory, domain, digest,
/// length` from column 6). The bench's B5/B7/B8 rows take the same file.
pub(crate) const ROOTS: &str = include_str!("../../../../sql/postgresql/chain_roots.sql");
/// Statement 2, `{schema}` substituted; `$1` member pack ids.
pub(crate) const PACKS: &str = include_str!("../../../../sql/postgresql/chain_packs.sql");

/// One located row's role and raw list, kept until the packs are known.
struct Located {
    object: LocatedObject,
    list: Option<Vec<u8>>,
}

/// One decoded list entry: pack id, group, record and canonical length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Entry {
    pub(crate) pack_id: i64,
    pub(crate) group_number: usize,
    pub(crate) record_number: usize,
    pub(crate) canonical_length: usize,
}

/// One LEB128 varint at `*at`; `None` when the list ends inside it or it does
/// not fit 64 bits.
fn varint(list: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *list.get(*at)?;
        *at += 1;
        let bits = u64::from(byte & 0x7f);
        if shift == 63 && bits > 1 {
            return None;
        }
        value |= bits << shift;
        if byte < 0x80 {
            return Some(value);
        }
    }
    None
}

/// Decodes one D1 list of a row stored in pack `own`: entries nearest base
/// first. `None` for a malformed list or an entry outside the port's bounds.
pub(crate) fn decode(own: i64, list: &[u8]) -> Option<Vec<Entry>> {
    let (mut at, mut previous, mut out) = (0usize, own, Vec::new());
    while at < list.len() {
        let zigzag = varint(list, &mut at)?;
        let delta = ((zigzag >> 1) as i64) ^ -((zigzag & 1) as i64);
        let pack_id = previous.checked_add(delta).filter(|id| *id > 0)?;
        let group_number = usize::from(*list.get(at)?);
        at += 1;
        let record_number = usize::try_from(varint(list, &mut at)?).ok()?;
        let canonical_length = usize::try_from(varint(list, &mut at)?.checked_add(1)?).ok()?;
        if group_number >= GROUP_COUNT_LIMIT
            || record_number >= RECORD_COUNT_LIMIT
            || canonical_length > CANONICAL_LIMIT
        {
            return None;
        }
        out.push(Entry {
            pack_id,
            group_number,
            record_number,
            canonical_length,
        });
        previous = pack_id;
    }
    Some(out)
}

/// The members a chain of `role` may carry under `policy`, as the reader
/// bounds them.
fn depth(policy: &StoragePolicy, role: ObjectRole) -> usize {
    usize::from(match role {
        ObjectRole::Chunk => policy.chunk_delta_max_depth(),
        ObjectRole::InodeLeaf => policy.metadata_delta_max_depth(),
        _ => policy.whole_file_delta_max_depth(),
    })
}

fn number(row: &Row, index: usize) -> Result<usize, BackendError> {
    let value: i64 = row.try_get(index).map_err(|_| BackendError::Integrity)?;
    usize::try_from(value).map_err(|_| BackendError::Integrity)
}

/// One pack row (`pack_id, directory, domain, digest, length` from column
/// `at`) as its descriptor and directory copy.
fn pack_row(row: &Row, at: usize) -> Result<(PackInfo, Vec<u8>), BackendError> {
    let get = |index| -> Result<i64, BackendError> {
        row.try_get(at + index).map_err(|_| BackendError::Integrity)
    };
    let directory: Vec<u8> = row.try_get(at + 1).map_err(|_| BackendError::Integrity)?;
    let digest: Vec<u8> = row.try_get(at + 3).map_err(|_| BackendError::Integrity)?;
    Ok((pack_info(get(0)?, get(2)?, &digest, get(4)?)?, directory))
}

/// Locates `roots` like `locate` into `rows` and returns the chains of the
/// located delta roots with the directories of every pack named.
pub(crate) fn locate_chains(
    tx: &Transaction<'_>,
    policy: &StoragePolicy,
    roots: &[ObjectId],
    rows: &mut Vec<LocatedObject>,
    chains: &mut LocatedChains,
) -> Result<(), PersistenceError> {
    let ids: Vec<Vec<u8>> = roots
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
    let sql = ROOTS.replace("{schema}", tx.schema);
    let mut located = Vec::new();
    let mut packs: BTreeMap<i64, (PackInfo, Vec<u8>)> = BTreeMap::new();
    for row in tx.typed(&sql, &[&ids, &keys], ids.len() as u64 * 40)? {
        let kind: i32 = row.try_get(0).map_err(|_| BackendError::Integrity)?;
        if kind == 1 {
            let (info, directory) = pack_row(&row, 6)?;
            packs.insert(info.pack_id, (info, directory));
            continue;
        }
        let id: Vec<u8> = row.try_get(1).map_err(|_| BackendError::Integrity)?;
        let object_id = ObjectId::from_bytes(&id).map_err(|_| BackendError::Integrity)?;
        let code = u8::try_from(number(&row, 2)?).map_err(|_| BackendError::Integrity)?;
        let role = ObjectRole::from_code(code).map_err(|_| BackendError::Integrity)?;
        let pack_id: i64 = row.try_get(6).map_err(|_| BackendError::Integrity)?;
        let list: Option<Vec<u8>> = row.try_get(7).map_err(|_| BackendError::Integrity)?;
        located.push((
            ObjectLocation {
                object_id,
                role,
                canonical_length: number(&row, 3)?,
                pack_id,
                group_number: number(&row, 4)?,
                record_number: number(&row, 5)?,
                base: None,
            },
            list,
        ));
    }
    // Every located row's pack came back with it, as `locate` joins it.
    let mut kept = Vec::with_capacity(located.len());
    for (location, list) in located {
        let pack = packs
            .get(&location.pack_id)
            .ok_or(BackendError::Integrity)?
            .0;
        kept.push(Located {
            object: LocatedObject { location, pack },
            list,
        });
    }
    // Decode every recorded list; a chain that cannot be served is dropped.
    let mut decoded: Vec<Option<Vec<Entry>>> = Vec::with_capacity(kept.len());
    let mut missing = BTreeSet::new();
    for row in &kept {
        let entries = row
            .list
            .as_deref()
            .filter(|list| !list.is_empty())
            .and_then(|list| decode(row.object.location.pack_id, list))
            .filter(|entries| entries.len() <= depth(policy, row.object.location.role));
        if let Some(entries) = &entries {
            missing.extend(
                entries
                    .iter()
                    .map(|entry| entry.pack_id)
                    .filter(|id| !packs.contains_key(id)),
            );
        }
        decoded.push(entries);
    }
    if !missing.is_empty() {
        let ids: Vec<i64> = missing.into_iter().collect();
        let sql = PACKS.replace("{schema}", tx.schema);
        for row in tx.typed(&sql, &[&ids], ids.len() as u64 * 8)? {
            let (info, directory) = pack_row(&row, 0)?;
            packs.insert(info.pack_id, (info, directory));
        }
    }
    let (mut named, mut chained) = (BTreeSet::new(), BTreeSet::new());
    for (row, entries) in kept.iter().zip(decoded) {
        named.insert(row.object.location.pack_id);
        let Some(entries) = entries else { continue };
        // One chain per root, as the port allows; a second row of an id
        // (refused by the doctor) adds none.
        if !chained.insert(row.object.location.object_id) {
            continue;
        }
        let members: Option<Vec<ChainMember>> = entries
            .iter()
            .map(|entry| {
                packs.get(&entry.pack_id).map(|(pack, _)| ChainMember {
                    object_id: None,
                    pack: *pack,
                    group_number: entry.group_number,
                    record_number: entry.record_number,
                    canonical_length: entry.canonical_length,
                })
            })
            .collect();
        // A pack no longer stored leaves its chain out.
        let Some(members) = members else { continue };
        named.extend(members.iter().map(|member| member.pack.pack_id));
        chains.chains.push(LocatedChain {
            root: row.object.location.object_id,
            members,
        });
    }
    chains
        .directories
        .extend(named.into_iter().filter_map(|id| {
            packs
                .remove(&id)
                .map(|(pack, prefix)| PackDirectory { pack, prefix })
        }));
    rows.extend(kept.into_iter().map(|row| row.object));
    Ok(())
}
