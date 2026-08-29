use super::build::add;
use super::state::{FileStateRoot, ObjectRead, ReadPlan, RopeCounters, Summary};
use super::validate::{child_summaries, validate_node, visit_extent_node};
use crate::error::{CoreError, CoreResult};
use crate::file::extent::{ExtentNodeV3, ExtentSliceV3, FileStateV3};
use crate::file::extent_codec::{decode_file_state, decode_node_with_context};
use crate::object::ObjectId;
use std::io::Write;
use std::ops::Range;

pub fn state<S: ObjectRead>(
    store: &S,
    root: FileStateRoot,
    counters: &mut RopeCounters,
) -> CoreResult<FileStateV3> {
    counters.nodes_read = add(counters.nodes_read, 1)?;
    store.with_authenticated_canonical(root.0, decode_file_state)
}

pub fn validate_file<S: ObjectRead>(store: &S, root: FileStateRoot) -> CoreResult<()> {
    let mut counters = RopeCounters::default();
    let state = state(store, root, &mut counters)?;
    validate_node(
        store,
        Summary {
            id: state.mapping_root,
            bytes: state.logical_len,
            extents: state.extent_count,
            level: state.tree_level,
        },
        true,
        &mut counters,
        &mut Vec::new(),
    )
}

pub fn read_range<S: ObjectRead, W: Write>(
    store: &S,
    root: FileStateRoot,
    range: Range<u64>,
    mut sink: W,
) -> CoreResult<RopeCounters> {
    let mut counters = RopeCounters::default();
    let plan = read_plan(store, root, &mut counters)?;
    read_range_with_plan_into(store, &plan, range, &mut sink, &mut counters)?;
    Ok(counters)
}

pub fn read_plan<S: ObjectRead>(
    store: &S,
    root: FileStateRoot,
    counters: &mut RopeCounters,
) -> CoreResult<ReadPlan> {
    let state = state(store, root, counters)?;
    let summary = Summary {
        id: state.mapping_root,
        bytes: state.logical_len,
        extents: state.extent_count,
        level: state.tree_level,
    };
    counters.nodes_read = add(counters.nodes_read, 1)?;
    let mapping = store.with_authenticated_canonical(summary.id, |canonical| {
        decode_node_with_context(canonical, true)
    })?;
    validate_summary(&mapping, summary)?;
    Ok(ReadPlan { state, mapping })
}

pub fn read_range_with_plan<S: ObjectRead, W: Write>(
    store: &S,
    plan: &ReadPlan,
    range: Range<u64>,
    mut sink: W,
) -> CoreResult<RopeCounters> {
    let mut counters = RopeCounters::default();
    read_range_with_plan_into(store, plan, range, &mut sink, &mut counters)?;
    Ok(counters)
}

fn read_range_with_plan_into<S: ObjectRead, W: Write>(
    store: &S,
    plan: &ReadPlan,
    range: Range<u64>,
    sink: &mut W,
    counters: &mut RopeCounters,
) -> CoreResult<()> {
    if range.start > range.end || range.end > plan.state.logical_len {
        return Err(CoreError::InvalidRange {
            start: range.start,
            end: range.end,
            length: plan.state.logical_len,
        });
    }
    if range.is_empty() {
        return Ok(());
    }
    let summary = Summary {
        id: plan.state.mapping_root,
        bytes: plan.state.logical_len,
        extents: plan.state.extent_count,
        level: plan.state.tree_level,
    };
    let mut ancestors = vec![summary.id];
    let mut selected = Vec::with_capacity(64);
    read_decoded_node(
        store,
        0,
        &range,
        sink,
        counters,
        &mut ancestors,
        &plan.mapping,
        &mut selected,
    )?;
    flush_read_batch(store, sink, counters, &mut selected)?;
    Ok(())
}

pub fn read_all<S: ObjectRead, W: Write>(
    store: &S,
    root: FileStateRoot,
    sink: W,
) -> CoreResult<RopeCounters> {
    read_all_bounded(store, root, u64::MAX, sink)
}

pub fn read_all_bounded<S: ObjectRead, W: Write>(
    store: &S,
    root: FileStateRoot,
    maximum: u64,
    mut sink: W,
) -> CoreResult<RopeCounters> {
    let mut counters = RopeCounters::default();
    let state = state(store, root, &mut counters)?;
    if state.logical_len > maximum {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let mut selected = Vec::with_capacity(64);
    read_node(
        store,
        Summary {
            id: state.mapping_root,
            bytes: state.logical_len,
            extents: state.extent_count,
            level: state.tree_level,
        },
        true,
        0,
        &(0..state.logical_len),
        &mut sink,
        &mut counters,
        &mut Vec::new(),
        &mut selected,
    )?;
    flush_read_batch(store, &mut sink, &mut counters, &mut selected)?;
    Ok(counters)
}

pub fn visit_extents<S: ObjectRead>(
    store: &S,
    root: FileStateRoot,
    mut visitor: impl FnMut(&[ExtentSliceV3]) -> CoreResult<()>,
) -> CoreResult<(FileStateV3, RopeCounters)> {
    let mut counters = RopeCounters::default();
    let state = state(store, root, &mut counters)?;
    visit_extent_node(
        store,
        Summary {
            id: state.mapping_root,
            bytes: state.logical_len,
            extents: state.extent_count,
            level: state.tree_level,
        },
        true,
        &mut counters,
        &mut Vec::new(),
        &mut visitor,
    )?;
    Ok((state, counters))
}

#[allow(clippy::too_many_arguments)]
fn read_node<S: ObjectRead, W: Write>(
    store: &S,
    expected: Summary,
    root: bool,
    base: u64,
    range: &Range<u64>,
    sink: &mut W,
    counters: &mut RopeCounters,
    ancestors: &mut Vec<ObjectId>,
    selected: &mut Vec<(ExtentSliceV3, u64, u64)>,
) -> CoreResult<()> {
    if ancestors.contains(&expected.id) {
        return Err(CoreError::MappingCycle);
    }
    ancestors.push(expected.id);
    counters.nodes_read = add(counters.nodes_read, 1)?;
    let node = store.with_authenticated_canonical(expected.id, |canonical| {
        decode_node_with_context(canonical, root)
    })?;
    validate_summary(&node, expected)?;
    read_decoded_node(
        store, base, range, sink, counters, ancestors, &node, selected,
    )?;
    ancestors.pop();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_decoded_node<S: ObjectRead, W: Write>(
    store: &S,
    base: u64,
    range: &Range<u64>,
    sink: &mut W,
    counters: &mut RopeCounters,
    ancestors: &mut Vec<ObjectId>,
    node: &ExtentNodeV3,
    selected: &mut Vec<(ExtentSliceV3, u64, u64)>,
) -> CoreResult<()> {
    match node {
        ExtentNodeV3::Leaf { extents, .. } => {
            let mut logical = base;
            for extent in extents {
                let end = logical
                    .checked_add(u64::from(extent.logical_length))
                    .ok_or(CoreError::LengthOverflow)?;
                if end > range.start && logical < range.end {
                    selected.push((
                        *extent,
                        range.start.max(logical) - logical,
                        range.end.min(end) - logical,
                    ));
                    if selected.len() == 64 {
                        flush_read_batch(store, sink, counters, selected)?;
                    }
                }
                if logical >= range.end {
                    break;
                }
                logical = end;
            }
        }
        ExtentNodeV3::Branch {
            level, children, ..
        } => {
            let first = children.partition_point(|child| {
                base.checked_add(child.cumulative_logical_end)
                    .is_some_and(|end| end <= range.start)
            });
            let mut prior = if first == 0 {
                0
            } else {
                children[first - 1].cumulative_logical_end
            };
            let summaries = child_summaries(children, *level - 1);
            for (child, summary) in children.iter().copied().zip(summaries).skip(first) {
                let child_start = base.checked_add(prior).ok_or(CoreError::LengthOverflow)?;
                if child_start >= range.end {
                    break;
                }
                read_node(
                    store,
                    summary,
                    false,
                    child_start,
                    range,
                    sink,
                    counters,
                    ancestors,
                    selected,
                )?;
                prior = child.cumulative_logical_end;
            }
        }
    }
    Ok(())
}

fn flush_read_batch<S: ObjectRead, W: Write>(
    store: &S,
    sink: &mut W,
    counters: &mut RopeCounters,
    selected: &mut Vec<(ExtentSliceV3, u64, u64)>,
) -> CoreResult<()> {
    if selected.is_empty() {
        return Ok(());
    }
    let ids = selected
        .iter()
        .map(|(extent, _, _)| extent.payload_object_id)
        .collect::<Vec<_>>();
    let mut index = 0_usize;
    store.get_authenticated_batch(&ids, |id, payload| {
        let (extent, overlap_start, overlap_end) = selected
            .get(index)
            .copied()
            .ok_or(CoreError::InvalidRecord("payload batch cardinality"))?;
        if id != extent.payload_object_id {
            return Err(CoreError::IdentityMismatch);
        }
        index += 1;
        if payload.len() > crate::file::cdc::MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ChunkLengthMismatch);
        }
        let source_end = extent
            .source_offset
            .checked_add(extent.logical_length)
            .ok_or(CoreError::LengthOverflow)? as usize;
        if source_end > payload.len() {
            return Err(CoreError::ChunkLengthMismatch);
        }
        let start = extent.source_offset as usize + overlap_start as usize;
        let stop = extent.source_offset as usize + overlap_end as usize;
        sink.write_all(&payload[start..stop])
            .map_err(|_| CoreError::Io)?;
        counters.payload_bytes_read = add(counters.payload_bytes_read, (stop - start) as u64)?;
        Ok(())
    })?;
    if index != selected.len() {
        return Err(CoreError::InvalidRecord("payload batch cardinality"));
    }
    selected.clear();
    Ok(())
}

pub(super) fn validate_summary(node: &ExtentNodeV3, expected: Summary) -> CoreResult<()> {
    if node.level() != expected.level
        || node.logical_len() != expected.bytes
        || node.extent_count() != expected.extents
    {
        Err(CoreError::InvalidRecord("extent summary"))
    } else {
        Ok(())
    }
}
