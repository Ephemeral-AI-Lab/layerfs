//! Exact immutable group extents and prospective physical publication costs.
use layerfs_storage::{location::PackDomain, pack::layout, policy, port::*};
pub(crate) fn views(
    pack: &PublishedPack,
) -> Result<(layout::PackHeader, Vec<layout::GroupView>), PersistenceError> {
    let header = layout::parse_header(&pack.body).map_err(|_| PersistenceError::Malformed)?;
    let views = layout::directory_group_views(&pack.body, header)
        .map_err(|_| PersistenceError::Malformed)?;
    Ok((header, views))
}
pub(crate) fn cost(pack: &PublishedPack) -> Result<(usize, u64), PersistenceError> {
    let (_, views) = views(pack)?;
    Ok((
        1 + views.len(),
        pack.body.len() as u64 + 56 + 32 * views.len() as u64,
    ))
}
pub(crate) fn validate_batch(batch: &Publication) -> Result<(), PersistenceError> {
    let mut rows = batch.objects.len()
        + batch.value_groups.len()
        + batch.signatures.len()
        + usize::from(batch.window_start.is_some())
        + usize::from(batch.release_ordinals.is_some());
    let mut bytes = 0u64;
    for pack in &batch.packs {
        let (r, b) = cost(pack)?;
        rows = rows.checked_add(r).ok_or(PersistenceError::Malformed)?;
        bytes = bytes.checked_add(b).ok_or(PersistenceError::Malformed)?;
    }
    let singleton = batch.objects.len() == 1
        && batch.packs.len() == 1
        && batch.packs[0].info.domain == PackDomain::Payload;
    if rows > policy::TRANSACTION_ROW_LIMIT as usize
        || bytes > policy::TRANSACTION_PHYSICAL_BYTES_LIMIT && !singleton
    {
        return Err(PersistenceError::Malformed);
    }
    Ok(())
}
