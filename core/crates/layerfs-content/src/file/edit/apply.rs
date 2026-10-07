//! Known-edit construction: one dispatch on the declared final length.
//!
//! The base is opened once, the edit stream is validated once and the
//! representation of the *result* decides the route, never the route deciding the
//! result: below the cutoff the final whole object is assembled into one
//! allocation, at or above it the retained bytes and the replacements run through
//! the same canonical builder complete-file construction uses. A stream whose
//! replacements are all byte-identical to their base ranges returns the base root
//! itself.

use layerfs_telemetry::timer::{Active, TimingScope};

use super::runs::{PlanRuns, ReplacementRuns};
use super::source::Source;
use crate::error::{ContentError, ContentResult};
use crate::file::content::{begin_whole_file_object, ConstructedFile, WHOLE_VALUE_HEADER};
use crate::file::edit::compare::{compare_source, NoOpVerdict};
use crate::file::edit::finish::{emit_empty_representation, emit_file_state};
use crate::file::edit::input::{EditSequence, EditSource, IndexedEditSource, Plan, Segment};
use crate::file::view::FileView;
use crate::file::{FileRun, FileRuns};
use crate::object::{
    AdvisoryPredecessors, AuthenticatedObjects, FinalizedConsumer, FinalizedObject, ObjectId,
    ObjectRole,
};
use crate::policy::{ConstructionCapacities, ConstructionPolicy};

/// One known edit: the immutable base, the declared stream and its bytes.
pub struct EditRequest<'a> {
    /// Root of the immutable base.
    pub root: ObjectId,
    /// Validated ordered edits in current-result coordinates.
    pub edits: &'a dyn EditSequence,
    /// Bounded replacement byte source.
    pub source: &'a dyn EditSource,
}

/// One indexed edit over an already-authenticated, retained base classification.
///
/// The caller keeps the view and demand provider in the same authorized operation
/// context. Construction borrows the view; it never reopens or reclassifies its
/// root. Source and edit records remain stable across the planned passes.
pub struct IndexedEditRequest<'a> {
    /// Retained authenticated base, including its original root bytes.
    pub view: &'a FileView,
    /// Ordered final edits in current-result coordinates.
    pub edits: &'a dyn EditSequence,
    /// Fallible indexed metadata and replacement runs.
    pub source: &'a dyn IndexedEditSource,
}

struct Input<'a> {
    edits: &'a dyn EditSequence,
    source: Source<'a>,
}

/// Applies one known edit and emits the finalized result to `consumer`.
pub fn apply_edits(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    reader: &dyn AuthenticatedObjects,
    request: EditRequest<'_>,
    consumer: &mut dyn FinalizedConsumer,
    scope: TimingScope<'_>,
) -> ContentResult<ConstructedFile> {
    apply_inner(policy, capacities, reader, request, consumer, None, scope)
}

/// Applies the same canonical edit algorithm with caller-owned indexed mutable
/// state. The provider owns its explicit operation/file scope and first original
/// failure custody; construction never opens backing or releases that scope.
#[allow(clippy::too_many_arguments)]
pub fn apply_edits_backed(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    reader: &dyn AuthenticatedObjects,
    request: EditRequest<'_>,
    consumer: &mut dyn FinalizedConsumer,
    backing: &mut dyn super::backing::IndexedEditBacking,
    scope: TimingScope<'_>,
) -> ContentResult<ConstructedFile> {
    apply_inner(
        policy,
        capacities,
        reader,
        request,
        consumer,
        Some(backing),
        scope,
    )
}

/// Applies the existing canonical driver to a retained view and fallible indexed
/// source. Policy-derived capacities and declared base length are checked before
/// source demands, consumer acceptance or mutable backing work. An immutable
/// no-op returns before the backed editor's scope guard. The caller retains all
/// providers and original failed/accepted-operation custody and fences them.
#[allow(clippy::too_many_arguments)]
pub fn apply_indexed_edits_view_backed(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    reader: &dyn AuthenticatedObjects,
    request: IndexedEditRequest<'_>,
    consumer: &mut dyn FinalizedConsumer,
    backing: &mut dyn super::backing::IndexedEditBacking,
    scope: TimingScope<'_>,
) -> ContentResult<ConstructedFile> {
    let policy = policy.validated()?;
    if *capacities != policy.capacities() {
        return Err(ContentError::InvalidRecord("edit policy capacities"));
    }
    scope.run(|edit| {
        apply_view(
            policy,
            capacities,
            reader,
            request.view,
            Input {
                edits: request.edits,
                source: Source::Indexed(request.source),
            },
            consumer,
            Some(backing),
            edit,
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_inner(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    reader: &dyn AuthenticatedObjects,
    request: EditRequest<'_>,
    consumer: &mut dyn FinalizedConsumer,
    backing: Option<&mut dyn super::backing::IndexedEditBacking>,
    scope: TimingScope<'_>,
) -> ContentResult<ConstructedFile> {
    policy.validated()?;
    scope.run(|edit| {
        let view = FileView::open(reader, request.root, edit.child("edit.base"))?;
        apply_view(
            policy,
            capacities,
            reader,
            &view,
            Input {
                edits: request.edits,
                source: Source::Legacy(request.source),
            },
            consumer,
            backing,
            edit,
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_view(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    reader: &dyn AuthenticatedObjects,
    view: &FileView,
    request: Input<'_>,
    consumer: &mut dyn FinalizedConsumer,
    backing: Option<&mut dyn super::backing::IndexedEditBacking>,
    edit: &TimingScope<'_, Active>,
) -> ContentResult<ConstructedFile> {
    if view.logical_len() != request.edits.base_len() {
        return Err(ContentError::InvalidEdit {
            what: "declared base length",
        });
    }
    if request.edits.is_empty() {
        // Nothing was declared: the base root is the exact result.
        return Ok(ConstructedFile {
            root: view.root(),
            logical_len: view.logical_len(),
            counters: crate::file::edit::EditCounters::default(),
        });
    }
    let final_len = request.edits.final_len();
    let representation = policy.representation(final_len);
    // One mapping-page memo for the whole operation. The comparison pass fills
    // it as it navigates, and the construction pass reads the pages it names
    // from it instead of demanding them a second time.
    let mut pages = crate::file::mapping::PageCache::new();
    if let NoOpVerdict::Equal = compare_source(
        view,
        reader,
        request.edits,
        request.source,
        &mut pages,
        edit.child("edit.compare"),
    )? {
        // Every replacement was byte-identical, so the result is the base.
        return Ok(ConstructedFile {
            root: view.root(),
            logical_len: view.logical_len(),
            counters: crate::file::edit::EditCounters::default(),
        });
    }
    match representation {
        crate::policy::Representation::Empty => {
            let emitted = emit_empty_representation(consumer)?;
            Ok(ConstructedFile {
                root: emitted.root,
                logical_len: 0,
                counters: crate::file::edit::EditCounters::default(),
            })
        }
        crate::policy::Representation::WholeFile => {
            // The canonical object is the sink: its envelope and value header
            // are written first and its payload area is reserved exactly, so
            // the assembly appends into the object that is emitted instead of
            // building a payload, a value and a canonical copy of it.
            let mut canonical = begin_whole_file_object(capacities, final_len)?;
            let logical_len = assemble_into(
                view,
                reader,
                request.edits,
                request.source,
                &mut pages,
                &mut canonical,
                edit.child("edit.assemble"),
            )?;
            let payload = crate::object::HEADER_LEN
                + crate::object::VALUE_LEN_BYTES
                + WHOLE_VALUE_HEADER
                + logical_len as usize;
            if final_len != logical_len || canonical.len() != payload {
                return Err(ContentError::LengthMismatch {
                    expected: final_len,
                    actual: logical_len,
                });
            }
            let mut object = edit
                .child("content.identify")
                .run(|_| FinalizedObject::new(ObjectRole::WholeFile, canonical))?;
            if view.root() != object.id() {
                object = object.with_predecessors(AdvisoryPredecessors::explicit(view.root())?);
            }
            let root = object.id();
            edit.child("content.emit")
                .run(|_| consumer.accept(object))?;
            Ok(ConstructedFile {
                root,
                logical_len,
                counters: crate::file::edit::EditCounters::default(),
            })
        }
        crate::policy::Representation::Chunked => match view.file_state()? {
            // The view already acquired and decoded the base root, so the
            // chunked route is handed that decoded state instead of reading
            // and decoding the same object a second time.
            Some(state) => replace_chunked(
                capacities, state, reader, &request, consumer, &mut pages, backing, edit,
            ),
            None => stream_combined(capacities, view, &request, consumer, edit),
        },
    }
}

/// Assembles the whole result into `out`, from retained ranges and replacements,
/// without reading any discarded range.
///
/// `out` is the caller's buffer: on the whole-file route it is the canonical
/// object's own pre-sized allocation, and its current length is where the payload
/// begins. The returned value is the payload length this assembly emitted, which
/// the caller checks against the declared final length.
#[allow(clippy::too_many_arguments)]
fn assemble_into(
    view: &FileView,
    reader: &dyn AuthenticatedObjects,
    stream: &dyn EditSequence,
    source: Source<'_>,
    pages: &mut crate::file::mapping::PageCache,
    out: &mut Vec<u8>,
    scope: TimingScope<'_, layerfs_telemetry::timer::Pending>,
) -> ContentResult<u64> {
    scope.run(|assemble| assemble_inner(view, reader, stream, source, pages, out, assemble))
}

#[allow(clippy::too_many_arguments)]
fn assemble_inner(
    view: &FileView,
    reader: &dyn AuthenticatedObjects,
    stream: &dyn EditSequence,
    source: Source<'_>,
    pages: &mut crate::file::mapping::PageCache,
    out: &mut Vec<u8>,
    scope: &TimingScope<'_, layerfs_telemetry::timer::Active>,
) -> ContentResult<u64> {
    let payload_start = out.len();
    // One plan yields each segment only when assembly reaches it. A chunked
    // base keeps one cursor for all retained runs, so shared pages retain the
    // same memo without holding an operation-sized segment list.
    let mut plan = Plan::new(stream);
    let mut cursor = match view.file_state()? {
        Some(state) => Some(crate::file::mapping::RangeCursor::new(
            reader, state, pages, scope,
        )?),
        None => None,
    };
    while let Some(segment) = plan.advance()? {
        match segment {
            Segment::Retain { base } => {
                if base.1 > base.0 {
                    match &mut cursor {
                        Some(cursor) => {
                            cursor.read_segment(base.0..base.1, out, pages)?;
                        }
                        None => {
                            view.read_range(reader, base.0..base.1, out, scope)?;
                        }
                    }
                }
            }
            Segment::Replace { index, len, .. } => {
                // The replaced base range is deliberately not read: only the
                // replacement bytes enter the assembled result.
                append_replacement(source, index, len, out)?;
            }
        }
    }
    Ok(out.len() as u64 - payload_start as u64)
}

fn append_replacement(
    source: Source<'_>,
    index: usize,
    length: u64,
    sink: &mut Vec<u8>,
) -> ContentResult<()> {
    source.check_legacy_length(index, length)?;
    let mut source = ReplacementRuns::new(source, index, length)?;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        match source.read_run(&mut buffer)? {
            FileRun::Data(count) => sink.extend_from_slice(&buffer[..count]),
            FileRun::Zero(length) => {
                let count = usize::try_from(length).map_err(|_| ContentError::LengthOverflow)?;
                let end = sink
                    .len()
                    .checked_add(count)
                    .ok_or(ContentError::LengthOverflow)?;
                sink.resize(end, 0);
            }
            FileRun::End => break,
        }
    }
    Ok(())
}

/// Applies one ordered edit stream to a chunked base with stored-node splits.
///
/// Each edit is four operations over summaries: split the running mapping at the
/// replacement start, split the remainder at the end of the deleted range, scan the
/// replacement into its own subtree, and concatenate the three parts. A subtree the
/// edit does not touch keeps its stored identity and is never read again, and only
/// the nodes the final mapping reaches are published.
#[allow(clippy::too_many_arguments)]
fn replace_chunked(
    capacities: &ConstructionCapacities,
    state: crate::file::mapping::FileState,
    reader: &dyn AuthenticatedObjects,
    request: &Input<'_>,
    consumer: &mut dyn FinalizedConsumer,
    pages: &mut crate::file::mapping::PageCache,
    backing: Option<&mut dyn super::backing::IndexedEditBacking>,
    edit: &TimingScope<'_, Active>,
) -> ContentResult<ConstructedFile> {
    // One read of the base root per edit, not two: the state the view decoded is
    // the state this route starts from, and the summary is derived from it.
    let mut summary = super::references::Summary {
        id: super::references::EditRef::Stored(state.mapping_root),
        bytes: state.logical_len,
        extents: state.extent_count,
        level: state.tree_level,
    };
    // The only fact the loop carries forward about the result is its length: the
    // mapping root lives in `summary` and every other field of the file state is
    // derived once, at emission.
    let mut result_len = state.logical_len;
    let state = match backing {
        Some(backing) => super::state::State::Backed(backing),
        None => super::state::State::Memory(super::state::Memory::default()),
    };
    let mut objects = crate::file::edit::tree::EditObjects::new(reader, consumer, pages, state);
    for index in 0..request.edits.len() {
        let declared = request.edits.edit_at(index)?;
        let replacement_len = declared.replacement_len();
        // Resolve fallible metadata before boundary mutation or base demands.
        request
            .source
            .check_indexed_length(index, replacement_len)?;
        let mut source = ReplacementRuns::from_length(request.source, index, replacement_len);
        let (left, tail) = edit.child("edit.split").run(|_| {
            crate::file::edit::tree::split(&mut objects, summary, declared.start(), true)
        })?;
        let (removed, right) = edit.child("edit.split").run(|_| match tail {
            Some(tail) => {
                crate::file::edit::tree::split(&mut objects, tail, declared.removed_len(), true)
            }
            None if declared.removed_len() == 0 => Ok((None, None)),
            None => Err(ContentError::InvalidRange {
                start: declared.removed_len(),
                end: declared.removed_len(),
                length: 0,
            }),
        })?;
        // The replaced range is dropped from the result, so the unfinished node
        // the split built for it is released here and never encoded.
        crate::file::edit::tree::discard(&mut objects, removed)?;
        // Keep the legacy source's original check and failure ordering. The
        // indexed source already resolved its fallible metadata before splits.
        request.source.check_legacy_length(index, replacement_len)?;
        let middle = if replacement_len == 0 {
            None
        } else {
            // The replacement is scanned into its own subtree through the same
            // canonical builder complete construction uses. Its first payload may
            // continue the retained payload immediately before the insert position,
            // which is a physical hint only — and only the scan consumes it, so a
            // pure deletion never pays the rightmost walk that computes it.
            let predecessor = match left {
                Some(left) => edit
                    .child("edit.split")
                    .run(|_| rightmost_payload(&mut objects, left))?,
                None => None,
            };
            edit.child("content.chunk").run(|_| {
                let mut sink = crate::file::edit::tree::DeferredSink::new(&mut objects);
                request.source.check_legacy_length(index, replacement_len)?;
                let build = crate::file::chunk_runs::build(
                    capacities,
                    &mut source,
                    predecessor,
                    &mut sink,
                )?;
                Ok(build
                    .root
                    .map(|root| super::references::Summary::from_canonical(root, true)))
            })?
        };
        let prefix = crate::file::edit::tree::concat_optional(&mut objects, left, middle)?;
        let joined = crate::file::edit::tree::concat_optional(&mut objects, prefix, right)?;
        let mapping = match joined {
            Some(mapping) => mapping,
            None => crate::file::edit::tree::emit_leaf(&mut objects, Vec::new())?,
        };
        summary = mapping;
        result_len = mapping.bytes;
        // The result of this edit is known: drafts the boundary work
        // disconnected are released here, so the retained frontier is the tree
        // the operation ends up with rather than the edits that produced it.
        objects.settle(mapping)?;
    }
    if result_len != request.edits.final_len() {
        return Err(ContentError::LengthMismatch {
            expected: request.edits.final_len(),
            actual: result_len,
        });
    }
    // The unfinished nodes the final mapping reaches are published children first,
    // then the file state that opens it. Nothing the edit discarded is emitted.
    let root = edit.child("edit.finish").run(|_| objects.finish(summary))?;
    Ok(ConstructedFile {
        root,
        logical_len: result_len,
        counters: objects.counters,
    })
}

/// Payload of the rightmost extent under `summary`, if it has any.
///
/// Walking one path to the boundary is bounded by the tree height and is the only
/// stored work the physical delta hint needs.
fn rightmost_payload(
    objects: &mut crate::file::edit::tree::EditObjects<'_>,
    summary: super::references::Summary,
) -> ContentResult<Option<ObjectId>> {
    let mut current = summary;
    let mut root = true;
    loop {
        match objects.load_node(current, root)? {
            super::references::Node::Leaf { extents, .. } => {
                return Ok(extents.last().map(|extent| extent.payload_object_id()))
            }
            super::references::Node::Branch {
                level, children, ..
            } => {
                if children.is_empty() {
                    return Err(ContentError::InvalidRecord("empty branch"));
                }
                let summaries = crate::file::edit::tree::child_summaries(&children, level - 1)?;
                current = *summaries
                    .last()
                    .ok_or(ContentError::InvalidRecord("empty branch"))?;
                root = false;
            }
        }
    }
}

/// Streams a whole-file base and its replacements through the complete builder.
fn stream_combined(
    capacities: &ConstructionCapacities,
    view: &FileView,
    request: &Input<'_>,
    consumer: &mut dyn FinalizedConsumer,
    edit: &TimingScope<'_, Active>,
) -> ContentResult<ConstructedFile> {
    let mut source = PlanRuns::new(view, request.edits, request.source)?;
    let build = edit
        .child("content.chunk")
        .run(|_| crate::file::chunk_runs::build(capacities, &mut source, None, consumer))?;
    if build.logical_len != request.edits.final_len() {
        return Err(ContentError::LengthMismatch {
            expected: request.edits.final_len(),
            actual: build.logical_len,
        });
    }
    let emitted = edit
        .child("edit.finish")
        .run(|_| emit_file_state(consumer, build))?;
    Ok(ConstructedFile {
        root: emitted.root,
        logical_len: emitted.logical_len,
        counters: crate::file::edit::EditCounters::default(),
    })
}
