use super::edit::load_node_cached;
use super::read::state;
use super::state::{FileStateRoot, ObjectRead, RopeCounters, Summary};
use super::validate::child_summaries;
use crate::error::{CoreError, CoreResult};
use crate::file::extent::{ExtentNodeV3, ExtentSliceV3};
use crate::object::ObjectId;
use std::ops::Range;

/// Emits coalesced logical ranges whose extent identities differ. Equal file
/// or mapping object identities stop before fetching descendants. A false
/// return means the logical lengths differ and the caller must use a full
/// fallback; mapping nodes and payloads are not read in that case.
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

#[derive(Clone, Copy)]
enum SpanKind {
    Node { summary: Summary, root: bool },
    Extent(ExtentSliceV3),
}

#[derive(Clone, Copy)]
struct Span {
    kind: SpanKind,
    offset: u64,
    len: u64,
}

impl Span {
    fn node(summary: Summary) -> Self {
        Self {
            kind: SpanKind::Node {
                summary,
                root: true,
            },
            offset: 0,
            len: summary.bytes,
        }
    }

    fn slice(self, offset: u64, len: u64) -> CoreResult<Self> {
        if offset.checked_add(len).is_none_or(|end| end > self.len) {
            return Err(CoreError::LengthOverflow);
        }
        Ok(Self {
            kind: self.kind,
            offset: self
                .offset
                .checked_add(offset)
                .ok_or(CoreError::LengthOverflow)?,
            len,
        })
    }
}

struct ChangedRanges<'a, F> {
    start: Option<u64>,
    visitor: &'a mut F,
}

impl<F: FnMut(Range<u64>) -> CoreResult<()>> ChangedRanges<'_, F> {
    fn record(&mut self, start: u64, len: u64, changed: bool) -> CoreResult<()> {
        let end = start.checked_add(len).ok_or(CoreError::LengthOverflow)?;
        if changed {
            self.start.get_or_insert(start);
        } else if let Some(changed_start) = self.start.take() {
            (self.visitor)(changed_start..start)?;
        }
        if end < start {
            return Err(CoreError::LengthOverflow);
        }
        Ok(())
    }

    fn finish(&mut self, end: u64) -> CoreResult<()> {
        if let Some(start) = self.start.take() {
            (self.visitor)(start..end)?;
        }
        Ok(())
    }
}

fn same_summary(left: Summary, right: Summary) -> bool {
    left.id == right.id
        && left.bytes == right.bytes
        && left.extents == right.extents
        && left.level == right.level
}

#[allow(clippy::too_many_arguments)]
fn diff_span<S: ObjectRead, F: FnMut(Range<u64>) -> CoreResult<()>>(
    store: &S,
    old: Span,
    new: Span,
    logical: u64,
    counters: &mut RopeCounters,
    old_ancestors: &mut Vec<ObjectId>,
    new_ancestors: &mut Vec<ObjectId>,
    old_cache: &mut Option<(ObjectId, ExtentNodeV3)>,
    new_cache: &mut Option<(ObjectId, ExtentNodeV3)>,
    emitter: &mut ChangedRanges<'_, F>,
) -> CoreResult<()> {
    if old.len != new.len {
        return Err(CoreError::LengthMismatch {
            expected: old.len,
            actual: new.len,
        });
    }
    match (old.kind, new.kind) {
        (
            SpanKind::Node {
                summary: old_summary,
                ..
            },
            SpanKind::Node {
                summary: new_summary,
                ..
            },
        ) if old_summary.id == new_summary.id && old.offset == new.offset => {
            if !same_summary(old_summary, new_summary) {
                return Err(CoreError::InvalidRecord("extent summary"));
            }
            emitter.record(logical, old.len, false)
        }
        (
            SpanKind::Node {
                summary: old_summary,
                root: old_root,
            },
            SpanKind::Node {
                summary: new_summary,
                root: new_root,
            },
        ) => {
            if old_ancestors.contains(&old_summary.id) || new_ancestors.contains(&new_summary.id) {
                return Err(CoreError::MappingCycle);
            }
            old_ancestors.push(old_summary.id);
            new_ancestors.push(new_summary.id);
            let old_spans = expand_span(store, old, old_root, counters, old_cache)?;
            let new_spans = expand_span(store, new, new_root, counters, new_cache)?;
            merge_spans(
                store,
                &old_spans,
                &new_spans,
                logical,
                counters,
                old_ancestors,
                new_ancestors,
                old_cache,
                new_cache,
                emitter,
            )?;
            old_ancestors.pop();
            new_ancestors.pop();
            Ok(())
        }
        (SpanKind::Extent(old_extent), SpanKind::Extent(new_extent)) => {
            let old_source = u64::from(old_extent.source_offset)
                .checked_add(old.offset)
                .ok_or(CoreError::LengthOverflow)?;
            let new_source = u64::from(new_extent.source_offset)
                .checked_add(new.offset)
                .ok_or(CoreError::LengthOverflow)?;
            emitter.record(
                logical,
                old.len,
                old_extent.payload_object_id != new_extent.payload_object_id
                    || old_source != new_source,
            )
        }
        (SpanKind::Node { summary, root }, _) => {
            if old_ancestors.contains(&summary.id) {
                return Err(CoreError::MappingCycle);
            }
            old_ancestors.push(summary.id);
            let spans = expand_span(store, old, root, counters, old_cache)?;
            merge_spans(
                store,
                &spans,
                &[new],
                logical,
                counters,
                old_ancestors,
                new_ancestors,
                old_cache,
                new_cache,
                emitter,
            )?;
            old_ancestors.pop();
            Ok(())
        }
        (_, SpanKind::Node { summary, root }) => {
            if new_ancestors.contains(&summary.id) {
                return Err(CoreError::MappingCycle);
            }
            new_ancestors.push(summary.id);
            let spans = expand_span(store, new, root, counters, new_cache)?;
            merge_spans(
                store,
                &[old],
                &spans,
                logical,
                counters,
                old_ancestors,
                new_ancestors,
                old_cache,
                new_cache,
                emitter,
            )?;
            new_ancestors.pop();
            Ok(())
        }
    }
}

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
