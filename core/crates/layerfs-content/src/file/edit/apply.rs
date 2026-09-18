//! Known-edit construction: one dispatch on the declared final length.
//!
//! The base is opened once, the edit stream is validated once and the
//! representation of the *result* decides the route, never the route deciding the
//! result: below the cutoff the final whole object is assembled into one
//! allocation, at or above it the retained bytes and the replacements run through
//! the same canonical builder complete-file construction uses. A stream whose
//! replacements are all byte-identical to their base ranges returns the base root
//! itself.

use std::io::Read;

use layerfs_telemetry::timer::{Active, TimingScope};

use crate::error::{ContentError, ContentResult};
use crate::file::content::{encode_whole_file, ConstructedFile};
use crate::file::edit::compare::{compare_replacements, NoOpVerdict};
use crate::file::edit::finish::{emit_empty_representation, emit_file_state};
use crate::file::edit::input::{EditSource, EditStream, Plan, ReplacementReader, Segment};
use crate::file::view::FileView;
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
    pub edits: &'a EditStream,
    /// Bounded replacement byte source.
    pub source: &'a dyn EditSource,
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
    policy.validated()?;
    scope.run(|edit| {
        let view = FileView::open(reader, request.root, edit.child("edit.base"))?;
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
        if let NoOpVerdict::Equal = compare_replacements(
            &view,
            reader,
            request.edits,
            request.source,
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
                let bytes = assemble_final(
                    &view,
                    reader,
                    request.edits,
                    request.source,
                    edit.child("edit.assemble"),
                )?;
                let canonical = edit
                    .child("content.encode")
                    .run(|_| encode_whole_file(capacities, &bytes))?;
                let mut object = FinalizedObject::new(ObjectRole::WholeFile, canonical)?;
                if view.root() != object.id() {
                    object = object.with_predecessors(AdvisoryPredecessors::explicit(view.root())?);
                }
                let root = object.id();
                edit.child("content.emit")
                    .run(|_| consumer.accept(object))?;
                Ok(ConstructedFile {
                    root,
                    logical_len: bytes.len() as u64,
                    counters: crate::file::edit::EditCounters::default(),
                })
            }
            crate::policy::Representation::Chunked => match view.file_state()? {
                // The view already acquired and decoded the base root, so the
                // chunked route is handed that decoded state instead of reading
                // and decoding the same object a second time.
                Some(state) => replace_chunked(capacities, state, reader, &request, consumer, edit),
                None => stream_combined(capacities, &view, &request, consumer, edit),
            },
        }
    })
}

/// Assembles the whole result into one allocation, from retained ranges and
/// replacements, without reading any discarded range.
#[allow(clippy::too_many_arguments)]
fn assemble_final(
    view: &FileView,
    reader: &dyn AuthenticatedObjects,
    stream: &EditStream,
    source: &dyn EditSource,
    scope: TimingScope<'_, layerfs_telemetry::timer::Pending>,
) -> ContentResult<Vec<u8>> {
    scope.run(|assemble| assemble_inner(view, reader, stream, source, assemble))
}

fn assemble_inner(
    view: &FileView,
    reader: &dyn AuthenticatedObjects,
    stream: &EditStream,
    source: &dyn EditSource,
    scope: &TimingScope<'_, layerfs_telemetry::timer::Active>,
) -> ContentResult<Vec<u8>> {
    let final_len = stream.final_len();
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve_exact(final_len as usize).map_err(|_| {
        ContentError::BoundedCapacityExceeded {
            what: "edit.assembly",
            limit: final_len,
            actual: final_len,
        }
    })?;
    // The plan is read once, into the segment list the assembly walks: a chunked
    // base then assembles through one cursor, so a mapping page two retained runs
    // share is demanded once for the whole assembly instead of once per run.
    let mut plan = Plan::new(stream);
    let mut segments: Vec<Segment> = Vec::new();
    while let Some(segment) = plan.advance()? {
        segments.push(segment);
    }
    let mut cursor = match view.file_state()? {
        Some(state) => Some(crate::file::mapping::RangeCursor::new(
            reader, state, scope,
        )?),
        None => None,
    };
    for segment in &segments {
        match *segment {
            Segment::Retain { base } => {
                if base.1 > base.0 {
                    match &mut cursor {
                        Some(cursor) => {
                            cursor.read_segment(base.0..base.1, &mut out)?;
                        }
                        None => {
                            view.read_range(reader, base.0..base.1, &mut out, scope)?;
                        }
                    }
                }
            }
            Segment::Replace { index, len, .. } => {
                // The replaced base range is deliberately not read: only the
                // replacement bytes enter the assembled result.
                append_replacement(source, index, len, &mut out)?;
            }
        }
    }
    if out.len() as u64 != final_len {
        return Err(ContentError::LengthMismatch {
            expected: final_len,
            actual: out.len() as u64,
        });
    }
    Ok(out)
}

fn append_replacement(
    source: &dyn EditSource,
    index: usize,
    length: u64,
    sink: &mut Vec<u8>,
) -> ContentResult<()> {
    if source.replacement_len(index) != length {
        return Err(ContentError::InvalidEdit {
            what: "replacement length",
        });
    }
    let mut reader = ReplacementReader::new(source, index, length);
    let mut buffer = [0_u8; 16 * 1024];
    let mut remaining = length;
    while remaining > 0 {
        let want = remaining.min(buffer.len() as u64) as usize;
        let read = reader
            .read(&mut buffer[..want])
            .map_err(|_| ContentError::Io)?;
        if read == 0 {
            return Err(ContentError::InvalidEdit {
                what: "replacement bytes",
            });
        }
        sink.extend_from_slice(&buffer[..read]);
        remaining -= read as u64;
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
    request: &EditRequest<'_>,
    consumer: &mut dyn FinalizedConsumer,
    edit: &TimingScope<'_, Active>,
) -> ContentResult<ConstructedFile> {
    // One read of the base root per edit, not two: the state the view decoded is
    // the state this route starts from, and the summary is derived from it.
    let mut summary = crate::file::mapping::NodeSummary {
        id: state.mapping_root,
        bytes: state.logical_len,
        extents: state.extent_count,
        level: state.tree_level,
    };
    // The only fact the loop carries forward about the result is its length: the
    // mapping root lives in `summary` and every other field of the file state is
    // derived once, at emission.
    let mut result_len = state.logical_len;
    let mut objects = crate::file::edit::tree::EditObjects::new(reader, consumer);
    for (index, declared) in request.edits.edits().iter().enumerate() {
        let replacement_len = declared.replacement_len();
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
        crate::file::edit::tree::discard(&mut objects, removed);
        // The declared replacement length is checked against the source before any
        // work: a source that cannot serve the declared bytes is a caller error, not
        // an I/O failure to be interpreted.
        if request.source.replacement_len(index) != replacement_len {
            return Err(ContentError::InvalidEdit {
                what: "replacement length",
            });
        }
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
                let mut builder = crate::file::mapping::ExtentBuilder::new(capacities);
                let mut sink = crate::file::edit::tree::DeferredSink::new(&mut objects);
                let source = crate::file::edit::input::ReplacementReader::new(
                    request.source,
                    index,
                    replacement_len,
                );
                let scanned = crate::file::cdc::FastCdc::new().scan(source, |chunk| {
                    builder
                        .push_chunk(chunk, predecessor, &mut sink)
                        .map(|_| ())
                })?;
                if scanned.bytes_scanned != replacement_len {
                    return Err(ContentError::InvalidEdit {
                        what: "replacement bytes",
                    });
                }
                builder.finish(&mut sink).map(|build| build.root)
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
        objects.settle(mapping);
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
        counters: objects.counters(),
    })
}

/// Payload of the rightmost extent under `summary`, if it has any.
///
/// Walking one path to the boundary is bounded by the tree height and is the only
/// stored work the physical delta hint needs.
fn rightmost_payload(
    objects: &mut crate::file::edit::tree::EditObjects<'_>,
    summary: crate::file::mapping::NodeSummary,
) -> ContentResult<Option<ObjectId>> {
    let mut current = summary;
    let mut root = true;
    loop {
        match objects.load_node(current, root)? {
            crate::file::mapping::ExtentNode::Leaf { extents, .. } => {
                return Ok(extents.last().map(|extent| extent.payload_object_id()))
            }
            crate::file::mapping::ExtentNode::Branch {
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
    request: &EditRequest<'_>,
    consumer: &mut dyn FinalizedConsumer,
    edit: &TimingScope<'_, Active>,
) -> ContentResult<ConstructedFile> {
    let source = PlanReader::new(view, request.edits, request.source)?;
    let build = edit
        .child("content.chunk")
        .run(|_| crate::file::mapping::build_streaming(capacities, source, consumer))?;
    let emitted = edit
        .child("edit.finish")
        .run(|_| emit_file_state(consumer, build))?;
    Ok(ConstructedFile {
        root: emitted.root,
        logical_len: emitted.logical_len,
        counters: crate::file::edit::EditCounters::default(),
    })
}

/// `Read` adapter that yields the planned result as one continuous byte stream.
struct PlanReader<'a> {
    plan: Plan<'a>,
    source: &'a dyn EditSource,
    payload: &'a [u8],
    current: Option<Stream<'a>>,
    done: bool,
}

enum Stream<'a> {
    Retained(&'a [u8]),
    Replacement(ReplacementReader<'a>),
}

impl<'a> PlanReader<'a> {
    fn new(
        view: &'a FileView,
        stream: &'a EditStream,
        source: &'a dyn EditSource,
    ) -> ContentResult<Self> {
        let payload = view
            .whole_file_bytes()?
            .ok_or(ContentError::WrongLogicalRole)?;
        Ok(Self {
            plan: Plan::new(stream),
            source,
            payload,
            current: None,
            done: false,
        })
    }
}

impl Read for PlanReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        loop {
            match &mut self.current {
                Some(Stream::Retained(bytes)) => {
                    if bytes.is_empty() {
                        self.current = None;
                        continue;
                    }
                    let take = bytes.len().min(buffer.len());
                    buffer[..take].copy_from_slice(&bytes[..take]);
                    *bytes = &bytes[take..];
                    return Ok(take);
                }
                Some(Stream::Replacement(reader)) => {
                    let read = reader.read(buffer)?;
                    if read == 0 {
                        self.current = None;
                        continue;
                    }
                    return Ok(read);
                }
                None => {}
            }
            if self.done {
                return Ok(0);
            }
            let next = self
                .plan
                .advance()
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
            match next {
                None => {
                    self.done = true;
                }
                Some(Segment::Retain { base }) => {
                    let slice = self
                        .payload
                        .get(base.0 as usize..base.1 as usize)
                        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
                    self.current = Some(Stream::Retained(slice));
                }
                Some(Segment::Replace { index, len, .. }) => {
                    self.current = Some(Stream::Replacement(ReplacementReader::new(
                        self.source,
                        index,
                        len,
                    )));
                }
            }
        }
    }
}
