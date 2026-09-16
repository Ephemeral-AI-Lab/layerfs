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
use crate::file::edit::frontier::EditFrontier;
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
            });
        }
        match representation {
            crate::policy::Representation::Empty => {
                let emitted = emit_empty_representation(consumer)?;
                Ok(ConstructedFile {
                    root: emitted.root,
                    logical_len: 0,
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
                })
            }
            crate::policy::Representation::Chunked => match view.file_state()? {
                Some(_) => stream_chunked(capacities, &view, reader, &request, consumer, edit),
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
    assemble: &TimingScope<'_, Active>,
) -> ContentResult<Vec<u8>> {
    let _ = assemble;
    let final_len = stream.final_len();
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve_exact(final_len as usize).map_err(|_| {
        ContentError::BoundedCapacityExceeded {
            what: "edit.assembly",
            limit: final_len,
            actual: final_len,
        }
    })?;
    let mut plan = Plan::new(stream);
    while let Some(segment) = plan.advance()? {
        match segment {
            Segment::Retain { base } => {
                if base.1 > base.0 {
                    view.read_range(reader, base.0..base.1, &mut out)?;
                }
            }
            Segment::Replace { index, base, len } => {
                if base.1 > base.0 {
                    // The replaced base range is deliberately not read.
                }
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

/// Streams a chunked base through the frontier, reusing retained extents.
fn stream_chunked(
    capacities: &ConstructionCapacities,
    view: &FileView,
    reader: &dyn AuthenticatedObjects,
    request: &EditRequest<'_>,
    consumer: &mut dyn FinalizedConsumer,
    edit: &TimingScope<'_, layerfs_telemetry::timer::Active>,
) -> ContentResult<ConstructedFile> {
    let mut frontier = EditFrontier::new(capacities);
    let mut plan = Plan::new(request.edits);
    let mut pending = plan.advance()?;
    let mut skip_to = 0_u64;
    let mut origin = 0_u64;
    let mut scan_error: Option<ContentError> = None;
    let walked = edit.child("edit.stream").run(|_| {
        view.walk_extents(reader, &mut |extent| {
            let end = origin
                .checked_add(u64::from(extent.logical_length()))
                .ok_or(ContentError::LengthOverflow)?;
            let mut local = origin.max(skip_to);
            while local < end {
                match pending {
                    Some(Segment::Replace { index, base, len }) if base.0 <= local => {
                        frontier.replace(request.source, index, len, consumer)?;
                        local = base.1;
                        skip_to = base.1;
                        pending = plan.advance()?;
                    }
                    Some(Segment::Replace { base, .. }) => {
                        let limit = end.min(base.0);
                        frontier.retain(extent, origin, local, limit, consumer)?;
                        local = limit;
                    }
                    Some(Segment::Retain { base }) => {
                        let limit = end.min(base.1);
                        if local < limit {
                            frontier.retain(extent, origin, local, limit, consumer)?;
                        }
                        local = limit;
                        if limit == base.1 {
                            pending = plan.advance()?;
                        }
                    }
                    None => {
                        frontier.retain(extent, origin, local, end, consumer)?;
                        local = end;
                    }
                }
            }
            origin = end;
            Ok(())
        })
    });
    if let Err(error) = walked {
        scan_error = Some(error);
    }
    if let Some(error) = scan_error {
        return Err(error);
    }
    if pending.is_some() {
        return Err(ContentError::InvalidEdit {
            what: "unreached segment",
        });
    }
    if frontier.logical_len() != request.edits.final_len() {
        return Err(ContentError::LengthMismatch {
            expected: request.edits.final_len(),
            actual: frontier.logical_len(),
        });
    }
    let build = edit
        .child("edit.finish")
        .run(|_| frontier.finish(consumer))?;
    let emitted = emit_file_state(consumer, build)?;
    Ok(ConstructedFile {
        root: emitted.root,
        logical_len: emitted.logical_len,
    })
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
