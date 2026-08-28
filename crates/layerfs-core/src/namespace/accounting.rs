fn add_namespace_counters(
    target: &mut NamespaceCounters,
    source: NamespaceCounters,
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
