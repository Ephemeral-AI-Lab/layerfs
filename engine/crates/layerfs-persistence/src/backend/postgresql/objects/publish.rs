//! One publication in two transactions. Pack bodies go first, by `COPY
//! BINARY`, in a write transaction that does not wait for the lane: their ids
//! were reserved, so no other writer can touch those rows. The short lane
//! transaction then adds the locations, inode values, signatures and totals.
//! A refused or failed second step leaves its packs unreferenced; pack ids
//! are never reused and nothing reads a pack except through a location.
use super::super::{metadata::writes, session::transaction::Transaction};
use crate::backend::records::{BackendError, Param};
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

pub(crate) fn rows(tx: &Transaction<'_>, batch: &Publication) -> Result<Published, BackendError> {
    let mut inserted = BTreeSet::new();
    if !batch.objects.is_empty() {
        let ids: Vec<&[u8]> = batch
            .objects
            .iter()
            .map(|o| o.object_id.as_bytes().as_slice())
            .collect();
        let column = |f: fn(&layerfs_storage::location::ObjectLocation) -> i64| {
            batch.objects.iter().map(f).collect::<Vec<_>>()
        };
        let roles = column(|o| i64::from(o.role.code()));
        let lengths = column(|o| o.canonical_length as i64);
        let packs = column(|o| o.pack_id);
        let groups = column(|o| o.group_number as i64);
        let records = column(|o| o.record_number as i64);
        for row in tx.typed(
            "INSERT INTO object_location(object_id,role,canonical_length,pack_id,group_number,record_number) SELECT * FROM unnest($1::bytea[],$2::bigint[],$3::bigint[],$4::bigint[],$5::bigint[],$6::bigint[]) ON CONFLICT(object_id) DO NOTHING RETURNING object_id",
            &[&ids, &roles, &lengths, &packs, &groups, &records],
            batch.objects.len() as u64 * 72,
        )? {
            let id: Vec<u8> = row.try_get(0).map_err(|_| BackendError::Integrity)?;
            inserted.insert(ObjectId::from_bytes(&id).map_err(|_| BackendError::Integrity)?);
        }
    }
    let canonical = batch
        .objects
        .iter()
        .filter(|row| inserted.contains(&row.object_id))
        .map(|row| row.canonical_length as u64)
        .sum::<u64>();
    let metadata = batch
        .packs
        .iter()
        .filter(|pack| pack.info.domain == PackDomain::Metadata)
        .map(|pack| pack.info.length as u64)
        .sum::<u64>();
    let stored = batch
        .packs
        .iter()
        .map(|pack| pack.info.length as u64)
        .sum::<u64>();
    if canonical != 0 || stored != 0 || metadata != 0 {
        tx.query(
            "UPDATE store_totals SET canonical=canonical+$1,stored=stored+$2,metadata=metadata+$3 WHERE id=1",
            vec![
                Param::I64(canonical as i64),
                Param::I64(stored as i64),
                Param::I64(metadata as i64),
            ],
        )?;
    }
    writes::inode_values(tx, &batch.inode_values)?;
    if let Some((first, count)) = batch.release_ordinals {
        tx.query(
            "UPDATE store_policy SET next_ordinal=$1 WHERE id=1 AND next_ordinal=$1::bigint+$2::bigint",
            vec![Param::I64(i64::from(first)), Param::I64(count as i64)],
        )?;
    }
    writes::signatures(tx, &batch.signatures)?;
    Ok(Published {
        lost: batch
            .objects
            .iter()
            .filter(|o| !inserted.contains(&o.object_id))
            .map(|o| o.object_id)
            .collect(),
    })
}
