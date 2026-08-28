fn expand_span<S: ObjectRead>(
    store: &S,
    span: Span,
    root: bool,
    counters: &mut RopeCounters,
    cache: &mut Option<(ObjectId, ExtentNodeV3)>,
) -> CoreResult<Vec<Span>> {
    let SpanKind::Node { summary, .. } = span.kind else {
        return Ok(vec![span]);
    };
    let node = load_node_cached(store, summary, root, counters, cache)?;
    let mut output = Vec::with_capacity(node.entry_count());
    let wanted_end = span
        .offset
        .checked_add(span.len)
        .ok_or(CoreError::LengthOverflow)?;
    match node {
        ExtentNodeV3::Leaf { extents, .. } => {
            let mut start = 0_u64;
            for extent in extents {
                let end = start
                    .checked_add(u64::from(extent.logical_length))
                    .ok_or(CoreError::LengthOverflow)?;
                push_overlap(
                    &mut output,
                    SpanKind::Extent(extent),
                    start,
                    end,
                    span.offset,
                    wanted_end,
                )?;
                start = end;
            }
        }
        ExtentNodeV3::Branch {
            level, children, ..
        } => {
            let mut start = 0_u64;
            for summary in child_summaries(&children, level - 1) {
                let end = start
                    .checked_add(summary.bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                push_overlap(
                    &mut output,
                    SpanKind::Node {
                        summary,
                        root: false,
                    },
                    start,
                    end,
                    span.offset,
                    wanted_end,
                )?;
                start = end;
            }
        }
    }
    if output.iter().try_fold(0_u64, |sum, item| {
        sum.checked_add(item.len).ok_or(CoreError::LengthOverflow)
    })? != span.len
    {
        return Err(CoreError::InvalidRecord("extent span"));
    }
    Ok(output)
}

fn push_overlap(
    output: &mut Vec<Span>,
    kind: SpanKind,
    start: u64,
    end: u64,
    wanted_start: u64,
    wanted_end: u64,
) -> CoreResult<()> {
    let overlap_start = start.max(wanted_start);
    let overlap_end = end.min(wanted_end);
    if overlap_start < overlap_end {
        output.push(Span {
            kind,
            offset: overlap_start - start,
            len: overlap_end - overlap_start,
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn merge_spans<S: ObjectRead, F: FnMut(Range<u64>) -> CoreResult<()>>(
    store: &S,
    old: &[Span],
    new: &[Span],
    mut logical: u64,
    counters: &mut RopeCounters,
    old_ancestors: &mut Vec<ObjectId>,
    new_ancestors: &mut Vec<ObjectId>,
    old_cache: &mut Option<(ObjectId, ExtentNodeV3)>,
    new_cache: &mut Option<(ObjectId, ExtentNodeV3)>,
    emitter: &mut ChangedRanges<'_, F>,
) -> CoreResult<()> {
    let (mut old_index, mut new_index) = (0_usize, 0_usize);
    let (mut old_used, mut new_used) = (0_u64, 0_u64);
    while old_index < old.len() && new_index < new.len() {
        let count = (old[old_index].len - old_used).min(new[new_index].len - new_used);
        diff_span(
            store,
            old[old_index].slice(old_used, count)?,
            new[new_index].slice(new_used, count)?,
            logical,
            counters,
            old_ancestors,
            new_ancestors,
            old_cache,
            new_cache,
            emitter,
        )?;
        logical = logical
            .checked_add(count)
            .ok_or(CoreError::LengthOverflow)?;
        old_used += count;
        new_used += count;
        if old_used == old[old_index].len {
            old_index += 1;
            old_used = 0;
        }
        if new_used == new[new_index].len {
            new_index += 1;
            new_used = 0;
        }
    }
    if old_index != old.len() || new_index != new.len() {
        return Err(CoreError::InvalidRecord("extent span"));
    }
    Ok(())
}
