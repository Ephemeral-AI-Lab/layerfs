//! Immutable bodies and bounded multi-row locator statements in one transaction.
use super::transaction::Transaction;
use crate::backend::records::{BackendError, Param};
use layerfs_content::ObjectId;
use layerfs_storage::{location::PackDomain, port::*};
use std::collections::BTreeSet;
pub(crate) fn run(tx: &Transaction<'_>, batch: &Publication) -> Result<Published, BackendError> {
    for p in &batch.packs {
        tx.borrowed(
            "INSERT INTO pack(pack_id,domain,digest,length,body) VALUES(?1,?2,?3,?4,?5)",
            &[
                &p.info.pack_id,
                &(if p.info.domain == PackDomain::Metadata {
                    0i64
                } else {
                    1
                }),
                &p.info.key.as_bytes().as_slice(),
                &(p.info.length as i64),
                &p.body.as_slice(),
            ],
            p.body.len() as u64 + 56,
        )?;
        let mut w = tx.work.borrow_mut();
        w.sealed_inserts += 1;
        w.sealed_body_bytes += p.body.len() as u64;
    }
    let mut inserted = BTreeSet::new();
    // Both actual limits constrain one statement. SQL size is bounded conservatively.
    let limit = tx.input_limit(6, 192, 32)?;
    for page in batch.objects.chunks(limit) {
        let sql=format!("INSERT INTO object_location(object_id,role,canonical_length,pack_id,group_number,record_number) VALUES {} ON CONFLICT(object_id) DO NOTHING RETURNING object_id",std::iter::repeat_n("(?,?,?,?,?,?)",page.len()).collect::<Vec<_>>().join(","));
        let params = page
            .iter()
            .flat_map(|o| {
                [
                    Param::Bytes(o.object_id.as_bytes().to_vec()),
                    Param::I64(i64::from(o.role.code())),
                    Param::I64(o.canonical_length as i64),
                    Param::I64(o.pack_id),
                    Param::I64(o.group_number as i64),
                    Param::I64(o.record_number as i64),
                ]
            })
            .collect();
        for r in tx.query(&sql, params)? {
            inserted.insert(
                ObjectId::from_bytes(&r.get::<Vec<u8>>(0)?).map_err(|_| BackendError::Integrity)?,
            );
        }
    }
    for g in &batch.value_groups {
        tx.query("INSERT INTO metadata_value_group(first_ordinal,count,pack_id,group_number,digest) VALUES(?1,?2,?3,?4,?5)",vec![Param::I64(i64::from(g.first_ordinal)),Param::I64(g.count as i64),Param::I64(g.pack_id),Param::I64(g.group_number as i64),Param::Bytes(g.digest.as_bytes().to_vec())])?;
        tx.query("UPDATE store_policy SET metadata_window_start=CASE WHEN metadata_window_values+?1>131072 THEN ?2 ELSE metadata_window_start END,metadata_window_values=CASE WHEN metadata_window_values+?1>131072 THEN ?1 ELSE metadata_window_values+?1 END WHERE id=1",vec![Param::I64(g.count as i64),Param::I64(i64::from(g.first_ordinal))])?;
    }
    if let Some(window) = batch.window_start {
        tx.query("UPDATE store_policy SET metadata_window_start=max(metadata_window_start,?1) WHERE id=1",vec![Param::I64(i64::from(window))])?;
    }
    if let Some((first, count)) = batch.release_ordinals {
        tx.query(
            "UPDATE store_policy SET next_ordinal=?1 WHERE id=1 AND next_ordinal=?1+?2",
            vec![Param::I64(i64::from(first)), Param::I64(count as i64)],
        )?;
    }
    super::metadata_signatures::write(tx, &batch.signatures)?;
    Ok(Published {
        lost: batch
            .objects
            .iter()
            .filter(|o| !inserted.contains(&o.object_id))
            .map(|o| o.object_id)
            .collect(),
    })
}
