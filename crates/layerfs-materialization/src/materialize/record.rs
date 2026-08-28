use super::*;

pub(super) fn record(
    engine: &impl ObjectRead,
    table: InodeTableRoot,
    inode: InodeId,
    counters: &mut OperationCounters,
) -> VfsResult<layerfs_core::inode::InodeRecordV1> {
    let mut inode_counters = InodeTableCounters::default();
    let id = inode_table_lookup(engine, table, inode, &mut inode_counters)?
        .ok_or(VfsError::InvalidState)?;
    counters.add_inode_table(inode_counters)?;
    Ok(engine.with_authenticated_canonical(id, decode_inode_record)?)
}
