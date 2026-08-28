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
