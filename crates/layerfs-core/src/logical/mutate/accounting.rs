fn merge_namespace(
    counters: &mut LogicalCounters,
    source: crate::namespace::NamespaceCounters,
) -> CoreResult<()> {
    counters.namespace.nodes_read = counters
        .namespace
        .nodes_read
        .checked_add(source.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    counters.namespace.nodes_created = counters
        .namespace
        .nodes_created
        .checked_add(source.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn merge_namespace_counters(
    target: &mut crate::namespace::NamespaceCounters,
    source: crate::namespace::NamespaceCounters,
) -> CoreResult<()> {
    target.nodes_read = target
        .nodes_read
        .checked_add(source.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    target.nodes_created = target
        .nodes_created
        .checked_add(source.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn merge_inode(
    counters: &mut LogicalCounters,
    source: crate::inode::InodeTableCounters,
) -> CoreResult<()> {
    counters.inode_table.nodes_read = counters
        .inode_table
        .nodes_read
        .checked_add(source.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    counters.inode_table.nodes_created = counters
        .inode_table
        .nodes_created
        .checked_add(source.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}
