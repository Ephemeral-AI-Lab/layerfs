//! One publication in one transaction (`Mode::Publish`). Pack bodies go
//! first, by `COPY BINARY`, before the lane row is taken: their ids were
//! reserved, so no other writer can touch those rows, and a slow body upload
//! never holds the lane. The lane row is then locked and one data-modifying
//! statement adds the locations, inode value blocks (#91), signatures and totals; the
//! COMMIT acknowledges packs and rows together. A refused or failed step
//! rolls the whole publication back; pack ids are never reused.
//!
//! A location is inserted when no row of its id exists (#90): the table has
//! no unique key on the id, only a B-tree on its first eight bytes, and
//! uniqueness holds because every location insert runs under the lane row.
//! A signature must name a location of this publication or a stored one.
//!
//! Each pack row carries `directory`, a copy of its body's header and group
//! directory (`body[0..body_offset)`); the schema's `pack_directory_check`
//! refuses a copy that is not that prefix. Each inserted location carries
//! its ancestor list (#758, `object_location.ancestors`): `''` for a FULL
//! record, else one D1 entry per base down to the FULL record, nearest
//! first, each `varint(zigzag(pack - previous pack)) || u8 group ||
//! varint(record) || varint(canonical length - 1)`, the first relative to
//! the row's own pack. The ROWS statement fills them from the catalog's
//! winning rows, which is sound because publications serialise on the lane
//! row and the statement runs under it in one snapshot: a base stored before
//! this publication, or one this batch lost to it, gives `entry(winner) ||
//! winner.ancestors`, found with the same prefix-index probe as the
//! first-wins check; a base this batch inserts gives `entry(its row) || its
//! list`, by recursion over the batch only. The entry being relative to the
//! row's own pack is what lets a base's list be appended verbatim.
use super::super::session::transaction::Transaction;
use crate::backend::records::BackendError;
use layerfs_content::ObjectId;
use layerfs_storage::{location::PackDomain, pack::layout, port::*};
use postgres::types::Type;
use std::collections::BTreeSet;

/// The pack bodies' `COPY`, which the SDK open prepares
/// (`mount::create_texts`).
pub(crate) const COPY: &str =
    "COPY pack (pack_id,domain,digest,length,body,directory) FROM STDIN (FORMAT binary)";

pub(crate) fn packs(tx: &Transaction<'_>, packs: &[PublishedPack]) -> Result<(), BackendError> {
    let directories = packs.iter().map(directory).collect::<Result<Vec<_>, _>>()?;
    tx.copy_in(
        COPY,
        &[
            Type::INT8,
            Type::INT8,
            Type::BYTEA,
            Type::INT8,
            Type::BYTEA,
            Type::BYTEA,
        ],
        |writer| {
            for (pack, directory) in packs.iter().zip(&directories) {
                let info = pack.info();
                let domain = i64::from(info.domain == PackDomain::Payload);
                writer.write(&[
                    &info.pack_id,
                    &domain,
                    &info.key.as_bytes().as_slice(),
                    &(info.length as i64),
                    &pack.body().as_slice(),
                    directory,
                ])?;
            }
            Ok(())
        },
    )?;
    Ok(())
}

/// A pack's directory copy: its header and group directory,
/// `body[0..body_offset)`, as the reader's group plan takes it.
fn directory(pack: &PublishedPack) -> Result<&[u8], BackendError> {
    let body = pack.body();
    let header = layout::parse_header(body).map_err(|_| BackendError::Integrity)?;
    body.get(..header.body_offset)
        .ok_or(BackendError::Integrity)
}

/// The whole publication: bodies, then the lane row, then the rows.
pub(crate) fn run(tx: &Transaction<'_>, batch: &Publication) -> Result<Published, BackendError> {
    if !batch.packs.is_empty() {
        packs(tx, &batch.packs)?;
    }
    tx.lane()?;
    rows(tx, batch)
}

/// Every row of one publication in one statement. Its sub-statements see
/// one snapshot, taken under the lane, and touch distinct rows; the totals
/// count only locations this statement inserted, and a signature may name
/// one of them. A NULL id row refuses the publication: a block's ordinals
/// were already stored, or a signature names no location. `OFFSET 0` keeps
/// each location probe a correlated index lookup: without it the planner
/// may hash the whole table, computing the prefix of every row, under the
/// lane.
///
/// `src` probes each batch id once (`lost`: a row of that id is stored).
/// `lists` derives the ancestor lists of the rows to insert (module doc):
/// its first branch takes FULL rows and rows whose base is not inserted by
/// this batch, the winner found by one more prefix probe; its recursive
/// branch walks the batch only, children after their in-batch base, at most
/// 64 levels deep (`l.d<64`: deeper than any readable chain, whose policy
/// ceiling is 50, so it only stops a malformed batch). Each entry's operands are columns of an `OFFSET 0`
/// subquery, so `ancestor_entry` stays inlined. A list over the column's 850
/// bytes (more entries than any readable chain) is left NULL, unrecorded,
/// as is one whose base is stored nowhere.
pub(crate) const ROWS: &str = include_str!("../../../../sql/postgresql/publish_rows.sql");

/// The index key of an object id: its first eight bytes as a signed
/// big-endian `bigint`, as the schema's `object_location_prefix` computes it.
pub(crate) fn prefix(id: &ObjectId) -> i64 {
    let mut head = [0_u8; 8];
    head.copy_from_slice(&id.as_bytes()[..8]);
    i64::from_be_bytes(head)
}

pub(crate) fn rows(tx: &Transaction<'_>, batch: &Publication) -> Result<Published, BackendError> {
    // Ids are distinct: `storage::publication` refuses a repeated one.
    let objects = &batch.objects;
    let keys: Vec<i64> = objects.iter().map(|o| prefix(&o.object_id)).collect();
    let ids: Vec<&[u8]> = objects
        .iter()
        .map(|o| o.object_id.as_bytes().as_slice())
        .collect();
    let column = |f: fn(&layerfs_storage::location::ObjectLocation) -> i64| {
        objects.iter().map(f).collect::<Vec<_>>()
    };
    let roles = column(|o| i64::from(o.role.code()));
    let lengths = column(|o| o.canonical_length as i64);
    let packs = column(|o| o.pack_id);
    let groups = column(|o| o.group_number as i64);
    let records = column(|o| o.record_number as i64);
    let metadata = batch
        .packs
        .iter()
        .filter(|pack| pack.info().domain == PackDomain::Metadata)
        .map(|pack| pack.info().length as i64)
        .sum::<i64>();
    let stored = batch
        .packs
        .iter()
        .map(|pack| pack.info().length as i64)
        .sum::<i64>();
    // Blocks are encoded here, before the statement that holds the lane.
    let values = crate::blocks::values::encode(&batch.inode_values)?;
    let ordinals: Vec<i64> = values.iter().map(|b| i64::from(b.first)).collect();
    let lasts: Vec<i64> = values.iter().map(|b| i64::from(b.last)).collect();
    let bodies: Vec<&[u8]> = values.iter().map(|b| b.body.as_slice()).collect();
    let (release, first, count) = match batch.release_ordinals {
        Some((first, count)) => (true, i64::from(first), count as i64),
        None => (false, 0, 0),
    };
    let signatures = &batch.signatures;
    let slots: Vec<i64> = signatures.iter().map(|r| r.slot as i64).collect();
    let stamps: Vec<i64> = signatures.iter().map(|r| r.stamp as i64).collect();
    let signed: Vec<&[u8]> = signatures
        .iter()
        .map(|r| r.object_id.as_bytes().as_slice())
        .collect();
    let digests: Vec<&[u8]> = signatures.iter().map(|r| r.signature.as_slice()).collect();
    let signed_keys: Vec<i64> = signatures.iter().map(|r| prefix(&r.object_id)).collect();
    // The logical base of each row, NULL for a FULL record (#758).
    let bases: Vec<Option<&[u8]>> = objects
        .iter()
        .map(|o| o.base.as_ref().map(|b| b.as_bytes().as_slice()))
        .collect();
    let base_keys: Vec<Option<i64>> = objects
        .iter()
        .map(|o| o.base.as_ref().map(prefix))
        .collect();
    let bytes = objects.len() as u64 * 112
        + values.iter().map(|b| b.body.len() as u64 + 16).sum::<u64>()
        + signatures.len() as u64 * 80
        + 48;
    let mut inserted = BTreeSet::new();
    for row in tx.typed(
        ROWS,
        &[
            &ids,
            &roles,
            &lengths,
            &packs,
            &groups,
            &records,
            &stored,
            &metadata,
            &ordinals,
            &bodies,
            &first,
            &count,
            &release,
            &slots,
            &stamps,
            &signed,
            &digests,
            &lasts,
            &keys,
            &signed_keys,
            &bases,
            &base_keys,
        ],
        bytes,
    )? {
        // A NULL id: a block's ordinals were already stored, or a signature
        // names no location; nothing commits.
        let id: Option<Vec<u8>> = row.try_get(0).map_err(|_| BackendError::Integrity)?;
        let id = id.ok_or(BackendError::Integrity)?;
        inserted.insert(ObjectId::from_bytes(&id).map_err(|_| BackendError::Integrity)?);
    }
    Ok(Published {
        lost: objects
            .iter()
            .filter(|o| !inserted.contains(&o.object_id))
            .map(|o| o.object_id)
            .collect(),
    })
}
