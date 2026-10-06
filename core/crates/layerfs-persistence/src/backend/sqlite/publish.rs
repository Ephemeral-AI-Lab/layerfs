//! Immutable bodies and bounded multi-row locator statements in one transaction.
use super::transaction::Transaction;
use crate::backend::records::{BackendError, Param};
use layerfs_content::ObjectId;
use layerfs_storage::{location::PackDomain, port::*};
use std::collections::BTreeSet;
pub(crate) fn run(tx: &Transaction<'_>, batch: &Publication) -> Result<Published, BackendError> {
    match tx.layout() {
        crate::SqlitePackLayout::Monolithic => write_packs(tx, &batch.packs)?,
        crate::SqlitePackLayout::GroupRows | crate::SqlitePackLayout::GroupRowsIndexed => {
            super::units_publish::write(tx, &batch.packs)?
        }
    }
    let mut inserted = BTreeSet::new();
    // Both actual limits constrain one statement. SQL size is bounded conservatively.
    let limit = tx
        .input_limit(6, 192, 32)?
        .min(layerfs_storage::policy::BATCH_OBJECT_LIMIT);
    let mut remaining = batch.objects.as_slice();
    while !remaining.is_empty() {
        // Stable powers of two reuse prepared statements without padding rows.
        // Every subpage remains ordered inside the same atomic publication.
        let count = 1usize << remaining.len().min(limit).ilog2();
        let (page, tail) = remaining.split_at(count);
        remaining = tail;
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

/// Borrow complete immutable bodies into bounded INSERT pages in caller order.
fn write_packs(tx: &Transaction<'_>, packs: &[PublishedPack]) -> Result<(), BackendError> {
    if packs.is_empty() {
        return Ok(());
    }
    const PREFIX: &str = "INSERT INTO pack(pack_id,domain,digest,length,body) VALUES ";
    const ROW: &str = "(?,?,?,?,?)";
    let limit = tx
        .input_limit(5, PREFIX.len(), ROW.len() + 1)?
        .min(layerfs_storage::policy::BATCH_OBJECT_LIMIT);
    let mut remaining = packs;
    while !remaining.is_empty() {
        // rusqlite's safe BLOB binding copies into SQLite. Keep the aggregate
        // binding charge within the preceding one-ordinary-pack statement bound.
        // An existing large singleton remains alone, with no companion bodies.
        let mut count = 1;
        let mut bytes = remaining[0].body.len() as u64 + 56;
        while count < remaining.len().min(limit) {
            let next = remaining[count].body.len() as u64 + 56;
            if bytes.saturating_add(next) > layerfs_storage::policy::PACK_LIMIT as u64 + 56 {
                break;
            }
            bytes += next;
            count += 1;
        }
        let (page, tail) = remaining.split_at(count);
        remaining = tail;
        let sql = format!(
            "{}{}",
            PREFIX,
            std::iter::repeat_n(ROW, page.len())
                .collect::<Vec<_>>()
                .join(",")
        );
        let encoded = page
            .iter()
            .map(|p| {
                (
                    p.info.pack_id,
                    if p.info.domain == PackDomain::Metadata {
                        0i64
                    } else {
                        1
                    },
                    p.info.key.as_bytes().as_slice(),
                    p.info.length as i64,
                    p.body.as_slice(),
                )
            })
            .collect::<Vec<_>>();
        let values = encoded
            .iter()
            .flat_map(|r| [&r.0 as &dyn rusqlite::ToSql, &r.1, &r.2, &r.3, &r.4])
            .collect::<Vec<_>>();
        tx.before_pack(page.iter().map(|p| p.body.len()).sum())?;
        tx.borrowed(&sql, &values, bytes)?;
        let mut w = tx.work.borrow_mut();
        w.sealed_inserts += page.len() as u64;
        w.sealed_body_bytes += page.iter().map(|p| p.body.len() as u64).sum::<u64>();
    }
    Ok(())
}
