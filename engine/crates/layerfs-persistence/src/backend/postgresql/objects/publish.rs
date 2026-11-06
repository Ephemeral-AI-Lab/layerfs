//! One publication in one transaction (`Mode::Publish`). Pack bodies go
//! first, by `COPY BINARY`, before the lane row is taken: their ids were
//! reserved, so no other writer can touch those rows, and a slow body upload
//! never holds the lane. The lane row is then locked and one data-modifying
//! statement adds the locations, inode values, signatures and totals; the
//! COMMIT acknowledges packs and rows together. A refused or failed step
//! rolls the whole publication back; pack ids are never reused.
use super::super::session::transaction::Transaction;
use crate::backend::records::BackendError;
use layerfs_content::ObjectId;
use layerfs_storage::{location::PackDomain, port::*};
use postgres::types::Type;
use std::collections::BTreeSet;

pub(crate) fn packs(tx: &Transaction<'_>, packs: &[PublishedPack]) -> Result<(), BackendError> {
    tx.copy_in(
        "COPY pack (pack_id,domain,digest,length,body) FROM STDIN (FORMAT binary)",
        &[Type::INT8, Type::INT8, Type::BYTEA, Type::INT8, Type::BYTEA],
        |writer| {
            for pack in packs {
                let domain = i64::from(pack.info.domain == PackDomain::Payload);
                writer.write(&[
                    &pack.info.pack_id,
                    &domain,
                    &pack.info.key.as_bytes().as_slice(),
                    &(pack.info.length as i64),
                    &pack.body.as_slice(),
                ])?;
            }
            Ok(())
        },
    )?;
    Ok(())
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
/// one snapshot and touch distinct rows; the totals count only locations
/// this statement inserted, and a signature may name one of them (foreign
/// keys are checked when the statement ends).
const ROWS: &str = "WITH ins AS (\
INSERT INTO object_location(object_id,role,canonical_length,pack_id,group_number,record_number) \
SELECT * FROM unnest($1::bytea[],$2::bigint[],$3::bigint[],$4::bigint[],$5::bigint[],$6::bigint[]) \
ON CONFLICT(object_id) DO NOTHING RETURNING object_id,canonical_length), \
added AS (SELECT coalesce(sum(canonical_length),0)::bigint AS n FROM ins), \
totals AS (UPDATE store_totals SET canonical=store_totals.canonical+added.n,stored=store_totals.stored+$7::bigint,metadata=store_totals.metadata+$8::bigint \
FROM added WHERE store_totals.id=1 AND (added.n<>0 OR $7::bigint<>0 OR $8::bigint<>0)), \
values_ AS (INSERT INTO inode_value(ordinal,value) SELECT * FROM unnest($9::bigint[],$10::bytea[])), \
released AS (UPDATE store_policy SET next_ordinal=$11::bigint WHERE $13::boolean AND id=1 AND next_ordinal=$11::bigint+$12::bigint), \
signed AS (INSERT INTO content_signature(slot,stamp,object_id,signature) \
SELECT * FROM unnest($14::bigint[],$15::bigint[],$16::bytea[],$17::bytea[]) \
ON CONFLICT(slot) DO UPDATE SET stamp=excluded.stamp,object_id=excluded.object_id,signature=excluded.signature \
WHERE excluded.stamp>=content_signature.stamp) \
SELECT object_id FROM ins";

pub(crate) fn rows(tx: &Transaction<'_>, batch: &Publication) -> Result<Published, BackendError> {
    let objects = &batch.objects;
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
        .filter(|pack| pack.info.domain == PackDomain::Metadata)
        .map(|pack| pack.info.length as i64)
        .sum::<i64>();
    let stored = batch
        .packs
        .iter()
        .map(|pack| pack.info.length as i64)
        .sum::<i64>();
    let values = &batch.inode_values;
    let ordinals: Vec<i64> = values.iter().map(|r| i64::from(r.ordinal)).collect();
    let bodies: Vec<&[u8]> = values.iter().map(|r| r.value.as_slice()).collect();
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
    let bytes =
        objects.len() as u64 * 72 + values.len() as u64 * 81 + signatures.len() as u64 * 80 + 48;
    let mut inserted = BTreeSet::new();
    for row in tx.typed(
        ROWS,
        &[
            &ids, &roles, &lengths, &packs, &groups, &records, &stored, &metadata, &ordinals,
            &bodies, &first, &count, &release, &slots, &stamps, &signed, &digests,
        ],
        bytes,
    )? {
        let id: Vec<u8> = row.try_get(0).map_err(|_| BackendError::Integrity)?;
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
