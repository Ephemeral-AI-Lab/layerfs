pub fn diff_ranges<S: ObjectRead>(
    store: &S,
    old: FileStateRoot,
    new: FileStateRoot,
    mut visitor: impl FnMut(Range<u64>) -> CoreResult<()>,
) -> CoreResult<(bool, RopeCounters)> {
    if old == new {
        return Ok((true, RopeCounters::default()));
    }
    let mut counters = RopeCounters::default();
    let old_state = state(store, old, &mut counters)?;
    let new_state = state(store, new, &mut counters)?;
    if old_state.logical_len != new_state.logical_len {
        return Ok((false, counters));
    }
    let old_summary = Summary {
        id: old_state.mapping_root,
        bytes: old_state.logical_len,
        extents: old_state.extent_count,
        level: old_state.tree_level,
    };
    let new_summary = Summary {
        id: new_state.mapping_root,
        bytes: new_state.logical_len,
        extents: new_state.extent_count,
        level: new_state.tree_level,
    };
    if old_summary.id == new_summary.id {
        if !same_summary(old_summary, new_summary) {
            return Err(CoreError::InvalidRecord("extent summary"));
        }
        return Ok((true, counters));
    }
    let mut emitter = ChangedRanges {
        start: None,
        visitor: &mut visitor,
    };
    let mut old_cache = None;
    let mut new_cache = None;
    diff_span(
        store,
        Span::node(old_summary),
        Span::node(new_summary),
        0,
        &mut counters,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut old_cache,
        &mut new_cache,
        &mut emitter,
    )?;
    emitter.finish(old_state.logical_len)?;
    Ok((true, counters))
}
