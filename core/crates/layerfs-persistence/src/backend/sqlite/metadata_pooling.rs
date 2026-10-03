//! Bounded chronological pooled catalogue pages and covering sets.
use crate::backend::{
    records::{BackendError, Param, Record},
    Transaction,
};
use layerfs_content::ObjectId;
use layerfs_storage::{location::ValueGroupRow, port::*};
fn group(r: &Record) -> Result<ValueGroupRow, BackendError> {
    Ok(ValueGroupRow {
        first_ordinal: u32::try_from(r.get::<i64>(0)?).map_err(|_| BackendError::Integrity)?,
        count: usize::try_from(r.get::<i64>(1)?).map_err(|_| BackendError::Integrity)?,
        pack_id: r.get(2)?,
        group_number: usize::try_from(r.get::<i64>(3)?).map_err(|_| BackendError::Integrity)?,
        digest: ObjectId::from_bytes(&r.get::<Vec<u8>>(4)?).map_err(|_| BackendError::Integrity)?,
    })
}
pub(crate) fn window(tx: &Transaction<'_>) -> Result<u32, PersistenceError> {
    let rows = tx.query(
        "SELECT metadata_window_start FROM store_policy WHERE id=1",
        vec![],
    )?;
    u32::try_from(
        rows.first()
            .ok_or(PersistenceError::Missing)?
            .get::<i64>(0)?,
    )
    .map_err(|_| PersistenceError::Malformed)
}
pub(crate) fn page(
    tx: &Transaction<'_>,
    from: u32,
    limit: usize,
) -> Result<Vec<ValueGroupRow>, PersistenceError> {
    tx.query("SELECT first_ordinal,count,pack_id,group_number,digest FROM metadata_value_group WHERE first_ordinal>=?1 ORDER BY first_ordinal LIMIT ?2",vec![Param::I64(i64::from(from)),Param::I64(limit as i64)])?.iter().map(group).collect::<Result<Vec<_>,_>>().map_err(Into::into)
}
pub(crate) fn covering(
    tx: &Transaction<'_>,
    ids: &[u32],
) -> Result<Vec<ValueGroupRow>, PersistenceError> {
    let mut found = Vec::new();
    for page in ids.chunks(tx.input_limit(1, 512, 4)?) {
        let sql=format!("WITH wanted(ordinal) AS(VALUES {}),covering AS(SELECT ordinal,(SELECT first_ordinal FROM metadata_value_group WHERE first_ordinal<=w.ordinal ORDER BY first_ordinal DESC LIMIT 1) first FROM wanted w) SELECT DISTINCT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest FROM covering c JOIN metadata_value_group g ON g.first_ordinal=c.first WHERE c.ordinal<g.first_ordinal+g.count ORDER BY g.first_ordinal",std::iter::repeat_n("(?)",page.len()).collect::<Vec<_>>().join(","));
        for r in tx.query(
            &sql,
            page.iter().map(|n| Param::I64(i64::from(*n))).collect(),
        )? {
            found.push(group(&r)?);
        }
    }
    Ok(found)
}
